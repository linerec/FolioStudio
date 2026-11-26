//! Paragraph option property names and checked API values.
use serde_json::Value;

pub struct Property {
    pub api: &'static str,
    pub xml: &'static str,
    pub default: &'static str,
    integer: bool,
    min: f64,
    max: f64,
    choices: &'static [(&'static str, &'static str)],
}
macro_rules! number {
    ($api:literal, $xml:literal, $default:literal, $min:literal, $max:literal) => {
        Property { api: $api, xml: $xml, default: $default, integer: false, min: $min, max: $max, choices: &[] }
    };
}
macro_rules! integer {
    ($api:literal, $xml:literal, $default:literal, $min:literal, $max:literal) => {
        Property { api: $api, xml: $xml, default: $default, integer: true, min: $min, max: $max, choices: &[] }
    };
}
macro_rules! flag {
    ($api:literal, $xml:literal, $default:literal) => { number!($api, $xml, $default, 0.0, 0.0) };
}
pub const PROPERTIES: &[Property] = &[
    number!("minimumWordSpacing", "MinimumWordSpacing", "80", 0.0, 1000.0),
    number!("desiredWordSpacing", "DesiredWordSpacing", "100", 0.0, 1000.0),
    number!("maximumWordSpacing", "MaximumWordSpacing", "133", 0.0, 1000.0),
    number!("minimumLetterSpacing", "MinimumLetterSpacing", "0", -100.0, 500.0),
    number!("desiredLetterSpacing", "DesiredLetterSpacing", "0", -100.0, 500.0),
    number!("maximumLetterSpacing", "MaximumLetterSpacing", "0", -100.0, 500.0),
    number!("minimumGlyphScaling", "MinimumGlyphScaling", "100", 50.0, 200.0),
    number!("desiredGlyphScaling", "DesiredGlyphScaling", "100", 50.0, 200.0),
    number!("maximumGlyphScaling", "MaximumGlyphScaling", "100", 50.0, 200.0),
    Property { api: "singleWordJustification", xml: "SingleWordJustification", default: "FullyJustified", integer: false, min: 0.0, max: 0.0,
        choices: &[("LEFT_ALIGN", "LeftAlign"), ("CENTER_ALIGN", "CenterAlign"), ("RIGHT_ALIGN", "RightAlign"), ("FULLY_JUSTIFIED", "FullyJustified")] },
    flag!("hyphenation", "Hyphenation", "true"),
    integer!("hyphenateWordsLongerThan", "HyphenateWordsLongerThan", "5", 3.0, 25.0),
    integer!("hyphenateAfterFirst", "HyphenateAfterFirst", "2", 1.0, 15.0),
    integer!("hyphenateBeforeLast", "HyphenateBeforeLast", "2", 1.0, 15.0),
    integer!("hyphenateLadderLimit", "HyphenateLadderLimit", "3", 0.0, 25.0),
    flag!("hyphenateCapitalizedWords", "HyphenateCapitalizedWords", "true"),
    flag!("hyphenateLastWord", "HyphenateLastWord", "true"),
    flag!("keepLinesTogether", "KeepLinesTogether", "false"),
    flag!("keepAllLinesTogether", "KeepAllLinesTogether", "false"),
    integer!("keepFirstLines", "KeepFirstLines", "2", 1.0, 50.0),
    integer!("keepLastLines", "KeepLastLines", "2", 1.0, 50.0),
    integer!("keepWithNext", "KeepWithNext", "0", 0.0, 5.0),
    number!("hyphenationZone", "HyphenationZone", "36", 0.0, 8640.0),
    integer!("hyphenWeight", "HyphenWeight", "5", 0.0, 10.0),
    flag!("hyphenateAcrossColumns", "HyphenateAcrossColumns", "true"),
    Property { api: "startParagraph", xml: "StartParagraph", default: "Anywhere", integer: false, min: 0.0, max: 0.0,
        choices: &[("ANYWHERE", "Anywhere"), ("NEXT_COLUMN", "NextColumn"), ("NEXT_FRAME", "NextFrame"), ("NEXT_PAGE", "NextPage")] },
    flag!("ignoreEdgeAlignment", "IgnoreEdgeAlignment", "false"),
    // Drop caps: lines 0..=25 and characters 0..=150 are the ranges of InDesign's Paragraph panel fields.
    integer!("dropCapLines", "DropCapLines", "0", 0.0, 25.0),
    integer!("dropCapCharacters", "DropCapCharacters", "0", 0.0, 150.0),
    // Bit field: 1 Align Left Edge (InDesign's default), 2 Scale for Descenders (InDesign DOM: `dropcapDetail`).
    integer!("dropCapDetail", "DropcapDetail", "1", 0.0, 3.0),
    // Paragraph panel menu ▸ Auto Tate-Chu-Yoko (vertical text, vtext-c2): digits 0 (off) to 9, and whether Roman letters count.
    integer!("autoTcy", "AutoTcy", "0", 0.0, 9.0),
    flag!("autoTcyIncludeRoman", "AutoTcyIncludeRoman", "false"),
];
pub fn property(api: &str) -> Option<&'static Property> {
    PROPERTIES.iter().find(|p| p.api == api)
}
impl Property {
    pub fn encode(&self, value: &Value) -> Option<String> {
        if matches!(self.default, "true" | "false") { return value.as_bool().map(|v| v.to_string()); }
        if !self.choices.is_empty() { return self.choices.iter().find(|(api, _)| Some(*api) == value.as_str()).map(|(_, xml)| (*xml).into()); }
        value.as_f64().filter(|n| n.is_finite() && *n >= self.min && *n <= self.max && (!self.integer || n.fract() == 0.0)).map(|n| n.to_string())
    }
    pub fn decode(&self, value: &str) -> Value {
        let value = if value.is_empty() { self.default } else { value };
        if matches!(self.default, "true" | "false") { return Value::Bool(value == "true"); }
        if !self.choices.is_empty() { return self.choices.iter().find(|(_, xml)| *xml == value).map(|(api, _)| Value::from(*api)).unwrap_or(Value::Null); }
        value.parse::<f64>().ok().map(Value::from).unwrap_or(Value::Null)
    }
}
/// Validate the effective values, including inherited values, as one transaction.
pub fn valid_ranges<'a>(get: impl Fn(&str) -> Option<&'a str>) -> bool {
    PROPERTIES[..9].chunks_exact(3).all(|group| {
        let values: Option<Vec<f64>> = group.iter().map(|p| get(p.xml).unwrap_or(p.default).parse().ok()).collect();
        values.is_some_and(|v| v[0] <= v[1] && v[1] <= v[2])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_bounds_and_order() {
        for (key, good, bad) in [("minimumWordSpacing", 0, -1), ("maximumWordSpacing", 1000, 1001),
            ("minimumLetterSpacing", -100, -101), ("maximumLetterSpacing", 500, 501),
            ("minimumGlyphScaling", 50, 49), ("maximumGlyphScaling", 200, 201)] {
            let p = property(key).unwrap();
            assert!(p.encode(&good.into()).is_some());
            assert!(p.encode(&bad.into()).is_none());
            assert!(p.encode(&"120%".into()).is_none());
        }
        assert!(valid_ranges(|_| None));
        // composer-c1: the hyphen weight slider is an integer 0-10 (InDesign rejects -1, 11 and 5.5); across columns is a flag
        let w = property("hyphenWeight").unwrap();
        assert_eq!((w.xml, w.default), ("HyphenWeight", "5"));
        assert_eq!(w.encode(&0.into()).as_deref(), Some("0"));
        assert_eq!(w.encode(&10.into()).as_deref(), Some("10"));
        assert!(w.encode(&(-1).into()).is_none() && w.encode(&11.into()).is_none() && w.encode(&5.5.into()).is_none());
        let across = property("hyphenateAcrossColumns").unwrap();
        assert_eq!((across.xml, across.default), ("HyphenateAcrossColumns", "true"));
        assert_eq!(across.decode(""), Value::Bool(true));
        assert_eq!(across.encode(&false.into()).as_deref(), Some("false"));
        assert!(!valid_ranges(|key| (key == "MinimumWordSpacing").then_some("101")));
    }
}
