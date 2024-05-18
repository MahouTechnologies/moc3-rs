use glam::vec3;

use super::{applicator::BlendShapeConstraints, BlendColor, ParamData};

use crate::{
    data::{Moc3Data, ParameterType, Version},
    puppet::applicator::{ApplicatorKind, ParamApplicator},
};

pub fn collect_blend_shape_constraints(
    read: &Moc3Data,
    constraint_index_start: usize,
    constraint_index_count: usize,
) -> Vec<BlendShapeConstraints> {
    let blend_shape_constraints = read.table.blend_shape_constraints.as_ref().unwrap();
    let blend_shape_constraint_indices =
        read.table.blend_shape_constraint_indices.as_ref().unwrap();
    let blend_shape_constraint_values = read.table.blend_shape_constraint_values.as_ref().unwrap();

    let mut ret = Vec::new();

    for i in constraint_index_start..constraint_index_start + constraint_index_count {
        let index =
            blend_shape_constraint_indices.blend_shape_constraint_sources_indices[i] as usize;

        let parameter_index = blend_shape_constraints.parameter_indices[index] as usize;
        let value_start =
            blend_shape_constraints.blend_shape_constraint_value_sources_starts[index] as usize;
        let value_count =
            blend_shape_constraints.blend_shape_constraint_value_sources_counts[index] as usize;

        ret.push(BlendShapeConstraints {
            parameter_index,
            keys: blend_shape_constraint_values.keys[value_start..value_start + value_count]
                .to_owned(),
            weights: blend_shape_constraint_values.weights[value_start..value_start + value_count]
                .to_owned(),
        })
    }

    ret
}

