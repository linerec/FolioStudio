//! Paragraph decoration property names and IDML value conversion.
use serde_json::Value;

#[derive(Clone, Copy)]
enum Kind { Toggle, Number(f64, f64), Width, Color, Stroke, Enum(&'static [(&'static str, &'static str)]) }

const TOP_ORIGIN: &[(&str, &str)] = &[("ASCENT_TOP_ORIGIN", "AscentTopOrigin"), ("BASELINE_TOP_ORIGIN", "BaselineTopOrigin"), ("LEADING_TOP_ORIGIN", "LeadingTopOrigin")];
const BOTTOM_ORIGIN: &[(&str, &str)] = &[("DESCENT_BOTTOM_ORIGIN", "DescentBottomOrigin"), ("BASELINE_BOTTOM_ORIGIN", "BaselineBottomOrigin")];
const NO_NUMBER: (f64, f64) = (f64::NEG_INFINITY, f64::INFINITY);

pub struct Property {
    pub api: &'static str,
    pub xml: &'static str,
    kind: Kind,
}

macro_rules! prop {
    ($api:literal, $xml:literal, $kind:expr) => {
        Property { api: $api, xml: $xml, kind: $kind }
    };
}

pub const PROPERTIES: &[Property] = &[
    prop!("ruleAbove", "RuleAbove", Kind::Toggle),
    prop!("ruleAboveLineWeight", "RuleAboveLineWeight", Kind::Number(0., f64::INFINITY)),
    prop!("ruleAboveColor", "RuleAboveColor", Kind::Color),
    prop!("ruleAboveTint", "RuleAboveTint", Kind::Number(-1., 100.)),
    prop!("ruleAboveOffset", "RuleAboveOffset", Kind::Number(f64::NEG_INFINITY, f64::INFINITY)),
    prop!("ruleAboveLeftIndent", "RuleAboveLeftIndent", Kind::Number(f64::NEG_INFINITY, f64::INFINITY)),
    prop!("ruleAboveRightIndent", "RuleAboveRightIndent", Kind::Number(f64::NEG_INFINITY, f64::INFINITY)),
    prop!("ruleAboveWidth", "RuleAboveWidth", Kind::Width),
    prop!("ruleBelow", "RuleBelow", Kind::Toggle),
    prop!("ruleBelowLineWeight", "RuleBelowLineWeight", Kind::Number(0., f64::INFINITY)),
    prop!("ruleBelowColor", "RuleBelowColor", Kind::Color),
    prop!("ruleBelowTint", "RuleBelowTint", Kind::Number(-1., 100.)),
    prop!("ruleBelowOffset", "RuleBelowOffset", Kind::Number(f64::NEG_INFINITY, f64::INFINITY)),
    prop!("ruleBelowLeftIndent", "RuleBelowLeftIndent", Kind::Number(f64::NEG_INFINITY, f64::INFINITY)),
    prop!("ruleBelowRightIndent", "RuleBelowRightIndent", Kind::Number(f64::NEG_INFINITY, f64::INFINITY)),
    prop!("ruleBelowWidth", "RuleBelowWidth", Kind::Width),
    prop!("ruleAboveOverprint", "RuleAboveOverprint", Kind::Toggle),
    prop!("ruleBelowOverprint", "RuleBelowOverprint", Kind::Toggle),
    prop!("keepRuleAboveInFrame", "KeepRuleAboveInFrame", Kind::Toggle),
    prop!("ruleAboveType", "RuleAboveType", Kind::Stroke),
    prop!("ruleBelowType", "RuleBelowType", Kind::Stroke),
    prop!("ruleAboveGapColor", "RuleAboveGapColor", Kind::Color),
    prop!("ruleAboveGapTint", "RuleAboveGapTint", Kind::Number(-1., 100.)),
    prop!("ruleAboveGapOverprint", "RuleAboveGapOverprint", Kind::Toggle),
    prop!("ruleBelowGapColor", "RuleBelowGapColor", Kind::Color),
    prop!("ruleBelowGapTint", "RuleBelowGapTint", Kind::Number(-1., 100.)),
    prop!("ruleBelowGapOverprint", "RuleBelowGapOverprint", Kind::Toggle),
    prop!("paragraphShadingOn", "ParagraphShadingOn", Kind::Toggle),
    prop!("paragraphShadingColor", "ParagraphShadingColor", Kind::Color),
    prop!("paragraphShadingTint", "ParagraphShadingTint", Kind::Number(-1., 100.)),
    prop!("paragraphShadingOverprint", "ParagraphShadingOverprint", Kind::Toggle),
    prop!("paragraphShadingTopOffset", "ParagraphShadingTopOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphShadingLeftOffset", "ParagraphShadingLeftOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphShadingBottomOffset", "ParagraphShadingBottomOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphShadingRightOffset", "ParagraphShadingRightOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphShadingTopOrigin", "ParagraphShadingTopOrigin", Kind::Enum(TOP_ORIGIN)),
    prop!("paragraphShadingBottomOrigin", "ParagraphShadingBottomOrigin", Kind::Enum(BOTTOM_ORIGIN)),
    prop!("paragraphShadingWidth", "ParagraphShadingWidth", Kind::Width),
    prop!("paragraphShadingClipToFrame", "ParagraphShadingClipToFrame", Kind::Toggle),
    prop!("paragraphShadingSuppressPrinting", "ParagraphShadingSuppressPrinting", Kind::Toggle),
    prop!("paragraphBorderOn", "ParagraphBorderOn", Kind::Toggle),
    prop!("paragraphBorderColor", "ParagraphBorderColor", Kind::Color),
    prop!("paragraphBorderTint", "ParagraphBorderTint", Kind::Number(-1., 100.)),
    prop!("paragraphBorderOverprint", "ParagraphBorderOverprint", Kind::Toggle),
    prop!("paragraphBorderTopLineWeight", "ParagraphBorderTopLineWeight", Kind::Number(0., f64::INFINITY)),
    prop!("paragraphBorderLeftLineWeight", "ParagraphBorderLeftLineWeight", Kind::Number(0., f64::INFINITY)),
    prop!("paragraphBorderBottomLineWeight", "ParagraphBorderBottomLineWeight", Kind::Number(0., f64::INFINITY)),
    prop!("paragraphBorderRightLineWeight", "ParagraphBorderRightLineWeight", Kind::Number(0., f64::INFINITY)),
    prop!("paragraphBorderTopOffset", "ParagraphBorderTopOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphBorderLeftOffset", "ParagraphBorderLeftOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphBorderBottomOffset", "ParagraphBorderBottomOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphBorderRightOffset", "ParagraphBorderRightOffset", Kind::Number(NO_NUMBER.0, NO_NUMBER.1)),
    prop!("paragraphBorderTopOrigin", "ParagraphBorderTopOrigin", Kind::Enum(TOP_ORIGIN)),
    prop!("paragraphBorderBottomOrigin", "ParagraphBorderBottomOrigin", Kind::Enum(BOTTOM_ORIGIN)),
    prop!("paragraphBorderWidth", "ParagraphBorderWidth", Kind::Width),
    prop!("paragraphBorderType", "ParagraphBorderType", Kind::Stroke),
    prop!("paragraphBorderDisplayIfSplits", "ParagraphBorderDisplayIfSplits", Kind::Toggle),
    prop!("paragraphBorderGapColor", "ParagraphBorderGapColor", Kind::Color),
    prop!("paragraphBorderGapTint", "ParagraphBorderGapTint", Kind::Number(-1., 100.)),
    prop!("paragraphBorderGapOverprint", "ParagraphBorderGapOverprint", Kind::Toggle),
    prop!("mergeConsecutiveParaBorders", "MergeConsecutiveParaBorders", Kind::Toggle),
];

/// Order among a Properties element's children: InDesign writes colours, then stroke types.
pub fn xml_order(xml: &str) -> Option<u8> {
    PROPERTIES.iter().find(|p| p.xml == xml).and_then(|p| matches!(p.kind, Kind::Stroke).then_some(4))
}

pub fn property(api: &str) -> Option<&'static Property> {
    PROPERTIES.iter().find(|p| p.api == api)
}

impl Property {
    pub fn is_reference(&self) -> bool { matches!(self.kind, Kind::Color | Kind::Stroke) }

