use glam::Vec2;
use moc3_rs::puppet::ParamData;

use crate::UpdateData;
use crate::data::{Physics3Data, PhysicsType};
use crate::pendulum::{Pendulum, PendulumPoint};

const MAX_WEIGHT: f32 = 100.0;
const MOVEMENT_EPISLON: f32 = 0.00001;

#[derive(Clone, Copy, Debug)]
struct NormalizationRange {
    minimum: f32,
    maximum: f32,
    default: f32,
}

#[derive(Clone, Debug)]
struct PhysicsInput {
    param_index: Option<usize>,
    weight: f32,
    reflect: bool,
    ty: PhysicsType,
}

#[derive(Clone, Debug)]
struct PhysicsOutput {
    param_index: Option<usize>,
    vertex_index: usize,
    scale: f32,
    weight: f32,
    reflect: bool,
    ty: PhysicsType,
}

struct PhysicsInstance {
    norm_position: NormalizationRange,
    norm_angle: NormalizationRange,
    inputs: Vec<PhysicsInput>,
    outputs: Vec<PhysicsOutput>,
    pendulum: Pendulum,
}

/// Physics simulation system for moc3 models.
pub struct PhysicsSystem {
    /// Runtime gravity direction.
    gravity: Vec2,
    /// Runtime wind force.
    wind: Vec2,
    fps: f32,

    instances: Vec<PhysicsInstance>,

    param_mins: Vec<f32>,
    param_maxes: Vec<f32>,

    /// Accumulated time not yet consumed by physics steps.
    remaining_accumulated_seconds: f32,

    /// Output values produced by the most recent physics step.
    outputs: Vec<Vec<f32>>,
    previous_outputs: Vec<Vec<f32>>,

    /// Working buffer for parameter values interpolated between fixed steps.
    parameters: Vec<f32>,
    previous_parameters: Vec<f32>,
}

impl PhysicsSystem {
    /// Build a physics system from parsed `physics3.json` data and the model's
    /// parameter metadata.
    pub fn from_data(data: &Physics3Data, param_data: &ParamData) -> Self {
        // Technically N^2 but only once so hopefully this won't be bad.
        let find_param =
            |id: &str| -> Option<usize> { param_data.ids.iter().position(|pid| pid == id) };

        let instances: Vec<PhysicsInstance> = data
            .physics_settings
            .iter()
            .map(|setting| {
                let norm = setting.normalization.unwrap_or_default();

                let inputs = setting
                    .input
                    .iter()
                    .map(|inp| PhysicsInput {
                        param_index: find_param(&inp.source.id),
                        weight: inp.weight,
                        reflect: inp.reflect,
                        ty: inp.ty,
                    })
                    .collect();

                let outputs = setting
                    .output
                    .iter()
                    .map(|out| PhysicsOutput {
                        param_index: find_param(&out.destination.id),
                        vertex_index: out.vertex_index,
                        scale: out.scale,
                        weight: out.weight,
                        reflect: out.reflect,
                        ty: out.ty,
                    })
                    .collect();

                PhysicsInstance {
                    norm_position: NormalizationRange {
                        minimum: norm.position.minimum,
                        maximum: norm.position.maximum,
                        default: norm.position.default,
                    },
                    norm_angle: NormalizationRange {
                        minimum: norm.angle.minimum,
                        maximum: norm.angle.maximum,
                        default: norm.angle.default,
                    },
                    inputs,
                    outputs,
                    pendulum: Pendulum::new(setting.vertices.iter().copied()),
                }
            })
            .collect();

        let outputs: Vec<Vec<f32>> = instances
            .iter()
            .map(|r| vec![0.0; r.outputs.len()])
            .collect();
        let previous_outputs: Vec<Vec<f32>> = outputs.clone();

        let mut system = PhysicsSystem {
            // You may be tempted to use the gravity value in the JSON file.
            // It does not seem like it does anything even when set to absurd values.
            // Just keep it as default.
            gravity: Vec2::new(0.0, -1.0),
            wind: Vec2::ZERO,
            fps: data.meta.fps as f32,
            instances,
            param_mins: param_data.mins.clone(),
            param_maxes: param_data.maxes.clone(),
            remaining_accumulated_seconds: 0.0,
            outputs,
            previous_outputs,
            parameters: vec![0.0; param_data.count as usize],
            previous_parameters: vec![f32::NAN; param_data.count as usize],
        };

        system.reset();
        system
    }

    pub fn set_wind(&mut self, wind: Vec2) {
        self.wind = wind;
    }

