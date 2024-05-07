use glam::Vec2;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Physics3Data {
    pub version: usize,
    pub meta: Physics3Meta,
    pub physics_settings: Vec<PhysicsSetting>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsSetting {
    pub id: String,
    #[serde(default)]
    pub input: Vec<PhysicsInput>,
    #[serde(default)]
    pub output: Vec<PhysicsOutput>,
    #[serde(default)]
    pub vertices: Vec<PhysicsVertex>,
    pub normalization: Option<PhysicsNormalization>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsInput {
    pub source: PhysicsTarget,
    pub weight: f32,
    #[serde(rename = "Type")]
    pub ty: String,
    pub reflect: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsOutput {
    pub destination: PhysicsTarget,
    pub vertex_index: usize,
    pub scale: f32,
    pub weight: f32,
    #[serde(rename = "Type")]
    pub ty: String,
    pub reflect: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsVertex {
    #[serde(deserialize_with = "deserialize_vec2")]
    pub position: Vec2,
    pub mobility: f32,
    pub delay: f32,
    pub acceleration: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsNormalization {
    pub position: ParamterData,
    pub angle: ParamterData,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParamterData {
    pub minimum: f32,
    pub maximum: f32,
    pub default: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsTarget {
    pub target: String,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Physics3Meta {
    pub total_input_count: usize,
    pub total_output_count: usize,
    pub vertex_count: usize,
    pub physics_setting_count: usize,
    pub effective_forces: ForceData,
    pub physics_dictionary: Vec<PhysicsIdData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PhysicsIdData {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ForceData {
    #[serde(default, deserialize_with = "deserialize_vec2")]
    pub gravity: Vec2,
    #[serde(default, deserialize_with = "deserialize_vec2")]
    pub wind: Vec2,
}

fn deserialize_vec2<'de, D>(deserializer: D) -> Result<Vec2, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Vec2Upper {
        x: f32,
        y: f32,
    }
    let res = Vec2Upper::deserialize(deserializer)?;

    Ok(Vec2::new(res.x, res.y))
}
