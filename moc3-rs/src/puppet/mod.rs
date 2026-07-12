mod applicator;
mod collect;
mod draw_order;
mod node;

use std::{mem::discriminant, slice};

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, vec2};

use crate::{
    data::{ArtMeshFlags, DrawOrderGroupObjectType, Moc3, ParameterType},
    deformer::{
        glue::apply_glue,
        rotation_deformer::{apply_rotation_deformer, calculate_rotation_deformer_angle},
        warp_deformer::apply_warp_deformer,
    },
};

use self::{
    applicator::key_table_is_outside,
    collect::{
        collect_blend_shapes, collect_colors_to_bind, collect_param_data,
        collect_parameter_bindings,
    },
    draw_order::sort_render_order,
};

pub use crate::deformer::rotation_deformer::TransformData;
pub use applicator::{ApplicatorKind, BlendShapeConstraints, ParamApplicator};
pub use draw_order::{DrawGroup, DrawObject};
pub use indextree::{Arena, NodeId};
pub use node::{
    ArtMeshData, DeformerNode, GlueNode, NodeKind, PartNode, RotationDeformerData, WarpDeformerData,
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
    /// How close a value must be to a key to count as sitting on it.
    pub snap_epsilons: Vec<f32>,
}

impl ParamData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        count: u32,
        ids: Vec<String>,
        defaults: Vec<f32>,
        maxes: Vec<f32>,
        mins: Vec<f32>,
        repeats: Vec<bool>,
        decimals: Vec<u32>,
        types: Vec<ParameterType>,
    ) -> Self {
        let snap_epsilons = decimals.iter().map(|&d| 0.1f32.powi(d as i32)).collect();
        Self {
            count,
            ids,
            defaults,
            maxes,
            mins,
            repeats,
            decimals,
            types,
            snap_epsilons,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObjectTable {
    /// The part an object sits in, or `-1`.
    pub parent_part: Vec<i32>,
    /// The deformer an object sits under, or `-1`. Empty for parts.
    pub parent_deformer: Vec<i32>,
    /// The keyform binding whose parameters drive this object.
    pub binding: Vec<u32>,
    pub is_enabled: Vec<bool>,
}

#[derive(Debug, Clone)]
pub struct Puppet {
    node_roots: Vec<NodeId>,
    nodes: Arena<DeformerNode>,

    glue_nodes: Vec<GlueNode>,

    part_roots: Vec<NodeId>,
    parts: Arena<PartNode>,

    params: ParamData,
    applicators: Vec<ParamApplicator>,

    part_table: ObjectTable,
    deformer_table: ObjectTable,
    art_mesh_table: ObjectTable,
    /// Whether each part was authored visible, which seeds its opacity.
    part_is_visible: Vec<bool>,
    /// The key tables of each keyform binding, by binding index. Shared by every
    /// object bound to it, so a binding's range is tested once per frame.
    binding_key_tables: Vec<Vec<(Vec<f32>, usize)>>,

    pub art_mesh_count: u32,
    warp_deformer_count: u32,
    rotation_deformer_count: u32,
    pub part_count: u32,
    glue_count: u32,

    warp_deformer_grid_count: Vec<u32>,

    pub art_mesh_uvs: Vec<Vec<Vec2>>,
    pub art_mesh_indices: Vec<Vec<u16>>,
    pub art_mesh_textures: Vec<u32>,
    pub art_mesh_flags: Vec<ArtMeshFlags>,
    pub art_mesh_mask_indices: Vec<Vec<u32>>,
    pub art_mesh_vertexes: Vec<u32>,

    draw_groups: Vec<DrawGroup>,
    draw_objects: Vec<DrawObject>,
}

pub struct PuppetParts {
    pub node_roots: Vec<NodeId>,
    pub nodes: Arena<DeformerNode>,

    pub glue_nodes: Vec<GlueNode>,

    pub part_roots: Vec<NodeId>,
    pub parts: Arena<PartNode>,

    pub params: ParamData,
    pub applicators: Vec<ParamApplicator>,

    pub part_table: ObjectTable,
    pub deformer_table: ObjectTable,
    pub art_mesh_table: ObjectTable,
    pub part_is_visible: Vec<bool>,
    pub binding_key_tables: Vec<Vec<(Vec<f32>, usize)>>,

    pub art_mesh_count: u32,
    pub warp_deformer_count: u32,
    pub rotation_deformer_count: u32,
    pub part_count: u32,
    pub glue_count: u32,

    pub warp_deformer_grid_count: Vec<u32>,

    pub art_mesh_uvs: Vec<Vec<Vec2>>,
    pub art_mesh_indices: Vec<Vec<u16>>,
    pub art_mesh_textures: Vec<u32>,
    pub art_mesh_flags: Vec<ArtMeshFlags>,
    pub art_mesh_mask_indices: Vec<Vec<u32>>,
    pub art_mesh_vertexes: Vec<u32>,

    pub draw_groups: Vec<DrawGroup>,
    pub draw_objects: Vec<DrawObject>,
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

    /// Per-part opacity.
    pub part_opacities: Vec<f32>,
    /// Each part's opacity with its ancestors.
    pub calculated_part_opacities: Vec<f32>,

    /// Whether each object is being drawn at all.
    pub part_enabled: Vec<bool>,
    pub deformer_enabled: Vec<bool>,
    pub art_mesh_enabled: Vec<bool>,
    /// Whether each keyform binding's parameters have left its key range.
    binding_out_of_range: Vec<bool>,

    art_mesh_draw_orders: Vec<f32>,
    part_draw_orders: Vec<f32>,

    /// The art meshes to draw, back to front. Invisible ones are left out.
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

    // Scratch for `sort_render_order`.
    draw_object_orders: Vec<i32>,
    draw_group_cursors: Vec<u32>,
    bucket_first: Vec<i32>,
    bucket_last: Vec<i32>,
    bucket_next: Vec<i32>,
    render_sequence: Vec<u32>,
}

impl PuppetFrameData {
    pub fn warp_deformer_grid(&self, warp_specific_index: usize) -> &[Vec2] {
        &self.warp_deformer_data[warp_specific_index]
    }

    pub fn rotation_deformer_transform(&self, rotation_specific_index: usize) -> TransformData {
        self.rotation_deformer_data[rotation_specific_index]
    }

    /// The accumulated scale a deformer applies to its children - the
    /// product of the deformer's own scale (1 for warp deformers) with every
    /// ancestor's.
    pub fn deformer_scale(&self, broad_index: usize) -> f32 {
        self.deformer_scale_data[broad_index]
    }

    /// Applies `deformer`'s current deformation to `points`, given in the
    /// deformer's local space (for warp deformers, the normalized space
    /// where `(0,0)..(1,1)` spans the grid).
    pub fn map_through_deformer(&self, deformer: &DeformerNode, points: &mut [Vec2]) {
        match &deformer.data {
            NodeKind::ArtMesh(_) => {}
            NodeKind::WarpDeformer(data, ind) => {
                apply_warp_deformer(
                    &self.warp_deformer_data[*ind as usize],
                    data.is_new_deformerr,
                    data.rows as usize,
                    data.columns as usize,
                    points,
                );
            }
            NodeKind::RotationDeformer(data, ind) => {
                let transform_data = self.rotation_deformer_data[*ind as usize]
                    .with_scale(self.deformer_scale_data[deformer.broad_index as usize]);
                apply_rotation_deformer(&transform_data, data.base_angle, points);
            }
        }
    }
}

/// One node in a flattened, depth-first walk of a [`Puppet`] . Roots have `depth` 0.
pub struct TreeNode<'a, T> {
    pub depth: usize,
    pub node: &'a T,
}

fn walk<'a, T>(arena: &'a Arena<T>, id: NodeId, depth: usize, out: &mut Vec<TreeNode<'a, T>>) {
    let mut stack = vec![(id, depth)];
    while let Some((id, depth)) = stack.pop() {
        out.push(TreeNode {
            depth,
            node: arena[id].get(),
        });
        // Push in reverse so children are popped (and thus visited) in original order.
        stack.extend(id.children(arena).rev().map(|child| (child, depth + 1)));
    }
}

