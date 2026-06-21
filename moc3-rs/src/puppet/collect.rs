use glam::vec3;

use super::{applicator::BlendShapeConstraints, BlendColor, ParamData};

use crate::{
    data::{Moc3, ParameterType, Version},
    puppet::applicator::{ApplicatorKind, ParamApplicator},
};

pub fn collect_blend_shape_constraints(
    read: Moc3<'_>,
    constraint_index_start: usize,
    constraint_index_count: usize,
) -> Vec<BlendShapeConstraints> {
    let constraint_indices = read.blend_shape_constraint_sources_indices().unwrap();
    let parameter_indices = read.blend_shape_constraint_parameter_indices().unwrap();
    let value_starts = read.blend_shape_constraint_value_sources_starts().unwrap();
    let value_counts = read.blend_shape_constraint_value_sources_counts().unwrap();
    let value_keys = read.blend_shape_constraint_value_keys().unwrap();
    let value_weights = read.blend_shape_constraint_value_weights().unwrap();

    let mut ret = Vec::new();

    for i in constraint_index_start..constraint_index_start + constraint_index_count {
        let index = constraint_indices[i] as usize;

        let parameter_index = parameter_indices[index] as usize;
        let value_start = value_starts[index] as usize;
        let value_count = value_counts[index] as usize;

        ret.push(BlendShapeConstraints {
            parameter_index,
            keys: value_keys[value_start..value_start + value_count].to_owned(),
            weights: value_weights[value_start..value_start + value_count].to_owned(),
        })
    }

    ret
}

pub fn collect_blend_shapes(
    read: Moc3<'_>,
    blend_shape_parameter_bindings_to_parameter: &[usize],
    applicators: &mut Vec<ParamApplicator>,
) {
    if read.version() < Version::V4_02 {
        return;
    }

    let positions = read.positions();
    let keys = read.keys();

    let kf_binding_indices = read
        .blend_shape_keyform_binding_parameter_binding_sources_indices()
        .unwrap();
    let kf_binding_starts = read
        .blend_shape_keyform_binding_keyform_sources_starts()
        .unwrap();
    let kf_binding_counts = read
        .blend_shape_keyform_binding_keyform_sources_counts()
        .unwrap();
    let kf_binding_constraint_starts = read
        .blend_shape_keyform_binding_constraint_index_sources_starts()
        .unwrap();
    let kf_binding_constraint_counts = read
        .blend_shape_keyform_binding_constraint_index_sources_counts()
        .unwrap();

    let param_binding_key_starts = read
        .blend_shape_parameter_binding_keys_sources_starts()
        .unwrap();
    let param_binding_key_counts = read
        .blend_shape_parameter_binding_keys_sources_counts()
        .unwrap();

    {
        let target_indices = read.blend_shape_art_mesh_target_indices().unwrap();
        let binding_starts = read
            .blend_shape_art_mesh_keyform_binding_sources_starts()
            .unwrap();
        let binding_counts = read
            .blend_shape_art_mesh_keyform_binding_sources_counts()
            .unwrap();

        let vertex_counts = read.art_mesh_vertex_counts();
        let position_starts = read.art_mesh_keyform_position_sources_starts();
        let opacities = read.art_mesh_keyform_opacities();
        let draw_orders = read.art_mesh_keyform_draw_orders();

        for i in 0..read.counts().blend_shape_art_meshes() {
            let i = i as usize;

            let target_index = target_indices[i] as usize;
            let vertexes = vertex_counts[target_index] as usize;
            let start = binding_starts[i] as usize;
            let count: usize = binding_counts[i] as usize;

            for a in start..start + count {
                let param_binding_index = kf_binding_indices[a] as usize;
                let keyform_start = kf_binding_starts[a] as usize;
                let keyform_count = kf_binding_counts[a] as usize;

                let mut positions_to_bind = Vec::new();
                for keyform in keyform_start..keyform_start + keyform_count {
                    let position_start = position_starts[keyform] as usize / 2;
                    positions_to_bind
                        .push(positions[position_start..position_start + vertexes].to_owned());
                }

                let opacities_to_bind =
                    opacities[keyform_start..keyform_start + keyform_count].to_vec();
                let draw_orders_to_bind =
                    draw_orders[keyform_start..keyform_start + keyform_count].to_vec();

                let x = {
                    let key_starts = param_binding_key_starts[param_binding_index] as usize;
                    let key_counts = param_binding_key_counts[param_binding_index] as usize;

                    (
                        keys[key_starts..key_starts + key_counts].to_owned(),
                        blend_shape_parameter_bindings_to_parameter[param_binding_index],
                    )
                };

                let constraint_index_start = kf_binding_constraint_starts[a] as usize;
                let constraint_index_count = kf_binding_constraint_counts[a] as usize;

                applicators.push(ParamApplicator {
                    kind_index: target_index as u32,
                    values: ApplicatorKind::ArtMesh(
                        positions_to_bind,
                        opacities_to_bind,
                        draw_orders_to_bind,
                        Vec::new(),
                    ),
                    data: vec![x],
                    blend: Some(collect_blend_shape_constraints(
                        read,
                        constraint_index_start,
                        constraint_index_count,
                    )),
                });
            }
        }
    }

    {
        let target_indices = read.blend_shape_warp_deformer_target_indices().unwrap();
        let binding_starts = read
            .blend_shape_warp_deformer_keyform_binding_sources_starts()
            .unwrap();
        let binding_counts = read
            .blend_shape_warp_deformer_keyform_binding_sources_counts()
            .unwrap();

        let vertex_counts = read.warp_deformer_vertex_counts();
        let position_starts = read.warp_deformer_keyform_position_sources_starts();
        let opacities = read.warp_deformer_keyform_opacities();

        for i in 0..read.counts().blend_shape_warp_deformers() {
            let i = i as usize;

            let target_index = target_indices[i] as usize;
            let vertexes = vertex_counts[target_index] as usize;
            let start = binding_starts[i] as usize;
            let count = binding_counts[i] as usize;

            for a in start..start + count {
                let param_binding_index = kf_binding_indices[a] as usize;
                let keyform_start = kf_binding_starts[a] as usize;
                let keyform_count = kf_binding_counts[a] as usize;

                let mut positions_to_bind = Vec::new();
                for keyform in keyform_start..keyform_start + keyform_count {
                    let position_start = position_starts[keyform] as usize / 2;
                    positions_to_bind
                        .push(positions[position_start..position_start + vertexes].to_owned());
                }

                let opacities_to_bind =
                    opacities[keyform_start..keyform_start + keyform_count].to_vec();

                let x = {
                    let key_starts = param_binding_key_starts[param_binding_index] as usize;
                    let key_counts = param_binding_key_counts[param_binding_index] as usize;

                    (
                        keys[key_starts..key_starts + key_counts].to_owned(),
                        blend_shape_parameter_bindings_to_parameter[param_binding_index],
                    )
                };

                let constraint_index_start = kf_binding_constraint_starts[a] as usize;
                let constraint_index_count = kf_binding_constraint_counts[a] as usize;

                applicators.push(ParamApplicator {
                    kind_index: target_index as u32,
                    values: ApplicatorKind::WarpDeformer(
                        positions_to_bind,
                        opacities_to_bind,
                        Vec::new(),
                    ),
                    data: vec![x],
                    blend: Some(collect_blend_shape_constraints(
                        read,
                        constraint_index_start,
                        constraint_index_count,
                    )),
                });
            }
        }
    }
}

