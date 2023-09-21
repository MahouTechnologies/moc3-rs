pub mod lerp;

/// Rescales `t` from `[lower, upper]` to `[0, 1]`
pub fn rescale(t: f32, lower: f32, upper: f32) -> f32 {
    (t - lower) / (upper - lower)
}
