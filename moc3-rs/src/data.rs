use binrw::{args, helpers::count_with, BinRead, FilePtr32, NullString};
use glam::Vec2;
use modular_bitfield::{bitfield, BitfieldSpecifier};

#[binrw::parser(reader, endian)]
fn vec2_parser() -> binrw::BinResult<Vec2> {
    <[f32; 2] as BinRead>::read_options(reader, endian, ()).map(|x| x.into())
}

#[derive(BinRead, Debug)]
#[br(magic = b"MOC3")]
pub struct Header {
    pub version: Version,
    pub big_endian: u8,
}

#[derive(BinRead, Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[br(repr = u8)]
pub enum Version {
    V3_00 = 1,
    V3_03 = 2,
    V4_00 = 3,
    V4_02 = 4,
}

#[derive(BinRead, Debug)]
#[br(import {
    version: Version
})]
pub struct CountInfoTable {
    pub parts: u32,
    pub deformers: u32,
    pub warp_deformers: u32,
    pub rotation_deformers: u32,
    pub art_meshes: u32,
    pub parameters: u32,
    pub part_keyforms: u32,
    pub warp_deformer_keyforms: u32,
    pub rotation_deformer_keyforms: u32,
    pub art_mesh_keyforms: u32,
    pub keyform_positions: u32,
    pub parameter_binding_indices: u32,
    pub keyform_bindings: u32,
    pub parameter_bindings: u32,
    pub keys: u32,
    pub uvs: u32,
    pub vertex_indices: u32,
    pub art_mesh_masks: u32,
    pub draw_order_groups: u32,
    pub draw_order_group_objects: u32,
    pub glues: u32,
    pub glue_infos: u32,
    pub glue_keyforms: u32,

    #[br(if(version >= Version::V4_02))]
    pub keyform_multiply_colors: u32,
    #[br(if(version >= Version::V4_02))]
    pub keyform_screen_colors: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_parameter_bindings: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_keyform_bindings: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_warp_deformers: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_art_meshes: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_constraint_indices: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_constraints: u32,
    #[br(if(version >= Version::V4_02))]
    pub blend_shape_constraint_values: u32,
}

#[derive(BinRead, Debug)]
pub struct CanvasInfo {
    pub pixels_per_unit: f32,
    pub x_origin: f32,
    pub y_origin: f32,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub canvas_flags: u8, // TODO
}

