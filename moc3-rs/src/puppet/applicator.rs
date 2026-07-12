use core::slice;

use bytemuck::{cast_slice, cast_slice_mut};
use glam::Vec2;

use crate::{deformer::rotation_deformer::TransformData, math::rescale};

use super::{BlendColor, PuppetFrameData};

/// Locates `value` within a sorted key table. Returns the index of
/// the key below it, and how far it sits between that key and the next, in `0..=1`.
fn key_segment(keys: &[f32], value: f32) -> (usize, f32) {
    let Some(last) = keys.len().checked_sub(1).filter(|&l| l > 0) else {
        return (0, 0.0);
    };

    if value <= keys[0] {
        return (0, 0.0);
    }
    if value >= keys[last] {
        return (last - 1, 1.0);
    }

    // `value` lies strictly inside the table, so this lands in `1..=last`.
    let upper = keys.partition_point(|&k| k <= value).clamp(1, last);
    (upper - 1, rescale(value, keys[upper - 1], keys[upper]))
}

/// Whether `value` falls outside a key table's range.
pub fn key_table_is_outside(keys: &[f32], value: f32, snap_eps: f32) -> bool {
    match keys {
        [] => false,
        [key] => value <= key - snap_eps || value >= key + snap_eps,
        [first, .., last] => value < first - snap_eps || value >= last + snap_eps,
    }
}

#[derive(Debug, Clone)]
pub struct BlendShapeConstraints {
    pub parameter_index: usize,
    pub keys: Vec<f32>,
    pub weights: Vec<f32>,
}

impl BlendShapeConstraints {
    pub fn process(&self, parameters: &[f32]) -> f32 {
        let param = parameters[self.parameter_index];
        let (lower, scaled) = key_segment(&self.keys, param);
        let upper = (lower + 1).min(self.weights.len() - 1);

        ((1.0 - scaled) * self.weights[lower]) + (scaled * self.weights[upper])
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn clamps_outside_the_table() {
        let keys = [-1.0, 0.0, 2.0];

        assert_eq!(key_segment(&keys, -5.0), (0, 0.0));
        assert_eq!(key_segment(&keys, 9.0), (1, 1.0));

        assert_eq!(key_segment(&keys, -0.5), (0, 0.5));
        assert_eq!(key_segment(&keys, 1.0), (1, 0.5));

        assert_eq!(key_segment(&keys, -1.0), (0, 0.0));
        assert_eq!(key_segment(&keys, 2.0), (1, 1.0));
    }

    #[test]
    fn lone_key_segment() {
        assert_eq!(key_segment(&[3.0], 3.0), (0, 0.0));
        assert_eq!(key_segment(&[3.0], -100.0), (0, 0.0));
    }

    #[test]
    fn value_overflow_test() {
        let keys = [-1.0, 0.0, 2.0];
        let eps = 0.01;

        assert!(!key_table_is_outside(&keys, -1.0, eps));
        assert!(!key_table_is_outside(&keys, 1.0, eps));
        assert!(!key_table_is_outside(&keys, 2.0, eps));

        assert!(key_table_is_outside(&keys, -1.5, eps));
        assert!(key_table_is_outside(&keys, 2.5, eps));

        assert!(!key_table_is_outside(&keys, -1.005, eps));

        assert!(!key_table_is_outside(&[1.0], 1.0, eps));
        assert!(key_table_is_outside(&[1.0], 1.5, eps));

        assert!(!key_table_is_outside(&[], 100.0, eps));
    }
}

/// A [ParamApplicator] is a type that can handle the work required
/// to transform the puppet data given the input parameters.
#[derive(Debug, Clone)]
pub struct ParamApplicator {
    pub data: Vec<(Vec<f32>, usize)>,

    pub kind_index: u32,
    pub values: ApplicatorKind,
    pub blend: Option<Vec<BlendShapeConstraints>>,
}

#[derive(Debug, Clone)]
pub enum ApplicatorKind {
    // vertexes, opacities, draw orders, (multiply, screen)
    ArtMesh(Vec<Vec<Vec2>>, Vec<f32>, Vec<f32>, Vec<BlendColor>),
    // vertexes, opacities, (multiply, screen)
    WarpDeformer(Vec<Vec<Vec2>>, Vec<f32>, Vec<BlendColor>),
    // (origin, scale, angle), opacities, (multiply, screen)
    RotationDeformer(Vec<TransformData>, Vec<f32>, Vec<BlendColor>),
    // intensities
    Glue(Vec<f32>),
    // draw orders
    Part(Vec<f32>),
}

impl ParamApplicator {
    // This entire thing needs to be shredded and rewritten.
    fn do_interpolate<'a, F>(&'a self, parameters: &[f32], out: &mut [f32], get_choices: F)
    where
        F: Fn(usize) -> &'a [f32],
    {
        let data = &self.data;
        let mut rescaled_params = [f32::NAN; 31];
        assert!(data.len() <= 31);

        let mut base_index = 0;
        {
            let mut last_size = 1;
            for (i, (keys, index)) in data.iter().enumerate() {
                let (lower, scaled) = key_segment(keys, parameters[*index]);
                rescaled_params[i] = scaled;

                base_index += lower * last_size;
                last_size *= keys.len();
            }
        }

        for num in 0..(1 << data.len()) {
            let mut mult = 1.0;
            let mut index = base_index;

            let mut last_size = 1;
            for (i, (keys, _)) in data.iter().enumerate() {
                if num & (1 << i) != 0 {
                    index += last_size;
                    mult *= rescaled_params[i];
                } else {
                    mult *= 1.0 - rescaled_params[i];
                }
                last_size *= keys.len();
            }

            if mult == 0.0 {
                continue;
            }

            let data = get_choices(index);
            debug_assert_eq!(data.len(), out.len());
            for (o, d) in out.iter_mut().zip(data) {
                *o += d * mult;
            }
        }
    }

