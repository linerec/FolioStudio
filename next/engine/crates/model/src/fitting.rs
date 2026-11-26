use serde::{Deserialize, Serialize};

/// Frame fitting preferences, with crops in points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FrameFitting {
    pub auto_fit: bool,
    pub top_crop: f64,
    pub bottom_crop: f64,
    pub left_crop: f64,
    pub right_crop: f64,
    pub fitting_alignment: String,
    pub fitting_on_empty_frame: String,
}

impl Default for FrameFitting {
    fn default() -> Self {
        Self { auto_fit: false, top_crop: 0.0, bottom_crop: 0.0, left_crop: 0.0, right_crop: 0.0,
            fitting_alignment: "CenterAnchor".into(), fitting_on_empty_frame: "None".into() }
    }
}
