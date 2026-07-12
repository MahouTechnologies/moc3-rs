//! Serializer for the MOC3 binary format.
use bytemuck::{Pod, cast_slice};
use glam::Vec2;

use crate::data::{
    CanvasInfo, HEADER_RESERVE, Id, MIN_FILE_SIZE, Moc3, OFF_ART_MESH_KEYFORMS,
    OFF_ART_MESH_KEYFORMS_V402, OFF_ART_MESH_MASKS, OFF_ART_MESHES, OFF_BLEND_SHAPE_ART_MESHES,
    OFF_BLEND_SHAPE_CONSTRAINT_INDICES, OFF_BLEND_SHAPE_CONSTRAINT_VALUES,
    OFF_BLEND_SHAPE_CONSTRAINTS, OFF_BLEND_SHAPE_KEYFORM_BINDINGS,
    OFF_BLEND_SHAPE_PARAMETER_BINDINGS, OFF_BLEND_SHAPE_WARP_DEFORMERS, OFF_CANVAS_INFO,
    OFF_COUNT_INFO, OFF_DEFORMERS, OFF_DRAW_ORDER_GROUP_OBJECTS, OFF_DRAW_ORDER_GROUPS,
    OFF_GLUE_INFOS, OFF_GLUE_KEYFORMS, OFF_GLUES, OFF_KEYFORM_BINDINGS,
    OFF_KEYFORM_MULTIPLY_COLORS, OFF_KEYFORM_POSITIONS, OFF_KEYFORM_SCREEN_COLORS, OFF_KEYS,
    OFF_PARAMETER_BINDING_INDICES, OFF_PARAMETER_BINDINGS, OFF_PARAMETER_EXTENSIONS,
    OFF_PARAMETERS, OFF_PARAMETERS_V402, OFF_PART_KEYFORMS, OFF_PARTS,
    OFF_ROTATION_DEFORMER_KEYFORMS, OFF_ROTATION_DEFORMER_KEYFORMS_V402, OFF_ROTATION_DEFORMERS,
    OFF_UVS, OFF_VERTEX_INDICES, OFF_WARP_DEFORMER_KEYFORMS, OFF_WARP_DEFORMER_KEYFORMS_V303,
    OFF_WARP_DEFORMER_KEYFORMS_V402, OFF_WARP_DEFORMERS, RUNTIME_DATA_START, Version, table_end,
};

/// Per-part parallel arrays.
#[derive(Debug, Clone, Default)]
pub struct PartsSection {
    pub ids: Vec<Id>,
    pub keyform_binding_sources_indices: Vec<u32>,
    pub keyform_sources_starts: Vec<u32>,
    pub keyform_sources_counts: Vec<u32>,
    pub is_visible: Vec<u32>,
    pub is_enabled: Vec<u32>,
    pub parent_part_indices: Vec<i32>,
}

/// Per-deformer arrays (warp and rotation deformers interleaved).
#[derive(Debug, Clone, Default)]
pub struct DeformersSection {
    pub ids: Vec<Id>,
    pub keyform_binding_sources_indices: Vec<u32>,
    pub is_visible: Vec<u32>,
    pub is_enabled: Vec<u32>,
    pub parent_part_indices: Vec<i32>,
    pub parent_deformer_indices: Vec<i32>,
    /// `0` = warp deformer, `1` = rotation deformer.
    pub types: Vec<u32>,
    pub specific_sources_indices: Vec<u32>,
}

/// Per-warp-deformer arrays.
#[derive(Debug, Clone, Default)]
pub struct WarpDeformersSection {
    pub keyform_binding_sources_indices: Vec<u32>,
    pub keyform_sources_starts: Vec<u32>,
    pub keyform_sources_counts: Vec<u32>,
    pub vertex_counts: Vec<u32>,
    pub rows: Vec<u32>,
    pub columns: Vec<u32>,
    /// v3.03+ section.
    pub is_new_deformer: Vec<u32>,
    /// v4.02+ section.
    pub keyform_color_sources_starts: Vec<u32>,
}

/// Per-rotation-deformer arrays.
#[derive(Debug, Clone, Default)]
pub struct RotationDeformersSection {
    pub keyform_binding_sources_indices: Vec<u32>,
    pub keyform_sources_starts: Vec<u32>,
    pub keyform_sources_counts: Vec<u32>,
    pub base_angles: Vec<f32>,
    /// v4.02+ section.
    pub keyform_color_sources_starts: Vec<u32>,
}

/// Per-art-mesh arrays.
#[derive(Debug, Clone, Default)]
pub struct ArtMeshesSection {
    pub ids: Vec<Id>,
    pub keyform_binding_sources_indices: Vec<u32>,
    pub keyform_sources_starts: Vec<u32>,
    pub keyform_sources_counts: Vec<u32>,
    pub is_visible: Vec<u32>,
    pub is_enabled: Vec<u32>,
    pub parent_part_indices: Vec<i32>,
    pub parent_deformer_indices: Vec<i32>,
    pub texture_nums: Vec<u32>,
    /// Packed [`crate::data::ArtMeshFlags`] bytes.
    pub flags: Vec<u8>,
    pub vertex_counts: Vec<u32>,
    /// In units of `f32` (i.e. `2 * Vec2` index), like the reader exposes them.
    pub uv_sources_starts: Vec<u32>,
    pub vertex_index_sources_starts: Vec<u32>,
    pub vertex_index_sources_counts: Vec<u32>,
    pub mask_sources_starts: Vec<u32>,
    pub mask_sources_counts: Vec<u32>,
    /// v4.02+ section.
    pub keyform_color_sources_starts: Vec<u32>,
}

/// Per-parameter arrays.
#[derive(Debug, Clone, Default)]
pub struct ParametersSection {
    pub ids: Vec<Id>,
    pub max_values: Vec<f32>,
    pub min_values: Vec<f32>,
    pub default_values: Vec<f32>,
    pub is_repeat: Vec<u32>,
    pub decimal_places: Vec<u32>,
    pub binding_sources_starts: Vec<u32>,
    pub binding_sources_counts: Vec<u32>,
    /// v4.02+: `0` = normal, `1` = blend shape.
    pub types: Vec<u32>,
    /// v4.02+ section.
    pub blend_shape_binding_sources_starts: Vec<u32>,
    /// v4.02+ section.
    pub blend_shape_binding_sources_counts: Vec<u32>,
    /// v4.02+ section.
    pub extension_keys_sources_starts: Vec<u32>,
    /// v4.02+ section.
    pub extension_keys_sources_counts: Vec<u32>,
}

/// Per-warp-deformer-keyform arrays.
#[derive(Debug, Clone, Default)]
pub struct WarpDeformerKeyformsSection {
    pub opacities: Vec<f32>,
    /// In units of `f32` (i.e. `2 * Vec2` index).
    pub position_sources_starts: Vec<u32>,
}

/// Per-rotation-deformer-keyform arrays.
#[derive(Debug, Clone, Default)]
pub struct RotationDeformerKeyformsSection {
    pub opacities: Vec<f32>,
    pub angles: Vec<f32>,
    pub x_origins: Vec<f32>,
    pub y_origins: Vec<f32>,
    pub scales: Vec<f32>,
    pub is_reflect_x: Vec<u32>,
    pub is_reflect_y: Vec<u32>,
}

/// Per-art-mesh-keyform arrays.
#[derive(Debug, Clone, Default)]
pub struct ArtMeshKeyformsSection {
    pub opacities: Vec<f32>,
    pub draw_orders: Vec<f32>,
    /// In units of `f32` (i.e. `2 * Vec2` index).
    pub position_sources_starts: Vec<u32>,
}

/// Per-keyform-binding arrays: each binding is a run in
/// `parameter_binding_indices`.
#[derive(Debug, Clone, Default)]
pub struct KeyformBindingsSection {
    pub parameter_binding_index_sources_starts: Vec<u32>,
    pub parameter_binding_index_sources_counts: Vec<u32>,
}

/// Per-parameter-binding arrays: each binding is a run in `keys`.
#[derive(Debug, Clone, Default)]
pub struct ParameterBindingsSection {
    pub keys_sources_starts: Vec<u32>,
    pub keys_sources_counts: Vec<u32>,
}

/// Per-draw-order-group arrays.
#[derive(Debug, Clone, Default)]
pub struct DrawOrderGroupsSection {
    pub object_sources_starts: Vec<u32>,
    pub object_sources_counts: Vec<u32>,
    pub object_sources_total_counts: Vec<u32>,
    pub maximum_draw_orders: Vec<u32>,
    pub minimum_draw_orders: Vec<u32>,
}