    pub fn apply(&self, frame_data: &mut PuppetFrameData) {
        let parameters = &frame_data.corrected_params;
        let ind = self.kind_index as usize;
        match &self.values {
            ApplicatorKind::ArtMesh(choices, opacities, draw_orders, colors) => {
                if let Some(constraints) = &self.blend {
                    let mut lowest_weight: f32 = 1.0;

                    for constraint in constraints {
                        lowest_weight = lowest_weight.min(constraint.process(parameters));
                    }

                    self.do_interpolate(
                        parameters,
                        bytemuck::cast_slice_mut(&mut frame_data.art_mesh_data[ind]),
                        |a| bytemuck::cast_slice(choices[a].as_slice()),
                    );
                } else {
                    frame_data.art_mesh_data[ind].fill(Vec2::ZERO);
                    self.do_interpolate(
                        parameters,
                        bytemuck::cast_slice_mut(&mut frame_data.art_mesh_data[ind]),
                        |a| bytemuck::cast_slice(choices[a].as_slice()),
                    );

                    frame_data.art_mesh_draw_orders[ind] = 0.0;
                    self.do_interpolate(
                        parameters,
                        slice::from_mut(&mut frame_data.art_mesh_draw_orders[ind]),
                        |a| slice::from_ref(&draw_orders[a]),
                    );

                    frame_data.art_mesh_opacities[ind] = 0.0;
                    self.do_interpolate(
                        parameters,
                        slice::from_mut(&mut frame_data.art_mesh_opacities[ind]),
                        |a| slice::from_ref(&opacities[a]),
                    );

                    if !colors.is_empty() {
                        frame_data.art_mesh_colors[ind] = BlendColor::ZERO;
                        self.do_interpolate(
                            parameters,
                            cast_slice_mut(slice::from_mut(&mut frame_data.art_mesh_colors[ind])),
                            |a| cast_slice(slice::from_ref(&colors[a])),
                        );
                    } else {
                        frame_data.art_mesh_colors[ind] = BlendColor::default();
                    }
                }
            }
            ApplicatorKind::WarpDeformer(choices, opacities, colors) => {
                if let Some(constraints) = &self.blend {
                    let mut lowest_weight: f32 = 1.0;

                    for constraint in constraints {
                        lowest_weight = lowest_weight.min(constraint.process(parameters));
                    }

                    self.do_interpolate(
                        parameters,
                        bytemuck::cast_slice_mut(&mut frame_data.warp_deformer_data[ind]),
                        |a| bytemuck::cast_slice(choices[a].as_slice()),
                    );
                } else {
                    frame_data.warp_deformer_data[ind].fill(Vec2::ZERO);
                    self.do_interpolate(
                        parameters,
                        bytemuck::cast_slice_mut(&mut frame_data.warp_deformer_data[ind]),
                        |a| bytemuck::cast_slice(choices[a].as_slice()),
                    );

                    frame_data.warp_deformer_opacities[ind] = 0.0;
                    self.do_interpolate(
                        parameters,
                        slice::from_mut(&mut frame_data.warp_deformer_opacities[ind]),
                        |a| slice::from_ref(&opacities[a]),
                    );

                    if !colors.is_empty() {
                        frame_data.warp_deformer_colors[ind] = BlendColor::ZERO;
                        self.do_interpolate(
                            parameters,
                            cast_slice_mut(slice::from_mut(
                                &mut frame_data.warp_deformer_colors[ind],
                            )),
                            |a| cast_slice(slice::from_ref(&colors[a])),
                        );
                    } else {
                        frame_data.warp_deformer_colors[ind] = BlendColor::default();
                    }
                }
            }
            ApplicatorKind::RotationDeformer(choices, opacities, colors) => {
                frame_data.rotation_deformer_data[ind] = TransformData::ZERO;
                self.do_interpolate(
                    parameters,
                    cast_slice_mut(slice::from_mut(&mut frame_data.rotation_deformer_data[ind])),
                    |a| cast_slice(slice::from_ref(&choices[a])),
                );

                frame_data.rotation_deformer_opacities[ind] = 0.0;
                self.do_interpolate(
                    parameters,
                    slice::from_mut(&mut frame_data.rotation_deformer_opacities[ind]),
                    |a| slice::from_ref(&opacities[a]),
                );

                if !colors.is_empty() {
                    frame_data.rotation_deformer_colors[ind] = BlendColor::ZERO;
                    self.do_interpolate(
                        parameters,
                        cast_slice_mut(slice::from_mut(
                            &mut frame_data.rotation_deformer_colors[ind],
                        )),
                        |a| cast_slice(slice::from_ref(&colors[a])),
                    );
                } else {
                    frame_data.rotation_deformer_colors[ind] = BlendColor::default();
                }
            }
            ApplicatorKind::Glue(intensities) => {
                frame_data.glue_data[ind] = 0.0;
                self.do_interpolate(
                    parameters,
                    slice::from_mut(&mut frame_data.glue_data[ind]),
                    |a| slice::from_ref(&intensities[a]),
                );
            }
            ApplicatorKind::Part(draw_orders) => {
                frame_data.part_draw_orders[ind] = 0.0;
                self.do_interpolate(
                    parameters,
                    slice::from_mut(&mut frame_data.part_draw_orders[ind]),
                    |a| slice::from_ref(&draw_orders[a]),
                );
            }
        }
    }
}
