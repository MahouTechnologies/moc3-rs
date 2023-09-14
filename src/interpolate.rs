use glam::{Vec2, Vec3, Vec3Swizzles};

// All the functions here assume this coordinate system.
//        +Z
//         |
//         |
//         |
//         |______ +X
//        /
//       /
//     -Y

// TODO: rewrite these to allocate less
// TODO: these are just abominations
// TODO: these are just useless, replace them with n-ary lerp

fn lerp(t: f32, input: Vec2, output: Vec2) -> f32 {
    (t - input.x) * (output.y - output.x) / (input.y - input.x) + output.x
}

// 0---------1
pub fn linear_interp(t: f32, left: (f32, &[f32]), right: (f32, &[f32])) -> Vec<f32> {
    let mut ret = Vec::new();

    for (left_val, right_val) in left.1.iter().zip(right.1.iter()) {
        ret.push(lerp(
            t,
            Vec2::new(left.0, right.0),
            Vec2::new(*left_val, *right_val),
        ));
    }
    ret
}

// 2---------3
// |         |
// |         |
// |         |
// 0---------1
pub fn bilinear_interp(
    t: Vec2,
    bottom_left: (Vec2, &[f32]),
    bottom_right: (Vec2, &[f32]),
    top_left: (Vec2, &[f32]),
    top_right: (Vec2, &[f32]),
) -> Vec<f32> {
    let bottom_interpolated = linear_interp(
        t.x,
        (bottom_left.0.x, bottom_left.1),
        (bottom_right.0.x, bottom_right.1),
    );

    let top_interpolated = linear_interp(
        t.x,
        (top_left.0.x, top_left.1),
        (top_right.0.x, top_right.1),
    );

    linear_interp(
        t.y,
        (bottom_left.0.y, &bottom_interpolated),
        (top_left.0.y, &top_interpolated),
    )
}

//    6---------7
//   /|        /|
//  / |       / |
// 4---------5  |
// |  2------|--3
// | /       | /
// |/        |/
// 0---------1
pub fn trilinear_interp(
    t: Vec3,
    bottom_front_left: (Vec3, &[f32]),
    bottom_front_right: (Vec3, &[f32]),
    bottom_back_left: (Vec3, &[f32]),
    bottom_back_right: (Vec3, &[f32]),
    top_front_left: (Vec3, &[f32]),
    top_front_right: (Vec3, &[f32]),
    top_back_left: (Vec3, &[f32]),
    top_back_right: (Vec3, &[f32]),
) -> Vec<f32> {
    let bottom_interpolated = bilinear_interp(
        Vec2::new(t.x, t.y),
        (bottom_front_left.0.xy(), bottom_front_left.1),
        (bottom_front_right.0.xy(), bottom_front_right.1),
        (bottom_back_left.0.xy(), bottom_back_left.1),
        (bottom_back_right.0.xy(), bottom_back_right.1),
    );

    let top_interpolated = bilinear_interp(
        Vec2::new(t.x, t.y),
        (top_front_left.0.xy(), top_front_left.1),
        (top_front_right.0.xy(), top_front_right.1),
        (top_back_left.0.xy(), top_back_left.1),
        (top_back_right.0.xy(), top_back_right.1),
    );

    linear_interp(
        t.z,
        (bottom_front_left.0.z, &bottom_interpolated),
        (top_front_left.0.z, &top_interpolated),
    )
}