    /// Iterator over every model parameter index that this system writes physics
    /// outputs to. A parameter targeted by multiple outputs may appear
    /// multiple times.
    pub fn output_param_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.instances
            .iter()
            .flat_map(|instance| instance.outputs.iter())
            .filter_map(|out| out.param_index)
    }

    /// Reset all particle states.
    pub fn reset(&mut self) {
        for instance in &mut self.instances {
            instance.pendulum.reset();
        }
    }

    fn calculate_inputs(&mut self, params: &[f32], si: usize) -> (Vec2, f32) {
        let (total_translation, total_angle) = {
            let instance = &self.instances[si];
            let mut tt = Vec2::ZERO;
            let mut ta = 0.0f32;

            for input in &instance.inputs {
                let Some(idx) = input.param_index else {
                    continue;
                };
                self.parameters[idx] = params[idx];
                let normalized = normalize_parameter_value(
                    params[idx],
                    self.param_mins[idx],
                    self.param_maxes[idx],
                    match input.ty {
                        PhysicsType::X | PhysicsType::Y => instance.norm_position,
                        PhysicsType::Angle => instance.norm_angle,
                    },
                    input.reflect,
                );
                let contribution = normalized * (input.weight / MAX_WEIGHT);
                match input.ty {
                    PhysicsType::X => tt.x += contribution,
                    PhysicsType::Y => tt.y += contribution,
                    PhysicsType::Angle => ta += contribution,
                }
            }
            (tt, ta.to_radians())
        };

        (
            total_translation.rotate(Vec2::from_angle(-total_angle)),
            total_angle,
        )
    }

    /// Find fixpoint of physics system with the given params.
    pub fn fixpoint(&mut self, params: &mut [f32]) {
        assert_eq!(params.len(), self.parameters.len());
        assert_eq!(params.len(), self.previous_parameters.len());
        self.parameters.copy_from_slice(params);
        self.previous_parameters.copy_from_slice(params);

        for si in 0..self.instances.len() {
            let (total_translation, total_angle) = self.calculate_inputs(params, si);
            let threshold = MOVEMENT_EPISLON * self.instances[si].norm_position.maximum;
            self.instances[si].pendulum.fixpoint(
                UpdateData {
                    translation: total_translation,
                    rotation: total_angle,
                },
                self.wind,
                threshold,
            );

            let output_count = self.instances[si].outputs.len();
            for oi in 0..output_count {
                let pendulum_points = self.instances[si].pendulum.points();
                let vertex_index = self.instances[si].outputs[oi].vertex_index;
                if vertex_index < 1 || vertex_index >= pendulum_points.len() {
                    continue;
                }

                let translation = pendulum_points[vertex_index].position()
                    - pendulum_points[vertex_index - 1].position();

                let raw_value = compute_output(
                    self.instances[si].outputs[oi].ty,
                    translation,
                    self.instances[si].pendulum.points(),
                    vertex_index,
                    self.instances[si].outputs[oi].reflect,
                    self.gravity,
                );

                self.outputs[si][oi] = raw_value;
                self.previous_outputs[si][oi] = raw_value;

                let Some(dest_idx) = self.instances[si].outputs[oi].param_index else {
                    continue;
                };

                let scale = self.instances[si].outputs[oi].scale;
                let weight = self.instances[si].outputs[oi].weight;
                let param_min = self.param_mins[dest_idx];
                let param_max = self.param_maxes[dest_idx];

                update_output_parameter(
                    &mut params[dest_idx],
                    raw_value,
                    param_min,
                    param_max,
                    scale,
                    weight,
                );

                self.parameters[dest_idx] = params[dest_idx];
            }
        }
    }

    /// Advance the physics simulation by `delta_seconds`.
    pub fn step(&mut self, params: &mut [f32], delta_seconds: f32) {
        assert_eq!(params.len(), self.parameters.len());
        assert_eq!(params.len(), self.previous_parameters.len());

        if delta_seconds <= 0.0 {
            return;
        }

        self.remaining_accumulated_seconds += delta_seconds;

        // Limit the physics to a reasonable amount of time.
        const MAX_DELTA_TIME: f32 = 3.0;
        if self.remaining_accumulated_seconds > MAX_DELTA_TIME {
            self.remaining_accumulated_seconds = MAX_DELTA_TIME;
        }

        let physics_delta = if self.fps > 0.0 {
            1.0 / self.fps
        } else {
            // effective fps
            delta_seconds
        };

        let param_count = params.len();

        while self.remaining_accumulated_seconds >= physics_delta {
            for (curr, prev) in self.outputs.iter().zip(self.previous_outputs.iter_mut()) {
                prev.copy_from_slice(curr);
            }

            // Lerp the inputs one physics step closer to the current frame.
            let input_weight = physics_delta / self.remaining_accumulated_seconds;
            for j in 0..param_count {
                let last = self.previous_parameters[j];
                self.parameters[j] = if last.is_nan() {
                    params[j]
                } else {
                    last * (1.0 - input_weight) + params[j] * input_weight
                };
                self.previous_parameters[j] = self.parameters[j];
            }

            // Run physics for each instance.
            for si in 0..self.instances.len() {
                let (total_translation, total_angle) = self.calculate_inputs(params, si);

                let threshold = MOVEMENT_EPISLON * self.instances[si].norm_position.maximum;
                self.instances[si].pendulum.step(
                    physics_delta,
                    UpdateData {
                        translation: total_translation,
                        rotation: total_angle,
                    },
                    self.wind,
                    threshold,
                );

                let output_count = self.instances[si].outputs.len();
                for oi in 0..output_count {
                    let pendulum_points = self.instances[si].pendulum.points();
                    let vertex_index = self.instances[si].outputs[oi].vertex_index;
                    if vertex_index < 1 || vertex_index >= pendulum_points.len() {
                        continue;
                    }

                    let translation = pendulum_points[vertex_index].position()
                        - pendulum_points[vertex_index - 1].position();

                    let raw_value = compute_output(
                        self.instances[si].outputs[oi].ty,
                        translation,
                        self.instances[si].pendulum.points(),
                        vertex_index,
                        self.instances[si].outputs[oi].reflect,
                        self.gravity,
                    );

                    self.outputs[si][oi] = raw_value;

                    let Some(dest_idx) = self.instances[si].outputs[oi].param_index else {
                        continue;
                    };

                    let scale = self.instances[si].outputs[oi].scale;
                    let weight = self.instances[si].outputs[oi].weight;
                    let param_min = self.param_mins[dest_idx];
                    let param_max = self.param_maxes[dest_idx];

                    update_output_parameter(
                        &mut self.parameters[dest_idx],
                        raw_value,
                        param_min,
                        param_max,
                        scale,
                        weight,
                    );
                }
            }

            self.remaining_accumulated_seconds -= physics_delta;
        }

        // Blend previous and current outputs by the fractional remainder and
        // write the result into the model's actual parameter array.
        let alpha = self.remaining_accumulated_seconds / physics_delta;
        for si in 0..self.instances.len() {
            for oi in 0..self.instances[si].outputs.len() {
                let Some(dest_idx) = self.instances[si].outputs[oi].param_index else {
                    continue;
                };

                let prev = self.previous_outputs[si][oi];
                let curr = self.outputs[si][oi];
                let blended = prev * (1.0 - alpha) + curr * alpha;

                let scale = self.instances[si].outputs[oi].scale;
                let weight = self.instances[si].outputs[oi].weight;
                let param_min = self.param_mins[dest_idx];
                let param_max = self.param_maxes[dest_idx];

                update_output_parameter(
                    &mut params[dest_idx],
                    blended,
                    param_min,
                    param_max,
                    scale,
                    weight,
                );
            }
        }
    }
}