/// Per-draw-order-group-object arrays.
#[derive(Debug, Clone, Default)]
pub struct DrawOrderGroupObjectsSection {
    /// `0` = art mesh, `1` = part.
    pub types: Vec<u32>,
    pub indices: Vec<u32>,
    pub self_indices: Vec<i32>,
}

/// Per-glue arrays.
#[derive(Debug, Clone, Default)]
pub struct GluesSection {
    pub ids: Vec<Id>,
    pub keyform_binding_sources_indices: Vec<u32>,
    pub keyform_sources_starts: Vec<u32>,
    pub keyform_sources_counts: Vec<u32>,
    pub art_mesh_indices_a: Vec<u32>,
    pub art_mesh_indices_b: Vec<u32>,
    pub info_sources_starts: Vec<u32>,
    pub info_sources_counts: Vec<u32>,
}

/// Per-glue-info arrays.
#[derive(Debug, Clone, Default)]
pub struct GlueInfosSection {
    pub weights: Vec<f32>,
    pub vertex_indices: Vec<u16>,
}

/// v4.02 keyform multiply/screen color arrays.
///
/// The multiply arrays are parallel to each other, and the screen arrays are
/// parallel to each other; objects reference runs via their
/// `keyform_color_sources_starts`.
#[derive(Debug, Clone, Default)]
pub struct KeyformColorsSection {
    pub multiply_reds: Vec<f32>,
    pub multiply_greens: Vec<f32>,
    pub multiply_blues: Vec<f32>,
    pub screen_reds: Vec<f32>,
    pub screen_greens: Vec<f32>,
    pub screen_blues: Vec<f32>,
}

/// v4.02 blend shape sections.
#[derive(Debug, Clone, Default)]
pub struct BlendShapesSection {
    // per blend-shape parameter binding
    pub parameter_binding_keys_sources_starts: Vec<u32>,
    pub parameter_binding_keys_sources_counts: Vec<u32>,
    pub parameter_binding_base_key_indices: Vec<u32>,
    // per blend-shape keyform binding
    pub keyform_binding_parameter_binding_sources_indices: Vec<u32>,
    pub keyform_binding_keyform_sources_starts: Vec<u32>,
    pub keyform_binding_keyform_sources_counts: Vec<u32>,
    pub keyform_binding_constraint_index_sources_starts: Vec<u32>,
    pub keyform_binding_constraint_index_sources_counts: Vec<u32>,
    // per blend-shape warp deformer target
    pub warp_deformer_target_indices: Vec<u32>,
    pub warp_deformer_keyform_binding_sources_starts: Vec<u32>,
    pub warp_deformer_keyform_binding_sources_counts: Vec<u32>,
    // per blend-shape art mesh target
    pub art_mesh_target_indices: Vec<u32>,
    pub art_mesh_keyform_binding_sources_starts: Vec<u32>,
    pub art_mesh_keyform_binding_sources_counts: Vec<u32>,
    // flat constraint index list
    pub constraint_sources_indices: Vec<u32>,
    // per constraint
    pub constraint_parameter_indices: Vec<u32>,
    pub constraint_value_sources_starts: Vec<u32>,
    pub constraint_value_sources_counts: Vec<u32>,
    // per constraint value
    pub constraint_value_keys: Vec<f32>,
    pub constraint_value_weights: Vec<f32>,
}

/// Owned mirror of every section of a MOC3 file: the input to [`write_moc3`].
#[derive(Debug, Clone, Default)]
pub struct Moc3WriteData {
    pub canvas: CanvasInfo,
    pub parts: PartsSection,
    pub deformers: DeformersSection,
    pub warp_deformers: WarpDeformersSection,
    pub rotation_deformers: RotationDeformersSection,
    pub art_meshes: ArtMeshesSection,
    pub parameters: ParametersSection,
    pub part_keyform_draw_orders: Vec<f32>,
    pub warp_deformer_keyforms: WarpDeformerKeyformsSection,
    pub rotation_deformer_keyforms: RotationDeformerKeyformsSection,
    pub art_mesh_keyforms: ArtMeshKeyformsSection,
    pub positions: Vec<Vec2>,
    pub parameter_binding_indices: Vec<u32>,
    pub keyform_bindings: KeyformBindingsSection,
    pub parameter_bindings: ParameterBindingsSection,
    pub keys: Vec<f32>,
    pub uvs: Vec<Vec2>,
    pub vertex_indices: Vec<u16>,
    pub art_mesh_mask_source_indices: Vec<u32>,
    pub draw_order_groups: DrawOrderGroupsSection,
    pub draw_order_group_objects: DrawOrderGroupObjectsSection,
    pub glues: GluesSection,
    pub glue_infos: GlueInfosSection,
    pub glue_keyform_intensities: Vec<f32>,
    pub keyform_colors: KeyformColorsSection,
    pub blend_shapes: BlendShapesSection,
}

