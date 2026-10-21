use serde::{Deserialize, Serialize};
use sketchcad_core::{BBox2D, Point2D, Vector2D};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const RED: Self = Self { r: 230, g: 50, b: 50, a: 255 };
    pub const GREEN: Self = Self { r: 50, g: 200, b: 50, a: 255 };
    pub const BLUE: Self = Self { r: 50, g: 120, b: 230, a: 255 };
    pub const CYAN: Self = Self { r: 0, g: 220, b: 220, a: 255 };
    pub const MAGENTA: Self = Self { r: 220, g: 0, b: 220, a: 255 };
    pub const YELLOW: Self = Self { r: 240, g: 210, b: 20, a: 255 };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// CAD Layer specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadLayer {
    pub name: String,
    pub color: Color,
    pub lineweight: f32, // in mm
    pub linetype: LineType,
    pub visible: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineType {
    Continuous,
    Dashed,
    Dotted,
    Center,
    Hidden,
}

impl Default for CadLayer {
    fn default() -> Self {
        Self {
            name: "0".to_string(),
            color: Color::WHITE,
            lineweight: 0.25,
            linetype: LineType::Continuous,
            visible: true,
            locked: false,
        }
    }
}

/// 2D Line segment with high-precision double floats
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line2D {
    pub start: Point2D,
    pub end: Point2D,
}

impl Line2D {
    pub fn new(start: Point2D, end: Point2D) -> Self {
        Self { start, end }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn direction(&self) -> Vector2D {
        (self.end - self.start).normalize()
    }

    pub fn normal(&self) -> Vector2D {
        let dir = self.direction();
        Vector2D::new(-dir.y, dir.x)
    }

    pub fn midpoint(&self) -> Point2D {
        (self.start + self.end) * 0.5
    }

    pub fn bbox(&self) -> BBox2D {
        BBox2D::from_points(&[self.start, self.end]).unwrap()
    }
}

/// 2D Polyline
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline2D {
    pub vertices: Vec<Point2D>,
    pub closed: bool,
}

impl Polyline2D {
    pub fn new(vertices: Vec<Point2D>, closed: bool) -> Self {
        Self { vertices, closed }
    }

    pub fn length(&self) -> f64 {
        if self.vertices.len() < 2 {
            return 0.0;
        }
        let mut total = 0.0;
        for i in 0..self.vertices.len() - 1 {
            total += self.vertices[i].distance(self.vertices[i + 1]);
        }
        if self.closed && self.vertices.len() > 2 {
            total += self.vertices.last().unwrap().distance(self.vertices[0]);
        }
        total
    }

    /// Computes the signed area of the polygon using the Shoelace formula.
    /// Positive if vertices are counter-clockwise, negative if clockwise.
    pub fn signed_area(&self) -> f64 {
        let n = self.vertices.len();
        if n < 3 {
            return 0.0;
        }
        let mut area = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            area += self.vertices[i].x * self.vertices[j].y;
            area -= self.vertices[j].x * self.vertices[i].y;
        }
        area * 0.5
    }

    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    pub fn bbox(&self) -> Option<BBox2D> {
        BBox2D::from_points(&self.vertices)
    }
}

/// 2D Arc segment
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Arc2D {
    pub center: Point2D,
    pub radius: f64,
    pub start_angle: f64, // radians
    pub end_angle: f64,   // radians
}

/// 2D Dimension annotation (linear dimension)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimension2D {
    pub point_a: Point2D,
    pub point_b: Point2D,
    pub offset: f64,
    pub text_override: Option<String>,
}

impl Dimension2D {
    pub fn measured_length(&self) -> f64 {
        self.point_a.distance(self.point_b)
    }

    pub fn display_text(&self) -> String {
        if let Some(ref text) = self.text_override {
            text.clone()
        } else {
            format!("{:.1}", self.measured_length())
        }
    }
}
