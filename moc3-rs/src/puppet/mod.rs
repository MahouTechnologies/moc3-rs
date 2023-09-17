mod applicator;
mod draw_order;
mod node;

use std::{mem::discriminant, slice};

use glam::{vec2, Vec2};
use indextree::{Arena, NodeId};

use crate::{
    data::{DrawOrderGroupObjectType, Moc3Data},
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
    draw_order::{draw_order_tree, DrawOrderNode},
    node::{DeformerNode, GlueNode},
};

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
pub struct Puppet {
    pub node_roots: Vec<NodeId>,
    pub nodes: Arena<DeformerNode>,
    pub glue_nodes: Vec<GlueNode>,

    pub params: Vec<f32>,
    pub applicators: Vec<ParamApplicator>,

    pub art_mesh_count: u32,
    pub art_mesh_uvs: Vec<Vec<Vec2>>,
    pub art_mesh_indices: Vec<Vec<u16>>,
    pub vertexes_count: Vec<u32>,

    pub draw_order_nodes: Arena<DrawOrderNode>,
    pub draw_order_roots: Vec<NodeId>,
    pub max_draw_order_children: u32,
}

#[derive(Debug, Clone)]
pub struct PuppetFrameData {
    pub art_mesh_data: Vec<Vec<Vec2>>,
    pub art_mesh_draw_orders: Vec<f32>,
    pub art_mesh_render_orders: Vec<u32>,
    pub art_mesh_opacities: Vec<f32>,

    pub warp_deformer_data: Vec<Vec<Vec2>>,
    pub rotation_deformer_data: Vec<TransformData>,
    pub deformer_scale_data: Vec<f32>,
    pub warp_deformer_opacities: Vec<f32>,
    pub rotation_deformer_opacities: Vec<f32>,
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

                let (child_changes, child_opacity) = match &child.data {
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::ArtMesh(_) => unsafe {
                        let vec_data = &mut *art_mesh_ptr.add(child.broad_index as usize);
                        (
                            vec_data.as_mut_slice(),
                            &mut *art_mesh_opacity_ptr.add(child.broad_index as usize),
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
                        )
                    },
                };

                // Apply the parent deformer to the child deformer or underlying art mesh.
                let parent_opacity = match &parent.data {
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
                        unsafe { *warp_deformer_opacity_ptr.add(*ind as usize) }
                    }
                    node::NodeKind::RotationDeformer(data, ind) => {
                        let transform = unsafe { &*rotation_deformer_ptr.add(*ind as usize) };
                        apply_rotation_deformer(transform, data.base_angle, child_changes);

                        // Safety: we guarantee above this will not overlap
                        unsafe { *rotation_deformer_opacity_ptr.add(*ind as usize) }
                    }
                };

                // Propogate down the opacity numbers
                *child_opacity *= parent_opacity;
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
    let art_mesh_keyforms = &read.table.art_mesh_keyforms;
    let parameters = &read.table.parameters;
    let parameter_bindings = &read.table.parameter_bindings;
    let parameter_binding_indices = &read.table.parameter_binding_indices;
    let keyform_bindings = &read.table.keyform_bindings;
    let positions = read.positions();
    let keys = read.keys();

    let mut parameter_bindings_to_parameter =
        vec![0usize; read.table.count_info.parameter_bindings as usize];
    for i in 0..read.table.count_info.parameters {
        let i = i as usize;

        let start = parameters.parameter_binding_sources_starts[i] as usize;
        let count = parameters.parameter_binding_sources_counts[i] as usize;

        for a in start..(start + count) {
            parameter_bindings_to_parameter[a] = i;
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
    let rotation_deformers = &read.table.rotation_deformers;
    let rotation_deformer_keyforms = &read.table.rotation_deformer_keyforms;

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
                values: ApplicatorKind::WarpDeformer(positions_to_bind, opacities_to_bind),
                x: x_index,
                y: y_index,
                z: z_index,
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
                values: ApplicatorKind::RotationDeformer(positions_to_bind, opacities_to_bind),
                x: x_index,
                y: y_index,
                z: z_index,
            });
        }
    }

    let uvs = read.uvs();
    let vertex_indices = read.vertex_indices();
    let mut art_mesh_uvs = Vec::with_capacity(read.table.count_info.art_meshes as usize);
    let mut art_mesh_indices = Vec::with_capacity(read.table.count_info.art_meshes as usize);
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
            values: ApplicatorKind::ArtMesh(
                positions_to_bind,
                opacities_to_bind,
                draw_orders_to_bind,
            ),
            x: x_index,
            y: y_index,
            z: z_index,
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
        });
    }

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
        art_mesh_uvs,
        vertexes_count: read.table.art_meshes.vertex_counts.clone(),

        draw_order_nodes,
        draw_order_roots: draw_order_roots.into_iter().map(|x| x.unwrap()).collect(),
        max_draw_order_children: 222,
    }
}