impl Moc3WriteData {
    /// Copy every section of a parsed view into owned storage.
    pub fn from_moc3(read: &Moc3<'_>) -> Self {
        let warp_count = read.warp_deformer_keyform_binding_sources_indices().len();
        let parameter_count = read.parameter_ids().len();

        // Default color sections for warp, rotation, and art mesh objects.
        let (warp_colors, rotation_colors, art_mesh_colors, colors);
        if let Some(starts) = read.warp_deformer_keyform_color_sources_start() {
            warp_colors = starts.to_vec();
            rotation_colors = read
                .rotation_deformer_keyform_color_sources_start()
                .unwrap()
                .to_vec();
            art_mesh_colors = read
                .art_mesh_keyform_color_sources_start()
                .unwrap()
                .to_vec();
            colors = KeyformColorsSection {
                multiply_reds: read.keyform_multiply_colors_red().unwrap().to_vec(),
                multiply_greens: read.keyform_multiply_colors_green().unwrap().to_vec(),
                multiply_blues: read.keyform_multiply_colors_blue().unwrap().to_vec(),
                screen_reds: read.keyform_screen_colors_red().unwrap().to_vec(),
                screen_greens: read.keyform_screen_colors_green().unwrap().to_vec(),
                screen_blues: read.keyform_screen_colors_blue().unwrap().to_vec(),
            };
        } else {
            let mut cursor = 0u32;
            let mut assign = |counts: &[u32]| -> Vec<u32> {
                counts
                    .iter()
                    .map(|&c| {
                        let start = cursor;
                        cursor += c;
                        start
                    })
                    .collect()
            };
            warp_colors = assign(read.warp_deformer_keyform_sources_counts());
            rotation_colors = assign(read.rotation_deformer_keyform_sources_counts());
            art_mesh_colors = assign(read.art_mesh_keyform_sources_counts());
            let total = cursor as usize;
            colors = KeyformColorsSection {
                multiply_reds: vec![1.0; total],
                multiply_greens: vec![1.0; total],
                multiply_blues: vec![1.0; total],
                screen_reds: vec![0.0; total],
                screen_greens: vec![0.0; total],
                screen_blues: vec![0.0; total],
            };
        }

        // Pre-4.02 files have no parameter extension section either. The runtime
        // synthesizes one for them by taking each parameter's largest key table,
        // so do the same and keep the parameter's keys once we commit to 4.02.
        let (extension_starts, extension_counts);
        if let Some(starts) = read.parameter_extension_keys_sources_starts() {
            extension_starts = starts.to_vec();
            extension_counts = read
                .parameter_extension_keys_sources_counts()
                .unwrap()
                .to_vec();
        } else {
            let table_starts = read.parameter_binding_keys_sources_starts();
            let table_counts = read.parameter_binding_keys_sources_counts();
            let binding_starts = read.parameter_binding_sources_starts();
            let binding_counts = read.parameter_binding_sources_counts();

            let mut starts = Vec::with_capacity(parameter_count);
            let mut counts = Vec::with_capacity(parameter_count);
            for i in 0..parameter_count {
                let first = binding_starts[i] as usize;
                let tables = binding_counts[i] as usize;
                // Ties go to the earliest table, as in the runtime.
                let best =
                    (first..first + tables).max_by_key(|&t| (table_counts[t], usize::MAX - t));
                match best {
                    Some(t) if table_counts[t] > 0 => {
                        starts.push(table_starts[t]);
                        counts.push(table_counts[t]);
                    }
                    _ => {
                        starts.push(0);
                        counts.push(0);
                    }
                }
            }
            extension_starts = starts;
            extension_counts = counts;
        }

        Moc3WriteData {
            canvas: read.canvas_info(),
            parts: PartsSection {
                ids: read.part_ids().to_vec(),
                keyform_binding_sources_indices: read
                    .part_keyform_binding_sources_indices()
                    .to_vec(),
                keyform_sources_starts: read.part_keyform_sources_starts().to_vec(),
                keyform_sources_counts: read.part_keyform_sources_counts().to_vec(),
                is_visible: read.part_is_visible().to_vec(),
                is_enabled: read.part_is_enabled().to_vec(),
                parent_part_indices: read.part_parent_part_indices().to_vec(),
            },
            deformers: DeformersSection {
                ids: read.deformer_ids().to_vec(),
                keyform_binding_sources_indices: read
                    .deformer_keyform_binding_sources_indices()
                    .to_vec(),
                is_visible: read.deformer_is_visible().to_vec(),
                is_enabled: read.deformer_is_enabled().to_vec(),
                parent_part_indices: read.deformer_parent_part_indices().to_vec(),
                parent_deformer_indices: read.deformer_parent_deformer_indices().to_vec(),
                types: read.deformer_types().to_vec(),
                specific_sources_indices: read.deformer_specific_sources_indices().to_vec(),
            },
            warp_deformers: WarpDeformersSection {
                keyform_binding_sources_indices: read
                    .warp_deformer_keyform_binding_sources_indices()
                    .to_vec(),
                keyform_sources_starts: read.warp_deformer_keyform_sources_starts().to_vec(),
                keyform_sources_counts: read.warp_deformer_keyform_sources_counts().to_vec(),
                vertex_counts: read.warp_deformer_vertex_counts().to_vec(),
                rows: read.warp_deformer_rows().to_vec(),
                columns: read.warp_deformer_columns().to_vec(),
                is_new_deformer: read
                    .warp_deformer_is_new_deformer()
                    .map(<[u32]>::to_vec)
                    .unwrap_or_else(|| vec![1; warp_count]),
                keyform_color_sources_starts: warp_colors,
            },
            rotation_deformers: RotationDeformersSection {
                keyform_binding_sources_indices: read
                    .rotation_deformer_keyform_binding_sources_indices()
                    .to_vec(),
                keyform_sources_starts: read.rotation_deformer_keyform_sources_starts().to_vec(),
                keyform_sources_counts: read.rotation_deformer_keyform_sources_counts().to_vec(),
                base_angles: read.rotation_deformer_base_angles().to_vec(),
                keyform_color_sources_starts: rotation_colors,
            },
            art_meshes: ArtMeshesSection {
                ids: read.art_mesh_ids().to_vec(),
                keyform_binding_sources_indices: read
                    .art_mesh_keyform_binding_sources_indices()
                    .to_vec(),
                keyform_sources_starts: read.art_mesh_keyform_sources_starts().to_vec(),
                keyform_sources_counts: read.art_mesh_keyform_sources_counts().to_vec(),
                is_visible: read.art_mesh_is_visible().to_vec(),
                is_enabled: read.art_mesh_is_enabled().to_vec(),
                parent_part_indices: read.art_mesh_parent_part_indices().to_vec(),
                parent_deformer_indices: read.art_mesh_parent_deformer_indices().to_vec(),
                texture_nums: read.art_mesh_texture_nums().to_vec(),
                flags: read.art_mesh_flags().to_vec(),
                vertex_counts: read.art_mesh_vertex_counts().to_vec(),
                uv_sources_starts: read.art_mesh_uv_sources_starts().to_vec(),
                vertex_index_sources_starts: read.art_mesh_vertex_index_sources_starts().to_vec(),
                vertex_index_sources_counts: read.art_mesh_vertex_index_sources_counts().to_vec(),
                mask_sources_starts: read.art_mesh_mask_sources_starts().to_vec(),
                mask_sources_counts: read.art_mesh_mask_sources_counts().to_vec(),
                keyform_color_sources_starts: art_mesh_colors,
            },
            parameters: ParametersSection {
                ids: read.parameter_ids().to_vec(),
                max_values: read.parameter_max_values().to_vec(),
                min_values: read.parameter_min_values().to_vec(),
                default_values: read.parameter_default_values().to_vec(),
                is_repeat: read.parameter_is_repeat().to_vec(),
                decimal_places: read.parameter_decimal_places().to_vec(),
                binding_sources_starts: read.parameter_binding_sources_starts().to_vec(),
                binding_sources_counts: read.parameter_binding_sources_counts().to_vec(),
                types: read
                    .parameter_types()
                    .map(<[u32]>::to_vec)
                    .unwrap_or_else(|| vec![0; parameter_count]),
                blend_shape_binding_sources_starts: read
                    .parameter_blend_shape_binding_sources_starts()
                    .map(<[u32]>::to_vec)
                    .unwrap_or_else(|| vec![0; parameter_count]),
                blend_shape_binding_sources_counts: read
                    .parameter_blend_shape_binding_sources_counts()
                    .map(<[u32]>::to_vec)
                    .unwrap_or_else(|| vec![0; parameter_count]),
                extension_keys_sources_starts: extension_starts,
                extension_keys_sources_counts: extension_counts,
            },
            part_keyform_draw_orders: read.part_keyform_draw_orders().to_vec(),
            warp_deformer_keyforms: WarpDeformerKeyformsSection {
                opacities: read.warp_deformer_keyform_opacities().to_vec(),
                position_sources_starts: read
                    .warp_deformer_keyform_position_sources_starts()
                    .to_vec(),
            },
            rotation_deformer_keyforms: RotationDeformerKeyformsSection {
                opacities: read.rotation_deformer_keyform_opacities().to_vec(),
                angles: read.rotation_deformer_keyform_angles().to_vec(),
                x_origins: read.rotation_deformer_keyform_x_origin().to_vec(),
                y_origins: read.rotation_deformer_keyform_y_origin().to_vec(),
                scales: read.rotation_deformer_keyform_scales().to_vec(),
                is_reflect_x: read.rotation_deformer_keyform_is_reflect_x().to_vec(),
                is_reflect_y: read.rotation_deformer_keyform_is_reflect_y().to_vec(),
            },
            art_mesh_keyforms: ArtMeshKeyformsSection {
                opacities: read.art_mesh_keyform_opacities().to_vec(),
                draw_orders: read.art_mesh_keyform_draw_orders().to_vec(),
                position_sources_starts: read.art_mesh_keyform_position_sources_starts().to_vec(),
            },
            positions: read.positions().to_vec(),
            parameter_binding_indices: read.parameter_binding_indices().to_vec(),
            keyform_bindings: KeyformBindingsSection {
                parameter_binding_index_sources_starts: read
                    .keyform_binding_parameter_binding_index_sources_starts()
                    .to_vec(),
                parameter_binding_index_sources_counts: read
                    .keyform_binding_parameter_binding_index_sources_counts()
                    .to_vec(),
            },
            parameter_bindings: ParameterBindingsSection {
                keys_sources_starts: read.parameter_binding_keys_sources_starts().to_vec(),
                keys_sources_counts: read.parameter_binding_keys_sources_counts().to_vec(),
            },
            keys: read.keys().to_vec(),
            uvs: read.uvs().to_vec(),
            vertex_indices: read.vertex_indices().to_vec(),
            art_mesh_mask_source_indices: read.art_mesh_mask_source_indices().to_vec(),
            draw_order_groups: DrawOrderGroupsSection {
                object_sources_starts: read.draw_order_group_object_sources_starts().to_vec(),
                object_sources_counts: read.draw_order_group_object_sources_counts().to_vec(),
                object_sources_total_counts: read
                    .draw_order_group_object_sources_total_counts()
                    .to_vec(),
                maximum_draw_orders: read.draw_order_group_maximum_draw_orders().to_vec(),
                minimum_draw_orders: read.draw_order_group_minimum_draw_orders().to_vec(),
            },
            draw_order_group_objects: DrawOrderGroupObjectsSection {
                types: read.draw_order_group_object_types().to_vec(),
                indices: read.draw_order_group_object_indices().to_vec(),
                self_indices: read.draw_order_group_object_self_indices().to_vec(),
            },
            glues: GluesSection {
                ids: read.glue_ids().to_vec(),
                keyform_binding_sources_indices: read
                    .glue_keyform_binding_sources_indices()
                    .to_vec(),
                keyform_sources_starts: read.glue_keyform_sources_starts().to_vec(),
                keyform_sources_counts: read.glue_keyform_sources_counts().to_vec(),
                art_mesh_indices_a: read.glue_art_mesh_indices_a().to_vec(),
                art_mesh_indices_b: read.glue_art_mesh_indices_b().to_vec(),
                info_sources_starts: read.glue_info_sources_starts().to_vec(),
                info_sources_counts: read.glue_info_sources_counts().to_vec(),
            },
            glue_infos: GlueInfosSection {
                weights: read.glue_info_weights().to_vec(),
                vertex_indices: read.glue_info_vertex_indices().to_vec(),
            },
            glue_keyform_intensities: read.glue_keyform_intensities().to_vec(),
            keyform_colors: colors,
            blend_shapes: BlendShapesSection {
                parameter_binding_keys_sources_starts: opt(
                    read.blend_shape_parameter_binding_keys_sources_starts()
                ),
                parameter_binding_keys_sources_counts: opt(
                    read.blend_shape_parameter_binding_keys_sources_counts()
                ),
                parameter_binding_base_key_indices: opt(
                    read.blend_shape_parameter_binding_base_key_indices()
                ),
                keyform_binding_parameter_binding_sources_indices: opt(
                    read.blend_shape_keyform_binding_parameter_binding_sources_indices()
                ),
                keyform_binding_keyform_sources_starts: opt(
                    read.blend_shape_keyform_binding_keyform_sources_starts()
                ),
                keyform_binding_keyform_sources_counts: opt(
                    read.blend_shape_keyform_binding_keyform_sources_counts()
                ),
                keyform_binding_constraint_index_sources_starts: opt(
                    read.blend_shape_keyform_binding_constraint_index_sources_starts()
                ),
                keyform_binding_constraint_index_sources_counts: opt(
                    read.blend_shape_keyform_binding_constraint_index_sources_counts()
                ),
                warp_deformer_target_indices: opt(read.blend_shape_warp_deformer_target_indices()),
                warp_deformer_keyform_binding_sources_starts: opt(
                    read.blend_shape_warp_deformer_keyform_binding_sources_starts()
                ),
                warp_deformer_keyform_binding_sources_counts: opt(
                    read.blend_shape_warp_deformer_keyform_binding_sources_counts()
                ),
                art_mesh_target_indices: opt(read.blend_shape_art_mesh_target_indices()),
                art_mesh_keyform_binding_sources_starts: opt(
                    read.blend_shape_art_mesh_keyform_binding_sources_starts()
                ),
                art_mesh_keyform_binding_sources_counts: opt(
                    read.blend_shape_art_mesh_keyform_binding_sources_counts()
                ),
                constraint_sources_indices: opt(read.blend_shape_constraint_sources_indices()),
                constraint_parameter_indices: opt(read.blend_shape_constraint_parameter_indices()),
                constraint_value_sources_starts: opt(
                    read.blend_shape_constraint_value_sources_starts()
                ),
                constraint_value_sources_counts: opt(
                    read.blend_shape_constraint_value_sources_counts()
                ),
                constraint_value_keys: opt(read.blend_shape_constraint_value_keys()),
                constraint_value_weights: opt(read.blend_shape_constraint_value_weights()),
            },
        }
    }
}

