#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

#[test]
fn footnote_options_survive_idml_export() {
    let source = support::oracle().join("fixtures/footnotes.idml");
    for (field, attr, values) in [
        ("start_at", "StartAt", ["3", "9"]),
        ("prefix", "Prefix", ["[", "note-"]),
        ("suffix", "Suffix", ["]", "."]),
        ("space_between", "SpaceBetween", ["4.5", "8.25"]),
        ("spacer", "Spacer", ["6.5", "10.25"]),
        ("min_first_baseline_offset", "FootnoteMinimumFirstBaselineOffset", ["3.5", "7.25"]),
        ("straddle", "EnableStraddling", ["true", "false"]),
        ("no_splitting", "NoSplitting", ["true", "false"]),
    ] {
        for value in values {
            let expected = serde_json::from_str::<Value>(value).unwrap_or_else(|_| json!(value));
            support::check(&format!("FootnoteOptions.{field}"), &source, "Resources/Preferences.xml",
                |xml| support::attribute(xml, "<FootnoteOption ", attr, value),
                |doc| serde_json::to_value(&doc.footnotes).unwrap()[field].clone(), expected);
        }
    }
    for (field, property, values) in [
        ("numbering", "FootnoteNumberingStyle", &["Arabic", "UpperRoman", "LowerLetters", "Asterisks"][..]),
        ("restart", "RestartNumbering", &["DontRestart", "PageRestart", "SpreadRestart", "SectionRestart"][..]),
        ("show_prefix_suffix", "ShowPrefixSuffix", &["NoPrefixSuffix", "PrefixSuffixReference", "PrefixSuffixMarker", "PrefixSuffixBoth"][..]),
    ] {
        for value in values {
            support::check(&format!("FootnoteOptions.{field}"), &source, "Resources/Preferences.xml",
                |xml| support::property(xml, "<FootnoteOption ", property, value),
                |doc| serde_json::to_value(&doc.footnotes).unwrap()[field].clone(), json!(value));
        }
    }
    for (value, expected) in [
        ("AscentOffset", "Ascent"), ("CapHeight", "CapHeight"), ("XHeight", "XHeight"),
        ("LeadingOffset", "Leading"), ("EmboxHeight", "EmBoxHeight"), ("FixedHeight", "Fixed"),
    ] {
        support::check("FootnoteOptions.first_baseline", &source, "Resources/Preferences.xml",
            |xml| support::attribute(xml, "<FootnoteOption ", "FootnoteFirstBaselineOffset", value),
            |doc| json!(doc.footnotes.first_baseline), json!(expected));
    }
}

#[test]
fn footnote_rule_fields_survive_idml_export() {
    let source = support::oracle().join("fixtures/footnotes.idml");
    for (field, attr, values) in [
        ("weight", "RuleLineWeight", ["0.5", "2.25"]),
        ("offset", "RuleOffset", ["3.5", "7.25"]),
        ("left_indent", "RuleLeftIndent", ["4.5", "8.25"]),
        ("length", "RuleWidth", ["60.5", "80.25"]),
        ("overprint", "RuleOverprint", ["true", "false"]),
    ] {
        for value in values {
            support::check(&format!("FootnoteRule.{field}"), &source, "Resources/Preferences.xml",
                |xml| support::attribute(xml, "<FootnoteOption ", attr, value),
                |doc| serde_json::to_value(&doc.footnotes.rule).unwrap()[field].clone(),
                serde_json::from_str(value).unwrap());
        }
    }
    for value in ["Solid", "Dashed"] {
        support::check("FootnoteRule.stroke_type", &source, "Resources/Preferences.xml",
            |xml| support::property(xml, "<FootnoteOption ", "RuleType", &format!("StrokeStyle/$ID/{value}")),
            |doc| json!(doc.footnotes.rule.as_ref().unwrap().stroke_type), json!(value));
    }
    for value in ["39.5", "288"] {
        support::check("FootnoteOptions.continuing_rule", &source, "Resources/Preferences.xml",
            |xml| support::attribute(xml, "<FootnoteOption ", "ContinuingRuleWidth", value),
            |doc| json!(doc.footnotes.continuing_rule.as_ref().unwrap().length), json!(value.parse::<f64>().unwrap()));
    }
}
