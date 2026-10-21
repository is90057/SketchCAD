use sketchcad_core::{DMat4, Point2D, Point3D, Vector3D};

/// 2D CAD Drafting Viewport Camera (Pan & Zoom)
#[derive(Debug, Clone)]
pub struct Camera2D {
    pub pan_offset: Point2D,
    pub zoom: f64, // scale factor, pixels per mm
}

impl Camera2D {
    pub fn new() -> Self {
        Self {
            pan_offset: Point2D::ZERO,
            zoom: 1.0,
        }
    }

    pub fn world_to_screen(&self, world_pt: Point2D, viewport_size: Point2D) -> Point2D {
        let center = viewport_size * 0.5;
        center + (world_pt - self.pan_offset) * self.zoom
    }

    pub fn screen_to_world(&self, screen_pt: Point2D, viewport_size: Point2D) -> Point2D {
        let center = viewport_size * 0.5;
        (screen_pt - center) / self.zoom + self.pan_offset
    }
}

impl Default for Camera2D {
    fn default() -> Self {
        Self::new()
    }
}

/// 3D Spatial Orbit Camera
#[derive(Debug, Clone)]
pub struct OrbitCamera3D {
    pub target: Point3D,
    pub distance: f64,
    pub yaw_rad: f64,
    pub pitch_rad: f64,
    pub fov_y_rad: f64,
}

impl OrbitCamera3D {
    pub fn new() -> Self {
        Self {
            target: Point3D::ZERO,
            distance: 8000.0, // 8 meters default viewing distance
            yaw_rad: std::f64::consts::FRAC_PI_4,
            pitch_rad: std::f64::consts::FRAC_PI_6,
            fov_y_rad: 45.0f64.to_radians(),
        }
    }

    pub fn eye_position(&self) -> Point3D {
        let cos_pitch = self.pitch_rad.cos();
        let sin_pitch = self.pitch_rad.sin();
        let cos_yaw = self.yaw_rad.cos();
        let sin_yaw = self.yaw_rad.sin();

        let offset = Vector3D::new(
            self.distance * cos_pitch * sin_yaw,
            -self.distance * cos_pitch * cos_yaw,
            self.distance * sin_pitch,
        );

        self.target + offset
    }

    pub fn view_matrix(&self) -> DMat4 {
        let eye = self.eye_position();
        let up = Vector3D::new(0.0, 0.0, 1.0);
        DMat4::look_at_rh(eye, self.target, up)
    }

    pub fn projection_matrix(&self, aspect_ratio: f64) -> DMat4 {
        DMat4::perspective_rh(self.fov_y_rad, aspect_ratio, 10.0, 500_000.0)
    }
}

impl Default for OrbitCamera3D {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orbit_camera_projection() {
        let cam = OrbitCamera3D {
            target: Point3D::new(4000.0, 3000.0, 1500.0),
            distance: 14000.0,
            yaw_rad: 0.75,
            pitch_rad: 0.55,
            fov_y_rad: 50.0f64.to_radians(),
        };
        let view = cam.view_matrix();
        let proj = cam.projection_matrix(1.5);
        let vp = proj * view;
        for pt in [
            cam.target,
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(8000.0, 0.0, 0.0),
            Point3D::new(8000.0, 6000.0, 3500.0),
        ] {
            let p4 = vp * pt.extend(1.0);
            println!("Point: {:?}, p4: {:?}, w: {}, ndc: {:?}", pt, p4, p4.w, p4.truncate() / p4.w);
        }
    }
}
