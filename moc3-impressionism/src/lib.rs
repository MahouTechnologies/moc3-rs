use glam::Vec2;

pub mod pendulum;
pub mod physics;

pub use pendulum::{Pendulum, UpdateData};
pub use physics::PhysicsSystem;

/// Rotate `v` by `radians`, bug-for-bug compatible with Cubism.
pub(crate) fn cubism_rotate(v: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();

    let x = v.x * cos - v.y * sin;
    Vec2::new(x, x * sin + v.y * cos)
}
