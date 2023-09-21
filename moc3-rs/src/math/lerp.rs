use glam::Vec2;

/// Traditional bilinear interpolation
pub fn bilinear_interp(
    t: Vec2,
    bottom_left: Vec2,
    bottom_right: Vec2,
    top_left: Vec2,
    top_right: Vec2,
) -> Vec2 {
    let neg = Vec2::ONE - t;

    bottom_left * neg.x * neg.y
        + bottom_right * t.x * neg.y
        + top_left * neg.x * t.y
        + top_right * t.x * t.y
}

/// Barycentric triangular interpolation
pub fn triangular_interp(
    t: Vec2,
    bottom_left: Vec2,
    bottom_right: Vec2,
    top_left: Vec2,
    top_right: Vec2,
) -> Vec2 {
    let neg = Vec2::ONE - t;

    if t.x + t.y > 1.0 {
        top_right + (top_left - top_right) * neg.x + (bottom_right - top_right) * neg.y
    } else {
        bottom_left + (bottom_right - bottom_left) * t.x + (top_left - bottom_left) * t.y
    }
}
