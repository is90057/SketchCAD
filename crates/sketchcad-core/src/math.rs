pub use glam::{DMat3, DMat4, DQuat, DVec2, DVec3, Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};

pub type Point2D = DVec2;
pub type Point3D = DVec3;
pub type Vector2D = DVec2;
pub type Vector3D = DVec3;

/// 2D Axis-Aligned Bounding Box (AABB)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox2D {
    pub min: Point2D,
    pub max: Point2D,
}

impl BBox2D {
    pub fn new(min: Point2D, max: Point2D) -> Self {
        Self { min, max }
    }

    pub fn from_points<'a, I>(points: I) -> Option<Self>
    where
        I: IntoIterator<Item = &'a Point2D>,
    {
        let mut iter = points.into_iter();
        let first = *iter.next()?;
        let mut min = first;
        let mut max = first;
        for p in iter {
            min = min.min(*p);
            max = max.max(*p);
        }
        Some(Self { min, max })
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Point2D {
        (self.min + self.max) * 0.5
    }

    pub fn contains(&self, p: Point2D) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

/// 3D Axis-Aligned Bounding Box (AABB)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox3D {
    pub min: Point3D,
    pub max: Point3D,
}

impl BBox3D {
    pub fn new(min: Point3D, max: Point3D) -> Self {
        Self { min, max }
    }

    pub fn from_points<'a, I>(points: I) -> Option<Self>
    where
        I: IntoIterator<Item = &'a Point3D>,
    {
        let mut iter = points.into_iter();
        let first = *iter.next()?;
        let mut min = first;
        let mut max = first;
        for p in iter {
            min = min.min(*p);
            max = max.max(*p);
        }
        Some(Self { min, max })
    }

    pub fn size(&self) -> Vector3D {
        self.max - self.min
    }

    pub fn center(&self) -> Point3D {
        (self.min + self.max) * 0.5
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

/// Transformation matrix representing Translation, Rotation and Scale in 3D CAD space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform3D {
    pub matrix: DMat4,
}

impl Transform3D {
    pub const IDENTITY: Self = Self {
        matrix: DMat4::IDENTITY,
    };

    pub fn from_translation(offset: Vector3D) -> Self {
        Self {
            matrix: DMat4::from_translation(offset),
        }
    }

    pub fn from_rotation_z(angle_rad: f64) -> Self {
        Self {
            matrix: DMat4::from_rotation_z(angle_rad),
        }
    }

    pub fn transform_point(&self, p: Point3D) -> Point3D {
        self.matrix.transform_point3(p)
    }

    pub fn transform_vector(&self, v: Vector3D) -> Vector3D {
        self.matrix.transform_vector3(v)
    }
}

impl Default for Transform3D {
    fn default() -> Self {
        Self::IDENTITY
    }
}
