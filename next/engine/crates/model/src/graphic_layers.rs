//! Per-placement layer visibility saved in GraphicLayerOption.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    pub update_link: String,
    pub layers: Vec<Layer>,
    /// `AppliedLayerComp` of the `LayerCompOption`: the position in `comps` of the layer comp
    /// last chosen, -1 for none.
    #[serde(default = "no_comp")]
    pub applied_comp: i32,
    /// The layer comps a PSD offers, in the order of the file (taken from the file, not saved).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comps: Vec<Comp>,
}

fn no_comp() -> i32 {
    -1
}

impl Default for Options {
    fn default() -> Options {
        Options { update_link: String::new(), layers: Vec::new(), applied_comp: -1, comps: Vec::new() }
    }
}

/// A layer comp of a PSD: its name and the layers (ids of the layer list) it shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Comp {
    pub id: i32,
    pub name: String,
    pub shown: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: u32,
    pub name: String,
    pub original: bool,
    pub visible: bool,
    /// The layers inside a PSD layer group, top first (InDesign nests them in the group's
    /// `GraphicLayer`; the object layer options list shows the group only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Layer>,
    /// An adjustment layer of a PSD (`GraphicLayer/@AdjustmentLayer`; a fill layer is not one).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adjustment: bool,
}
