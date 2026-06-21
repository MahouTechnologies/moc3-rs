use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Pan, in model units. The origin is the center of the puppet's space.
    pub position: Vec2,
    /// Rotation around the view center, in radians.
    pub rotation: f32,
    /// Zoom factor. Larger values zoom in.
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            rotation: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    /// Builds the view-projection matrix for the given viewport size (in
    /// pixels), ready to be handed to the renderer.
    pub fn matrix(&self, viewport: Vec2) -> Mat4 {
        let aspect = viewport.x / viewport.y;
        let (half_w, half_h) = if aspect >= 1.0 {
            (aspect, 1.0)
        } else {
            (1.0, 1.0 / aspect)
        };

        // Change the coordinate conversions to fit WGPU
        let projection = Mat4::orthographic_rh(-half_w, half_w, half_h, -half_h, -1.0, 1.0);

        // Compose the user transforms
        let view = Mat4::from_scale(Vec3::splat(self.zoom))
            * Mat4::from_rotation_z(self.rotation)
            * Mat4::from_translation((-self.position).extend(0.0));

        projection * view
    }
}