fn opt<T: Clone>(s: Option<&[T]>) -> Vec<T> {
    s.map(<[T]>::to_vec).unwrap_or_default()
}

/// Every section is padded to this alignment.
const SECTION_ALIGN: usize = 64;

fn pad_to_align(buf: &mut Vec<u8>) {
    let len = buf.len().next_multiple_of(SECTION_ALIGN);
    buf.resize(len, 0);
}

/// Pad to the section alignment, then patch the table slot at `table_off` to
/// point at the current end of the buffer.
fn put_ptr(buf: &mut Vec<u8>, table_off: usize) {
    pad_to_align(buf);
    let ptr = buf.len() as u32;
    buf[table_off..table_off + 4].copy_from_slice(&ptr.to_le_bytes());
}

/// Emit `items` as the section pointed to by the table slot at `table_off`.
fn put_section<T: Pod>(buf: &mut Vec<u8>, table_off: usize, items: &[T]) {
    put_ptr(buf, table_off);
    buf.extend_from_slice(cast_slice(items));
}

/// Emit one of the runtime pointer arrays: `count` native (64-bit) pointer
/// slots, zeroed.
fn put_runtime_ptr_array(buf: &mut Vec<u8>, table_off: usize, count: usize) {
    put_ptr(buf, table_off);
    buf.resize(buf.len() + count * size_of::<u64>(), 0);
}