/// Map a raw parameter value to a normalised value.
fn normalize_parameter_value(
    value: f32,
    param_min: f32,
    param_max: f32,
    norm: NormalizationRange,
    reflect: bool,
) -> f32 {
    let value = value.clamp(param_min.min(param_max), param_min.max(param_max));
    let norm_min = norm.minimum.min(norm.maximum);
    let norm_max = norm.minimum.max(norm.maximum);
    let norm_default = norm.default;

    let middle_param = f32::midpoint(param_min, param_max);
    let offset = value - middle_param;

    let result = if offset > 0.0 {
        let denom = param_min.max(param_max) - middle_param;
        if denom == 0.0 {
            norm_default
        } else {
            offset * (norm_max - norm_default) / denom + norm_default
        }
    } else if offset < 0.0 {
        let denom = param_min.min(param_max) - middle_param;
        if denom == 0.0 {
            norm_default
        } else {
            offset * (norm_min - norm_default) / denom + norm_default
        }
    } else {
        norm_default
    };

    // This is reversed from what one would expect it to be.
    // I think I have a flipped sign convention somewhere,
    if reflect { result } else { -result }
}

// Compute the output value from the
// pendulum simulation.
fn compute_output(
    ty: PhysicsType,
    translation: Vec2,
    points: &[PendulumPoint],
    point_index: usize,
    reflect: bool,
    gravity: Vec2,
) -> f32 {
    match ty {
        PhysicsType::X => {
            if reflect {
                -translation.x
            } else {
                translation.x
            }
        }
        PhysicsType::Y => {
            if reflect {
                -translation.y
            } else {
                translation.y
            }
        }
        PhysicsType::Angle => {
            // Angle of the parent segment, or negated gravity if no parent.
            let parent_direction = if point_index >= 2 {
                points[point_index - 1].position() - points[point_index - 2].position()
            } else {
                -gravity
            };
            let angle = parent_direction.angle_to(translation);
            if reflect { -angle } else { angle }
        }
    }
}

fn update_output_parameter(
    param_value: &mut f32,
    raw_value: f32,
    param_min: f32,
    param_max: f32,
    scale: f32,
    weight: f32,
) {
    debug_assert!(weight >= 0.0);

    let value = raw_value * scale;
    let value = value.clamp(param_min, param_max);

    // Clamp alpha to no higher than 1.0.
    let alpha = (weight / MAX_WEIGHT).min(1.0);
    *param_value = *param_value * (1.0 - alpha) + value * alpha;
}
