use crate::xml;
use std::collections::BTreeMap;

pub const DEFAULT_OPTIONS: &str = include_str!("endnotes-defaults.xml");

pub fn settings(source: &str) -> Result<BTreeMap<String, String>, String> {
    let root = xml::parse(source).map_err(|e| e.0)?;
    let defaults = xml::parse(DEFAULT_OPTIONS).map_err(|e| e.0)?;
    let option = root.child("EndnoteOption").unwrap_or(&defaults);
    let mut out: BTreeMap<_, _> = defaults.attrs.iter().map(|(k,v)| (k.to_string(),v.to_string())).collect();
    out.extend(option.attrs.iter().map(|(k,v)| (k.to_string(),v.to_string())));
    if let Some(properties) = option.child("Properties") {
        for value in properties.elements() { out.insert(value.name.to_owned(),value.text()); }
    }
    Ok(out)
}

/// How the document's endnote numbers print (its `EndnoteOption`, else the defaults).
pub fn options(designmap: &xml::Element<'_>) -> folio_model::EndnoteOptions {
    let mut out = folio_model::EndnoteOptions::default();
    let Some(option) = designmap.child("EndnoteOption") else { return out };
    let property = |name: &str| option.child("Properties").and_then(|p| p.child(name)).map(|e| e.text()).filter(|s| !s.is_empty());
    if let Some(v) = property("EndnoteNumberingStyle") { out.numbering = v; }
    if let Some(v) = property("RestartEndnoteNumbering") { out.restart = v; }
    if let Some(v) = property("ShowEndnotePrefixSuffix") { out.show_prefix_suffix = v; }
    out.prefix = option.attr("EndnotePrefix").unwrap_or_default().to_owned();
    out.suffix = option.attr("EndnoteSuffix").unwrap_or_default().to_owned();
    out
}

/// The endnote option enumerations (the values InDesign 2025's DOM offers, IDML spelling; artifacts/build-b3-20261023/endnotes/options-probe.json).
const NUMBERING: &[&str] = &["Arabic", "UpperRoman", "LowerRoman", "UpperLetters", "LowerLetters", "Symbols", "Asterisks", "Kanji", "FullWidthArabic",
    "SingleLeadingZeros", "DoubleLeadingZeros", "ArabicAlifBaTah", "ArabicAbjad", "HebrewBiblical", "HebrewNonStandard"];
const ENUMS: &[(&str, &[&str])] = &[
    ("EndnoteNumberingStyle", NUMBERING),
    ("RestartEndnoteNumbering", &["Continuous", "StoryRestart"]),
    ("EndnoteMarkerPositioning", &["NormalMarker", "SuperscriptMarker", "SubscriptMarker", "RubyMarker"]),
    ("ScopeValue", &["EndnoteDocumentScope", "StoryScope"]),
    ("FrameCreateOption", &["NewPage", "LoadEndnotePlaceGun"]),
    ("ShowEndnotePrefixSuffix", &["NoPrefixSuffix", "PrefixSuffixReference", "PrefixSuffixMarker", "PrefixSuffixBoth"]),
];

/// Whether an option is written in the `EndnoteOption`'s `Properties` (an enumeration) rather than as an attribute.
pub fn is_property(name: &str) -> bool { ENUMS.iter().any(|(n, _)| *n == name) }

pub fn option_value(key: &str, value: &str) -> Result<(&'static str, String), String> {
    let name = match key {
        "endnoteTitle" | "EndnoteTitle" => "EndnoteTitle",
        "startEndnoteNumberAt" | "StartEndnoteNumberAt" => "StartEndnoteNumberAt",
        "endnoteSeparatorText" | "EndnoteSeparatorText" => "EndnoteSeparatorText",
        "endnotePrefix" | "EndnotePrefix" => "EndnotePrefix",
        "endnoteSuffix" | "EndnoteSuffix" => "EndnoteSuffix",
        "endnoteTitleStyle" | "EndnoteTitleStyle" => "EndnoteTitleStyle",
        "endnoteTextStyle" | "EndnoteTextStyle" => "EndnoteTextStyle",
        "endnoteMarkerStyle" | "EndnoteMarkerStyle" => "EndnoteMarkerStyle",
        "endnoteNumberingStyle" | "EndnoteNumberingStyle" => "EndnoteNumberingStyle",
        "restartEndnoteNumbering" | "RestartEndnoteNumbering" => "RestartEndnoteNumbering",
        "endnoteMarkerPositioning" | "EndnoteMarkerPositioning" => "EndnoteMarkerPositioning",
        "scopeValue" | "ScopeValue" => "ScopeValue",
        "frameCreateOption" | "FrameCreateOption" => "FrameCreateOption",
        "showEndnotePrefixSuffix" | "ShowEndnotePrefixSuffix" => "ShowEndnotePrefixSuffix",
        _ => return Err(format!("unsupported endnote option: {key}")),
    };
    if let Some((_, allowed)) = ENUMS.iter().find(|(n, _)| *n == name) {
        // (the DOM spelling, `FootnoteNumberingStyle.UPPER_ROMAN` or `UPPER_ROMAN`, is taken too)
        let token = value.rsplit('.').next().unwrap_or(value).replace('_', "");
        return allowed.iter().find(|s| s.eq_ignore_ascii_case(&token)).map(|s| (name, s.to_string())).ok_or_else(|| format!("invalid {key}: {value}"));
    }
    if let Some(kind) = match name { "EndnoteTitleStyle" | "EndnoteTextStyle" => Some("ParagraphStyle"), "EndnoteMarkerStyle" => Some("CharacterStyle"), _ => None } {
        let style = if value.starts_with(&format!("{kind}/")) { value.to_owned() }
            else if value == "[Basic Paragraph]" && kind == "ParagraphStyle" { "ParagraphStyle/$ID/NormalParagraphStyle".into() }
            else if value == "[None]" && kind == "CharacterStyle" { "CharacterStyle/$ID/[No character style]".into() }
            else { format!("{kind}/{value}") };
        return Ok((name, style));
    }
    if name == "StartEndnoteNumberAt" {
        let n = value.parse::<u32>().map_err(|_| format!("invalid {key}: {value}"))?;
        if !(1..=100000).contains(&n) { return Err(format!("invalid {key}: {value}")); }
        return Ok((name,n.to_string()));
    }
    if value.chars().any(|c| c < ' ' && c != '\t') { return Err(format!("invalid {key}: {value}")); }
    Ok((name,value.to_owned()))
}
