//! Supported document footnote settings, shared by editing and the oracle adapter.
enum Kind { Number(f64), Bool, Text, Enum(&'static [&'static str]), Style(&'static str) }
struct Spec { key: &'static str, name: &'static str, property: bool, kind: Kind }
use Kind::*;
macro_rules! options {
    ($( $key:literal => $name:literal, $property:literal, $kind:expr; )*) => {
        const OPTIONS: &[Spec] = &[$(Spec { key: $key, name: $name, property: $property, kind: $kind }),*];
    }
}
options! {
    "startAt" => "StartAt", false, Number(1.0);
    "footnoteNumberingStyle" => "FootnoteNumberingStyle", true, Enum(&["Arabic", "LowerRoman", "UpperRoman", "LowerLetters", "UpperLetters", "Asterisks", "Symbols"]);
    "restartNumbering" => "RestartNumbering", true, Enum(&["DontRestart", "PageRestart", "SpreadRestart", "SectionRestart"]);
    "showPrefixSuffix" => "ShowPrefixSuffix", true, Enum(&["NoPrefixSuffix", "PrefixSuffixReference", "PrefixSuffixMarker", "PrefixSuffixBoth"]);
    "markerPositioning" => "MarkerPositioning", true, Enum(&["NormalMarker", "SuperscriptMarker", "SubscriptMarker"]);
    "prefix" => "Prefix", false, Text;
    "suffix" => "Suffix", false, Text;
    "separatorText" => "SeparatorText", false, Text;
    "footnoteTextStyle" => "FootnoteTextStyle", false, Style("ParagraphStyle");
    "footnoteMarkerStyle" => "FootnoteMarkerStyle", false, Style("CharacterStyle");
    "spaceBetween" => "SpaceBetween", false, Number(0.0);
    "spacer" => "Spacer", false, Number(0.0);
    "footnoteFirstBaselineOffset" => "FootnoteFirstBaselineOffset", false, Enum(&["AscentOffset", "CapHeight", "LeadingOffset", "EmboxHeight", "XHeight", "FixedHeight"]);
    "footnoteMinimumFirstBaselineOffset" => "FootnoteMinimumFirstBaselineOffset", false, Number(0.0);
    "eosPlacement" => "EosPlacement", false, Bool;
    "noSplitting" => "NoSplitting", false, Bool;
    "enableStraddling" => "EnableStraddling", false, Bool;
    "ruleOn" => "RuleOn", false, Bool;
    "ruleLineWeight" => "RuleLineWeight", false, Number(0.0);
    "ruleWidth" => "RuleWidth", false, Number(0.0);
    "ruleOffset" => "RuleOffset", false, Number(0.0);
    "ruleLeftIndent" => "RuleLeftIndent", false, Number(0.0);
    "continuingRuleOn" => "ContinuingRuleOn", false, Bool;
    "continuingRuleLineWeight" => "ContinuingRuleLineWeight", false, Number(0.0);
    "continuingRuleWidth" => "ContinuingRuleWidth", false, Number(0.0);
    "continuingRuleOffset" => "ContinuingRuleOffset", false, Number(0.0);
    "continuingRuleLeftIndent" => "ContinuingRuleLeftIndent", false, Number(0.0);
}

pub fn option_value(key: &str, value: &str) -> Result<(&'static str, String, bool), String> {
    let spec = OPTIONS.iter().find(|s| s.key == key || s.name == key).ok_or_else(|| format!("unsupported footnote option: {key}"))?;
    let invalid = || format!("invalid {key}: {value}");
    let normalized = match spec.kind {
        Text => {
            if value.chars().any(|c| c < ' ' && c != '\t') { return Err(invalid()); }
            value.to_owned()
        }
        Bool if matches!(value, "true" | "false") => value.to_owned(),
        Bool => return Err(invalid()),
        Number(min) => {
            let n: f64 = value.parse().map_err(|_| invalid())?;
            if !n.is_finite() || n < min || (spec.name == "StartAt" && (n.fract() != 0.0 || n > u32::MAX as f64)) { return Err(invalid()); }
            n.to_string()
        }
        Enum(allowed) => {
            let token = value.rsplit('.').next().unwrap_or(value).replace('_', "");
            allowed.iter().find(|s| s.eq_ignore_ascii_case(&token)).ok_or_else(invalid)?.to_string()
        }
        Style(kind) => {
            if value.starts_with(&format!("{kind}/")) { value.to_owned() }
            else if value == "[Basic Paragraph]" && kind == "ParagraphStyle" { "ParagraphStyle/$ID/NormalParagraphStyle".into() }
            else if value == "[None]" && kind == "CharacterStyle" { "CharacterStyle/$ID/[No character style]".into() }
            else { format!("{kind}/{value}") }
        }
    };
    Ok((spec.name, normalized, spec.property))
}
