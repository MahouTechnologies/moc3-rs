use core::slice;

use bytemuck::{cast_slice, cast_vec};
use glam::{vec2, vec3, Vec2, Vec3};

use crate::{
    deformer::{rotation_deformer::TransformData, warp_deformer::rescale},
    interpolate::{bilinear_interp, linear_interp, trilinear_interp},
};

use super::PuppetFrameData;

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
    pub x: Option<(Vec<f32>, usize)>,
    pub y: Option<(Vec<f32>, usize)>,
    pub z: Option<(Vec<f32>, usize)>,
    pub kind_index: u32,
    pub values: ApplicatorKind,
    pub blend: Option<Vec<BlendShapeConstraints>>,
}

#[derive(Debug, Clone)]
pub enum ApplicatorKind {
    // keyform vec vertexes
    ArtMesh(Vec<Vec<Vec2>>, Vec<f32>, Vec<f32>),
    // keyform vec of grid points
    WarpDeformer(Vec<Vec<Vec2>>, Vec<f32>),
    // keyform vec of origin, scale, angle
    RotationDeformer(Vec<TransformData>, Vec<f32>),
    // keyform vec of intensities
    Glue(Vec<f32>),
}

impl ParamApplicator {
    // This entire thing needs to be shredded and rewritten.
    fn do_interpolate(&self, parameters: &[f32], choices: &[Vec<Vec2>]) -> Vec<Vec2> {
        let data: Vec<Vec2> = cast_vec(if self.z.is_some() {
            let x_unwrapped = self.x.as_ref().unwrap();
            let y_unwrapped = self.y.as_ref().unwrap();
            let z_unwrapped = self.z.as_ref().unwrap();

            let x_val = parameters[x_unwrapped.1];
            let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);
            let y_val = parameters[y_unwrapped.1];
            let (y_lower_i, y_upper_i) = lower_upper_indices(&y_unwrapped.0, &y_val);
            let z_val = parameters[z_unwrapped.1];
            let (z_lower_i, z_upper_i) = lower_upper_indices(&z_unwrapped.0, &z_val);

            let temp = |x: usize, y: usize, z: usize| -> (Vec3, &[f32]) {
                let point = vec3(x_unwrapped.0[x], y_unwrapped.0[y], z_unwrapped.0[z]);
                let index = x + y * x_unwrapped.0.len() + z * y_unwrapped.0.len();

                (point, cast_slice(&choices[index]))
            };

            trilinear_interp(
                vec3(x_val, y_val, z_val),
                temp(x_lower_i, y_lower_i, z_lower_i),
                temp(x_upper_i, y_lower_i, z_lower_i),
                temp(x_lower_i, y_upper_i, z_lower_i),
                temp(x_upper_i, y_upper_i, z_lower_i),
                temp(x_lower_i, y_lower_i, z_upper_i),
                temp(x_upper_i, y_lower_i, z_upper_i),
                temp(x_lower_i, y_upper_i, z_upper_i),
                temp(x_upper_i, y_upper_i, z_upper_i),
            )
        } else if self.y.is_some() {
            let x_unwrapped = self.x.as_ref().unwrap();
            let y_unwrapped = self.y.as_ref().unwrap();
            let x_val = parameters[x_unwrapped.1];
            let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);
            let y_val = parameters[y_unwrapped.1];
            let (y_lower_i, y_upper_i) = lower_upper_indices(&y_unwrapped.0, &y_val);

            let temp = |x: usize, y: usize| -> (Vec2, &[f32]) {
                let point = vec2(x_unwrapped.0[x], y_unwrapped.0[y]);
                let index = x + y * x_unwrapped.0.len();

                (point, cast_slice(&choices[index]))
            };

            bilinear_interp(
                vec2(x_val, y_val),
                temp(x_lower_i, y_lower_i),
                temp(x_upper_i, y_lower_i),
                temp(x_lower_i, y_upper_i),
                temp(x_upper_i, y_upper_i),
            )
        } else if self.x.is_some() {
            let x_unwrapped = self.x.as_ref().unwrap();
            let x_val = parameters[x_unwrapped.1];
            let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);

            let temp = |x: usize| -> (f32, &[f32]) {
                let point = x_unwrapped.0[x];
                let index = x;

                (point, cast_slice(&choices[index]))
            };

