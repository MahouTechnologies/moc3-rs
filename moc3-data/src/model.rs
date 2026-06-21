use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Motion {
    pub file: PathBuf,
    pub fade_out_time: Option<f64>,
    pub fade_in_time: Option<f64>,
    pub sound: Option<PathBuf>,
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
    pub physics: Option<PathBuf>,
    pub display_info: PathBuf,
    pub motion_sync: Option<PathBuf>,
    #[serde(default)]
    pub expressions: Vec<Expression>,
    #[serde(default)]
    pub motions: BTreeMap<String, Vec<Motion>>,
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
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub hit_areas: Vec<HitArea>,
    pub version: u8,
}
