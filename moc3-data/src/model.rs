use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Motion {
    pub file: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_out_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_in_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_sync: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Expression {
    pub name: String,
    pub file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct FileReferences {
    pub moc: PathBuf,
    pub textures: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_info: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_sync: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<Expression>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub motions: BTreeMap<String, Vec<Motion>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pose: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Group {
    pub target: String,
    pub name: String,
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct HitArea {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Model3 {
    pub file_references: FileReferences,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hit_areas: Vec<HitArea>,
    pub version: u8,
}