            linear_interp(x_val, temp(x_lower_i), temp(x_upper_i))
        } else {
            cast_slice(&choices[0]).to_vec()
        });

        data
    }

    fn do_interpolate_single(&self, parameters: &[f32], choices: &[f32]) -> f32 {
        let data = if self.z.is_some() {
            let x_unwrapped = self.x.as_ref().unwrap();
            let y_unwrapped = self.y.as_ref().unwrap();
            let z_unwrapped = self.z.as_ref().unwrap();

            let x_val = parameters[x_unwrapped.1];
            let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);
            let y_val = parameters[y_unwrapped.1];
            let (y_lower_i, y_upper_i) = lower_upper_indices(&y_unwrapped.0, &y_val);
            let z_val = parameters[z_unwrapped.1];
            let (z_lower_i, z_upper_i) = lower_upper_indices(&z_unwrapped.0, &z_val);

            let temp = |x: usize, y: usize, z: usize| -> (Vec3, &[f32]) {
                let point = vec3(x_unwrapped.0[x], y_unwrapped.0[y], z_unwrapped.0[z]);
                let index = x + y * x_unwrapped.0.len() + z * y_unwrapped.0.len();

                (point, slice::from_ref(&choices[index]))
            };

            trilinear_interp(
                vec3(x_val, y_val, z_val),
                temp(x_lower_i, y_lower_i, z_lower_i),
                temp(x_upper_i, y_lower_i, z_lower_i),
                temp(x_lower_i, y_upper_i, z_lower_i),
                temp(x_upper_i, y_upper_i, z_lower_i),
                temp(x_lower_i, y_lower_i, z_upper_i),
                temp(x_upper_i, y_lower_i, z_upper_i),
                temp(x_lower_i, y_upper_i, z_upper_i),
                temp(x_upper_i, y_upper_i, z_upper_i),
            )[0]
        } else if self.y.is_some() {
            let x_unwrapped = self.x.as_ref().unwrap();
            let y_unwrapped = self.y.as_ref().unwrap();
            let x_val = parameters[x_unwrapped.1];
            let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);
            let y_val = parameters[y_unwrapped.1];
            let (y_lower_i, y_upper_i) = lower_upper_indices(&y_unwrapped.0, &y_val);

            let temp = |x: usize, y: usize| -> (Vec2, &[f32]) {
                let point = vec2(x_unwrapped.0[x], y_unwrapped.0[y]);
                let index = x + y * x_unwrapped.0.len();

                (point, slice::from_ref(&choices[index]))
            };

            bilinear_interp(
                vec2(x_val, y_val),
                temp(x_lower_i, y_lower_i),
                temp(x_upper_i, y_lower_i),
                temp(x_lower_i, y_upper_i),
                temp(x_upper_i, y_upper_i),
            )[0]
        } else if self.x.is_some() {
            let x_unwrapped = self.x.as_ref().unwrap();
            let x_val = parameters[x_unwrapped.1];
            let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);

            let temp = |x: usize| -> (f32, &[f32]) {
                let point = x_unwrapped.0[x];
                let index = x;

                (point, slice::from_ref(&choices[index]))
            };

            linear_interp(x_val, temp(x_lower_i), temp(x_upper_i))[0]
        } else {
            choices[0]
        };

        data
    }

    pub fn apply(&self, parameters: &[f32], frame_data: &mut PuppetFrameData) {
        let ind = self.kind_index as usize;
        match &self.values {
            ApplicatorKind::ArtMesh(choices, opacities, draw_orders) => {
                let data = self.do_interpolate(parameters, choices);

                if let Some(constraints) = &self.blend {
                    let mut lowest_weight: f32 = 1.0;

                    for constraint in constraints {
                        lowest_weight = lowest_weight.min(dbg!(constraint.process(parameters)));
                    }

                    for (change, diff) in frame_data.art_mesh_data[ind].iter_mut().zip(data) {
                        *change += diff * lowest_weight;
                    }
                } else {
                    frame_data.art_mesh_data[ind] = data;
                    frame_data.art_mesh_draw_orders[ind] =
                        self.do_interpolate_single(parameters, draw_orders);
                    frame_data.art_mesh_opacities[ind] =
                        self.do_interpolate_single(parameters, opacities);
                }
            }
            ApplicatorKind::WarpDeformer(choices, opacities) => {
                let data = self.do_interpolate(parameters, choices);

                frame_data.warp_deformer_opacities[ind] =
                    self.do_interpolate_single(parameters, opacities);
                frame_data.warp_deformer_data[ind] = data;
            }
            ApplicatorKind::RotationDeformer(choices, opacities) => {
                frame_data.rotation_deformer_opacities[ind] =
                    self.do_interpolate_single(parameters, opacities);
                let res = if self.z.is_some() {
                    let x_unwrapped = self.x.as_ref().unwrap();
                    let y_unwrapped = self.y.as_ref().unwrap();
                    let z_unwrapped = self.z.as_ref().unwrap();

                    let x_val = parameters[x_unwrapped.1];
                    let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);
                    let y_val = parameters[y_unwrapped.1];
                    let (y_lower_i, y_upper_i) = lower_upper_indices(&y_unwrapped.0, &y_val);
                    let z_val = parameters[z_unwrapped.1];
                    let (z_lower_i, z_upper_i) = lower_upper_indices(&z_unwrapped.0, &z_val);

                    let temp = |x: usize, y: usize, z: usize| -> (Vec3, &[f32]) {
                        let point = vec3(x_unwrapped.0[x], y_unwrapped.0[y], z_unwrapped.0[z]);
                        let index = x + y * x_unwrapped.0.len() + z * y_unwrapped.0.len();

                        (point, cast_slice(slice::from_ref(&choices[index])))
                    };

                    trilinear_interp(
                        vec3(x_val, y_val, z_val),
                        temp(x_lower_i, y_lower_i, z_lower_i),
                        temp(x_upper_i, y_lower_i, z_lower_i),
                        temp(x_lower_i, y_upper_i, z_lower_i),
                        temp(x_upper_i, y_upper_i, z_lower_i),
                        temp(x_lower_i, y_lower_i, z_upper_i),
                        temp(x_upper_i, y_lower_i, z_upper_i),
                        temp(x_lower_i, y_upper_i, z_upper_i),
                        temp(x_upper_i, y_upper_i, z_upper_i),
                    )
                } else if self.y.is_some() {
                    let x_unwrapped = self.x.as_ref().unwrap();
                    let y_unwrapped = self.y.as_ref().unwrap();
                    let x_val = parameters[x_unwrapped.1];
                    let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);
                    let y_val = parameters[y_unwrapped.1];
                    let (y_lower_i, y_upper_i) = lower_upper_indices(&y_unwrapped.0, &y_val);

                    let temp = |x: usize, y: usize| -> (Vec2, &[f32]) {
                        let point = vec2(x_unwrapped.0[x], y_unwrapped.0[y]);
                        let index = x + y * x_unwrapped.0.len();

                        (point, cast_slice(slice::from_ref(&choices[index])))
                    };

                    bilinear_interp(
                        vec2(x_val, y_val),
                        temp(x_lower_i, y_lower_i),
                        temp(x_upper_i, y_lower_i),
                        temp(x_lower_i, y_upper_i),
                        temp(x_upper_i, y_upper_i),
                    )
                } else if self.x.is_some() {
                    let x_unwrapped = self.x.as_ref().unwrap();
                    let x_val = parameters[x_unwrapped.1];
                    let (x_lower_i, x_upper_i) = lower_upper_indices(&x_unwrapped.0, &x_val);

                    let temp = |x: usize| -> (f32, &[f32]) {
                        let point = x_unwrapped.0[x];
                        let index = x;

                        (point, cast_slice(slice::from_ref(&choices[index])))
                    };

                    linear_interp(x_val, temp(x_lower_i), temp(x_upper_i))
                } else {
                    cast_slice(slice::from_ref(&choices[0])).to_vec()
                };

                frame_data.rotation_deformer_data[ind] = TransformData {
                    origin: vec2(res[0], res[1]),
                    scale: res[2],
                    angle: res[3],
                };
            }
            ApplicatorKind::Glue(intensities) => {
                frame_data.glue_data[ind] = intensities[intensities.len() / 2];
            }
        }
    }
}