/// Serialize to a little-endian V4.02 `.moc3` byte buffer.
pub fn write_moc3(d: &Moc3WriteData) -> Vec<u8> {
    let parts = d.parts.ids.len();
    let deformers = d.deformers.ids.len();
    let warp_deformers = d.warp_deformers.keyform_binding_sources_indices.len();
    let rotation_deformers = d.rotation_deformers.keyform_binding_sources_indices.len();
    let art_meshes = d.art_meshes.ids.len();
    let parameters = d.parameters.ids.len();
    let glues = d.glues.ids.len();

    check_lengths(d);

    let counts: [u32; 32] = [
        parts as u32,
        deformers as u32,
        warp_deformers as u32,
        rotation_deformers as u32,
        art_meshes as u32,
        parameters as u32,
        d.part_keyform_draw_orders.len() as u32,
        d.warp_deformer_keyforms.opacities.len() as u32,
        d.rotation_deformer_keyforms.opacities.len() as u32,
        d.art_mesh_keyforms.opacities.len() as u32,
        // positions and uvs are counted in units of f32, not Vec2
        (d.positions.len() * 2) as u32,
        d.parameter_binding_indices.len() as u32,
        d.keyform_bindings
            .parameter_binding_index_sources_starts
            .len() as u32,
        d.parameter_bindings.keys_sources_starts.len() as u32,
        d.keys.len() as u32,
        (d.uvs.len() * 2) as u32,
        d.vertex_indices.len() as u32,
        d.art_mesh_mask_source_indices.len() as u32,
        d.draw_order_groups.object_sources_starts.len() as u32,
        d.draw_order_group_objects.types.len() as u32,
        glues as u32,
        d.glue_infos.weights.len() as u32,
        d.glue_keyform_intensities.len() as u32,
        d.keyform_colors.multiply_reds.len() as u32,
        d.keyform_colors.screen_reds.len() as u32,
        d.blend_shapes.parameter_binding_keys_sources_starts.len() as u32,
        d.blend_shapes
            .keyform_binding_parameter_binding_sources_indices
            .len() as u32,
        d.blend_shapes.warp_deformer_target_indices.len() as u32,
        d.blend_shapes.art_mesh_target_indices.len() as u32,
        d.blend_shapes.constraint_sources_indices.len() as u32,
        d.blend_shapes.constraint_parameter_indices.len() as u32,
        d.blend_shapes.constraint_value_keys.len() as u32,
    ];

    debug_assert!(table_end(Version::V4_02) <= RUNTIME_DATA_START);
    let mut buf = vec![0u8; HEADER_RESERVE];
    buf[0..4].copy_from_slice(b"MOC3");
    buf[4] = Version::V4_02 as u8;
    buf[5] = 0; // little-endian

    put_section(&mut buf, OFF_COUNT_INFO, &counts);

    // Canvas info: five f32 fields plus the flags byte.
    put_ptr(&mut buf, OFF_CANVAS_INFO);
    for v in [
        d.canvas.pixels_per_unit,
        d.canvas.x_origin,
        d.canvas.y_origin,
        d.canvas.canvas_width,
        d.canvas.canvas_height,
    ] {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf.push(d.canvas.canvas_flags);

    // ----- parts -----
    put_runtime_ptr_array(&mut buf, OFF_PARTS, parts);
    put_section(&mut buf, OFF_PARTS + 4, &d.parts.ids);
    put_section(
        &mut buf,
        OFF_PARTS + 8,
        &d.parts.keyform_binding_sources_indices,
    );
    put_section(&mut buf, OFF_PARTS + 12, &d.parts.keyform_sources_starts);
    put_section(&mut buf, OFF_PARTS + 16, &d.parts.keyform_sources_counts);
    put_section(&mut buf, OFF_PARTS + 20, &d.parts.is_visible);
    put_section(&mut buf, OFF_PARTS + 24, &d.parts.is_enabled);
    put_section(&mut buf, OFF_PARTS + 28, &d.parts.parent_part_indices);

    // ----- deformers -----
    put_runtime_ptr_array(&mut buf, OFF_DEFORMERS, deformers);
    put_section(&mut buf, OFF_DEFORMERS + 4, &d.deformers.ids);
    put_section(
        &mut buf,
        OFF_DEFORMERS + 8,
        &d.deformers.keyform_binding_sources_indices,
    );
    put_section(&mut buf, OFF_DEFORMERS + 12, &d.deformers.is_visible);
    put_section(&mut buf, OFF_DEFORMERS + 16, &d.deformers.is_enabled);
    put_section(
        &mut buf,
        OFF_DEFORMERS + 20,
        &d.deformers.parent_part_indices,
    );
    put_section(
        &mut buf,
        OFF_DEFORMERS + 24,
        &d.deformers.parent_deformer_indices,
    );
    put_section(&mut buf, OFF_DEFORMERS + 28, &d.deformers.types);
    put_section(
        &mut buf,
        OFF_DEFORMERS + 32,
        &d.deformers.specific_sources_indices,
    );

    // ----- warp deformers -----
    put_section(
        &mut buf,
        OFF_WARP_DEFORMERS,
        &d.warp_deformers.keyform_binding_sources_indices,
    );
    put_section(
        &mut buf,
        OFF_WARP_DEFORMERS + 4,
        &d.warp_deformers.keyform_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_WARP_DEFORMERS + 8,
        &d.warp_deformers.keyform_sources_counts,
    );
    put_section(
        &mut buf,
        OFF_WARP_DEFORMERS + 12,
        &d.warp_deformers.vertex_counts,
    );
    put_section(&mut buf, OFF_WARP_DEFORMERS + 16, &d.warp_deformers.rows);
    put_section(&mut buf, OFF_WARP_DEFORMERS + 20, &d.warp_deformers.columns);

    // ----- rotation deformers -----
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMERS,
        &d.rotation_deformers.keyform_binding_sources_indices,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMERS + 4,
        &d.rotation_deformers.keyform_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMERS + 8,
        &d.rotation_deformers.keyform_sources_counts,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMERS + 12,
        &d.rotation_deformers.base_angles,
    );

    // ----- art meshes -----
    // Four runtime pointer arrays: ids, uvs, vertex indices, masks.
    for k in [0, 4, 8, 12] {
        put_runtime_ptr_array(&mut buf, OFF_ART_MESHES + k, art_meshes);
    }
    put_section(&mut buf, OFF_ART_MESHES + 16, &d.art_meshes.ids);
    put_section(
        &mut buf,
        OFF_ART_MESHES + 20,
        &d.art_meshes.keyform_binding_sources_indices,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 24,
        &d.art_meshes.keyform_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 28,
        &d.art_meshes.keyform_sources_counts,
    );
    put_section(&mut buf, OFF_ART_MESHES + 32, &d.art_meshes.is_visible);
    put_section(&mut buf, OFF_ART_MESHES + 36, &d.art_meshes.is_enabled);
    put_section(
        &mut buf,
        OFF_ART_MESHES + 40,
        &d.art_meshes.parent_part_indices,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 44,
        &d.art_meshes.parent_deformer_indices,
    );
    put_section(&mut buf, OFF_ART_MESHES + 48, &d.art_meshes.texture_nums);
    put_section(&mut buf, OFF_ART_MESHES + 52, &d.art_meshes.flags);
    put_section(&mut buf, OFF_ART_MESHES + 56, &d.art_meshes.vertex_counts);
    put_section(
        &mut buf,
        OFF_ART_MESHES + 60,
        &d.art_meshes.uv_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 64,
        &d.art_meshes.vertex_index_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 68,
        &d.art_meshes.vertex_index_sources_counts,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 72,
        &d.art_meshes.mask_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ART_MESHES + 76,
        &d.art_meshes.mask_sources_counts,
    );

    // ----- parameters -----
    put_runtime_ptr_array(&mut buf, OFF_PARAMETERS, parameters);
    put_section(&mut buf, OFF_PARAMETERS + 4, &d.parameters.ids);
    put_section(&mut buf, OFF_PARAMETERS + 8, &d.parameters.max_values);
    put_section(&mut buf, OFF_PARAMETERS + 12, &d.parameters.min_values);
    put_section(&mut buf, OFF_PARAMETERS + 16, &d.parameters.default_values);
    put_section(&mut buf, OFF_PARAMETERS + 20, &d.parameters.is_repeat);
    put_section(&mut buf, OFF_PARAMETERS + 24, &d.parameters.decimal_places);
    put_section(
        &mut buf,
        OFF_PARAMETERS + 28,
        &d.parameters.binding_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_PARAMETERS + 32,
        &d.parameters.binding_sources_counts,
    );

    // ----- keyforms -----
    put_section(&mut buf, OFF_PART_KEYFORMS, &d.part_keyform_draw_orders);

    put_section(
        &mut buf,
        OFF_WARP_DEFORMER_KEYFORMS,
        &d.warp_deformer_keyforms.opacities,
    );
    put_section(
        &mut buf,
        OFF_WARP_DEFORMER_KEYFORMS + 4,
        &d.warp_deformer_keyforms.position_sources_starts,
    );

    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS,
        &d.rotation_deformer_keyforms.opacities,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS + 4,
        &d.rotation_deformer_keyforms.angles,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS + 8,
        &d.rotation_deformer_keyforms.x_origins,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS + 12,
        &d.rotation_deformer_keyforms.y_origins,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS + 16,
        &d.rotation_deformer_keyforms.scales,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS + 20,
        &d.rotation_deformer_keyforms.is_reflect_x,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS + 24,
        &d.rotation_deformer_keyforms.is_reflect_y,
    );

    put_section(
        &mut buf,
        OFF_ART_MESH_KEYFORMS,
        &d.art_mesh_keyforms.opacities,
    );
    put_section(
        &mut buf,
        OFF_ART_MESH_KEYFORMS + 4,
        &d.art_mesh_keyforms.draw_orders,
    );
    put_section(
        &mut buf,
        OFF_ART_MESH_KEYFORMS + 8,
        &d.art_mesh_keyforms.position_sources_starts,
    );

    put_section(&mut buf, OFF_KEYFORM_POSITIONS, &d.positions);
    put_section(
        &mut buf,
        OFF_PARAMETER_BINDING_INDICES,
        &d.parameter_binding_indices,
    );

    put_section(
        &mut buf,
        OFF_KEYFORM_BINDINGS,
        &d.keyform_bindings.parameter_binding_index_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_KEYFORM_BINDINGS + 4,
        &d.keyform_bindings.parameter_binding_index_sources_counts,
    );

    put_section(
        &mut buf,
        OFF_PARAMETER_BINDINGS,
        &d.parameter_bindings.keys_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_PARAMETER_BINDINGS + 4,
        &d.parameter_bindings.keys_sources_counts,
    );

    put_section(&mut buf, OFF_KEYS, &d.keys);
    put_section(&mut buf, OFF_UVS, &d.uvs);
    put_section(&mut buf, OFF_VERTEX_INDICES, &d.vertex_indices);
    put_section(
        &mut buf,
        OFF_ART_MESH_MASKS,
        &d.art_mesh_mask_source_indices,
    );

    // ----- draw order groups -----
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUPS,
        &d.draw_order_groups.object_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUPS + 4,
        &d.draw_order_groups.object_sources_counts,
    );
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUPS + 8,
        &d.draw_order_groups.object_sources_total_counts,
    );
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUPS + 12,
        &d.draw_order_groups.maximum_draw_orders,
    );
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUPS + 16,
        &d.draw_order_groups.minimum_draw_orders,
    );

    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUP_OBJECTS,
        &d.draw_order_group_objects.types,
    );
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUP_OBJECTS + 4,
        &d.draw_order_group_objects.indices,
    );
    put_section(
        &mut buf,
        OFF_DRAW_ORDER_GROUP_OBJECTS + 8,
        &d.draw_order_group_objects.self_indices,
    );

    // ----- glues -----
    put_runtime_ptr_array(&mut buf, OFF_GLUES, glues);
    put_section(&mut buf, OFF_GLUES + 4, &d.glues.ids);
    put_section(
        &mut buf,
        OFF_GLUES + 8,
        &d.glues.keyform_binding_sources_indices,
    );
    put_section(&mut buf, OFF_GLUES + 12, &d.glues.keyform_sources_starts);
    put_section(&mut buf, OFF_GLUES + 16, &d.glues.keyform_sources_counts);
    put_section(&mut buf, OFF_GLUES + 20, &d.glues.art_mesh_indices_a);
    put_section(&mut buf, OFF_GLUES + 24, &d.glues.art_mesh_indices_b);
    put_section(&mut buf, OFF_GLUES + 28, &d.glues.info_sources_starts);
    put_section(&mut buf, OFF_GLUES + 32, &d.glues.info_sources_counts);

    put_section(&mut buf, OFF_GLUE_INFOS, &d.glue_infos.weights);
    put_section(&mut buf, OFF_GLUE_INFOS + 4, &d.glue_infos.vertex_indices);

    put_section(&mut buf, OFF_GLUE_KEYFORMS, &d.glue_keyform_intensities);

    // ----- v3.03+ -----
    put_section(
        &mut buf,
        OFF_WARP_DEFORMER_KEYFORMS_V303,
        &d.warp_deformers.is_new_deformer,
    );

    // ----- v4.02+ -----
    put_runtime_ptr_array(&mut buf, OFF_PARAMETER_EXTENSIONS, parameters);
    put_section(
        &mut buf,
        OFF_PARAMETER_EXTENSIONS + 4,
        &d.parameters.extension_keys_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_PARAMETER_EXTENSIONS + 8,
        &d.parameters.extension_keys_sources_counts,
    );

    put_section(
        &mut buf,
        OFF_WARP_DEFORMER_KEYFORMS_V402,
        &d.warp_deformers.keyform_color_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ROTATION_DEFORMER_KEYFORMS_V402,
        &d.rotation_deformers.keyform_color_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_ART_MESH_KEYFORMS_V402,
        &d.art_meshes.keyform_color_sources_starts,
    );

    put_section(
        &mut buf,
        OFF_KEYFORM_MULTIPLY_COLORS,
        &d.keyform_colors.multiply_reds,
    );
    put_section(
        &mut buf,
        OFF_KEYFORM_MULTIPLY_COLORS + 4,
        &d.keyform_colors.multiply_greens,
    );
    put_section(
        &mut buf,
        OFF_KEYFORM_MULTIPLY_COLORS + 8,
        &d.keyform_colors.multiply_blues,
    );
    put_section(
        &mut buf,
        OFF_KEYFORM_SCREEN_COLORS,
        &d.keyform_colors.screen_reds,
    );
    put_section(
        &mut buf,
        OFF_KEYFORM_SCREEN_COLORS + 4,
        &d.keyform_colors.screen_greens,
    );
    put_section(
        &mut buf,
        OFF_KEYFORM_SCREEN_COLORS + 8,
        &d.keyform_colors.screen_blues,
    );

    put_section(&mut buf, OFF_PARAMETERS_V402, &d.parameters.types);
    put_section(
        &mut buf,
        OFF_PARAMETERS_V402 + 4,
        &d.parameters.blend_shape_binding_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_PARAMETERS_V402 + 8,
        &d.parameters.blend_shape_binding_sources_counts,
    );

    let bs = &d.blend_shapes;
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_PARAMETER_BINDINGS,
        &bs.parameter_binding_keys_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_PARAMETER_BINDINGS + 4,
        &bs.parameter_binding_keys_sources_counts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_PARAMETER_BINDINGS + 8,
        &bs.parameter_binding_base_key_indices,
    );

    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS,
        &bs.keyform_binding_parameter_binding_sources_indices,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 4,
        &bs.keyform_binding_keyform_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 8,
        &bs.keyform_binding_keyform_sources_counts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 12,
        &bs.keyform_binding_constraint_index_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 16,
        &bs.keyform_binding_constraint_index_sources_counts,
    );

    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_WARP_DEFORMERS,
        &bs.warp_deformer_target_indices,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_WARP_DEFORMERS + 4,
        &bs.warp_deformer_keyform_binding_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_WARP_DEFORMERS + 8,
        &bs.warp_deformer_keyform_binding_sources_counts,
    );

    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_ART_MESHES,
        &bs.art_mesh_target_indices,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_ART_MESHES + 4,
        &bs.art_mesh_keyform_binding_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_ART_MESHES + 8,
        &bs.art_mesh_keyform_binding_sources_counts,
    );

    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_CONSTRAINT_INDICES,
        &bs.constraint_sources_indices,
    );

    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_CONSTRAINTS,
        &bs.constraint_parameter_indices,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_CONSTRAINTS + 4,
        &bs.constraint_value_sources_starts,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_CONSTRAINTS + 8,
        &bs.constraint_value_sources_counts,
    );

    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_CONSTRAINT_VALUES,
        &bs.constraint_value_keys,
    );
    put_section(
        &mut buf,
        OFF_BLEND_SHAPE_CONSTRAINT_VALUES + 4,
        &bs.constraint_value_weights,
    );

    // Trailing pad so the file end is as aligned as every section start.
    pad_to_align(&mut buf);
    if buf.len() < MIN_FILE_SIZE {
        buf.resize(MIN_FILE_SIZE, 0);
    }
    buf
}

