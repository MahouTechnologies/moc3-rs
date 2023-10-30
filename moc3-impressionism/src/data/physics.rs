use glam::Vec2;
use serde::{Deserialize, Serialize, Deserializer};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Physics3Data {
    version: usize,
    meta: Physics3Meta,
    physics_settings: Vec<PhysicsSetting>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsSetting {
    id: String,
    #[serde(default)]
    input: Vec<PhysicsInput>,
    #[serde(default)]
    output: Vec<PhysicsOutput>,
    #[serde(default)]
    vertices: Vec<PhysicsVertex>,
    normalization: Option<PhysicsNormalization>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsInput {
    source: PhysicsTarget,
    weight: f32,
    #[serde(rename = "Type")]
    ty: String,
    reflect: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsOutput {
    destination: PhysicsTarget,
    vertex_index: usize,
    scale: f32,
    weight: f32,
    #[serde(rename = "Type")]
    ty: String,
    reflect: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsVertex {
    #[serde(deserialize_with = "deserialize_vec2")]
    position: Vec2,
    mobility: f32,
    delay: f32,
    acceleration: f32,
    radius: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsNormalization {
    position: ParamterData,
    angle: ParamterData,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParamterData {
    minimum: f32,
    maximum: f32,
    default: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsTarget {
    target: String,
    id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Physics3Meta {
    total_input_count: usize,
    total_output_count: usize,
    vertex_count: usize,
    physics_setting_count: usize,
    effective_forces: ForceData,
    physics_dictionary: Vec<PhysicsIdData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsIdData {
    id: String,
    name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ForceData {
    #[serde(default, deserialize_with = "deserialize_vec2")]
    gravity: Vec2,
    #[serde(default, deserialize_with = "deserialize_vec2")]
    wind: Vec2,
}


fn deserialize_vec2<'de, D>(deserializer: D) -> Result<Vec2, D::Error>
    where D: Deserializer<'de> {
    
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Vec2Upper {
        x: f32,
        y: f32,
    }
    let res = Vec2Upper::deserialize(deserializer)?;

    Ok(Vec2::new(res.x, res.y))
}