    /// IDML type of the property element: a colour is an object reference except the rule's
    /// "Text Color", which is the string itself.
    pub fn property_type(&self, value: &str) -> &'static str {
        if matches!(self.kind, Kind::Color) && value == "Text Color" { "string" } else { "object" }
    }

    pub fn encode(&self, value: &Value) -> Option<String> {
        Some(match self.kind {
            Kind::Toggle => value.as_bool()?.to_string(),
            Kind::Number(min, max) => {
                let n = value.as_f64()?;
                if !n.is_finite() || n < min || n > max { return None; }
                n.to_string()
            }
            Kind::Width => match value.as_str()? {
                "COLUMN_WIDTH" => "ColumnWidth".into(),
                "TEXT_WIDTH" => "TextWidth".into(),
                _ => return None,
            },
            Kind::Color => {
                let s = value.as_str()?;
                if s == "Text Color" && self.api.starts_with("rule") { return Some(s.into()); }
                if !["Color/", "Swatch/", "Tint/", "Gradient/"].iter().any(|p| s.starts_with(p)) { return None; }
                s.into()
            }
            Kind::Stroke => {
                let s = value.as_str()?;
                if !s.starts_with("StrokeStyle/") { return None; }
                s.into()
            }
            Kind::Enum(values) => {
                let s = value.as_str()?;
                values.iter().find(|(api, _)| *api == s)?.1.into()
            }
        })
    }

    pub fn decode(&self, value: &str) -> Value {
        match self.kind {
            Kind::Toggle => match value { "true" => true.into(), "false" => false.into(), _ => Value::Null },
            Kind::Number(_, _) => value.parse::<f64>().ok().map(Value::from).unwrap_or(Value::Null),
            Kind::Width => match value { "ColumnWidth" => "COLUMN_WIDTH".into(), "TextWidth" => "TEXT_WIDTH".into(), _ => Value::Null },
            Kind::Color | Kind::Stroke => if value.is_empty() { Value::Null } else { value.into() },
            Kind::Enum(values) => values.iter().find(|(_, xml)| *xml == value).map_or(Value::Null, |(api, _)| (*api).into()),
        }
    }
}
