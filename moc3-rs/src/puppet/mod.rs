mod applicator;
mod draw_order;
mod node;

use std::{mem::discriminant, slice};

use bytemuck::{Pod, Zeroable};
use glam::{vec2, vec3, Vec2, Vec3};
use indextree::{Arena, NodeId};

use crate::{
    data::{ArtMeshFlags, DrawOrderGroupObjectType, Moc3Data, ParameterType, Version},
    deformer::{
        glue::apply_glue,
        rotation_deformer::{apply_rotation_deformer, TransformData},
        warp_deformer::apply_warp_deformer,
    },
    puppet::{
        applicator::{ApplicatorKind, ParamApplicator},
        node::{ArtMeshData, RotationDeformerData, WarpDeformerData},
    },
};

use self::{
    applicator::BlendShapeConstraints,
    draw_order::{draw_order_tree, DrawOrderNode},
    node::{DeformerNode, GlueNode},
};

#[derive(Debug, Clone)]
pub struct Puppet {
    pub node_roots: Vec<NodeId>,
    pub nodes: Arena<DeformerNode>,
    pub glue_nodes: Vec<GlueNode>,

    pub params: Vec<f32>,
    pub applicators: Vec<ParamApplicator>,

    pub art_mesh_count: u32,
    pub art_mesh_uvs: Vec<Vec<Vec2>>,
    pub art_mesh_indices: Vec<Vec<u16>>,
    pub art_mesh_textures: Vec<u32>,
    pub art_mesh_flags: Vec<ArtMeshFlags>,

    pub vertexes_count: Vec<u32>,

    pub draw_order_nodes: Arena<DrawOrderNode>,
    pub draw_order_roots: Vec<NodeId>,
    pub max_draw_order_children: u32,
}

#[derive(Pod, Zeroable, Debug, Clone, Copy)]
#[repr(C)]
pub struct BlendColor {
    pub multiply_color: Vec3,
    pub screen_color: Vec3,
}

impl Default for BlendColor {
    fn default() -> Self {
        Self {
            multiply_color: Vec3::ONE,
            screen_color: Vec3::ZERO,
        }
    }
}

