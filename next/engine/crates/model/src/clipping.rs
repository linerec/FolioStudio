use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub kind: String,
    pub threshold: f64,
    pub tolerance: f64,
    pub inset: f64,
    pub invert: bool,
    pub inside_edges: bool,
    pub restrict_to_frame: bool,
    pub high_resolution: bool,
    pub path_name: String,
    pub index: i32,
}