impl Puppet {
    /// Assembles a `Puppet` from already-computed parts.
    pub fn from_parts(parts: PuppetParts) -> Self {
        Puppet {
            node_roots: parts.node_roots,
            nodes: parts.nodes,

            glue_nodes: parts.glue_nodes,

            part_roots: parts.part_roots,
            parts: parts.parts,

            params: parts.params,
            applicators: parts.applicators,

            part_table: parts.part_table,
            deformer_table: parts.deformer_table,
            art_mesh_table: parts.art_mesh_table,
            part_is_visible: parts.part_is_visible,
            binding_key_tables: parts.binding_key_tables,

            art_mesh_count: parts.art_mesh_count,
            warp_deformer_count: parts.warp_deformer_count,
            rotation_deformer_count: parts.rotation_deformer_count,
            part_count: parts.part_count,
            glue_count: parts.glue_count,

            warp_deformer_grid_count: parts.warp_deformer_grid_count,

            art_mesh_uvs: parts.art_mesh_uvs,
            art_mesh_indices: parts.art_mesh_indices,
            art_mesh_textures: parts.art_mesh_textures,
            art_mesh_flags: parts.art_mesh_flags,
            art_mesh_mask_indices: parts.art_mesh_mask_indices,
            art_mesh_vertexes: parts.art_mesh_vertexes,

            draw_groups: parts.draw_groups,
            draw_objects: parts.draw_objects,
        }
    }

