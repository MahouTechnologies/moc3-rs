use core::slice;

use bytemuck::{cast_slice, cast_slice_mut};
use glam::Vec2;

use crate::{deformer::rotation_deformer::TransformData, math::rescale};

use super::{BlendColor, PuppetFrameData};

// Returns the index of the element directly less than and the index of the element directly
// greater than the given element.
// Note this the values given are not *strictly* greater or less - if the given element
// is present in the slice, the index of the given element will be returned, but it is
// unspecified whether it will be the greater or lesser value.
//
// This function assumes the slice is sorted, and will give meaningless results otherwise.
// This function also assumes that the given element exists in the bounds of the slice.
fn lower_upper_indices(slice: &[f32], elem: &f32) -> (usize, usize) {
    debug_assert!(slice.len() > 1);

    let value = slice.binary_search_by(|x| x.total_cmp(elem));
    match value {
        Ok(index) => {
            if index == 0 {
                // Element was first value, we can only return second
                (0, 1)
            } else if index == slice.len() - 1 {
                // Element was last value, we can only return second-to-last
                (slice.len() - 2, slice.len() - 1)
            } else {
                // We can chose either side here - this is arbitrary
                (index, index + 1)
            }
        }
        Err(index) => {
            // We assume that an invalid value is in between the first and last element, so
            // this subtraction will work fine.
            (index - 1, index)
        }
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
        let (lower, upper) = lower_upper_indices(&self.keys, &param);
        let scaled = rescale(param, self.keys[lower], self.keys[upper]);

        let res = ((1.0 - scaled) * self.weights[lower]) + (scaled * self.weights[upper]);
        res
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
                let (lower, upper) = lower_upper_indices(keys, &parameters[*index]);
                rescaled_params[i] = rescale(parameters[*index], keys[lower], keys[upper]);

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

            let data = get_choices(index);
            debug_assert_eq!(data.len(), out.len());
            for (o, d) in out.iter_mut().zip(data) {
                *o += d * mult;
            }
        }
    }

    pub fn apply(&self, parameters: &[f32], frame_data: &mut PuppetFrameData) {
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
                        cast_slice_mut(slice::from_mut(&mut frame_data.warp_deformer_colors[ind])),
                        |a| cast_slice(slice::from_ref(&colors[a])),
                    );
                } else {
                    frame_data.warp_deformer_colors[ind] = BlendColor::default();
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
        }
    }
}
