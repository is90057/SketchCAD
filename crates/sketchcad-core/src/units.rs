use serde::{Deserialize, Serialize};
use std::fmt;

/// Standard length representation internally in Millimeters (mm).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Length(pub f64);

impl Length {
    pub fn from_mm(mm: f64) -> Self {
        Self(mm)
    }

    pub fn from_m(m: f64) -> Self {
        Self(m * 1000.0)
    }

    pub fn from_cm(cm: f64) -> Self {
        Self(cm * 10.0)
    }

    pub fn as_mm(&self) -> f64 {
        self.0
    }

    pub fn as_m(&self) -> f64 {
        self.0 / 1000.0
    }

    pub fn as_cm(&self) -> f64 {
        self.0 / 10.0
    }
}

/// Area representation internally in Square Millimeters (mm²).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Area(pub f64);

impl Area {
    pub fn from_sq_m(m2: f64) -> Self {
        Self(m2 * 1_000_000.0)
    }

    pub fn as_sq_m(&self) -> f64 {
        self.0 / 1_000_000.0
    }

    /// Convert to Taiwanese Ping (坪, 1 坪 ≈ 3.305785 m²)
    pub fn as_ping(&self) -> f64 {
        self.as_sq_m() / 3.305785
    }
}

/// Volume representation internally in Cubic Millimeters (mm³).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Volume(pub f64);

impl Volume {
    pub fn from_cu_m(m3: f64) -> Self {
        Self(m3 * 1_000_000_000.0)
    }

    pub fn as_cu_m(&self) -> f64 {
        self.0 / 1_000_000_000.0
    }
}

/// Monetary cost specification.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Cost {
    pub amount: f64,
    pub currency: Currency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Currency {
    TWD,
    USD,
    EUR,
    JPY,
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {:.2}", self.currency, self.amount)
    }
}
