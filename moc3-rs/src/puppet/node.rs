#[derive(Debug, Clone)]
pub struct DeformerNode {
    pub data: NodeKind,
    pub broad_index: u32,
    pub parent_part_index: i32,
    pub is_enabled: bool,
    pub id: String,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    ArtMesh(ArtMeshData),
    WarpDeformer(WarpDeformerData, u32),
    RotationDeformer(RotationDeformerData, u32),
}

#[derive(Debug, Clone)]
pub struct ArtMeshData {
    pub vertexes: u32,
}

#[derive(Debug, Clone)]
pub struct WarpDeformerData {
    pub rows: u32,
    pub columns: u32,
    pub is_new_deformerr: bool,
}

#[derive(Debug, Clone)]
pub struct RotationDeformerData {
    pub base_angle: f32,
}

#[derive(Debug, Clone)]
pub struct GlueNode {
    pub id: String,
    pub kind_index: u32,
    pub art_mesh_index: [u32; 2],
    pub weights: Vec<f32>,
    pub mesh_indices: Vec<u16>,
}