pub fn collect_colors_to_bind(
    read: Moc3<'_>,
    colors_start: usize,
    count: usize,
) -> Vec<BlendColor> {
    let multiply_red = read.keyform_multiply_colors_red().unwrap();
    let multiply_green = read.keyform_multiply_colors_green().unwrap();
    let multiply_blue = read.keyform_multiply_colors_blue().unwrap();
    let screen_red = read.keyform_screen_colors_red().unwrap();
    let screen_green = read.keyform_screen_colors_green().unwrap();
    let screen_blue = read.keyform_screen_colors_blue().unwrap();

    let mut ret = Vec::with_capacity(count);

    for i in colors_start..colors_start + count {
        let multiply_color = vec3(multiply_red[i], multiply_green[i], multiply_blue[i]);
        let screen_color = vec3(screen_red[i], screen_green[i], screen_blue[i]);
        ret.push(BlendColor {
            multiply_color,
            screen_color,
        });
    }
    ret
}

pub fn collect_parameter_bindings(
    read: Moc3<'_>,
    parameter_bindings_to_parameter: &[usize],
    parameter_bindings_start: usize,
    parameter_bindings_count: usize,
) -> Vec<(Vec<f32>, usize)> {
    let key_starts = read.parameter_binding_keys_sources_starts();
    let key_counts = read.parameter_binding_keys_sources_counts();
    let binding_indices = read.parameter_binding_indices();
    let keys = read.keys();

    let mut ret = Vec::new();

    for i in parameter_bindings_start..parameter_bindings_start + parameter_bindings_count {
        let ind = binding_indices[i] as usize;
        let key_starts = key_starts[ind] as usize;
        let key_counts = key_counts[ind] as usize;

        ret.push((
            keys[key_starts..key_starts + key_counts].to_owned(),
            parameter_bindings_to_parameter[ind],
        ))
    }

    ret
}

pub fn collect_param_data(read: Moc3<'_>) -> ParamData {
    let param_count = read.counts().parameters();

    let mut param_ids = Vec::new();
    for i in read.parameter_ids().iter() {
        param_ids.push(i.name().to_string());
    }

    let mut param_repeats = Vec::new();
    for i in read.parameter_is_repeat().iter() {
        param_repeats.push(*i == 1);
    }

    let mut param_types = Vec::new();
    if let Some(parameter_types) = read.parameter_types() {
        for &t in parameter_types {
            param_types.push(if t == ParameterType::BlendShape as u32 {
                ParameterType::BlendShape
            } else {
                ParameterType::Normal
            });
        }
    } else {
        for _ in 0..param_count {
            param_types.push(ParameterType::Normal);
        }
    }

    ParamData {
        count: param_count,
        ids: param_ids,
        defaults: read.parameter_default_values().to_vec(),
        maxes: read.parameter_max_values().to_vec(),
        mins: read.parameter_min_values().to_vec(),
        repeats: param_repeats,
        decimals: read.parameter_decimal_places().to_vec(),
        types: param_types,
    }
}
