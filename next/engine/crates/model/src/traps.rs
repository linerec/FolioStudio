use serde::{Deserialize, Serialize};

/// The two presets InDesign puts in every document (Resources/Styles.xml `Self`).
pub const DEFAULT_PRESET: &str = "TrapPreset/$ID/kDefaultTrapStyleName";
pub const NO_TRAP_PRESET: &str = "TrapPreset/$ID/k[No Trap Preset]";

/// A trap preset. In a command a missing number/flag or an empty text means "keep" when editing and
/// InDesign's default when creating (`Preset::with_defaults`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub default_width: Option<f64>,
    pub black_width: Option<f64>,
    pub join: String,
    pub end: String,
    pub objects_to_images: Option<bool>,
    pub images_to_images: Option<bool>,
    pub internal_images: Option<bool>,
    pub one_bit_images: Option<bool>,
    pub image_placement: String,
    pub step_threshold: Option<f64>,
    pub black_color_threshold: Option<f64>,
    pub black_density: Option<f64>,
    pub sliding_threshold: Option<f64>,
    pub color_reduction: Option<f64>,
}

impl Preset {
    /// A preset added with only a name: InDesign's values (`doc.trapPresets.add({name})`, panels-traps.ko.md).
    pub fn with_defaults(name: &str) -> Preset {
        Preset {
            id: String::new(), name: name.into(), default_width: Some(0.25), black_width: Some(0.5),
            join: "MiterEndJoin".into(), end: "MiterTrapEnds".into(),
            objects_to_images: Some(true), images_to_images: Some(true), internal_images: Some(false), one_bit_images: Some(true),
            image_placement: "CenterEdges".into(), step_threshold: Some(10.0), black_color_threshold: Some(100.0),
            black_density: Some(1.6), sliding_threshold: Some(70.0), color_reduction: Some(100.0),
        }
    }
    /// The two presets InDesign creates in every document.
    pub fn built_in(&self) -> bool { self.id == DEFAULT_PRESET || self.id == NO_TRAP_PRESET }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Assignment {
    pub page: String,
    pub preset: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub presets: Vec<Preset>,
    pub assignments: Vec<Assignment>,
}
