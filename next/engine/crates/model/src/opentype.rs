use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenTypeFeature {
    DiscretionaryLigature, Fraction, Ordinal, Swash, Titling, ContextualAlternate,
    SlashedZero, Historical, OverlapSwash, StylisticAlternate, JustificationAlternate,
    StretchedAlternate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FigureStyle {
    #[default]
    Default,
    TabularLining, ProportionalOldstyle, ProportionalLining, TabularOldstyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionalForm {
    #[default]
    None,
    Calculate, Initial, Medial, Final, Isolated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenType {
    pub features: BTreeSet<OpenTypeFeature>,
    pub stylistic_sets: u32,
    pub figure_style: FigureStyle,
    pub positional_form: PositionalForm,
}

impl Default for OpenType {
    fn default() -> Self {
        Self {
            features: BTreeSet::from([OpenTypeFeature::ContextualAlternate]),
            stylistic_sets: 0,
            figure_style: FigureStyle::Default,
            positional_form: PositionalForm::None,
        }
    }
}