    pub fn param_data(&self) -> &ParamData {
        &self.params
    }

    /// The part each art mesh belongs to, or `-1`. Indexed by art mesh.
    pub fn art_mesh_parent_parts(&self) -> &[i32] {
        &self.art_mesh_table.parent_part
    }

    /// The part each part sits inside, or `-1`. Indexed by part.
    pub fn part_parent_parts(&self) -> &[i32] {
        &self.part_table.parent_part
    }

    /// Depth-first flattened view of the deformer tree.
    pub fn deformer_tree(&self) -> Vec<TreeNode<'_, DeformerNode>> {
        let mut out = Vec::new();
        for &root in &self.node_roots {
            walk(&self.nodes, root, 0, &mut out);
        }
        out
    }

    /// Depth-first flattened view of the Parts tree.
    pub fn part_tree(&self) -> Vec<TreeNode<'_, PartNode>> {
        let mut out = Vec::new();
        for &root in &self.part_roots {
            walk(&self.parts, root, 0, &mut out);
        }
        out
    }

    /// Glue relationships (mesh-to-mesh stitching), not part of either tree.
    pub fn glues(&self) -> &[GlueNode] {
        &self.glue_nodes
    }

    /// Whether the parameters driving each keyform binding still sit inside the
    /// range its keys cover. Once one leaves, everything bound to it is disabled.
    fn resolve_bindings(&self, frame_data: &mut PuppetFrameData) {
        for (binding, key_tables) in self.binding_key_tables.iter().enumerate() {
            frame_data.binding_out_of_range[binding] =
                key_tables.iter().any(|(keys, parameter)| {
                    key_table_is_outside(
                        keys,
                        frame_data.corrected_params[*parameter],
                        self.params.snap_epsilons[*parameter],
                    )
                });
        }
    }

    /// Work out all of the parts, deformers, and artmeshes that are enabled.
    fn resolve_enabled(&self, frame_data: &mut PuppetFrameData) {
        let out_of_range = &frame_data.binding_out_of_range;

        let parts = &self.part_table;
        for i in 0..self.part_count as usize {
            let parent = parts.parent_part[i];
            frame_data.part_enabled[i] = parts.is_enabled[i]
                && !out_of_range[parts.binding[i] as usize]
                && (parent == -1 || frame_data.part_enabled[parent as usize]);
        }

        let deformers = &self.deformer_table;
        for i in 0..deformers.is_enabled.len() {
            let part = deformers.parent_part[i];
            let deformer = deformers.parent_deformer[i];
            frame_data.deformer_enabled[i] = deformers.is_enabled[i]
                && !out_of_range[deformers.binding[i] as usize]
                && (part == -1 || frame_data.part_enabled[part as usize])
                && (deformer == -1 || frame_data.deformer_enabled[deformer as usize]);
        }

        let art_meshes = &self.art_mesh_table;
        for i in 0..self.art_mesh_count as usize {
            let part = art_meshes.parent_part[i];
            let deformer = art_meshes.parent_deformer[i];
            frame_data.art_mesh_enabled[i] = art_meshes.is_enabled[i]
                && !out_of_range[art_meshes.binding[i] as usize]
                && (part == -1 || frame_data.part_enabled[part as usize])
                && (deformer == -1 || frame_data.deformer_enabled[deformer as usize]);
        }
    }

    pub fn update(&self, input_params: &[f32], frame_data: &mut PuppetFrameData) {
        for i in 0..input_params.len() {
            let res = input_params[i].clamp(self.params.mins[i], self.params.maxes[i]);
            frame_data.corrected_params[i] = res;
        }

        self.resolve_bindings(frame_data);
        self.resolve_enabled(frame_data);

        // A part's opacity is its own multiplied by everything it sits inside.
        for i in 0..self.part_count as usize {
            let opacity = frame_data.part_opacities[i].clamp(0.0, 1.0);
            frame_data.part_opacities[i] = opacity;

            let parent = self.part_table.parent_part[i];
            frame_data.calculated_part_opacities[i] = if parent == -1 {
                opacity
            } else {
                opacity * frame_data.calculated_part_opacities[parent as usize]
            };
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
                // TODO: Don't do this on every update.
                assert_ne!(
                    (discriminant(&child.data), child.broad_index),
                    (discriminant(&parent.data), parent.broad_index),
                );

                let (child_changes, child_opacity, child_color) = match &child.data {
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::ArtMesh(_) => unsafe {
                        let broad_index = child.broad_index as usize;
                        debug_assert!(broad_index < frame_data.art_mesh_data.len());
                        debug_assert!(broad_index < frame_data.art_mesh_opacities.len());
                        debug_assert!(broad_index < frame_data.art_mesh_colors.len());

                        let vec_data = &mut *art_mesh_ptr.add(broad_index);
                        (
                            vec_data.as_mut_slice(),
                            &mut *art_mesh_opacity_ptr.add(broad_index),
                            &mut *art_mesh_color_ptr.add(broad_index),
                        )
                    },
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::WarpDeformer(_, ind) => unsafe {
                        let ind = *ind as usize;
                        debug_assert!(ind < frame_data.warp_deformer_data.len());
                        debug_assert!(ind < frame_data.warp_deformer_opacities.len());
                        debug_assert!(ind < frame_data.warp_deformer_colors.len());

                        let vec_data = &mut *warp_deformer_ptr.add(ind);
                        (
                            vec_data.as_mut_slice(),
                            &mut *warp_deformer_opacity_ptr.add(ind),
                            &mut *warp_deformer_color_ptr.add(ind),
                        )
                    },
                    // Safety: We ensure above that we will not have overlapping references.
                    node::NodeKind::RotationDeformer(_, ind) => unsafe {
                        let ind = *ind as usize;
                        debug_assert!(ind < frame_data.rotation_deformer_data.len());
                        debug_assert!(ind < frame_data.rotation_deformer_opacities.len());
                        debug_assert!(ind < frame_data.rotation_deformer_colors.len());

                        let slice_data =
                            slice::from_mut(&mut (*rotation_deformer_ptr.add(ind)).origin);

                        (
                            slice_data,
                            &mut *rotation_deformer_opacity_ptr.add(ind),
                            &mut *rotation_deformer_color_ptr.add(ind),
                        )
                    },
                };

                let child_angle = if let node::NodeKind::RotationDeformer(_, ind) = &child.data {
                    let ind = *ind as usize;
                    debug_assert!(ind < frame_data.rotation_deformer_data.len());

                    // SAFETY: We check that ind is in-bounds in the parsing step.
                    let child_deformer_ptr = unsafe { rotation_deformer_ptr.add(ind) };
                    let child_angle = unsafe { &mut (*child_deformer_ptr).angle };
                    let scale = unsafe { &(*child_deformer_ptr).scale };
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

                // Propogate down the opacities.
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

        // An art mesh also takes the opacity from the part it belongs to.
        for i in 0..self.art_mesh_count as usize {
            let part = self.art_mesh_table.parent_part[i];
            if part != -1 {
                frame_data.art_mesh_opacities[i] *=
                    frame_data.calculated_part_opacities[part as usize];
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

        sort_render_order(&self.draw_groups, &self.draw_objects, frame_data);
    }
}

pub fn puppet_from_moc3(read: Moc3<'_>) -> Puppet {
    let positions = read.positions();
    let kb_starts = read.keyform_binding_parameter_binding_index_sources_starts();
    let kb_counts = read.keyform_binding_parameter_binding_index_sources_counts();

    // We store our data in a slightly different way than how it was intended, so we
    // need this map of parameter binding index back up to the parameter itself. This is
    // the parameter binding pulling the data, instead of the parameter pushing the data to
    // all of the bindings. Which is better or worse for performance / code I'm not sure.
    let mut parameter_bindings_to_parameter =
        vec![0usize; read.counts().parameter_bindings() as usize];
    let mut blend_shape_parameter_bindings_to_parameter =
        vec![0usize; read.counts().blend_shape_parameter_bindings() as usize];
    for i in 0..read.counts().parameters() {
        let i = i as usize;

        let start = read.parameter_binding_sources_starts()[i] as usize;
        let count = read.parameter_binding_sources_counts()[i] as usize;

        for a in start..(start + count) {
            parameter_bindings_to_parameter[a] = i;
        }

        // I think this works, as the way the format should not have regular
        // parameter bindings for blend shape parameters.
        if let Some(parameter_types) = read.parameter_types() {
            if parameter_types[i] == ParameterType::BlendShape as u32 {
                let start =
                    read.parameter_blend_shape_binding_sources_starts().unwrap()[i] as usize;
                let count =
                    read.parameter_blend_shape_binding_sources_counts().unwrap()[i] as usize;

                for a in start..(start + count) {
                    blend_shape_parameter_bindings_to_parameter[a] = i;
                }
            }
        }
    }

    // Everything down from here, delimited by horizontal ASCII lines, represents all of the possible
    // things that can affect the final model directly. This includes deformers (both rotation and warp),
    // art mesh deformation, as well as glues. Here, the ParamApplicators for regular parameters as well as
    // blendshapes occurs.

    // ----- BEGIN PARAMETER STUFF -----

    let mut applicators = Vec::new();

    let mut node_roots: Vec<NodeId> = Vec::new();
    let mut node_arena = Arena::<DeformerNode>::with_capacity(
        (read.counts().art_meshes()
            + read.counts().warp_deformers()
            + read.counts().rotation_deformers()) as usize,
    );

    let mut deformer_indices_to_node_ids: Vec<Option<NodeId>> =
        vec![None; read.counts().deformers() as usize];

    for i in 0..read.counts().deformers() {
        let i: usize = i as usize;
        let specific = read.deformer_specific_sources_indices()[i] as usize;

        let parent_deformer_index = read.deformer_parent_deformer_indices()[i];
        if read.deformer_types()[i] == 0 {
            let vertexes = read.warp_deformer_vertex_counts()[specific] as usize;

            let is_new_deformerr = read
                .warp_deformer_is_new_deformer()
                .map(|x| x[specific])
                .unwrap_or(0);

            {
                let node_to_append = DeformerNode {
                    id: read.deformer_ids()[i].name().to_string(),
                    broad_index: i as u32,
                    parent_part_index: read.deformer_parent_part_indices()[i],
                    is_enabled: read.deformer_is_enabled()[i] != 0,
                    data: node::NodeKind::WarpDeformer(
                        WarpDeformerData {
                            rows: read.warp_deformer_rows()[specific],
                            columns: read.warp_deformer_columns()[specific],
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

            let binding_index =
                read.warp_deformer_keyform_binding_sources_indices()[specific] as usize;
            let start = read.warp_deformer_keyform_sources_starts()[specific] as usize;
            let count = read.warp_deformer_keyform_sources_counts()[specific] as usize;

            let mut positions_to_bind = Vec::new();
            for i in start..start + count {
                let position_start =
                    read.warp_deformer_keyform_position_sources_starts()[i] as usize / 2;
                positions_to_bind
                    .push(positions[position_start..position_start + vertexes].to_owned());
            }
            let opacities_to_bind =
                read.warp_deformer_keyform_opacities()[start..start + count].to_vec();
            let colors_to_bind =
                if let Some(color_starts) = read.warp_deformer_keyform_color_sources_start() {
                    let colors_start = color_starts[specific] as usize;

                    collect_colors_to_bind(read, colors_start, count)
                } else {
                    Vec::new()
                };

            let parameter_bindings_count = kb_counts[binding_index] as usize;
            let parameter_bindings_start: usize = kb_starts[binding_index] as usize;

            applicators.push(ParamApplicator {
                kind_index: read.deformer_specific_sources_indices()[i],
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
        } else if read.deformer_types()[i] == 1 {
            let base_angle = read.rotation_deformer_base_angles()[specific];

            {
                let node_to_append = DeformerNode {
                    id: read.deformer_ids()[i].name().to_string(),
                    broad_index: i as u32,
                    parent_part_index: read.deformer_parent_part_indices()[i],
                    is_enabled: read.deformer_is_enabled()[i] != 0,
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
                read.rotation_deformer_keyform_binding_sources_indices()[specific] as usize;
            let start = read.rotation_deformer_keyform_sources_starts()[specific] as usize;
            let count = read.rotation_deformer_keyform_sources_counts()[specific] as usize;

            let mut positions_to_bind = Vec::new();
            for i in start..start + count {
                let x_origin = read.rotation_deformer_keyform_x_origin()[i];
                let y_origin = read.rotation_deformer_keyform_y_origin()[i];
                let scale = read.rotation_deformer_keyform_scales()[i];
                let angle = read.rotation_deformer_keyform_angles()[i];
                positions_to_bind.push(TransformData {
                    origin: vec2(x_origin, y_origin),
                    scale,
                    angle,
                });
            }
            let opacities_to_bind =
                read.rotation_deformer_keyform_opacities()[start..start + count].to_vec();
            let colors_to_bind =
                if let Some(color_starts) = read.rotation_deformer_keyform_color_sources_start() {
                    let colors_start = color_starts[specific] as usize;

                    collect_colors_to_bind(read, colors_start, count)
                } else {
                    Vec::new()
                };

            let parameter_bindings_count = kb_counts[binding_index] as usize;
            let parameter_bindings_start: usize = kb_starts[binding_index] as usize;

            applicators.push(ParamApplicator {
                kind_index: read.deformer_specific_sources_indices()[i],
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
    let mut art_mesh_uvs = Vec::with_capacity(read.counts().art_meshes() as usize);
    let mut art_mesh_indices = Vec::with_capacity(read.counts().art_meshes() as usize);
    let mut art_mesh_mask_indices = Vec::with_capacity(read.counts().art_meshes() as usize);

    for i in 0..read.counts().art_meshes() {
        let i = i as usize;
        let uv_start = read.art_mesh_uv_sources_starts()[i] as usize / 2;
        let vertexes = read.art_mesh_vertex_counts()[i] as usize;
        let index_start = read.art_mesh_vertex_index_sources_starts()[i] as usize;
        let index_count = read.art_mesh_vertex_index_sources_counts()[i] as usize;
        art_mesh_uvs.push(uvs[uv_start..uv_start + vertexes].to_vec());
        art_mesh_indices.push(vertex_indices[index_start..index_start + index_count].to_vec());

        let mask_start = read.art_mesh_mask_sources_starts()[i] as usize;
        let mask_count = read.art_mesh_mask_sources_counts()[i] as usize;
        art_mesh_mask_indices.push(
            read.art_mesh_mask_source_indices()[mask_start..mask_start + mask_count].to_owned(),
        );

        let binding_index = read.art_mesh_keyform_binding_sources_indices()[i] as usize;
        let start = read.art_mesh_keyform_sources_starts()[i] as usize;
        let count = read.art_mesh_keyform_sources_counts()[i] as usize;

        let mut positions_to_bind = Vec::new();
        for i in start..start + count {
            let position_start = read.art_mesh_keyform_position_sources_starts()[i] as usize / 2;
            positions_to_bind.push(positions[position_start..position_start + vertexes].to_owned());
        }
        let opacities_to_bind = read.art_mesh_keyform_opacities()[start..start + count].to_vec();
        let draw_orders_to_bind =
            read.art_mesh_keyform_draw_orders()[start..start + count].to_vec();
        let colors_to_bind = if let Some(color_starts) = read.art_mesh_keyform_color_sources_start()
        {
            let colors_start = color_starts[i] as usize;

            collect_colors_to_bind(read, colors_start, count)
        } else {
            Vec::new()
        };

        {
            let parent_deformer_index = read.art_mesh_parent_deformer_indices()[i];

            let node_to_append = DeformerNode {
                id: read.art_mesh_ids()[i].name().to_string(),
                broad_index: i as u32,
                parent_part_index: read.art_mesh_parent_part_indices()[i],
                is_enabled: read.art_mesh_is_enabled()[i] != 0,
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

        let parameter_bindings_count = kb_counts[binding_index] as usize;
        let parameter_bindings_start = kb_starts[binding_index] as usize;

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

    for i in 0..read.counts().glues() {
        let i = i as usize;

        let glue_info_start = read.glue_info_sources_starts()[i] as usize;
        let glue_info_count = read.glue_info_sources_counts()[i] as usize;

        let binding_index = read.glue_keyform_binding_sources_indices()[i] as usize;
        let start = read.glue_keyform_sources_starts()[i] as usize;
        let count = read.glue_keyform_sources_counts()[i] as usize;

        let mesh_indices =
            &read.glue_info_vertex_indices()[glue_info_start..glue_info_start + glue_info_count];
        let weights = &read.glue_info_weights()[glue_info_start..glue_info_start + glue_info_count];

        let intensities_to_bind = read.glue_keyform_intensities()[start..start + count].to_vec();

        glue_nodes.push(GlueNode {
            id: read.glue_ids()[i].name().to_string(),
            kind_index: i as u32,
            art_mesh_index: [
                read.glue_art_mesh_indices_a()[i],
                read.glue_art_mesh_indices_b()[i],
            ],
            mesh_indices: mesh_indices.to_vec(),
            weights: weights.to_vec(),
        });

        let parameter_bindings_count = kb_counts[binding_index] as usize;
        let parameter_bindings_start = kb_starts[binding_index] as usize;

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

    let mut part_roots: Vec<NodeId> = Vec::new();
    let mut part_arena = Arena::<PartNode>::with_capacity(read.counts().parts() as usize);

    let mut part_indices_to_node_ids: Vec<Option<NodeId>> =
        vec![None; read.counts().parts() as usize];

    for i in 0..read.counts().parts() {
        let i = i as usize;

        let binding_index = read.part_keyform_binding_sources_indices()[i] as usize;
        let start = read.part_keyform_sources_starts()[i] as usize;
        let count = read.part_keyform_sources_counts()[i] as usize;

        let draw_orders_to_bind = read.part_keyform_draw_orders()[start..start + count].to_vec();

        {
            let parent_part_index = read.part_parent_part_indices()[i];

            let node_to_append = PartNode {
                id: read.part_ids()[i].name().to_string(),
                kind_index: i as u32,
                is_enabled: read.part_is_enabled()[i] != 0,
                is_visible: read.part_is_visible()[i] != 0,
            };

            let res = if parent_part_index != -1 {
                part_indices_to_node_ids[parent_part_index as usize]
                    .unwrap()
                    .append_value(node_to_append, &mut part_arena)
            } else {
                let it: NodeId = part_arena.new_node(node_to_append);
                part_roots.push(it);
                it
            };

            part_indices_to_node_ids[i] = Some(res);
        }

        let parameter_bindings_count = kb_counts[binding_index] as usize;
        let parameter_bindings_start = kb_starts[binding_index] as usize;

        applicators.push(ParamApplicator {
            kind_index: i as u32,
            values: ApplicatorKind::Part(draw_orders_to_bind),
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

    let draw_groups: Vec<DrawGroup> = (0..read.counts().draw_order_groups() as usize)
        .map(|i| DrawGroup {
            object_start: read.draw_order_group_object_sources_starts()[i],
            object_count: read.draw_order_group_object_sources_counts()[i],
            total_count: read.draw_order_group_object_sources_total_counts()[i],
            min_order: read.draw_order_group_minimum_draw_orders()[i] as i32,
            max_order: read.draw_order_group_maximum_draw_orders()[i] as i32,
        })
        .collect();

    let draw_objects: Vec<DrawObject> = (0..read.counts().draw_order_group_objects() as usize)
        .map(|i| DrawObject {
            is_part: read.draw_order_group_object_types()[i]
                == DrawOrderGroupObjectType::Part as u32,
            index: read.draw_order_group_object_indices()[i],
            owned_group: u32::try_from(read.draw_order_group_object_self_indices()[i]).ok(),
        })
        .collect();

    let binding_key_tables: Vec<Vec<(Vec<f32>, usize)>> = (0..read.counts().keyform_bindings()
        as usize)
        .map(|i| {
            collect_parameter_bindings(
                read,
                &parameter_bindings_to_parameter,
                kb_starts[i] as usize,
                kb_counts[i] as usize,
            )
        })
        .collect();

    let part_table = ObjectTable {
        parent_part: read.part_parent_part_indices().to_vec(),
        parent_deformer: Vec::new(),
        binding: read.part_keyform_binding_sources_indices().to_vec(),
        is_enabled: read.part_is_enabled().iter().map(|&e| e != 0).collect(),
    };
    let part_is_visible = read.part_is_visible().iter().map(|&v| v != 0).collect();

    let deformer_table = ObjectTable {
        parent_part: read.deformer_parent_part_indices().to_vec(),
        parent_deformer: read.deformer_parent_deformer_indices().to_vec(),
        binding: (0..read.counts().deformers() as usize)
            .map(|i| {
                let specific = read.deformer_specific_sources_indices()[i] as usize;
                if read.deformer_types()[i] == 0 {
                    read.warp_deformer_keyform_binding_sources_indices()[specific]
                } else {
                    read.rotation_deformer_keyform_binding_sources_indices()[specific]
                }
            })
            .collect(),
        is_enabled: read.deformer_is_enabled().iter().map(|&e| e != 0).collect(),
    };

    let art_mesh_table = ObjectTable {
        parent_part: read.art_mesh_parent_part_indices().to_vec(),
        parent_deformer: read.art_mesh_parent_deformer_indices().to_vec(),
        binding: read.art_mesh_keyform_binding_sources_indices().to_vec(),
        is_enabled: read.art_mesh_is_enabled().iter().map(|&e| e != 0).collect(),
    };

    // Here we parse all of the data related to parameters onto the puppet. Right now,
    // only the default value is saved, but this will be filled with all of the other data
    // in the future.

    let mut warp_deformer_grid_count = Vec::new();
    for i in 0..read.counts().warp_deformers() as usize {
        // Fencepost error warning: rows and columns measure the user-visbile middle, not the edges
        // containg the numbers.
        warp_deformer_grid_count
            .push((read.warp_deformer_rows()[i] + 1) * (read.warp_deformer_columns()[i] + 1));
    }

    let params = collect_param_data(read);

    Puppet::from_parts(PuppetParts {
        node_roots,
        nodes: node_arena,

        glue_nodes,

        part_roots,
        parts: part_arena,

        params,
        applicators,

        part_table,
        deformer_table,
        art_mesh_table,
        part_is_visible,
        binding_key_tables,

        art_mesh_count: read.counts().art_meshes(),
        warp_deformer_count: read.counts().warp_deformers(),
        rotation_deformer_count: read.counts().rotation_deformers(),
        part_count: read.counts().parts(),
        glue_count: read.counts().glues(),

        warp_deformer_grid_count,

        art_mesh_uvs,
        art_mesh_indices,
        art_mesh_textures: read.art_mesh_texture_nums().to_vec(),
        art_mesh_flags: read
            .art_mesh_flags()
            .iter()
            .map(|&b| ArtMeshFlags::from_bytes([b]).unwrap())
            .collect(),
        art_mesh_mask_indices,
        art_mesh_vertexes: read.art_mesh_vertex_counts().to_vec(),

        draw_groups,
        draw_objects,
    })
}

#[cfg(test)]
pub(crate) fn framedata_for_tests(
    art_meshes: usize,
    parts: usize,
    groups: &[DrawGroup],
    objects: &[DrawObject],
) -> PuppetFrameData {
    let order_levels = groups
        .iter()
        .map(|g| (g.max_order - g.min_order + 1).max(0) as usize)
        .max()
        .unwrap_or(0);
    let group_objects = groups
        .iter()
        .map(|g| g.object_count as usize)
        .max()
        .unwrap_or(0);

    PuppetFrameData {
        corrected_params: Vec::new(),
        part_opacities: vec![1.0; parts],
        calculated_part_opacities: vec![1.0; parts],

        part_enabled: vec![true; parts],
        deformer_enabled: Vec::new(),
        art_mesh_enabled: vec![true; art_meshes],
        binding_out_of_range: Vec::new(),

        art_mesh_draw_orders: vec![0.0; art_meshes],
        part_draw_orders: vec![0.0; parts],

        art_mesh_render_orders: Vec::new(),

        draw_object_orders: vec![0; objects.len()],
        draw_group_cursors: vec![0; groups.len()],
        bucket_first: vec![-1; order_levels],
        bucket_last: vec![-1; order_levels],
        bucket_next: vec![-1; group_objects],
        render_sequence: vec![u32::MAX; art_meshes],

        art_mesh_data: Vec::new(),
        art_mesh_opacities: vec![1.0; art_meshes],
        art_mesh_colors: Vec::new(),

        warp_deformer_data: Vec::new(),
        rotation_deformer_data: Vec::new(),
        warp_deformer_opacities: Vec::new(),
        rotation_deformer_opacities: Vec::new(),
        warp_deformer_colors: Vec::new(),
        rotation_deformer_colors: Vec::new(),

        deformer_scale_data: Vec::new(),
        glue_data: Vec::new(),
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

    let deformer_count =
        puppet.warp_deformer_count as usize + puppet.rotation_deformer_count as usize;

    let order_levels = puppet
        .draw_groups
        .iter()
        .map(|g| (i64::from(g.max_order) - i64::from(g.min_order) + 1).max(0) as usize)
        .max()
        .unwrap_or(0);
    let group_objects = puppet
        .draw_groups
        .iter()
        .map(|g| g.object_count as usize)
        .max()
        .unwrap_or(0);

    PuppetFrameData {
        corrected_params: puppet.params.defaults.clone(),

        part_opacities: puppet
            .part_is_visible
            .iter()
            .map(|&visible| if visible { 1.0 } else { 0.0 })
            .collect(),
        calculated_part_opacities: vec![1.0; puppet.part_count as usize],

        part_enabled: vec![true; puppet.part_count as usize],
        deformer_enabled: vec![true; deformer_count],
        art_mesh_enabled: vec![true; puppet.art_mesh_count as usize],
        binding_out_of_range: vec![false; puppet.binding_key_tables.len()],

        art_mesh_draw_orders: vec![0.0; puppet.art_mesh_count as usize],
        part_draw_orders: vec![0.0; puppet.part_count as usize],

        art_mesh_render_orders: Vec::with_capacity(puppet.art_mesh_count as usize),

        draw_object_orders: vec![0; puppet.draw_objects.len()],
        draw_group_cursors: vec![0; puppet.draw_groups.len()],
        bucket_first: vec![-1; order_levels],
        bucket_last: vec![-1; order_levels],
        bucket_next: vec![-1; group_objects],
        render_sequence: vec![u32::MAX; puppet.art_mesh_count as usize],

        art_mesh_data,
        art_mesh_opacities: vec![0.0; puppet.art_mesh_count as usize],
        art_mesh_colors: vec![BlendColor::NAN; puppet.art_mesh_count as usize],

        warp_deformer_data,
        rotation_deformer_data: vec![TransformData::NAN; puppet.rotation_deformer_count as usize],
        warp_deformer_opacities: vec![f32::NAN; puppet.warp_deformer_count as usize],
        rotation_deformer_opacities: vec![f32::NAN; puppet.rotation_deformer_count as usize],
        warp_deformer_colors: vec![BlendColor::NAN; puppet.warp_deformer_count as usize],
        rotation_deformer_colors: vec![BlendColor::NAN; puppet.rotation_deformer_count as usize],

        deformer_scale_data: vec![f32::NAN; deformer_count],
        glue_data: vec![f32::NAN; puppet.glue_count as usize],
    }
}
