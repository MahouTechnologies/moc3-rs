use glam::{vec2, Vec2};

// Live2D deformers are more complex than just a simple interpolation,
// but not by that much.
//
// Live2D deformers are split into three regions.
// /-----------\
// |     B     |
// |   1---1   |
// |   | A |   |
// |   0---1   |
// |     B     |
// \-----------/
// The A case is the normal case, the B case is the transition case,
// and the C case is the extreme extrapolation case.
//
// In the A case, points are bilinearly (or barycentricrly) interpolated
// between various grid points - the inputs [0, 1] are mapped to the values
// behind the closest grid point.
//
// In the C case, points are projected onto a parallelogram,
// which represents the behavior as x and y approach positive and negatiev
// infinity. The exact method of determining this is unknown, but some
// math analysis has suggested a method of deriving two vectors from
// diagonal vectors. Here, the points appear to ignore the bilinear
// interpolation flags in favor of the older barycentric.
//
// In the B case, points are simply bilinearly interpolated again
// this time between the grid points on the edge or corner, and the points
// that make a rectangle laying on the outer edge of the C area.

// Traditional bilinear interpolation
fn bilinear_interp(
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

// Barycentric triangular interpolation
fn triangular_interp(
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

/// Rescales `t` from `[lower, upper]` to `[0, 1]`
pub fn rescale(t: f32, lower: f32, upper: f32) -> f32 {
    (t - lower) / (upper - lower)
}

// the cases are as follows
// | 6 | 7 | 8 |
// | 3 | 4 | 5 |
// | 0 | 1 | 2 |
fn calc_case_index(point: Vec2) -> u32 {
    let x_ind = if point.x >= 1.0 {
        2
    } else if point.x >= 0.0 {
        1
    } else {
        0
    };

    let y_ind = if point.y >= 1.0 {
        2
    } else if point.y >= 0.0 {
        1
    } else {
        0
    };

    x_ind + y_ind * 3
}

// TODO: grid should be something with 2D indexing
pub fn apply_warp_deformer(
    grid: &[Vec2],
    is_new_deformer: bool,
    rows: usize,
    columns: usize,
    points_to_transform: &mut [Vec2],
) {
    // `columns` here is the number of columns in the deformer, which is defined
    // by `columns + 1` points
    //
    // | 1 | 2 | ... | columns - 1 | columns |
    let column_points = columns + 1;

    for point_ref in points_to_transform.iter_mut() {
        // rescales the point to be within ([0, columns], [0, rows]) for future indexing work.
        let point = *point_ref;
        let point_grid = point * vec2(columns as f32, rows as f32);
        let grid_x = point_grid.x as usize;
        let grid_y = point_grid.y as usize;

        // Whether the point is directly inside the deformer - the simple case.
        let is_normal = point.x >= 0.0 && point.x < 1.0 && point.y >= 0.0 && point.y < 1.0;
        if is_normal {
            // Trunced down, so this is the bottom-left corner of the grid.
            let grid_index = grid_x + grid_y * column_points;

            // It looks like the format started out with the barycenter interpolation,
            // and then later switched to regular bilinear.
            let res = if is_new_deformer {
                bilinear_interp(
                    point_grid.fract(),
                    grid[grid_index],
                    grid[grid_index + 1],
                    grid[grid_index + column_points],
                    grid[grid_index + column_points + 1],
                )
            } else {
                triangular_interp(
                    point_grid.fract(),
                    grid[grid_index],
                    grid[grid_index + 1],
                    grid[grid_index + column_points],
                    grid[grid_index + column_points + 1],
                )
            };

            *point_ref = res;
        } else {
            // Oh boy. This is fun. Basically the mesh turns into parallelograms at the exteremes,
            // and in the transition zone it gets interpolated between the original shape and the
            // extreme parallelogram.
            let centroid = (grid[0]
                + grid[columns]
                + grid[rows * column_points]
                + grid[columns + rows * column_points])
                / 4.0;

            // The following code approximates a parallelogram from an arbitrary quadrilateral.
            //
            // This was determined via educated guess, so I'm unsure if this is correct.
            // Research online states that only the 4 corners of the deformer affect this,
            // in particular, this appears to match Live2D behavior for when the top left
            // and top right corners are inverted.
            //
            // Calculate the diagonals of the quadrilateral
            let diagonal_one = grid[columns + rows * column_points] - grid[0];
            let diagonal_two = grid[columns] - grid[rows * column_points];

            // Calculate the approximate parallelogram (vectors) of the quadrilateral.
            let v_x: Vec2 = (diagonal_one + diagonal_two) / 2.0;
            let v_y = (diagonal_one - diagonal_two) / 2.0;

            // Move from the centroid to the new origin of the paralleogram
            let origin = centroid - diagonal_one * 0.5;

            let is_transition =
                point.x >= -2.0 && point.x <= 3.0 && point.y >= -2.0 && point.y <= 3.0;
            if is_transition {
                // These don't appear to change interpolation mode between old and new,
                // so I'm guessing that they remain the older barycentric interpolation.
                // Not sure why, but I guess this is a rarer case anyways.
                let res = match calc_case_index(point) {
                    // Let's handle the side cases first
                    7 => {
                        let adjusted_grid_x = grid_x.min(columns - 1);
                        let first_f = adjusted_grid_x as f32 / columns as f32;
                        let second_f = (adjusted_grid_x + 1) as f32 / columns as f32;

                        triangular_interp(
                            vec2(
                                point_grid.x - adjusted_grid_x as f32,
                                rescale(point.y, 1.0, 3.0),
                            ),
                            grid[adjusted_grid_x + rows * column_points],
                            grid[adjusted_grid_x + 1 + rows * column_points],
                            origin + (v_x * first_f) + (v_y * 3.0),
                            origin + (v_x * second_f) + (v_y * 3.0),
                        )
                    }
                    1 => {
                        let adjusted_grid_x = grid_x.min(columns - 1);
                        let first_f = adjusted_grid_x as f32 / columns as f32;
                        let second_f = (adjusted_grid_x + 1) as f32 / columns as f32;

                        triangular_interp(
                            vec2(
                                point_grid.x - adjusted_grid_x as f32,
                                rescale(point.y, -2.0, 0.0),
                            ),
                            origin + (v_x * first_f) + (v_y * -2.0),
                            origin + (v_x * second_f) + (v_y * -2.0),
                            grid[adjusted_grid_x],
                            grid[adjusted_grid_x + 1],
                        )
                    }
                    3 => {
                        let adjusted_grid_y = grid_y.min(rows - 1);
                        let first_f = adjusted_grid_y as f32 / rows as f32;
                        let second_f = (adjusted_grid_y + 1) as f32 / rows as f32;

                        triangular_interp(
                            vec2(
                                rescale(point.x, -2.0, 0.0),
                                point_grid.y - adjusted_grid_y as f32,
                            ),
                            origin + (v_x * -2.0) + (v_y * first_f),
                            grid[adjusted_grid_y * column_points],
                            origin + (v_x * -2.0) + (v_y * second_f),
                            grid[(adjusted_grid_y + 1) * column_points],
                        )
                    }
                    5 => {
                        let adjusted_grid_y = grid_y.min(rows - 1);
                        let first_f = adjusted_grid_y as f32 / rows as f32;
                        let second_f = (adjusted_grid_y + 1) as f32 / rows as f32;

                        triangular_interp(
                            vec2(
                                rescale(point.x, 1.0, 3.0),
                                point_grid.y - adjusted_grid_y as f32,
                            ),
                            grid[columns + adjusted_grid_y * column_points],
                            origin + (v_x * 3.0) + (v_y * first_f),
                            grid[columns + (adjusted_grid_y + 1) * column_points],
                            origin + (v_x * 3.0) + (v_y * second_f),
                        )
                    }

                    // Now let's do the corner cases
                    6 => triangular_interp(
                        vec2(rescale(point.x, -2.0, 0.0), rescale(point.y, 1.0, 3.0)),
                        origin + (v_x * -2.0) + (v_y * 1.0),
                        grid[rows * column_points],
                        origin + (v_x * -2.0) + (v_y * 3.0),
                        origin + (v_x * 0.0) + (v_y * 3.0),
                    ),
                    8 => triangular_interp(
                        vec2(rescale(point.x, 1.0, 3.0), rescale(point.y, 1.0, 3.0)),
                        grid[columns + rows * column_points],
                        origin + (v_x * 3.0) + (v_y * 1.0),
                        origin + (v_x * 1.0) + (v_y * 3.0),
                        origin + (v_x * 3.0) + (v_y * 3.0),
                    ),
                    0 => triangular_interp(
                        vec2(rescale(point.x, -2.0, 0.0), rescale(point.y, -2.0, 0.0)),
                        origin + (v_x * -2.0) + (v_y * -2.0),
                        origin + (v_x * 0.0) + (v_y * -2.0),
                        origin + (v_x * -2.0) + (v_y * 0.0),
                        grid[0],
                    ),
                    2 => triangular_interp(
                        vec2(rescale(point.x, 1.0, 3.0), rescale(point.y, -2.0, 0.0)),
                        origin + (v_x * 1.0) + (v_y * -2.0),
                        origin + (v_x * 3.0) + (v_y * -2.0),
                        grid[columns],
                        origin + (v_x * 3.0) + (v_y * 0.0),
                    ),

                    // 4 (and everything else) is unreachable
                    _ => unreachable!(),
                };

                *point_ref = res;
            } else {
                // Simple extrapolation case
                *point_ref = origin + Vec2::splat(point.x) * v_x + Vec2::splat(point.y) * v_y;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_index() {
        assert_eq!(calc_case_index(vec2(-2.0, 3.0)), 6);
        assert_eq!(calc_case_index(vec2(-1.0, 2.0)), 6);

        assert_eq!(calc_case_index(vec2(0.5, 3.0)), 7);
        assert_eq!(calc_case_index(vec2(0.5, 2.0)), 7);

        assert_eq!(calc_case_index(vec2(3.0, 3.0)), 8);
        assert_eq!(calc_case_index(vec2(2.0, 2.0)), 8);

        assert_eq!(calc_case_index(vec2(-2.0, 0.5)), 3);
        assert_eq!(calc_case_index(vec2(-1.0, 0.5)), 3);

        assert_eq!(calc_case_index(vec2(0.0, 0.0)), 4);
        assert_eq!(calc_case_index(vec2(0.5, 0.5)), 4);

        assert_eq!(calc_case_index(vec2(3.0, 0.5)), 5);
        assert_eq!(calc_case_index(vec2(2.0, 0.5)), 5);

        assert_eq!(calc_case_index(vec2(-2.0, -3.0)), 0);
        assert_eq!(calc_case_index(vec2(-1.0, -2.0)), 0);

        assert_eq!(calc_case_index(vec2(0.5, -3.0)), 1);
        assert_eq!(calc_case_index(vec2(0.5, -2.0)), 1);

        assert_eq!(calc_case_index(vec2(3.0, -3.0)), 2);
        assert_eq!(calc_case_index(vec2(2.0, -2.0)), 2);
    }
}
