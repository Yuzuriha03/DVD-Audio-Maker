use serde::{Deserialize, Serialize};
use serde_json::Map;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Patch {
    pub packet_index: i32,
    pub presentation_time: f64,
    pub position: i64,
    pub size: i32,
    pub sample_count: i32,
    pub end_bit: i64,
    pub previous_bits: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Repair {
    pub original_path: String,
    pub repaired_path: String,
    pub patches: Vec<Patch>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Issue {
    pub level: String,
    pub title: String,
    pub path: String,
    pub reason: String,
    pub detail: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManifestTrack {
    pub n: usize,
    pub src: String,
    pub name: String,
    pub title: String,
    pub date: String,
    pub track: String,
    pub album: String,
    pub dur: f64,
    pub resample_to: Option<i32>,
    pub repaired: usize,
    pub repair_detail: Option<Vec<String>>,
    pub orig_src: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManifestGroup {
    pub sr: i32,
    pub bits: i32,
    pub count: usize,
    pub files: Vec<ManifestTrack>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Result {
    pub manifest: Map<String, serde_json::Value>,
    pub issues: Vec<Issue>,
    pub checked_tracks: usize,
    pub repairs: Vec<Repair>,
    pub failure_count: usize,
    pub warning_count: usize,
}
