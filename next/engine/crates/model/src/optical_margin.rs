use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpticalMargin {
    pub enabled: bool,
    pub size: f64,
}

impl Default for OpticalMargin {
    fn default() -> Self { Self { enabled: false, size: 12.0 } }
}