#[derive(BinRead, Debug)]
pub struct Id {
    #[br(pad_size_to = 64)]
    pub name: NullString,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct PartOffsets {
    // FilePtr to count * 8 bytes of 0s
    pub data: u32,
    #[br(args { inner: args! { count } })]
    pub ids: FilePtr32<Vec<Id>>,
    #[br(args { inner: args! { count } })]
    pub keyform_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_visible: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_enabled: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub parent_part_indices: FilePtr32<Vec<i32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct DeformerOffsets {
    // FilePtr to count * 8 bytes of 0s
    pub data: u32,
    #[br(args { inner: args! { count } })]
    pub ids: FilePtr32<Vec<Id>>,
    #[br(args { inner: args! { count } })]
    pub keyform_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_visible: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_enabled: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub parent_part_indices: FilePtr32<Vec<i32>>,
    #[br(args { inner: args! { count } })]
    pub parent_deformer_indices: FilePtr32<Vec<i32>>,
    #[br(args { inner: args! { count } })]
    pub types: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub specific_sources_indices: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct WarpDeformerOffsets {
    #[br(args { inner: args! { count } })]
    pub keyform_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub vertex_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub rows: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub columns: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct RotationDeformerOffsets {
    #[br(args { inner: args! { count } })]
    pub keyform_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub base_angles: FilePtr32<Vec<f32>>,
}

#[derive(BitfieldSpecifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum BlendMode {
    Normal = 0,
    Additive = 1 << 0,
    Multiplicative = 1 << 1,
}

#[bitfield(filled = false)]
#[derive(BinRead, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[br(try_map = Self::from_bytes)]
pub struct ArtMeshFlags {
    pub blend_mode: BlendMode,
    pub double_sided: bool,
    pub inverted: bool,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ArtMeshOffsets {
    pub runtime_ignored: [u32; 4],
    #[br(args { inner: args! { count } })]
    pub ids: FilePtr32<Vec<Id>>,
    #[br(args { inner: args! { count } })]
    pub keyform_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_visible: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_enabled: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub parent_part_indices: FilePtr32<Vec<i32>>,
    #[br(args { inner: args! { count } })]
    pub parent_deformer_indices: FilePtr32<Vec<i32>>,
    #[br(args { inner: args! { count } })]
    pub texture_nums: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub art_mesh_flags: FilePtr32<Vec<ArtMeshFlags>>,
    #[br(args { inner: args! { count } })]
    pub vertex_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub uv_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub vertex_index_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub vertex_index_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub art_mesh_mask_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub art_mesh_mask_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ParameterOffsets {
    pub unused: u32,
    #[br(args { inner: args! { count } })]
    pub ids: FilePtr32<Vec<Id>>,
    #[br(args { inner: args! { count } })]
    pub max_values: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub min_values: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub default_values: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub is_repeat: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub decimal_places: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub parameter_binding_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub parameter_binding_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[br(repr = u32)]
#[non_exhaustive]
pub enum ParameterType {
    Normal = 0,
    BlendShape = 1,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ParameterOffsetsV4_02 {
    #[br(args { inner: args! { count } })]
    pub parameter_types: FilePtr32<Vec<ParameterType>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_parameter_binding_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_parameter_binding_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct BlendShapeParameterBindingOffsets {
    #[br(args { inner: args! { count } })]
    pub keys_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keys_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub base_key_indices: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct BlendShapeKeyformBindingOffsets {
    #[br(args { inner: args! { count } })]
    pub blend_shape_parameter_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_blend_shape_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_blend_shape_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_constraint_index_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_constraint_index_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct BlendShapeOffsets {
    #[br(args { inner: args! { count } })]
    pub target_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_keyform_binding_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_keyform_binding_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct BlendShapeConstraintIndicesOffsets {
    #[br(args { inner: args! { count } })]
    pub blend_shape_constraint_sources_indices: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct BlendShapeConstraintOffsets {
    #[br(args { inner: args! { count } })]
    pub parameter_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_constraint_value_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub blend_shape_constraint_value_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct BlendShapeConstraintValueOffsets {
    #[br(args { inner: args! { count } })]
    pub keys: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub weights: FilePtr32<Vec<f32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct PartKeyformOffsets {
    #[br(args { inner: args! { count } })]
    pub draw_orders: FilePtr32<Vec<f32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct WarpDeformerKeyformOffsets {
    #[br(args { inner: args! { count } })]
    pub opacities: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_position_sources_starts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct WarpDeformerKeyformOffsetsV303 {
    #[br(args { inner: args! { count } })]
    pub is_new_deformerrs: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct WarpDeformerKeyformOffsetsV402 {
    #[br(args { inner: args! { count } })]
    pub keyform_color_sources_start: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct RotationDeformerKeyformOffsets {
    #[br(args { inner: args! { count } })]
    pub opacities: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub angles: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub x_origin: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub y_origin: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub scales: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub is_reflect_x: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub is_reflect_y: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct RotationDeformerKeyformOffsetsV402 {
    #[br(args { inner: args! { count } })]
    pub keyform_color_sources_start: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ArtMeshKeyformOffsets {
    #[br(args { inner: args! { count } })]
    pub opacities: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub draw_orders: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_position_sources_starts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ArtMeshKeyformOffsetsV402 {
    #[br(args { inner: args! { count } })]
    pub keyform_color_sources_start: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct KeyformPositionOffsets {
    #[br(parse_with = FilePtr32::with(count_with(count / 2, vec2_parser)))]
    pub coords: FilePtr32<Vec<Vec2>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ParameterBindingIndicesOffsets {
    #[br(args { inner: args! { count } })]
    pub binding_sources_indices: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct KeyformBindingOffsets {
    #[br(args { inner: args! { count } })]
    pub parameter_binding_index_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub parameter_binding_index_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ParameterBindingOffsets {
    #[br(args { inner: args! { count } })]
    pub keys_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keys_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct KeyOffsets {
    #[br(args { inner: args! { count } })]
    pub values: FilePtr32<Vec<f32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct UvOffsets {
    #[br(parse_with = FilePtr32::with(count_with(count / 2, vec2_parser)))]
    pub uvs: FilePtr32<Vec<Vec2>>, // TODO: Vec2
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct VertexIndicesOffsets {
    #[br(args { inner: args! { count } })]
    pub indices: FilePtr32<Vec<u16>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ArtMeshMaskOffsets {
    #[br(args { inner: args! { count } })]
    pub art_mesh_source_indices: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct DrawOrderGroupOffsets {
    #[br(args { inner: args! { count } })]
    pub object_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub object_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub object_sources_total_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub maximum_draw_orders: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub minimum_draw_orders: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[br(repr = u32)]
pub enum DrawOrderGroupObjectType {
    ArtMesh = 0,
    Part = 1,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct DrawOrderGroupObjectOffsets {
    #[br(args { inner: args! { count } })]
    pub types: FilePtr32<Vec<DrawOrderGroupObjectType>>,
    #[br(args { inner: args! { count } })]
    pub indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub self_indices: FilePtr32<Vec<i32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct GlueOffsets {
    pub unused: u32,
    #[br(args { inner: args! { count } })]
    pub ids: FilePtr32<Vec<Id>>,
    #[br(args { inner: args! { count } })]
    pub keyform_binding_sources_indices: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keyform_sources_counts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub art_mesh_indices_a: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub art_mesh_indices_b: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub glue_info_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub glue_info_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct GlueInfoOffsets {
    #[br(args { inner: args! { count } })]
    pub weights: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub vertex_indices: FilePtr32<Vec<u16>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct GlueKeyformOffsets {
    #[br(args { inner: args! { count } })]
    pub intensities: FilePtr32<Vec<f32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    version: Version
})]
pub struct SectionOffsetTable {
    #[br(deref_now)]
    #[br(args {
        inner: args! { version }
    })]
    pub count_info: FilePtr32<CountInfoTable>,
    pub canvas_info: FilePtr32<CanvasInfo>,
    #[br(count(count_info.parts))]
    pub parts: PartOffsets,
    #[br(count(count_info.deformers))]
    pub deformers: DeformerOffsets,
    #[br(count(count_info.warp_deformers))]
    pub warp_deformers: WarpDeformerOffsets,
    #[br(count(count_info.rotation_deformers))]
    pub rotation_deformers: RotationDeformerOffsets,
    #[br(count(count_info.art_meshes))]
    pub art_meshes: ArtMeshOffsets,
    #[br(count(count_info.parameters))]
    pub parameters: ParameterOffsets,
    #[br(count(count_info.part_keyforms))]
    pub part_keyforms: PartKeyformOffsets,
    #[br(count(count_info.warp_deformer_keyforms))]
    pub warp_deformer_keyforms: WarpDeformerKeyformOffsets,
    #[br(count(count_info.rotation_deformer_keyforms))]
    pub rotation_deformer_keyforms: RotationDeformerKeyformOffsets,
    #[br(count(count_info.art_mesh_keyforms))]
    pub art_mesh_keyforms: ArtMeshKeyformOffsets,
    #[br(count(count_info.keyform_positions))]
    pub keyform_positions: KeyformPositionOffsets,
    #[br(count(count_info.parameter_binding_indices))]
    pub parameter_binding_indices: ParameterBindingIndicesOffsets,
    #[br(count(count_info.keyform_bindings))]
    pub keyform_bindings: KeyformBindingOffsets,
    #[br(count(count_info.parameter_bindings))]
    pub parameter_bindings: ParameterBindingOffsets,
    #[br(count(count_info.keys))]
    pub keys: KeyOffsets,
    #[br(count(count_info.uvs))]
    pub uvs: UvOffsets,
    #[br(count(count_info.vertex_indices))]
    pub vertex_indices: VertexIndicesOffsets,
    #[br(count(count_info.art_mesh_masks))]
    pub art_mesh_masks: ArtMeshMaskOffsets,
    #[br(count(count_info.draw_order_groups))]
    pub draw_order_groups: DrawOrderGroupOffsets,
    #[br(count(count_info.draw_order_group_objects))]
    pub draw_order_group_objects: DrawOrderGroupObjectOffsets,
    #[br(count(count_info.glues))]
    pub glues: GlueOffsets,
    #[br(count(count_info.glue_infos))]
    pub glue_infos: GlueInfoOffsets,
    #[br(count(count_info.glue_keyforms))]
    pub glue_keyforms: GlueKeyformOffsets,

    #[br(if(version >= Version::V3_03), count(count_info.warp_deformers))]
    pub warp_deformer_keyforms_v303: Option<WarpDeformerKeyformOffsetsV303>,

    #[br(if(version >= Version::V4_02), count(count_info.parameters))]
    pub parameter_extensions: Option<ParameterExtensionsOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.warp_deformers))]
    pub warp_deformer_keyforms_v402: Option<WarpDeformerKeyformOffsetsV402>,
    #[br(if(version >= Version::V4_02), count(count_info.rotation_deformers))]
    pub rotation_deformer_keyforms_v402: Option<RotationDeformerKeyformOffsetsV402>,
    #[br(if(version >= Version::V4_02), count(count_info.art_meshes))]
    pub art_mesh_deformer_keyforms_v402: Option<ArtMeshKeyformOffsetsV402>,
    #[br(if(version >= Version::V4_02), count(count_info.keyform_multiply_colors))]
    pub keyform_multiply_colors: Option<KeyformColorOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.keyform_screen_colors))]
    pub keyform_screen_colors: Option<KeyformColorOffsets>,

    #[br(if(version >= Version::V4_02), count(count_info.parameters))]
    pub parameters_v402: Option<ParameterOffsetsV4_02>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_parameter_bindings))]
    pub blend_shape_parameter_bindings: Option<BlendShapeParameterBindingOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_keyform_bindings))]
    pub blend_shape_keyform_bindings: Option<BlendShapeKeyformBindingOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_warp_deformers))]
    pub blend_shape_warp_deformers: Option<BlendShapeOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_art_meshes))]
    pub blend_shape_art_meshes: Option<BlendShapeOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_constraint_indices))]
    pub blend_shape_constraint_indices: Option<BlendShapeConstraintIndicesOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_constraints))]
    pub blend_shape_constraints: Option<BlendShapeConstraintOffsets>,
    #[br(if(version >= Version::V4_02), count(count_info.blend_shape_constraint_values))]
    pub blend_shape_constraint_values: Option<BlendShapeConstraintValueOffsets>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct KeyformColorOffsets {
    #[br(args { inner: args! { count } })]
    pub red: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub green: FilePtr32<Vec<f32>>,
    #[br(args { inner: args! { count } })]
    pub blue: FilePtr32<Vec<f32>>,
}

#[derive(BinRead, Debug)]
#[br(import {
    count: usize
})]
pub struct ParameterExtensionsOffsets {
    // FilePtr to count * 8 bytes of 0s
    pub data: u32,
    #[br(args { inner: args! { count } })]
    pub keys_sources_starts: FilePtr32<Vec<u32>>,
    #[br(args { inner: args! { count } })]
    pub keys_sources_counts: FilePtr32<Vec<u32>>,
}

#[derive(BinRead, Debug)]
pub struct Moc3Data {
    #[br(pad_size_to = 64)]
    pub header: Header,
    #[br(args {
        version: header.version
    })]
    pub table: SectionOffsetTable,
}

impl Moc3Data {
    pub fn keys(&self) -> &[f32] {
        &self.table.keys.values
    }

    pub fn vertex_indices(&self) -> &[u16] {
        &self.table.vertex_indices.indices
    }

    // L2D indexes this in elements of f32, not Vec2, so divide
    // indices by 2 before using this.
    pub fn positions(&self) -> &[Vec2] {
        self.table.keyform_positions.coords.value.as_ref().unwrap()
    }

    pub fn uvs(&self) -> &[Vec2] {
        // TODO: nya want deref
        self.table.uvs.uvs.value.as_ref().unwrap()
    }
}
