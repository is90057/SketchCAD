use crate::id::ElementId;
use petgraph::algo::{has_path_connecting, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Cyclic dependency detected when adding edge from {from} to {to}")]
    CyclicDependency { from: ElementId, to: ElementId },
    #[error("Element {0} not found in dependency graph")]
    ElementNotFound(ElementId),
}

/// Reactive Dependency Graph (DAG) for managing propagation of changes across
/// 2D geometry, 3D meshes, BIM smart objects, and quantity schedules.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    graph: DiGraph<ElementId, ()>,
    node_indices: HashMap<ElementId, NodeIndex>,
    dirty_nodes: HashSet<ElementId>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            dirty_nodes: HashSet::new(),
        }
    }

    /// Register a new element node in the graph.
    pub fn add_element(&mut self, id: ElementId) -> NodeIndex {
        if let Some(&idx) = self.node_indices.get(&id) {
            return idx;
        }
        let idx = self.graph.add_node(id);
        self.node_indices.insert(id, idx);
        self.dirty_nodes.insert(id);
        idx
    }

    /// Remove an element and all connected dependency edges.
    pub fn remove_element(&mut self, id: ElementId) {
        if let Some(idx) = self.node_indices.remove(&id) {
            self.graph.remove_node(idx);
            self.dirty_nodes.remove(&id);
            // Re-index remaining nodes since petgraph remove_node shifts indices
            self.node_indices.clear();
            for n_idx in self.graph.node_indices() {
                self.node_indices.insert(self.graph[n_idx], n_idx);
            }
        }
    }

    /// Add a dependency: `dependent` depends on `dependency`.
    /// When `dependency` changes, `dependent` must be updated.
    pub fn add_dependency(&mut self, dependency: ElementId, dependent: ElementId) -> Result<(), GraphError> {
        let dep_idx = self.add_element(dependency);
        let dep_on_idx = self.add_element(dependent);

        // Check if adding this edge would create a cycle
        if has_path_connecting(&self.graph, dep_on_idx, dep_idx, None) {
            return Err(GraphError::CyclicDependency {
                from: dependency,
                to: dependent,
            });
        }

        self.graph.update_edge(dep_idx, dep_on_idx, ());
        Ok(())
    }

    /// Mark an element as dirty, and cascade dirty state to all downstream dependents.
    pub fn mark_dirty(&mut self, id: ElementId) {
        let mut queue = vec![id];
        while let Some(current_id) = queue.pop() {
            if self.dirty_nodes.insert(current_id) {
                if let Some(&current_idx) = self.node_indices.get(&current_id) {
                    for neighbor in self.graph.neighbors_directed(current_idx, Direction::Outgoing) {
                        let dependent_id = self.graph[neighbor];
                        queue.push(dependent_id);
                    }
                }
            }
        }
    }

    /// Check if an element is currently marked as dirty.
    pub fn is_dirty(&self, id: ElementId) -> bool {
        self.dirty_nodes.contains(&id)
    }

    /// Clear dirty state for a specific element after it has been re-evaluated.
    pub fn clear_dirty(&mut self, id: ElementId) {
        self.dirty_nodes.remove(&id);
    }

    /// Clear all dirty states.
    pub fn clear_all_dirty(&mut self) {
        self.dirty_nodes.clear();
    }

    /// Returns dirty nodes sorted in topological dependency order.
    /// This guarantees that every dependency is recomputed before its dependents.
    pub fn get_evaluation_order(&self) -> Vec<ElementId> {
        let sorted = match toposort(&self.graph, None) {
            Ok(order) => order,
            Err(_) => return self.dirty_nodes.iter().copied().collect(),
        };

        sorted
            .into_iter()
            .map(|idx| self.graph[idx])
            .filter(|id| self.dirty_nodes.contains(id))
            .collect()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reactive_dirty_propagation() {
        let mut graph = DependencyGraph::new();
        let wall_id = ElementId::new();
        let door_id = ElementId::new();
        let schedule_id = ElementId::new();

        // Door depends on Wall
        graph.add_dependency(wall_id, door_id).unwrap();
        // Schedule depends on Door
        graph.add_dependency(door_id, schedule_id).unwrap();

        graph.clear_all_dirty();
        assert!(!graph.is_dirty(wall_id));
        assert!(!graph.is_dirty(door_id));
        assert!(!graph.is_dirty(schedule_id));

        // When wall changes, door and schedule must become dirty
        graph.mark_dirty(wall_id);
        assert!(graph.is_dirty(wall_id));
        assert!(graph.is_dirty(door_id));
        assert!(graph.is_dirty(schedule_id));

        let order = graph.get_evaluation_order();
        assert_eq!(order, vec![wall_id, door_id, schedule_id]);
    }
}
