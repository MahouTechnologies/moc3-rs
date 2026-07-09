use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayInfoParameter {
    pub id: String,
    #[serde(default)]
    pub group_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayInfoParameterGroup {
    pub id: String,
    #[serde(default)]
    pub group_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayInfoPart {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Cdi3Data {
    pub version: u32,
    #[serde(default)]
    pub parameters: Vec<DisplayInfoParameter>,
    #[serde(default)]
    pub parameter_groups: Vec<DisplayInfoParameterGroup>,
    #[serde(default)]
    pub parts: Vec<DisplayInfoPart>,
    #[serde(default)]
    pub combined_parameters: Vec<Vec<String>>,
}