pub fn collect_blend_shapes(
    read: &Moc3Data,
    blend_shape_parameter_bindings_to_parameter: &[usize],
    applicators: &mut Vec<ParamApplicator>,
) {
    if read.header.version < Version::V4_02 {
        return;
    }

    let positions = read.positions();
    let keys = read.keys();

    let blend_shape_keyform_bindings = read.table.blend_shape_keyform_bindings.as_ref().unwrap();
    let blend_shape_parameter_bindings =
        read.table.blend_shape_parameter_bindings.as_ref().unwrap();

    {
        let blend_shape_art_meshes = read.table.blend_shape_art_meshes.as_ref().unwrap();

        let art_meshes = &read.table.art_meshes;
        let art_mesh_keyforms = &read.table.art_mesh_keyforms;

        for i in 0..read.table.count_info.blend_shape_art_meshes {
            let i = i as usize;

            let target_index = blend_shape_art_meshes.target_indices[i] as usize;
            let vertexes = art_meshes.vertex_counts[target_index] as usize;
            let start =
                blend_shape_art_meshes.blend_shape_keyform_binding_sources_starts[i] as usize;
            let count: usize =
                blend_shape_art_meshes.blend_shape_keyform_binding_sources_counts[i] as usize;

            for a in start..start + count {
                let param_binding_index = blend_shape_keyform_bindings
                    .blend_shape_parameter_binding_sources_indices[a]
                    as usize;
                let keyform_start =
                    blend_shape_keyform_bindings.keyform_sources_blend_shape_starts[a] as usize;
                let keyform_count =
                    blend_shape_keyform_bindings.keyform_sources_blend_shape_counts[a] as usize;

                let mut positions_to_bind = Vec::new();
                for keyform in keyform_start..keyform_start + keyform_count {
                    let position_start =
                        art_mesh_keyforms.keyform_position_sources_starts[keyform] as usize / 2;
                    positions_to_bind
                        .push(positions[position_start..position_start + vertexes].to_owned());
                }

                let opacities_to_bind = art_mesh_keyforms.opacities
                    [keyform_start..keyform_start + keyform_count]
                    .to_vec();
                let draw_orders_to_bind = art_mesh_keyforms.draw_orders
                    [keyform_start..keyform_start + keyform_count]
                    .to_vec();

                let x = {
                    let key_starts = blend_shape_parameter_bindings.keys_sources_starts
                        [param_binding_index] as usize;
                    let key_counts = blend_shape_parameter_bindings.keys_sources_counts
                        [param_binding_index] as usize;

                    (
                        keys[key_starts..key_starts + key_counts].to_owned(),
                        blend_shape_parameter_bindings_to_parameter[param_binding_index],
                    )
                };

                let constraint_index_start = blend_shape_keyform_bindings
                    .blend_shape_constraint_index_sources_starts[a]
                    as usize;
                let constraint_index_count = blend_shape_keyform_bindings
                    .blend_shape_constraint_index_sources_counts[a]
                    as usize;

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
        let blend_shape_warp_deformers = read.table.blend_shape_warp_deformers.as_ref().unwrap();

        let warp_deformers = &read.table.warp_deformers;
        let warp_deformer_keyforms = &read.table.warp_deformer_keyforms;

        for i in 0..read.table.count_info.blend_shape_warp_deformers {
            let i = i as usize;

            let target_index = blend_shape_warp_deformers.target_indices[i] as usize;
            let vertexes = warp_deformers.vertex_counts[target_index] as usize;
            let start =
                blend_shape_warp_deformers.blend_shape_keyform_binding_sources_starts[i] as usize;
            let count =
                blend_shape_warp_deformers.blend_shape_keyform_binding_sources_counts[i] as usize;

            for a in start..start + count {
                let param_binding_index = blend_shape_keyform_bindings
                    .blend_shape_parameter_binding_sources_indices[a]
                    as usize;
                let keyform_start =
                    blend_shape_keyform_bindings.keyform_sources_blend_shape_starts[a] as usize;
                let keyform_count =
                    blend_shape_keyform_bindings.keyform_sources_blend_shape_counts[a] as usize;

                let mut positions_to_bind = Vec::new();
                for keyform in keyform_start..keyform_start + keyform_count {
                    let position_start = warp_deformer_keyforms.keyform_position_sources_starts
                        [keyform] as usize
                        / 2;
                    positions_to_bind
                        .push(positions[position_start..position_start + vertexes].to_owned());
                }

                let opacities_to_bind = warp_deformer_keyforms.opacities
                    [keyform_start..keyform_start + keyform_count]
                    .to_vec();

                let x = {
                    let key_starts = blend_shape_parameter_bindings.keys_sources_starts
                        [param_binding_index] as usize;
                    let key_counts = blend_shape_parameter_bindings.keys_sources_counts
                        [param_binding_index] as usize;

                    (
                        keys[key_starts..key_starts + key_counts].to_owned(),
                        blend_shape_parameter_bindings_to_parameter[param_binding_index],
                    )
                };

                let constraint_index_start = blend_shape_keyform_bindings
                    .blend_shape_constraint_index_sources_starts[a]
                    as usize;
                let constraint_index_count = blend_shape_keyform_bindings
                    .blend_shape_constraint_index_sources_counts[a]
                    as usize;

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
    read: &Moc3Data,
    colors_start: usize,
    count: usize,
) -> Vec<BlendColor> {
    let keyform_multiply_colors = read.table.keyform_multiply_colors.as_ref().unwrap();
    let keyform_screen_colors = read.table.keyform_screen_colors.as_ref().unwrap();

    let mut ret = Vec::with_capacity(count);

    for i in colors_start..colors_start + count {
        let multiply_color = vec3(
            keyform_multiply_colors.red[i],
            keyform_multiply_colors.green[i],
            keyform_multiply_colors.blue[i],
        );

        let screen_color = vec3(
            keyform_screen_colors.red[i],
            keyform_screen_colors.green[i],
            keyform_screen_colors.blue[i],
        );
        ret.push(BlendColor {
            multiply_color,
            screen_color,
        });
    }
    ret
}

pub fn collect_parameter_bindings(
    read: &Moc3Data,
    parameter_bindings_to_parameter: &[usize],
    parameter_bindings_start: usize,
    parameter_bindings_count: usize,
) -> Vec<(Vec<f32>, usize)> {
    let parameter_bindings = &read.table.parameter_bindings;
    let parameter_binding_indices = &read.table.parameter_binding_indices;
    let keys = read.keys();

    let mut ret = Vec::new();

    for i in parameter_bindings_start..parameter_bindings_start + parameter_bindings_count {
        let ind = parameter_binding_indices.binding_sources_indices[i] as usize;
        let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
        let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;

        ret.push((
            keys[key_starts..key_starts + key_counts].to_owned(),
            parameter_bindings_to_parameter[ind],
        ))
    }

    ret
}

pub fn collect_param_data(read: &Moc3Data) -> ParamData {
    let param_count = read.table.count_info.parameters;
    let parameters = &read.table.parameters;

    let mut param_ids = Vec::new();
    for i in parameters.ids.iter() {
        param_ids.push(i.name.to_string());
    }

    let mut param_repeats = Vec::new();
    for i in parameters.is_repeat.iter() {
        param_repeats.push(*i == 1);
    }

    let mut param_types = Vec::new();
    if let Some(parameters_v402) = &read.table.parameters_v402 {
        for i in parameters_v402.parameter_types.iter() {
            param_types.push(*i);
        }
    } else {
        for _ in 0..param_count {
            param_types.push(ParameterType::Normal);
        }
    }

    ParamData {
        count: param_count,
        ids: param_ids,
        defaults: parameters.default_values.clone(),
        maxes: parameters.max_values.clone(),
        mins: parameters.min_values.clone(),
        repeats: param_repeats,
        decimals: parameters.decimal_places.clone(),
        types: param_types,
    }
}
