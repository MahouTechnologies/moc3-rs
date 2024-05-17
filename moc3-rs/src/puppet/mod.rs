mod applicator;
mod collect;
mod draw_order;
mod node;

use std::{mem::discriminant, slice};

use bytemuck::{Pod, Zeroable};
use glam::{vec2, Vec2, Vec3};
use indextree::{Arena, NodeId};

use crate::{
    data::{ArtMeshFlags, DrawOrderGroupObjectType, Moc3Data, ParameterType},
    deformer::{
        glue::apply_glue,
        rotation_deformer::{
            apply_rotation_deformer, calculate_rotation_deformer_angle, TransformData,
        },
        warp_deformer::apply_warp_deformer,
    },
    puppet::{
        applicator::{ApplicatorKind, ParamApplicator},
        node::{ArtMeshData, RotationDeformerData, WarpDeformerData},
    },
};

use self::{
    collect::{
        collect_blend_shapes, collect_colors_to_bind, collect_param_data,
        collect_parameter_bindings,
    },
    draw_order::{draw_order_tree, DrawOrderNode},
    node::{DeformerNode, GlueNode},
};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ParamData {
    pub count: u32,
    pub ids: Vec<String>,
    pub defaults: Vec<f32>,
    pub maxes: Vec<f32>,
    pub mins: Vec<f32>,
    pub repeats: Vec<bool>,
    pub decimals: Vec<u32>,
    pub types: Vec<ParameterType>,
}

#[derive(Debug, Clone)]
pub struct Puppet {
    node_roots: Vec<NodeId>,
    nodes: Arena<DeformerNode>,
    glue_nodes: Vec<GlueNode>,

    params: ParamData,
    applicators: Vec<ParamApplicator>,

    pub art_mesh_count: u32,
    warp_deformer_count: u32,
    rotation_deformer_count: u32,
    glue_count: u32,

    warp_deformer_grid_count: Vec<u32>,

    pub art_mesh_uvs: Vec<Vec<Vec2>>,
    pub art_mesh_indices: Vec<Vec<u16>>,
    pub art_mesh_textures: Vec<u32>,
    pub art_mesh_flags: Vec<ArtMeshFlags>,
    pub art_mesh_mask_indices: Vec<Vec<u32>>,
    pub art_mesh_vertexes: Vec<u32>,

    draw_order_nodes: Arena<DrawOrderNode>,
    draw_order_roots: Vec<NodeId>,
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
    pub const ZERO: Self = BlendColor {
        multiply_color: Vec3::ZERO,
        screen_color: Vec3::ZERO,
    };

    pub const NAN: Self = BlendColor {
        multiply_color: Vec3::NAN,
        screen_color: Vec3::NAN,
    };

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
    corrected_params: Vec<f32>,

    art_mesh_draw_orders: Vec<f32>,

    pub art_mesh_render_orders: Vec<u32>,
    pub art_mesh_data: Vec<Vec<Vec2>>,
    pub art_mesh_opacities: Vec<f32>,
    pub art_mesh_colors: Vec<BlendColor>,

    warp_deformer_data: Vec<Vec<Vec2>>,
    rotation_deformer_data: Vec<TransformData>,
    warp_deformer_opacities: Vec<f32>,
    rotation_deformer_opacities: Vec<f32>,
    warp_deformer_colors: Vec<BlendColor>,
    rotation_deformer_colors: Vec<BlendColor>,

    deformer_scale_data: Vec<f32>,
    glue_data: Vec<f32>,
}

impl Puppet {
    pub fn param_data(&self) -> &ParamData {
        &self.params
    }

