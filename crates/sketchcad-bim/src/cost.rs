use serde::{Deserialize, Serialize};
use sketchcad_core::{Currency, ElementId};

/// Single line item in an engineering bill of quantities / cost schedule
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostLineItem {
    pub element_id: ElementId,
    pub category: String,
    pub description: String,
    pub quantity: f64,
    pub unit: String,
    pub unit_price: f64,
    pub total_price: f64,
}

impl CostLineItem {
    pub fn new(
        element_id: ElementId,
        category: impl Into<String>,
        description: impl Into<String>,
        quantity: f64,
        unit: impl Into<String>,
        unit_price: f64,
    ) -> Self {
        Self {
            element_id,
            category: category.into(),
            description: description.into(),
            quantity,
            unit: unit.into(),
            unit_price,
            total_price: quantity * unit_price,
        }
    }
}

/// Quantity Takeoff (QTO) and Cost Summary table for an element or entire project
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuantityTakeoff {
    pub currency: Option<Currency>,
    pub items: Vec<CostLineItem>,
}

impl QuantityTakeoff {
    pub fn new() -> Self {
        Self {
            currency: Some(Currency::TWD),
            items: Vec::new(),
        }
    }

    pub fn add_item(&mut self, item: CostLineItem) {
        self.items.push(item);
    }

    pub fn total_cost(&self) -> f64 {
        self.items.iter().map(|item| item.total_price).sum()
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn append(&mut self, other: &QuantityTakeoff) {
        self.items.extend_from_slice(&other.items);
    }
}