impl BlendColor {
    pub fn blend(&self, child: &BlendColor) -> BlendColor {
        Self {
            multiply_color: self.multiply_color * child.multiply_color,
            screen_color: (self.screen_color + child.screen_color)
                - (self.screen_color * child.screen_color),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PuppetFrameData {
    pub art_mesh_render_orders: Vec<u32>,
    pub art_mesh_draw_orders: Vec<f32>,

    pub warp_deformer_data: Vec<Vec<Vec2>>,
    pub rotation_deformer_data: Vec<TransformData>,
    pub art_mesh_data: Vec<Vec<Vec2>>,

    pub deformer_scale_data: Vec<f32>,

    pub warp_deformer_opacities: Vec<f32>,
    pub rotation_deformer_opacities: Vec<f32>,
    pub art_mesh_opacities: Vec<f32>,

    pub warp_deformer_colors: Vec<BlendColor>,
    pub rotation_deformer_colors: Vec<BlendColor>,
    pub art_mesh_colors: Vec<BlendColor>,

    pub glue_data: Vec<f32>,
}

impl Puppet {
    pub fn update(&self, parameter_values: &[f32], frame_data: &mut PuppetFrameData) {
        for applicator in &self.applicators {
            applicator.apply(&parameter_values, frame_data);
        }

        let art_mesh_ptr = frame_data.art_mesh_data.as_mut_ptr();
        let warp_deformer_ptr = frame_data.warp_deformer_data.as_mut_ptr();
        let rotation_deformer_ptr = frame_data.rotation_deformer_data.as_mut_ptr();

        let art_mesh_opacity_ptr = frame_data.art_mesh_opacities.as_mut_ptr();
        let warp_deformer_opacity_ptr = frame_data.warp_deformer_opacities.as_mut_ptr();
        let rotation_deformer_opacity_ptr = frame_data.rotation_deformer_opacities.as_mut_ptr();

        let art_mesh_color_ptr = frame_data.art_mesh_colors.as_mut_ptr();
        let warp_deformer_color_ptr = frame_data.warp_deformer_colors.as_mut_ptr();
        let rotation_deformer_color_ptr = frame_data.rotation_deformer_colors.as_mut_ptr();

        for root in &self.node_roots {
            for child_id in root.descendants(&self.nodes).skip(1) {
                let parent_id = self.nodes[child_id]
                    .parent()
                    .expect("node should be child node");

                let parent = self.nodes[parent_id].get();
                let child = self.nodes[child_id].get();

                // A well-formed file will not have a parent and child referring to the same data,
                // but this is here to deal with malformed files.
                assert_ne!(
                    (discriminant(&child.data), child.broad_index),
                    (discriminant(&parent.data), parent.broad_index),
                );

                let (child_changes, child_opacity, child_color) = match &child.data {
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::ArtMesh(_) => unsafe {
                        let vec_data = &mut *art_mesh_ptr.add(child.broad_index as usize);
                        (
                            vec_data.as_mut_slice(),
                            &mut *art_mesh_opacity_ptr.add(child.broad_index as usize),
                            &mut *art_mesh_color_ptr.add(child.broad_index as usize),
                        )
                    },
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::WarpDeformer(_, ind) => unsafe {
                        frame_data.deformer_scale_data[child.broad_index as usize] =
                            frame_data.deformer_scale_data[parent.broad_index as usize];
                        let vec_data = &mut *warp_deformer_ptr.add(*ind as usize);
                        (
                            vec_data.as_mut_slice(),
                            &mut *warp_deformer_opacity_ptr.add(*ind as usize),
                            &mut *warp_deformer_color_ptr.add(*ind as usize),
                        )
                    },
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::RotationDeformer(_, ind) => unsafe {
                        let scale_ref = &mut (*rotation_deformer_ptr.add(*ind as usize)).scale;

                        *scale_ref *= frame_data.deformer_scale_data[parent.broad_index as usize];

                        frame_data.deformer_scale_data[child.broad_index as usize] = *scale_ref;

                        let slice_data = slice::from_mut(
                            &mut (*rotation_deformer_ptr.add(*ind as usize)).origin,
                        );

                        (
                            slice_data,
                            &mut *rotation_deformer_opacity_ptr.add(*ind as usize),
                            &mut *rotation_deformer_color_ptr.add(child.broad_index as usize),
                        )
                    },
                };

                // Apply the parent deformer to the child deformer or underlying art mesh.
                let (parent_opacity, parent_color) = match &parent.data {
                    node::NodeKind::ArtMesh(_) => {
                        unreachable!("art mesh should not have children")
                    }
                    node::NodeKind::WarpDeformer(data, ind) => {
                        // Safety: We ensure above that we will not have overlapping references.
                        let grid = unsafe { &*warp_deformer_ptr.add(*ind as usize) };
                        apply_warp_deformer(
                            grid,
                            data.is_new_deformerr,
                            data.rows as usize,
                            data.columns as usize,
                            child_changes,
                        );

                        // Safety: we guarantee above this will not overlap
                        (
                            unsafe { *warp_deformer_opacity_ptr.add(*ind as usize) },
                            unsafe { *warp_deformer_color_ptr.add(*ind as usize) },
                        )
                    }
                    node::NodeKind::RotationDeformer(data, ind) => {
                        let transform = unsafe { &*rotation_deformer_ptr.add(*ind as usize) };
                        apply_rotation_deformer(transform, data.base_angle, child_changes);

                        // Safety: we guarantee above this will not overlap
                        (
                            unsafe { *rotation_deformer_opacity_ptr.add(*ind as usize) },
                            unsafe { *rotation_deformer_color_ptr.add(*ind as usize) },
                        )
                    }
                };

                // Propogate down the opacity numbers
                *child_opacity *= parent_opacity;
                *child_color = parent_color.blend(&child_color);
            }
        }

        for glue in &self.glue_nodes {
            assert_ne!(glue.art_mesh_index[0], glue.art_mesh_index[1]);

            apply_glue(
                frame_data.glue_data[glue.kind_index as usize],
                &glue.mesh_indices,
                &glue.weights,
                // Safety: We ensure above that we will not have overlapping references.
                unsafe { &mut (*art_mesh_ptr.add(glue.art_mesh_index[0] as usize)) },
                unsafe { &mut (*art_mesh_ptr.add(glue.art_mesh_index[1] as usize)) },
            )
        }

        draw_order_tree(&self.draw_order_nodes, self.draw_order_roots[0], frame_data);
    }
}

fn collect_blend_shape_constraints(
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

fn collect_blend_shapes(read: &Moc3Data, applicators: &mut Vec<ParamApplicator>) {
    if read.header.version < Version::V4_02 {
        return;
    }

    let positions = read.positions();
    let keys = read.keys();

    // let parameters = &read.table.parameters;
    let parameters_v402 = read.table.parameters_v402.as_ref().unwrap();

    let mut blend_shape_parameter_bindings_to_parameter =
        vec![0usize; read.table.count_info.blend_shape_parameter_bindings as usize];

    for i in 0..read.table.count_info.parameters {
        let i = i as usize;

        if parameters_v402.parameter_types[i] == ParameterType::BlendShape {
            let start = parameters_v402.blend_shape_parameter_binding_sources_starts[i] as usize;
            let count = parameters_v402.blend_shape_parameter_binding_sources_counts[i] as usize;

            for a in start..(start + count) {
                blend_shape_parameter_bindings_to_parameter[a] = i;
            }
        }
    }

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
            let count =
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
                    kind_index: i as u32,
                    values: ApplicatorKind::ArtMesh(
                        positions_to_bind,
                        opacities_to_bind,
                        draw_orders_to_bind,
                        Vec::new(),
                    ),
                    x: Some(x),
                    y: None,
                    z: None,
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
                    kind_index: i as u32,
                    values: ApplicatorKind::WarpDeformer(
                        positions_to_bind,
                        opacities_to_bind,
                        Vec::new(),
                    ),
                    x: Some(x),
                    y: None,
                    z: None,
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

fn collect_colors_to_bind(read: &Moc3Data, colors_start: usize, count: usize) -> Vec<BlendColor> {
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

pub fn puppet_from_moc3(read: &Moc3Data) -> Puppet {
    let art_meshes = &read.table.art_meshes;
    let parameters = &read.table.parameters;
    let parameter_bindings = &read.table.parameter_bindings;
    let parameter_binding_indices = &read.table.parameter_binding_indices;
    let keyform_bindings = &read.table.keyform_bindings;
    let positions = read.positions();
    let keys = read.keys();

    // We store our data in a slightly different way than how it was intended, so we
    // need this map of parameter binding index back up to the parameter itself. This is
    // the parameter binding pulling the data, instead of the parameter pushing the data to
    // all of the bindings. Which is better or worse for performance / code I'm not sure.
    let mut parameter_bindings_to_parameter =
        vec![0usize; read.table.count_info.parameter_bindings as usize];
    let mut blend_shape_parameter_bindings_to_parameter =
        vec![0usize; read.table.count_info.blend_shape_parameter_bindings as usize];
    for i in 0..read.table.count_info.parameters {
        let i = i as usize;

        let start = parameters.parameter_binding_sources_starts[i] as usize;
        let count = parameters.parameter_binding_sources_counts[i] as usize;

        for a in start..(start + count) {
            parameter_bindings_to_parameter[a] = i;
        }

        // I think this works, as the way the format should not have regular
        // parameter bindings for blend shape parameters.
        if let Some(parameters_v402) = &read.table.parameters_v402 {
            if parameters_v402.parameter_types[i] == ParameterType::BlendShape {
                let start =
                    parameters_v402.blend_shape_parameter_binding_sources_starts[i] as usize;
                let count =
                    parameters_v402.blend_shape_parameter_binding_sources_counts[i] as usize;

                for a in start..(start + count) {
                    blend_shape_parameter_bindings_to_parameter[a] = i;
                }
            }
        }
    }

    let mut applicators = Vec::new();
    let mut node_arena = Arena::<DeformerNode>::with_capacity(
        (read.table.count_info.art_meshes
            + read.table.count_info.warp_deformers
            + read.table.count_info.rotation_deformers) as usize,
    );

    let deformers = &read.table.deformers;
    let warp_deformers = &read.table.warp_deformers;
    let warp_deformer_keyforms = &read.table.warp_deformer_keyforms;
    let warp_deformer_keyforms_v402 = read.table.warp_deformer_keyforms_v402.as_ref();
    let rotation_deformers = &read.table.rotation_deformers;
    let rotation_deformer_keyforms = &read.table.rotation_deformer_keyforms;
    let rotation_deformer_keyforms_v402 = read.table.rotation_deformer_keyforms_v402.as_ref();

    // Everything down from here, delimited by horizontal ASCII lines, represents all of the possible
    // things that can affect the final model directly. This includes deformers (both rotation and warp),
    // art mesh deformation, as well as glues. Here, the ParamApplicators for regular parameters as well as
    // blendshapes occurs.

    // ----- BEGIN PARAMETER STUFF -----
    let mut deformer_indices_to_node_ids: Vec<Option<NodeId>> =
        vec![None; read.table.count_info.deformers as usize];

    let mut node_roots: Vec<NodeId> = Vec::new();

    for i in 0..read.table.count_info.deformers {
        let i: usize = i as usize;
        let specific = deformers.specific_sources_indices[i] as usize;

        let parent_deformer_index = deformers.parent_deformer_indices[i];
        if deformers.types[i] == 0 {
            let vertexes = warp_deformers.vertex_counts[specific] as usize;

            let is_new_deformerr = read
                .table
                .warp_deformer_keyforms_v303
                .as_ref()
                .map(|x| x.is_new_deformerrs[specific])
                .unwrap_or(0);

            {
                let node_to_append = DeformerNode {
                    id: deformers.ids[i].name.to_string(),
                    broad_index: i as u32,
                    parent_part_index: deformers.parent_part_indices[i],
                    is_enabled: deformers.is_enabled[i] != 0,
                    data: node::NodeKind::WarpDeformer(
                        WarpDeformerData {
                            rows: warp_deformers.rows[specific],
                            columns: warp_deformers.columns[specific],
                            is_new_deformerr: is_new_deformerr != 0,
                        },
                        specific as u32,
                    ),
                };

                let res = if parent_deformer_index != -1 {
                    deformer_indices_to_node_ids[parent_deformer_index as usize]
                        .unwrap()
                        .append_value(node_to_append, &mut node_arena)
                } else {
                    let it = node_arena.new_node(node_to_append);
                    node_roots.push(it);
                    it
                };

                deformer_indices_to_node_ids[i] = Some(res);
            }

            let binding_index = warp_deformers.keyform_binding_sources_indices[specific] as usize;
            let start = warp_deformers.keyform_sources_starts[specific] as usize;
            let count = warp_deformers.keyform_sources_counts[specific] as usize;

            let mut positions_to_bind = Vec::new();
            for i in start..start + count {
                let position_start =
                    warp_deformer_keyforms.keyform_position_sources_starts[i] as usize / 2;
                positions_to_bind
                    .push(positions[position_start..position_start + vertexes].to_owned());
            }
            let opacities_to_bind = warp_deformer_keyforms.opacities[start..start + count].to_vec();
            let colors_to_bind =
                if let Some(warp_deformer_keyforms_v402) = warp_deformer_keyforms_v402 {
                    let colors_start =
                        warp_deformer_keyforms_v402.keyform_color_sources_start[i] as usize;

                    collect_colors_to_bind(read, colors_start, count)
                } else {
                    Vec::new()
                };

            let parameter_bindings_count =
                keyform_bindings.parameter_binding_index_sources_counts[binding_index] as usize;
            let parameter_bindings_start: usize =
                keyform_bindings.parameter_binding_index_sources_starts[binding_index] as usize;

            // TODO: replace this, I was told the 3 is a suggestion and not a hard cap
            assert!(parameter_bindings_count <= 3);
            let x_index = if parameter_bindings_count >= 1 {
                let ind = parameter_binding_indices.binding_sources_indices
                    [parameter_bindings_start] as usize;
                let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
                let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
                Some((
                    keys[key_starts..key_starts + key_counts].to_owned(),
                    parameter_bindings_to_parameter[ind],
                ))
            } else {
                None
            };

            let y_index = if parameter_bindings_count >= 2 {
                let ind = parameter_binding_indices.binding_sources_indices
                    [parameter_bindings_start + 1] as usize;
                let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
                let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
                Some((
                    keys[key_starts..key_starts + key_counts].to_owned(),
                    parameter_bindings_to_parameter[ind],
                ))
            } else {
                None
            };

            let z_index = if parameter_bindings_count >= 3 {
                let ind = parameter_binding_indices.binding_sources_indices
                    [parameter_bindings_start + 2] as usize;
                let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
                let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
                Some((
                    keys[key_starts..key_starts + key_counts].to_owned(),
                    parameter_bindings_to_parameter[ind],
                ))
            } else {
                None
            };

            applicators.push(ParamApplicator {
                kind_index: deformers.specific_sources_indices[i],
                values: ApplicatorKind::WarpDeformer(
                    positions_to_bind,
                    opacities_to_bind,
                    colors_to_bind,
                ),
                x: x_index,
                y: y_index,
                z: z_index,
                blend: None,
            });
        } else if deformers.types[i] == 1 {
            let base_angle = rotation_deformers.base_angles[specific];

            {
                let node_to_append = DeformerNode {
                    id: deformers.ids[i].name.to_string(),
                    broad_index: i as u32,
                    parent_part_index: deformers.parent_part_indices[i],
                    is_enabled: deformers.is_enabled[i] != 0,
                    data: node::NodeKind::RotationDeformer(
                        RotationDeformerData { base_angle },
                        specific as u32,
                    ),
                };

                let res = if parent_deformer_index != -1 {
                    deformer_indices_to_node_ids[parent_deformer_index as usize]
                        .unwrap()
                        .append_value(node_to_append, &mut node_arena)
                } else {
                    let it = node_arena.new_node(node_to_append);
                    node_roots.push(it);
                    it
                };

                deformer_indices_to_node_ids[i] = Some(res);
            }

            let binding_index =
                rotation_deformers.keyform_binding_sources_indices[specific] as usize;
            let start = rotation_deformers.keyform_sources_starts[specific] as usize;
            let count = rotation_deformers.keyform_sources_counts[specific] as usize;

            let mut positions_to_bind = Vec::new();
            for i in start..start + count {
                let x_origin = rotation_deformer_keyforms.x_origin[i];
                let y_origin = rotation_deformer_keyforms.y_origin[i];
                let scale = rotation_deformer_keyforms.scales[i];
                let angle = rotation_deformer_keyforms.angles[i];
                positions_to_bind.push(TransformData {
                    origin: vec2(x_origin, y_origin),
                    scale,
                    angle,
                });
            }
            let opacities_to_bind =
                rotation_deformer_keyforms.opacities[start..start + count].to_vec();
            let colors_to_bind =
                if let Some(rotation_deformer_keyforms_v402) = rotation_deformer_keyforms_v402 {
                    let colors_start =
                        rotation_deformer_keyforms_v402.keyform_color_sources_start[i] as usize;

                    collect_colors_to_bind(read, colors_start, count)
                } else {
                    Vec::new()
                };

            let parameter_bindings_count =
                keyform_bindings.parameter_binding_index_sources_counts[binding_index] as usize;
            let parameter_bindings_start: usize =
                keyform_bindings.parameter_binding_index_sources_starts[binding_index] as usize;

            // TODO: replace this, I was told the 3 is a suggestion and not a hard cap
            assert!(parameter_bindings_count <= 3);
            let x_index = if parameter_bindings_count >= 1 {
                let ind = parameter_binding_indices.binding_sources_indices
                    [parameter_bindings_start] as usize;
                let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
                let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
                Some((
                    keys[key_starts..key_starts + key_counts].to_owned(),
                    parameter_bindings_to_parameter[ind],
                ))
            } else {
                None
            };

            let y_index = if parameter_bindings_count >= 2 {
                let ind = parameter_binding_indices.binding_sources_indices
                    [parameter_bindings_start + 1] as usize;
                let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
                let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
                Some((
                    keys[key_starts..key_starts + key_counts].to_owned(),
                    parameter_bindings_to_parameter[ind],
                ))
            } else {
                None
            };

            let z_index = if parameter_bindings_count >= 3 {
                let ind = parameter_binding_indices.binding_sources_indices
                    [parameter_bindings_start + 2] as usize;
                let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
                let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
                Some((
                    keys[key_starts..key_starts + key_counts].to_owned(),
                    parameter_bindings_to_parameter[ind],
                ))
            } else {
                None
            };

            applicators.push(ParamApplicator {
                kind_index: deformers.specific_sources_indices[i],
                values: ApplicatorKind::RotationDeformer(
                    positions_to_bind,
                    opacities_to_bind,
                    colors_to_bind,
                ),
                x: x_index,
                y: y_index,
                z: z_index,
                blend: None,
            });
        }
    }

    let uvs = read.uvs();
    let vertex_indices = read.vertex_indices();
    let mut art_mesh_uvs = Vec::with_capacity(read.table.count_info.art_meshes as usize);
    let mut art_mesh_indices = Vec::with_capacity(read.table.count_info.art_meshes as usize);
    let art_mesh_keyforms = &read.table.art_mesh_keyforms;
    let art_mesh_deformer_keyforms_v402 = read.table.art_mesh_deformer_keyforms_v402.as_ref();

    for i in 0..read.table.count_info.art_meshes {
        let i = i as usize;
        let uv_start = art_meshes.uv_sources_starts[i] as usize / 2;
        let vertexes = art_meshes.vertex_counts[i] as usize;
        let index_start = art_meshes.vertex_index_sources_starts[i] as usize;
        let index_count = art_meshes.vertex_index_sources_counts[i] as usize;
        art_mesh_uvs.push(uvs[uv_start..uv_start + vertexes].to_vec());
        art_mesh_indices.push(vertex_indices[index_start..index_start + index_count].to_vec());

        let binding_index = art_meshes.keyform_binding_sources_indices[i] as usize;
        let start = art_meshes.keyform_sources_starts[i] as usize;
        let count = art_meshes.keyform_sources_counts[i] as usize;

        let mut positions_to_bind = Vec::new();
        for i in start..start + count {
            let position_start = art_mesh_keyforms.keyform_position_sources_starts[i] as usize / 2;
            positions_to_bind.push(positions[position_start..position_start + vertexes].to_owned());
        }
        let opacities_to_bind = art_mesh_keyforms.opacities[start..start + count].to_vec();
        let draw_orders_to_bind = art_mesh_keyforms.draw_orders[start..start + count].to_vec();
        let colors_to_bind =
            if let Some(art_mesh_deformer_keyforms_v402) = art_mesh_deformer_keyforms_v402 {
                let colors_start =
                    art_mesh_deformer_keyforms_v402.keyform_color_sources_start[i] as usize;

                collect_colors_to_bind(read, colors_start, count)
            } else {
                Vec::new()
            };

        {
            let parent_deformer_index = art_meshes.parent_deformer_indices[i];

            let node_to_append = DeformerNode {
                id: art_meshes.ids[i].name.to_string(),
                broad_index: i as u32,
                parent_part_index: art_meshes.parent_part_indices[i],
                is_enabled: art_meshes.is_enabled[i] != 0,
                data: node::NodeKind::ArtMesh(ArtMeshData {
                    vertexes: vertexes as u32,
                }),
            };

            if parent_deformer_index != -1 {
                deformer_indices_to_node_ids[parent_deformer_index as usize]
                    .unwrap()
                    .append_value(node_to_append, &mut node_arena);
            } else {
                let it = node_arena.new_node(node_to_append);
                node_roots.push(it);
            };
        }

        let parameter_bindings_count =
            keyform_bindings.parameter_binding_index_sources_counts[binding_index] as usize;
        let parameter_bindings_start =
            keyform_bindings.parameter_binding_index_sources_starts[binding_index] as usize;

        // TODO: replace this, I was told the 3 is a suggestion and not a hard cap
        // assert!(parameter_bindings_count <= 3);
        let x_index = if parameter_bindings_count >= 1 {
            let ind = parameter_binding_indices.binding_sources_indices[parameter_bindings_start]
                as usize;
            let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
            let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
            Some((
                keys[key_starts..key_starts + key_counts].to_owned(),
                parameter_bindings_to_parameter[ind],
            ))
        } else {
            None
        };

        let y_index = if parameter_bindings_count >= 2 {
            let ind = parameter_binding_indices.binding_sources_indices
                [parameter_bindings_start + 1] as usize;
            let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
            let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
            Some((
                keys[key_starts..key_starts + key_counts].to_owned(),
                parameter_bindings_to_parameter[ind],
            ))
        } else {
            None
        };

        let z_index = if parameter_bindings_count == 3 {
            let ind = parameter_binding_indices.binding_sources_indices
                [parameter_bindings_start + 2] as usize;
            let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
            let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
            Some((
                keys[key_starts..key_starts + key_counts].to_owned(),
                parameter_bindings_to_parameter[ind],
            ))
        } else {
            None
        };

        applicators.push(ParamApplicator {
            kind_index: i as u32,
            values: ApplicatorKind::ArtMesh(
                positions_to_bind,
                opacities_to_bind,
                draw_orders_to_bind,
                colors_to_bind,
            ),
            x: x_index,
            y: y_index,
            z: z_index,
            blend: None,
        });
    }

    let mut glue_nodes = Vec::new();

    let glues = &read.table.glues;
    let glue_infos = &read.table.glue_infos;
    let glue_keyforms = &read.table.glue_keyforms;
    for i in 0..read.table.count_info.glues {
        let i = i as usize;

        let glue_info_start = glues.glue_info_sources_starts[i] as usize;
        let glue_info_count = glues.glue_info_sources_counts[i] as usize;

        let binding_index = glues.keyform_binding_sources_indices[i] as usize;
        let start = glues.keyform_sources_starts[i] as usize;
        let count = glues.keyform_sources_counts[i] as usize;

        let mesh_indices =
            &glue_infos.vertex_indices[glue_info_start..glue_info_start + glue_info_count];
        let weights = &glue_infos.weights[glue_info_start..glue_info_start + glue_info_count];

        let intensities_to_bind = glue_keyforms.intensities[start..start + count].to_vec();

        glue_nodes.push(GlueNode {
            id: glues.ids[i].name.to_string(),
            kind_index: i as u32,
            art_mesh_index: [glues.art_mesh_indices_a[i], glues.art_mesh_indices_b[i]],
            mesh_indices: mesh_indices.to_vec(),
            weights: weights.to_vec(),
        });

        let parameter_bindings_count =
            keyform_bindings.parameter_binding_index_sources_counts[binding_index] as usize;
        let parameter_bindings_start =
            keyform_bindings.parameter_binding_index_sources_starts[binding_index] as usize;

        // TODO: replace this, I was told the 3 is a suggestion and not a hard cap
        assert!(parameter_bindings_count <= 3);
        let x_index = if parameter_bindings_count >= 1 {
            let ind = parameter_binding_indices.binding_sources_indices[parameter_bindings_start]
                as usize;
            let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
            let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
            Some((
                keys[key_starts..key_starts + key_counts].to_owned(),
                parameter_bindings_to_parameter[ind],
            ))
        } else {
            None
        };

        let y_index = if parameter_bindings_count >= 2 {
            let ind = parameter_binding_indices.binding_sources_indices
                [parameter_bindings_start + 1] as usize;
            let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
            let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
            Some((
                keys[key_starts..key_starts + key_counts].to_owned(),
                parameter_bindings_to_parameter[ind],
            ))
        } else {
            None
        };

        let z_index = if parameter_bindings_count == 3 {
            let ind = parameter_binding_indices.binding_sources_indices
                [parameter_bindings_start + 2] as usize;
            let key_starts = parameter_bindings.keys_sources_starts[ind] as usize;
            let key_counts = parameter_bindings.keys_sources_counts[ind] as usize;
            Some((
                keys[key_starts..key_starts + key_counts].to_owned(),
                parameter_bindings_to_parameter[ind],
            ))
        } else {
            None
        };

        applicators.push(ParamApplicator {
            kind_index: i as u32,
            values: ApplicatorKind::Glue(intensities_to_bind),
            x: x_index,
            y: y_index,
            z: z_index,
            blend: None,
        });
    }
    // ----- END PARAMETER STUFF -----
    collect_blend_shapes(read, &mut applicators);

    // Here we do the draw order groups. This lets us apply draw orders to the mesh depending on how
    // the draw order groups interact, and lets us calculate the actual priority when the nodes have the
    // same draw order by breaking ties via tree position.

    // TODO: something like this for parts
    let draw_order_groups = &read.table.draw_order_groups;
    let draw_order_group_objects = &read.table.draw_order_group_objects;

    let mut draw_order_nodes = Arena::<DrawOrderNode>::with_capacity(
        read.table.count_info.draw_order_group_objects as usize,
    );

    let mut draw_order_roots: Vec<Option<NodeId>> =
        vec![None; read.table.count_info.draw_order_groups as usize];

    let mut max_draw_order_children = 0;
    draw_order_roots[0] = Some(draw_order_nodes.new_node(DrawOrderNode::Part { index: u32::MAX }));

    for i in 0..read.table.count_info.draw_order_groups {
        let i = i as usize;

        let object_sources_start = draw_order_groups.object_sources_starts[i];
        let object_sources_count = draw_order_groups.object_sources_counts[i];
        max_draw_order_children = max_draw_order_children.max(object_sources_count);

        for a in object_sources_start..(object_sources_start + object_sources_count) {
            let a = a as usize;

            let type_index = draw_order_group_objects.indices[a];
            let to_append =
                if draw_order_group_objects.types[a] == DrawOrderGroupObjectType::ArtMesh {
                    DrawOrderNode::ArtMesh { index: type_index }
                } else {
                    DrawOrderNode::Part { index: type_index }
                };

            let res = draw_order_roots[i]
                .unwrap()
                .append_value(to_append, &mut draw_order_nodes);
            let self_index = draw_order_group_objects.self_indices[a];
            if self_index != u32::MAX {
                draw_order_roots[self_index as usize] = Some(res);
            }
        }
    }

    // Here we parse all of the data related to parameters onto the puppet. Right now,
    // only the default value is saved, but this will be filled with all of the other data
    // in the future.
    let mut params = Vec::with_capacity(read.table.count_info.parameters as usize);
    for i in 0..read.table.count_info.parameters {
        let i = i as usize;
        params.push(parameters.default_values[i]);
    }

    Puppet {
        node_roots,
        nodes: node_arena,
        glue_nodes,

        params,
        applicators,

        art_mesh_count: read.table.count_info.art_meshes,
        art_mesh_indices,
        art_mesh_textures: read.table.art_meshes.texture_nums.clone(),
        art_mesh_flags: read.table.art_meshes.art_mesh_flags.clone(),
        art_mesh_uvs,
        vertexes_count: read.table.art_meshes.vertex_counts.clone(),

        draw_order_nodes,
        draw_order_roots: draw_order_roots.into_iter().map(|x| x.unwrap()).collect(),
        max_draw_order_children: 222,
    }
}
