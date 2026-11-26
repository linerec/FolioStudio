//! Footnote options and addresses of their nested text. Addresses are transient, never IDML Self IDs.
use crate::{color, props::Props, styles, warn::Warnings, xml, xml::Element};
use std::collections::BTreeMap;
mod options;
pub use options::option_value;
pub const DEFAULT_OPTIONS: &str = include_str!("footnotes/defaults.xml");

pub fn text_id(story: &str, index: usize) -> String { format!("{story}/footnote/{index}") }

pub fn address(id: &str) -> Option<(&str, usize)> {
    let (story, index) = id.rsplit_once("/footnote/")?;
    Some((story, index.parse().ok()?))
}

pub fn settings(source: &str) -> Result<BTreeMap<String, String>, String> {
    let root = xml::parse(source).map_err(|e| e.0)?;
    let defaults = xml::parse(DEFAULT_OPTIONS).map_err(|e| e.0)?;
    let f = root.child("FootnoteOption").unwrap_or(&defaults);
    let mut out: BTreeMap<_, _> = f.attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    if let Some(p) = f.child("Properties") {
        for e in p.elements() { out.insert(e.name.to_owned(), e.text()); }
    }
    Ok(out)
}

pub fn marker_position(settings: &BTreeMap<String, String>) -> Result<Option<&'static str>, String> {
    match settings.get("MarkerPositioning").map(String::as_str).unwrap_or("SuperscriptMarker") {
        "SuperscriptMarker" => Ok(Some("Superscript")),
        "SubscriptMarker" => Ok(Some("Subscript")),
        "NormalMarker" => Ok(None),
        other => Err(format!("unsupported footnote marker positioning: {other}")),
    }
}

pub(crate) fn options(roots: &[&Element<'_>], colors: &color::Colors, w: &mut Warnings) -> folio_model::FootnoteOptions {
    let mut o = folio_model::FootnoteOptions::default();
    let defaults = xml::parse(DEFAULT_OPTIONS).expect("footnote defaults XML");
    let f = roots.iter().find_map(|r| r.child("FootnoteOption")).unwrap_or(&defaults);
    let s = |k: &str| f.str(k).map(str::to_owned);
    if let Some(v) = f.f64("StartAt") { o.start_at = v.round().max(0.0) as u32; }
    if let Some(v) = s("FootnoteNumberingStyle") { o.numbering = v; }
    if let Some(v) = s("RestartNumbering") { o.restart = v; }
    o.prefix = s("Prefix").unwrap_or_default();
    o.suffix = s("Suffix").unwrap_or_default();
    if let Some(v) = s("ShowPrefixSuffix") { o.show_prefix_suffix = v; }
    o.space_between = f.f64("SpaceBetween").unwrap_or(0.0);
    o.spacer = f.f64("Spacer").unwrap_or(0.0);
    o.first_baseline = match f.str("FootnoteFirstBaselineOffset") {
        Some("AscentOffset") => folio_model::FirstBaseline::Ascent,
        Some("CapHeight") => folio_model::FirstBaseline::CapHeight,
        Some("XHeight") => folio_model::FirstBaseline::XHeight,
        Some("EmboxHeight") => folio_model::FirstBaseline::EmBoxHeight,
        Some("FixedHeight") => folio_model::FirstBaseline::Fixed,
        _ => folio_model::FirstBaseline::Leading,
    };
    o.min_first_baseline_offset = f.f64("FootnoteMinimumFirstBaselineOffset").unwrap_or(0.0);
    o.straddle = f.bool("EnableStraddling").unwrap_or(false);
    o.end_of_story = f.bool("EosPlacement").unwrap_or(false);
    o.no_splitting = f.bool("NoSplitting").unwrap_or(false);
    let mut rule = |prefix: &str, width| {
        let key = |name| format!("{prefix}Rule{name}");
        (f.bool(&key("On")) != Some(false)).then(|| {
            let kind = f.str(&key("Type")).map(styles::stroke_type_name).unwrap_or_else(|| "Solid".into());
            folio_model::FootnoteRule {
                weight: f.f64(&key("LineWeight")).unwrap_or(1.0),
                paint: colors.paint(Some(f.str(&key("Color")).unwrap_or("Color/Black")), f.f64(&key("Tint")), w),
                offset: f.f64(&key("Offset")).unwrap_or(0.0),
                left_indent: f.f64(&key("LeftIndent")).unwrap_or(0.0),
                length: f.f64(&key("Width")).unwrap_or(width),
                overprint: f.bool(&key("Overprint")).unwrap_or(false),
                stroke_type: kind,
            }
        })
    };
    o.rule = rule("", 72.0);
    o.continuing_rule = rule("Continuing", 288.0);
    o
}
