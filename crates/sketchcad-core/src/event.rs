use crate::id::ElementId;
use serde::{Deserialize, Serialize};

/// Notification events dispatched when model modifications occur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeEvent {
    ElementCreated(ElementId),
    ElementModified(ElementId),
    ElementDeleted(ElementId),
    PropertyUpdated {
        id: ElementId,
        property_name: String,
    },
    DependencyLinked {
        dependency: ElementId,
        dependent: ElementId,
    },
    GlobalSyncRequired,
}
