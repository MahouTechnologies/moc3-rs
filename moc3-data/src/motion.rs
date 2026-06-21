use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserData {
    pub time: f64,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Curve {
    pub target: String,
    pub id: String,
    pub fade_in_time: Option<f64>,
    pub fade_out_time: Option<f64>,
    pub segments: Vec<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Motion3Meta {
    pub duration: f64,
    pub fps: f64,
    #[serde(rename = "Loop")]
    pub loop_: Option<bool>,
    pub are_beziers_restricted: Option<bool>,
    pub fade_in_time: Option<f64>,
    pub fade_out_time: Option<f64>,
    pub curve_count: u64,
    pub total_segment_count: u64,
    pub total_point_count: u64,
    pub user_data_count: Option<u64>,
    pub total_user_data_size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Motion3Data {
    pub curves: Vec<Curve>,
    pub meta: Motion3Meta,
    #[serde(default)]
    pub user_data: Vec<UserData>,
    pub version: u8,
}
