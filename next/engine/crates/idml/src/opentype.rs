//! OpenType property names, value conversion and model inheritance.
use crate::props::Props;
use folio_model::opentype::{FigureStyle, OpenType, OpenTypeFeature, PositionalForm};
use serde_json::Value;

#[derive(Clone, Copy)]
enum Kind { Toggle(OpenTypeFeature), Sets, Figure, Form }

pub struct Property {
    pub api: &'static str,
    pub xml: &'static str,
    pub default: &'static str,
    kind: Kind,
}

macro_rules! toggle {
    ($name:ident, $api:literal, $xml:literal, $default:literal) => {
        Property { api: $api, xml: $xml, default: $default, kind: Kind::Toggle(OpenTypeFeature::$name) }
    };
}

pub const PROPERTIES: &[Property] = &[
    toggle!(DiscretionaryLigature, "otfDiscretionaryLigature", "OTFDiscretionaryLigature", "false"),
    toggle!(Fraction, "otfFraction", "OTFFraction", "false"),
    toggle!(Ordinal, "otfOrdinal", "OTFOrdinal", "false"),
    toggle!(Swash, "otfSwash", "OTFSwash", "false"),
    toggle!(Titling, "otfTitling", "OTFTitling", "false"),
    toggle!(ContextualAlternate, "otfContextualAlternate", "OTFContextualAlternate", "true"),
    toggle!(SlashedZero, "otfSlashedZero", "OTFSlashedZero", "false"),
    toggle!(Historical, "otfHistorical", "OTFHistorical", "false"),
    toggle!(OverlapSwash, "otfOverlapSwash", "OTFOverlapSwash", "false"),
    toggle!(StylisticAlternate, "otfStylisticAlternate", "OTFStylisticAlternate", "false"),
    toggle!(JustificationAlternate, "otfJustificationAlternate", "OTFJustificationAlternate", "false"),
    toggle!(StretchedAlternate, "otfStretchedAlternate", "OTFStretchedAlternate", "false"),
    Property { api: "otfStylisticSets", xml: "OTFStylisticSets", default: "0", kind: Kind::Sets },
    Property { api: "otfFigureStyle", xml: "OTFFigureStyle", default: "Default", kind: Kind::Figure },
    Property { api: "positionalForm", xml: "PositionalForm", default: "None", kind: Kind::Form },
];

const FIGURES: &[(&str, &str, FigureStyle)] = &[
    ("DEFAULT_VALUE", "Default", FigureStyle::Default),
    ("TABULAR_LINING", "TabularLining", FigureStyle::TabularLining),
    ("PROPORTIONAL_OLDSTYLE", "ProportionalOldstyle", FigureStyle::ProportionalOldstyle),
    ("PROPORTIONAL_LINING", "ProportionalLining", FigureStyle::ProportionalLining),
    ("TABULAR_OLDSTYLE", "TabularOldstyle", FigureStyle::TabularOldstyle),
];
const FORMS: &[(&str, &str, PositionalForm)] = &[
    ("NONE", "None", PositionalForm::None),
    ("CALCULATE", "Calculate", PositionalForm::Calculate),
    ("INITIAL", "Initial", PositionalForm::Initial),
    ("MEDIAL", "Medial", PositionalForm::Medial),
    ("FINAL", "Final", PositionalForm::Final),
    ("ISOLATED", "Isolated", PositionalForm::Isolated),
];

pub fn property(api: &str) -> Option<&'static Property> {
    PROPERTIES.iter().find(|p| p.api == api)
}

impl Property {
    pub fn encode(&self, value: &Value) -> Option<String> {
        Some(match self.kind {
            Kind::Toggle(_) => value.as_bool()?.to_string(),
            Kind::Sets => value.as_u64().filter(|n| *n <= 0x1fffff)?.to_string(),
            Kind::Figure => FIGURES.iter().find(|v| Some(v.0) == value.as_str())?.1.into(),
            Kind::Form => FORMS.iter().find(|v| Some(v.0) == value.as_str())?.1.into(),
        })
    }

    pub fn decode(&self, value: &str) -> Value {
        match self.kind {
            Kind::Toggle(_) => match value { "true" => true.into(), "false" => false.into(), _ => Value::Null },
            Kind::Sets => value.parse::<u32>().ok().map(Value::from).unwrap_or(Value::Null),
            Kind::Figure => FIGURES.iter().find(|v| v.1 == value).map(|v| Value::from(v.0)).unwrap_or(Value::Null),
            Kind::Form => FORMS.iter().find(|v| v.1 == value).map(|v| Value::from(v.0)).unwrap_or(Value::Null),
        }
    }
}

pub(crate) fn apply<'a>(p: &impl Props<'a>, a: &mut OpenType) {
    for prop in PROPERTIES {
        let Some(value) = p.str(prop.xml) else { continue };
        match prop.kind {
            Kind::Toggle(feature) => match value {
                "true" => { a.features.insert(feature); }
                "false" => { a.features.remove(&feature); }
                _ => {}
            },
            Kind::Sets => if let Ok(n) = value.parse() { a.stylistic_sets = n; },
            Kind::Figure => if let Some(v) = FIGURES.iter().find(|v| v.1 == value) { a.figure_style = v.2; },
            Kind::Form => if let Some(v) = FORMS.iter().find(|v| v.1 == value) { a.positional_form = v.2; },
        }
    }
}