/// Debug-check that every parallel array in each section has the same length.
fn check_lengths(d: &Moc3WriteData) {
    macro_rules! same_len {
        ($base:expr, $($v:expr),+ $(,)?) => {
            $(debug_assert_eq!($base, $v.len(), stringify!($v));)+
        };
    }

    same_len!(
        d.parts.ids.len(),
        d.parts.keyform_binding_sources_indices,
        d.parts.keyform_sources_starts,
        d.parts.keyform_sources_counts,
        d.parts.is_visible,
        d.parts.is_enabled,
        d.parts.parent_part_indices,
    );
    same_len!(
        d.deformers.ids.len(),
        d.deformers.keyform_binding_sources_indices,
        d.deformers.is_visible,
        d.deformers.is_enabled,
        d.deformers.parent_part_indices,
        d.deformers.parent_deformer_indices,
        d.deformers.types,
        d.deformers.specific_sources_indices,
    );
    same_len!(
        d.warp_deformers.keyform_binding_sources_indices.len(),
        d.warp_deformers.keyform_sources_starts,
        d.warp_deformers.keyform_sources_counts,
        d.warp_deformers.vertex_counts,
        d.warp_deformers.rows,
        d.warp_deformers.columns,
        d.warp_deformers.is_new_deformer,
        d.warp_deformers.keyform_color_sources_starts,
    );
    same_len!(
        d.rotation_deformers.keyform_binding_sources_indices.len(),
        d.rotation_deformers.keyform_sources_starts,
        d.rotation_deformers.keyform_sources_counts,
        d.rotation_deformers.base_angles,
        d.rotation_deformers.keyform_color_sources_starts,
    );
    same_len!(
        d.art_meshes.ids.len(),
        d.art_meshes.keyform_binding_sources_indices,
        d.art_meshes.keyform_sources_starts,
        d.art_meshes.keyform_sources_counts,
        d.art_meshes.is_visible,
        d.art_meshes.is_enabled,
        d.art_meshes.parent_part_indices,
        d.art_meshes.parent_deformer_indices,
        d.art_meshes.texture_nums,
        d.art_meshes.flags,
        d.art_meshes.vertex_counts,
        d.art_meshes.uv_sources_starts,
        d.art_meshes.vertex_index_sources_starts,
        d.art_meshes.vertex_index_sources_counts,
        d.art_meshes.mask_sources_starts,
        d.art_meshes.mask_sources_counts,
        d.art_meshes.keyform_color_sources_starts,
    );
    same_len!(
        d.parameters.ids.len(),
        d.parameters.max_values,
        d.parameters.min_values,
        d.parameters.default_values,
        d.parameters.is_repeat,
        d.parameters.decimal_places,
        d.parameters.binding_sources_starts,
        d.parameters.binding_sources_counts,
        d.parameters.types,
        d.parameters.blend_shape_binding_sources_starts,
        d.parameters.blend_shape_binding_sources_counts,
        d.parameters.extension_keys_sources_starts,
        d.parameters.extension_keys_sources_counts,
    );
    same_len!(
        d.warp_deformer_keyforms.opacities.len(),
        d.warp_deformer_keyforms.position_sources_starts,
    );
    same_len!(
        d.rotation_deformer_keyforms.opacities.len(),
        d.rotation_deformer_keyforms.angles,
        d.rotation_deformer_keyforms.x_origins,
        d.rotation_deformer_keyforms.y_origins,
        d.rotation_deformer_keyforms.scales,
        d.rotation_deformer_keyforms.is_reflect_x,
        d.rotation_deformer_keyforms.is_reflect_y,
    );
    same_len!(
        d.art_mesh_keyforms.opacities.len(),
        d.art_mesh_keyforms.draw_orders,
        d.art_mesh_keyforms.position_sources_starts,
    );
    same_len!(
        d.keyform_bindings
            .parameter_binding_index_sources_starts
            .len(),
        d.keyform_bindings.parameter_binding_index_sources_counts,
    );
    same_len!(
        d.parameter_bindings.keys_sources_starts.len(),
        d.parameter_bindings.keys_sources_counts,
    );
    same_len!(
        d.draw_order_groups.object_sources_starts.len(),
        d.draw_order_groups.object_sources_counts,
        d.draw_order_groups.object_sources_total_counts,
        d.draw_order_groups.maximum_draw_orders,
        d.draw_order_groups.minimum_draw_orders,
    );
    same_len!(
        d.draw_order_group_objects.types.len(),
        d.draw_order_group_objects.indices,
        d.draw_order_group_objects.self_indices,
    );
    same_len!(
        d.glues.ids.len(),
        d.glues.keyform_binding_sources_indices,
        d.glues.keyform_sources_starts,
        d.glues.keyform_sources_counts,
        d.glues.art_mesh_indices_a,
        d.glues.art_mesh_indices_b,
        d.glues.info_sources_starts,
        d.glues.info_sources_counts,
    );
    same_len!(d.glue_infos.weights.len(), d.glue_infos.vertex_indices);
    same_len!(
        d.keyform_colors.multiply_reds.len(),
        d.keyform_colors.multiply_greens,
        d.keyform_colors.multiply_blues,
    );
    same_len!(
        d.keyform_colors.screen_reds.len(),
        d.keyform_colors.screen_greens,
        d.keyform_colors.screen_blues,
    );
    same_len!(
        d.blend_shapes.parameter_binding_keys_sources_starts.len(),
        d.blend_shapes.parameter_binding_keys_sources_counts,
        d.blend_shapes.parameter_binding_base_key_indices,
    );
    same_len!(
        d.blend_shapes
            .keyform_binding_parameter_binding_sources_indices
            .len(),
        d.blend_shapes.keyform_binding_keyform_sources_starts,
        d.blend_shapes.keyform_binding_keyform_sources_counts,
        d.blend_shapes
            .keyform_binding_constraint_index_sources_starts,
        d.blend_shapes
            .keyform_binding_constraint_index_sources_counts,
    );
    same_len!(
        d.blend_shapes.warp_deformer_target_indices.len(),
        d.blend_shapes.warp_deformer_keyform_binding_sources_starts,
        d.blend_shapes.warp_deformer_keyform_binding_sources_counts,
    );
    same_len!(
        d.blend_shapes.art_mesh_target_indices.len(),
        d.blend_shapes.art_mesh_keyform_binding_sources_starts,
        d.blend_shapes.art_mesh_keyform_binding_sources_counts,
    );
    same_len!(
        d.blend_shapes.constraint_parameter_indices.len(),
        d.blend_shapes.constraint_value_sources_starts,
        d.blend_shapes.constraint_value_sources_counts,
    );
    same_len!(
        d.blend_shapes.constraint_value_keys.len(),
        d.blend_shapes.constraint_value_weights,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{TABLE, TABLE_SLOTS};

    fn id(s: &str) -> Id {
        let mut b = [0u8; 64];
        b[..s.len()].copy_from_slice(s.as_bytes());
        Id(b)
    }

    /// A tiny hand-built model.
    fn synthetic() -> Moc3WriteData {
        Moc3WriteData {
            canvas: CanvasInfo {
                pixels_per_unit: 1000.0,
                x_origin: 500.0,
                y_origin: 500.0,
                canvas_width: 1000.0,
                canvas_height: 1000.0,
                canvas_flags: 0,
            },
            parts: PartsSection {
                ids: vec![id("Part1")],
                keyform_binding_sources_indices: vec![0],
                keyform_sources_starts: vec![0],
                // One parameter is bound, so the part needs 2^1 keyforms.
                keyform_sources_counts: vec![2],
                is_visible: vec![1],
                is_enabled: vec![1],
                parent_part_indices: vec![-1],
            },
            parameters: ParametersSection {
                ids: vec![id("Param1")],
                max_values: vec![1.0],
                min_values: vec![0.0],
                default_values: vec![0.0],
                is_repeat: vec![0],
                decimal_places: vec![2],
                binding_sources_starts: vec![0],
                binding_sources_counts: vec![1],
                types: vec![0],
                blend_shape_binding_sources_starts: vec![0],
                blend_shape_binding_sources_counts: vec![0],
                extension_keys_sources_starts: vec![0],
                extension_keys_sources_counts: vec![2],
            },
            art_meshes: ArtMeshesSection {
                ids: vec![id("ArtMesh1")],
                keyform_binding_sources_indices: vec![0],
                keyform_sources_starts: vec![0],
                keyform_sources_counts: vec![2],
                is_visible: vec![1],
                is_enabled: vec![1],
                parent_part_indices: vec![0],
                parent_deformer_indices: vec![-1],
                texture_nums: vec![0],
                flags: vec![0],
                vertex_counts: vec![3],
                uv_sources_starts: vec![0],
                vertex_index_sources_starts: vec![0],
                vertex_index_sources_counts: vec![3],
                mask_sources_starts: vec![0],
                mask_sources_counts: vec![0],
                keyform_color_sources_starts: vec![0],
            },
            part_keyform_draw_orders: vec![500.0, 500.0],
            art_mesh_keyforms: ArtMeshKeyformsSection {
                opacities: vec![1.0, 0.5],
                draw_orders: vec![500.0, 500.0],
                // in units of f32: keyform 1 starts at Vec2 index 3
                position_sources_starts: vec![0, 6],
            },
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(0.0, 2.0),
            ],
            parameter_binding_indices: vec![0],
            keyform_bindings: KeyformBindingsSection {
                parameter_binding_index_sources_starts: vec![0],
                parameter_binding_index_sources_counts: vec![1],
            },
            parameter_bindings: ParameterBindingsSection {
                keys_sources_starts: vec![0],
                keys_sources_counts: vec![2],
            },
            keys: vec![0.0, 1.0],
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ],
            vertex_indices: vec![0, 1, 2],
            draw_order_groups: DrawOrderGroupsSection {
                object_sources_starts: vec![0],
                object_sources_counts: vec![1],
                object_sources_total_counts: vec![1],
                maximum_draw_orders: vec![1000],
                minimum_draw_orders: vec![0],
            },
            draw_order_group_objects: DrawOrderGroupObjectsSection {
                types: vec![0],
                indices: vec![0],
                self_indices: vec![-1],
            },
            keyform_colors: KeyformColorsSection {
                multiply_reds: vec![1.0, 1.0],
                multiply_greens: vec![1.0, 1.0],
                multiply_blues: vec![1.0, 1.0],
                screen_reds: vec![0.0, 0.0],
                screen_greens: vec![0.0, 0.0],
                screen_blues: vec![0.0, 0.0],
            },
            blend_shapes: BlendShapesSection {
                constraint_sources_indices: vec![0],
                constraint_parameter_indices: vec![0],
                constraint_value_sources_starts: vec![0],
                constraint_value_sources_counts: vec![2],
                constraint_value_keys: vec![0.0, 1.0],
                constraint_value_weights: vec![1.0, 1.0],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn synthetic_writes_and_parses() {
        let data = synthetic();
        let bytes = write_moc3(&data);
        let read = Moc3::new(&bytes).expect("written file failed to parse");

        assert_eq!(read.version(), Version::V4_02);
        assert_eq!(read.counts().parts(), 1);
        assert_eq!(read.counts().parameters(), 1);
        assert_eq!(read.counts().art_meshes(), 1);
        assert_eq!(read.counts().art_mesh_keyforms(), 2);
        assert_eq!(read.counts().keyform_positions(), 12); // f32 units
        assert_eq!(read.counts().uvs(), 6); // f32 units
        assert_eq!(read.counts().blend_shape_constraints(), 1);

        assert_eq!(read.canvas_info(), data.canvas);
        assert_eq!(read.part_ids()[0].name(), "Part1");
        assert_eq!(read.parameter_ids()[0].name(), "Param1");
        assert_eq!(read.art_mesh_ids()[0].name(), "ArtMesh1");
        assert_eq!(read.keys(), &[0.0, 1.0]);
        assert_eq!(read.positions(), data.positions.as_slice());
        assert_eq!(read.uvs(), data.uvs.as_slice());
        assert_eq!(read.vertex_indices(), &[0, 1, 2]);
        assert_eq!(read.art_mesh_keyform_opacities(), &[1.0, 0.5]);
        assert_eq!(read.art_mesh_keyform_position_sources_starts(), &[0, 6]);
        assert_eq!(
            read.blend_shape_constraint_value_keys(),
            Some(&[0.0, 1.0][..])
        );
        assert_eq!(
            read.parameter_extension_keys_sources_counts(),
            Some(&[2u32][..])
        );
    }

    #[test]
    fn empty_model_writes_and_parses() {
        let bytes = write_moc3(&Moc3WriteData::default());
        let read = Moc3::new(&bytes).expect("empty file failed to parse");
        assert_eq!(read.counts().parts(), 0);
        assert!(read.part_ids().is_empty());
        assert!(read.positions().is_empty());
    }

    /// Assert that every accessor of two views returns identical data.
    fn assert_views_equal(a: &Moc3<'_>, b: &Moc3<'_>) {
        macro_rules! same {
            ($($f:ident),+ $(,)?) => {
                $(assert_eq!(a.$f(), b.$f(), stringify!($f));)+
            };
        }

        assert_eq!(a.canvas_info(), b.canvas_info());

        same!(
            // parts
            part_ids,
            part_keyform_binding_sources_indices,
            part_keyform_sources_starts,
            part_keyform_sources_counts,
            part_is_visible,
            part_is_enabled,
            part_parent_part_indices,
            // deformers
            deformer_ids,
            deformer_keyform_binding_sources_indices,
            deformer_is_visible,
            deformer_is_enabled,
            deformer_parent_part_indices,
            deformer_parent_deformer_indices,
            deformer_types,
            deformer_specific_sources_indices,
            // warp deformers
            warp_deformer_keyform_binding_sources_indices,
            warp_deformer_keyform_sources_starts,
            warp_deformer_keyform_sources_counts,
            warp_deformer_vertex_counts,
            warp_deformer_rows,
            warp_deformer_columns,
            // rotation deformers
            rotation_deformer_keyform_binding_sources_indices,
            rotation_deformer_keyform_sources_starts,
            rotation_deformer_keyform_sources_counts,
            rotation_deformer_base_angles,
            // art meshes
            art_mesh_ids,
            art_mesh_keyform_binding_sources_indices,
            art_mesh_keyform_sources_starts,
            art_mesh_keyform_sources_counts,
            art_mesh_is_visible,
            art_mesh_is_enabled,
            art_mesh_parent_part_indices,
            art_mesh_parent_deformer_indices,
            art_mesh_texture_nums,
            art_mesh_flags,
            art_mesh_vertex_counts,
            art_mesh_uv_sources_starts,
            art_mesh_vertex_index_sources_starts,
            art_mesh_vertex_index_sources_counts,
            art_mesh_mask_sources_starts,
            art_mesh_mask_sources_counts,
            // parameters
            parameter_ids,
            parameter_max_values,
            parameter_min_values,
            parameter_default_values,
            parameter_is_repeat,
            parameter_decimal_places,
            parameter_binding_sources_starts,
            parameter_binding_sources_counts,
            // keyforms
            part_keyform_draw_orders,
            warp_deformer_keyform_opacities,
            warp_deformer_keyform_position_sources_starts,
            rotation_deformer_keyform_opacities,
            rotation_deformer_keyform_angles,
            rotation_deformer_keyform_x_origin,
            rotation_deformer_keyform_y_origin,
            rotation_deformer_keyform_scales,
            rotation_deformer_keyform_is_reflect_x,
            rotation_deformer_keyform_is_reflect_y,
            art_mesh_keyform_opacities,
            art_mesh_keyform_draw_orders,
            art_mesh_keyform_position_sources_starts,
            // flat data
            positions,
            parameter_binding_indices,
            keyform_binding_parameter_binding_index_sources_starts,
            keyform_binding_parameter_binding_index_sources_counts,
            parameter_binding_keys_sources_starts,
            parameter_binding_keys_sources_counts,
            keys,
            uvs,
            vertex_indices,
            art_mesh_mask_source_indices,
            // draw order groups
            draw_order_group_object_sources_starts,
            draw_order_group_object_sources_counts,
            draw_order_group_object_sources_total_counts,
            draw_order_group_maximum_draw_orders,
            draw_order_group_minimum_draw_orders,
            draw_order_group_object_types,
            draw_order_group_object_indices,
            draw_order_group_object_self_indices,
            // glues
            glue_ids,
            glue_keyform_binding_sources_indices,
            glue_keyform_sources_starts,
            glue_keyform_sources_counts,
            glue_art_mesh_indices_a,
            glue_art_mesh_indices_b,
            glue_info_sources_starts,
            glue_info_sources_counts,
            glue_info_weights,
            glue_info_vertex_indices,
            glue_keyform_intensities,
        );

        if a.version() < Version::V4_02 {
            return;
        }

        same!(
            warp_deformer_is_new_deformer,
            parameter_extension_keys_sources_starts,
            parameter_extension_keys_sources_counts,
            warp_deformer_keyform_color_sources_start,
            rotation_deformer_keyform_color_sources_start,
            art_mesh_keyform_color_sources_start,
            keyform_multiply_colors_red,
            keyform_multiply_colors_green,
            keyform_multiply_colors_blue,
            keyform_screen_colors_red,
            keyform_screen_colors_green,
            keyform_screen_colors_blue,
            parameter_types,
            parameter_blend_shape_binding_sources_starts,
            parameter_blend_shape_binding_sources_counts,
            blend_shape_parameter_binding_keys_sources_starts,
            blend_shape_parameter_binding_keys_sources_counts,
            blend_shape_parameter_binding_base_key_indices,
            blend_shape_keyform_binding_parameter_binding_sources_indices,
            blend_shape_keyform_binding_keyform_sources_starts,
            blend_shape_keyform_binding_keyform_sources_counts,
            blend_shape_keyform_binding_constraint_index_sources_starts,
            blend_shape_keyform_binding_constraint_index_sources_counts,
            blend_shape_warp_deformer_target_indices,
            blend_shape_warp_deformer_keyform_binding_sources_starts,
            blend_shape_warp_deformer_keyform_binding_sources_counts,
            blend_shape_art_mesh_target_indices,
            blend_shape_art_mesh_keyform_binding_sources_starts,
            blend_shape_art_mesh_keyform_binding_sources_counts,
            blend_shape_constraint_sources_indices,
            blend_shape_constraint_parameter_indices,
            blend_shape_constraint_value_sources_starts,
            blend_shape_constraint_value_sources_counts,
            blend_shape_constraint_value_keys,
            blend_shape_constraint_value_weights,
        );
    }

    /// Some asserts that the cubism runtime enforces.
    fn assert_runtime_layout(file: &[u8]) {
        let len = file.len();
        let slot = |i: usize| -> usize {
            u32::from_le_bytes(file[TABLE + i * 4..TABLE + i * 4 + 4].try_into().unwrap()) as usize
        };

        let count_info = slot(0);
        assert!(
            count_info >= RUNTIME_DATA_START,
            "count_info at {count_info} overlaps the runtime data at {RUNTIME_DATA_START}",
        );
        assert!(
            file[RUNTIME_DATA_START..count_info].iter().all(|&b| b == 0),
            "the runtime data region must be left zeroed",
        );
        assert!(
            len >= MIN_FILE_SIZE,
            "file of {len} bytes is too small to load"
        );

        let mut previous = 0;
        for i in 0..TABLE_SLOTS {
            let offset = slot(i);
            assert_eq!(
                offset % 8,
                0,
                "section {i} at {offset} is not 8-byte aligned"
            );
            assert!(
                offset <= len,
                "section {i} at {offset} runs past the {len}-byte file"
            );
            // Slots past the end of this version's table are left zero.
            if offset == 0 {
                continue;
            }
            assert!(
                offset >= previous,
                "section {i} at {offset} precedes section {previous}"
            );
            previous = offset;
        }
    }

    #[test]
    fn written_file_has_a_loadable_layout() {
        assert_runtime_layout(&write_moc3(&synthetic()));
        assert_runtime_layout(&write_moc3(&Moc3WriteData::default()));
    }
}
