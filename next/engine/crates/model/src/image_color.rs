use serde::{Deserialize, Serialize};

/// The placed raster's source profile and rendering intent, as stored in IDML.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageColor {
    pub profile: String,
    pub intent: String,
}

pub const DOCUMENT_PROFILE: &str = "$ID/Use Document Default";
pub const INTENTS: &[&str] = &[
    "UseColorSettings", "Perceptual", "RelativeColorimetric", "Saturation", "AbsoluteColorimetric",
];

impl Default for ImageColor {
    fn default() -> Self {
        Self { profile: DOCUMENT_PROFILE.into(), intent: "UseColorSettings".into() }
    }
}