    pub fn update(&self, input_params: &[f32], frame_data: &mut PuppetFrameData) {
        for i in 0..input_params.len() {
            let res = input_params[i].clamp(self.params.mins[i], self.params.maxes[i]);
            frame_data.corrected_params[i] = res;
        }
        for applicator in &self.applicators {
            applicator.apply(frame_data);
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

        for root_id in self.node_roots.iter().copied() {
            {
                let root = self.nodes[root_id].get();
                match &root.data {
                    node::NodeKind::RotationDeformer(_, ind) => {
                        let scale = unsafe { &(*rotation_deformer_ptr.add(*ind as usize)).scale };
                        frame_data.deformer_scale_data[root.broad_index as usize] = *scale;
                    }
                    node::NodeKind::WarpDeformer(_, _) => {
                        frame_data.deformer_scale_data[root.broad_index as usize] = 1.0;
                    }
                    node::NodeKind::ArtMesh(_) => {}
                }
            }
            for child_id in root_id.descendants(&self.nodes).skip(1) {
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
                        let vec_data = &mut *warp_deformer_ptr.add(*ind as usize);
                        (
                            vec_data.as_mut_slice(),
                            &mut *warp_deformer_opacity_ptr.add(*ind as usize),
                            &mut *warp_deformer_color_ptr.add(*ind as usize),
                        )
                    },
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::RotationDeformer(_, ind) => unsafe {
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

                let child_angle = if let node::NodeKind::RotationDeformer(_, ind) = &child.data {
                    let child_angle =
                        unsafe { &mut (*rotation_deformer_ptr.add(*ind as usize)).angle };
                    let scale = unsafe { &(*rotation_deformer_ptr.add(*ind as usize)).scale };
                    frame_data.deformer_scale_data[child.broad_index as usize] = *scale;
                    Some(child_angle)
                } else {
                    None
                };

                // Apply the parent deformer to the child deformer or underlying art mesh.
                let (parent_opacity, parent_color) = match &parent.data {
                    node::NodeKind::ArtMesh(_) => {
                        unreachable!("art mesh should not have children")
                    }
                    node::NodeKind::WarpDeformer(data, ind) => {
                        // Safety: We ensure above that we will not have overlapping references.
                        let grid = unsafe { &*warp_deformer_ptr.add(*ind as usize) };

                        let transform = |p| {
                            let mut ret = p;
                            apply_warp_deformer(
                                grid,
                                data.is_new_deformerr,
                                data.rows as usize,
                                data.columns as usize,
                                slice::from_mut(&mut ret),
                            );
                            ret
                        };

                        // If the child is a rotation deformer, we need to fix up the angle.
                        if let Some(child_angle) = child_angle {
                            let angle_diff =
                                calculate_rotation_deformer_angle(child_changes[0], 0.1, transform);

                            *child_angle += angle_diff;
                            child_changes[0] = transform(child_changes[0]);
                        } else {
                            apply_warp_deformer(
                                grid,
                                data.is_new_deformerr,
                                data.rows as usize,
                                data.columns as usize,
                                child_changes,
                            );
                        }

                        // Safety: we guarantee above this will not overlap
                        (
                            unsafe { *warp_deformer_opacity_ptr.add(*ind as usize) },
                            unsafe { *warp_deformer_color_ptr.add(*ind as usize) },
                        )
                    }
                    node::NodeKind::RotationDeformer(data, ind) => {
                        let transform_data = unsafe { &*rotation_deformer_ptr.add(*ind as usize) };
                        let new_transform_data = transform_data.with_scale(
                            frame_data.deformer_scale_data[parent.broad_index as usize],
                        );

                        // If the child is a rotation deformer, we need to fix up the angle.
                        if let Some(child_angle) = child_angle {
                            let transform = |p| {
                                let mut ret = p;
                                apply_rotation_deformer(
                                    &new_transform_data,
                                    data.base_angle,
                                    slice::from_mut(&mut ret),
                                );
                                ret
                            };

                            let angle_diff = calculate_rotation_deformer_angle(
                                child_changes[0],
                                10.0,
                                transform,
                            );

                            *child_angle += angle_diff;
                            child_changes[0] = transform(child_changes[0]);
                        } else {
                            apply_rotation_deformer(
                                &new_transform_data,
                                data.base_angle,
                                child_changes,
                            );
                        }

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

                match &child.data {
                    // We don't need to fix scale for artmeshes.
                    node::NodeKind::ArtMesh(_) => {}
                    node::NodeKind::WarpDeformer(_, _) => {
                        frame_data.deformer_scale_data[child.broad_index as usize] =
                            frame_data.deformer_scale_data[parent.broad_index as usize];
                    }
                    node::NodeKind::RotationDeformer(_, _) => {
                        frame_data.deformer_scale_data[child.broad_index as usize] *=
                            frame_data.deformer_scale_data[parent.broad_index as usize];
                    }
                };
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

pub fn puppet_from_moc3(read: &Moc3Data) -> Puppet {
    let art_meshes = &read.table.art_meshes;
    let parameters = &read.table.parameters;
    let keyform_bindings = &read.table.keyform_bindings;
    let positions = read.positions();

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
                        warp_deformer_keyforms_v402.keyform_color_sources_start[specific] as usize;

                    collect_colors_to_bind(read, colors_start, count)
                } else {
                    Vec::new()
                };

            let parameter_bindings_count =
                keyform_bindings.parameter_binding_index_sources_counts[binding_index] as usize;
            let parameter_bindings_start: usize =
                keyform_bindings.parameter_binding_index_sources_starts[binding_index] as usize;

            applicators.push(ParamApplicator {
                kind_index: deformers.specific_sources_indices[i],
                values: ApplicatorKind::WarpDeformer(
                    positions_to_bind,
                    opacities_to_bind,
                    colors_to_bind,
                ),
                data: collect_parameter_bindings(
                    read,
                    &parameter_bindings_to_parameter,
                    parameter_bindings_start,
                    parameter_bindings_count,
                ),
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
                    let colors_start = rotation_deformer_keyforms_v402.keyform_color_sources_start
                        [specific] as usize;

                    collect_colors_to_bind(read, colors_start, count)
                } else {
                    Vec::new()
                };

            let parameter_bindings_count =
                keyform_bindings.parameter_binding_index_sources_counts[binding_index] as usize;
            let parameter_bindings_start: usize =
                keyform_bindings.parameter_binding_index_sources_starts[binding_index] as usize;

            applicators.push(ParamApplicator {
                kind_index: deformers.specific_sources_indices[i],
                values: ApplicatorKind::RotationDeformer(
                    positions_to_bind,
                    opacities_to_bind,
                    colors_to_bind,
                ),
                data: collect_parameter_bindings(
                    read,
                    &parameter_bindings_to_parameter,
                    parameter_bindings_start,
                    parameter_bindings_count,
                ),
                blend: None,
            });
        }
    }

    let uvs = read.uvs();
    let vertex_indices = read.vertex_indices();
    let mut art_mesh_uvs = Vec::with_capacity(read.table.count_info.art_meshes as usize);
    let mut art_mesh_indices = Vec::with_capacity(read.table.count_info.art_meshes as usize);
    let mut art_mesh_mask_indices = Vec::with_capacity(read.table.count_info.art_meshes as usize);

    let art_mesh_keyforms = &read.table.art_mesh_keyforms;
    let art_mesh_deformer_keyforms_v402 = read.table.art_mesh_deformer_keyforms_v402.as_ref();
    let art_mesh_masks = &read.table.art_mesh_masks;

    for i in 0..read.table.count_info.art_meshes {
        let i = i as usize;
        let uv_start = art_meshes.uv_sources_starts[i] as usize / 2;
        let vertexes = art_meshes.vertex_counts[i] as usize;
        let index_start = art_meshes.vertex_index_sources_starts[i] as usize;
        let index_count = art_meshes.vertex_index_sources_counts[i] as usize;
        art_mesh_uvs.push(uvs[uv_start..uv_start + vertexes].to_vec());
        art_mesh_indices.push(vertex_indices[index_start..index_start + index_count].to_vec());

        let mask_start = art_meshes.art_mesh_mask_sources_starts[i] as usize;
        let mask_count = art_meshes.art_mesh_mask_sources_counts[i] as usize;
        art_mesh_mask_indices.push(
            art_mesh_masks.art_mesh_source_indices[mask_start..mask_start + mask_count].to_owned(),
        );

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

        applicators.push(ParamApplicator {
            kind_index: i as u32,
            values: ApplicatorKind::ArtMesh(
                positions_to_bind,
                opacities_to_bind,
                draw_orders_to_bind,
                colors_to_bind,
            ),
            data: collect_parameter_bindings(
                read,
                &parameter_bindings_to_parameter,
                parameter_bindings_start,
                parameter_bindings_count,
            ),
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

        applicators.push(ParamApplicator {
            kind_index: i as u32,
            values: ApplicatorKind::Glue(intensities_to_bind),
            data: collect_parameter_bindings(
                read,
                &parameter_bindings_to_parameter,
                parameter_bindings_start,
                parameter_bindings_count,
            ),
            blend: None,
        });
    }
    // ----- END PARAMETER STUFF -----
    collect_blend_shapes(
        read,
        &blend_shape_parameter_bindings_to_parameter,
        &mut applicators,
    );

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

    let mut warp_deformer_grid_count = Vec::new();
    for i in 0..read.table.count_info.warp_deformers as usize {
        // Fencepost error warning: rows and columns measure the user-visbile middle, not the edges
        // containg the numbers.
        warp_deformer_grid_count
            .push((warp_deformers.rows[i] + 1) * (warp_deformers.columns[i] + 1));
    }

    let params = collect_param_data(read);

    Puppet {
        node_roots,
        nodes: node_arena,
        glue_nodes,

        params,
        applicators,

        art_mesh_count: read.table.count_info.art_meshes,
        warp_deformer_count: read.table.count_info.warp_deformers,
        rotation_deformer_count: read.table.count_info.rotation_deformers,
        glue_count: read.table.count_info.glues,

        warp_deformer_grid_count,

        art_mesh_uvs,
        art_mesh_indices,
        art_mesh_textures: read.table.art_meshes.texture_nums.clone(),
        art_mesh_flags: read.table.art_meshes.art_mesh_flags.clone(),
        art_mesh_mask_indices,
        art_mesh_vertexes: read.table.art_meshes.vertex_counts.clone(),

        draw_order_nodes,
        draw_order_roots: draw_order_roots.into_iter().map(|x| x.unwrap()).collect(),
        max_draw_order_children,
    }
}

pub fn framedata_for_puppet(puppet: &Puppet) -> PuppetFrameData {
    let mut warp_deformer_data = Vec::new();
    for count in &puppet.warp_deformer_grid_count {
        warp_deformer_data.push(vec![Vec2::NAN; *count as usize]);
    }

    let mut art_mesh_data = Vec::new();
    for count in &puppet.art_mesh_vertexes {
        art_mesh_data.push(vec![Vec2::NAN; *count as usize]);
    }

    PuppetFrameData {
        corrected_params: puppet.params.defaults.clone(),

        art_mesh_draw_orders: vec![0.0; puppet.art_mesh_count as usize],
        art_mesh_render_orders: vec![0; puppet.art_mesh_count as usize],

        art_mesh_data,
        art_mesh_opacities: vec![0.0; puppet.art_mesh_count as usize],
        art_mesh_colors: vec![BlendColor::NAN; puppet.art_mesh_count as usize],

        warp_deformer_data,
        rotation_deformer_data: vec![TransformData::NAN; puppet.rotation_deformer_count as usize],
        warp_deformer_opacities: vec![f32::NAN; puppet.warp_deformer_count as usize],
        rotation_deformer_opacities: vec![f32::NAN; puppet.rotation_deformer_count as usize],
        warp_deformer_colors: vec![BlendColor::NAN; puppet.warp_deformer_count as usize],
        rotation_deformer_colors: vec![BlendColor::NAN; puppet.rotation_deformer_count as usize],

        deformer_scale_data: vec![
            f32::NAN;
            puppet.warp_deformer_count as usize
                + puppet.rotation_deformer_count as usize
        ],
        glue_data: vec![f32::NAN; puppet.glue_count as usize],
    }
}
