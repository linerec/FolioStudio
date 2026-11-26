//! Write document display preferences and ruler settings.

use super::*;

/// How a document setting is written, with the range InDesign accepts for it (measured in points: oracle/indesign/probe-grid-prefs.jsx).
enum Pref {
    Switch,
    Number(f64, f64),
    Count(f64, f64),
    Relative,
    Colour,
    Choice(&'static [&'static str]),
}

/// The measurement units the ruler takes in an English InDesign (measured: oracle/indesign/probe-units-prefs.jsx; `Custom`
/// needs custom point sizes and is left out).
const RULER_UNITS: &[&str] = &["Points", "Picas", "Inches", "InchesDecimal", "Millimeters", "Centimeters", "Ciceros", "Agates", "Pixels"];

/// View ▸ Grids & Guides and Preferences ▸ Grids, kept in the document's preferences (oracle: basic-view-prefs, basic-grid-prefs).
pub(super) fn set_view_pref(w: &mut Work<'_>, key: &str, value: &serde_json::Value) -> Result<(), EditError> {
    use Pref::*;
    // the pasteboard also decides how far apart the spreads lie: the page panel's layout code does it
    let bad_pasteboard = |what: &str| EditError::new("invalid", format!("{key}: {what}"));
    match key {
        "pasteboardMargins" => {
            let pair: Vec<f64> = value.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).filter(|x| x.is_finite()).collect()).unwrap_or_default();
            return match pair[..] {
                [h, v] if v >= 0.0 => pages::set_pasteboard(w, Some((h, v)), None),
                _ => Err(bad_pasteboard("horizontal and vertical margin, the vertical one 0 or more")),
            };
        }
        "matchPreviewBackground" => return pages::set_pasteboard(w, None, Some(value.as_bool().ok_or_else(|| bad_pasteboard("on or off"))?)),
        "zeroPoint" => return set_zero_point(w, value),
        _ => {}
    }
    let (element, attr, how) = match key {
        "guidesLocked" => ("GuidePreference", "GuidesLocked", Switch),
        "guidesSnap" => ("GuidePreference", "GuidesSnapto", Switch),
        "guidesInBack" => ("GuidePreference", "GuidesInBack", Switch),
        "columnGuidesLocked" => ("DocumentPreference", "ColumnGuideLocked", Switch),
        "baselineGridShown" => ("GridPreference", "BaselineGridShown", Switch),
        "documentGridShown" => ("GridPreference", "DocumentGridShown", Switch),
        "documentGridSnap" => ("GridPreference", "DocumentGridSnapto", Switch),
        "gridsInBack" => ("GridPreference", "GridsInBack", Switch),
        "baselineStart" => ("GridPreference", "BaselineStart", Number(0.0, 1000.0)),
        "baselineDivision" => ("GridPreference", "BaselineDivision", Number(1.0, 8640.0)),
        "baselineThreshold" => ("GridPreference", "BaselineViewThreshold", Number(5.0, 4000.0)),
        "horizontalDivision" => ("GridPreference", "HorizontalGridlineDivision", Number(0.01, 1000.0)),
        "verticalDivision" => ("GridPreference", "VerticalGridlineDivision", Number(0.01, 1000.0)),
        "horizontalSubdivision" => ("GridPreference", "HorizontalGridSubdivision", Count(1.0, 1000.0)),
        "verticalSubdivision" => ("GridPreference", "VerticalGridSubdivision", Count(1.0, 1000.0)),
        "baselineRelativeTo" => ("GridPreference", "BaselineGridRelativeOption", Relative),
        "baselineColor" => ("GridPreference", "BaselineColor", Colour),
        "gridColor" => ("GridPreference", "GridColor", Colour),
        "horizontalUnits" => ("ViewPreference", "HorizontalMeasurementUnits", Choice(RULER_UNITS)),
        "verticalUnits" => ("ViewPreference", "VerticalMeasurementUnits", Choice(RULER_UNITS)),
        "strokeUnits" => ("ViewPreference", "StrokeMeasurementUnits", Choice(&["Points", "Millimeters"])),
        "rulerOrigin" => ("ViewPreference", "RulerOrigin", Choice(&["SpreadOrigin", "PageOrigin", "SpineOrigin"])),
        "cursorKeyIncrement" => ("ViewPreference", "CursorKeyIncrement", Number(0.001, 100.0)),
        "snapZone" => ("ViewPreference", "GuideSnaptoZone", Count(1.0, 36.0)),
        "leadingKeyIncrement" => ("TextPreference", "LeadingKeyIncrement", Number(0.001, 200.0)),
        "baselineShiftKeyIncrement" => ("TextPreference", "BaselineShiftKeyIncrement", Number(0.001, 200.0)),
        "kerningKeyIncrement" => ("TextPreference", "KerningKeyIncrement", Number(1.0, 100.0)),
        "rulerGuidesThreshold" => ("GuidePreference", "RulerGuidesViewThreshold", Number(5.0, 4000.0)),
        "rulerGuidesColor" => ("GuidePreference", "RulerGuidesColor", Colour),
        "previewBackgroundColor" => ("PasteboardPreference", "PreviewBackgroundColor", Colour),
        "bleedGuideColor" => ("PasteboardPreference", "BleedGuideColor", Colour),
        "slugGuideColor" => ("PasteboardPreference", "SlugGuideColor", Colour),
        _ => return Err(EditError::new("invalid", format!("unknown view setting {key}"))),
    };
    let bad = |what: &str| EditError::new("invalid", format!("{key}: {what}"));
    let part = "Resources/Preferences.xml";
    let t = w.text(part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == element).ok_or_else(|| EditError::new("xml", format!("missing {element}")))?;
    let text = match how {
        Switch => if value.as_bool().ok_or_else(|| bad("on or off"))? { "true" } else { "false" }.to_owned(),
        Number(lo, hi) | Count(lo, hi) => {
            let v = value.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad("a number"))?;
            if v < lo || v > hi {
                return Err(bad(&format!("between {lo} and {hi}")));
            }
            if matches!(how, Count(..)) && v.fract() != 0.0 {
                return Err(bad("a whole number"));
            }
            geom::num(v)
        }
        Relative => match value.as_str() {
            Some("page") => "TopOfPageOfBaselineGridRelativeOption",
            Some("margin") => "TopOfMarginOfBaselineGridRelativeOption",
            _ => return Err(bad("page or margin")),
        }
        .to_owned(),
        Choice(names) => value.as_str().filter(|n| names.contains(n)).ok_or_else(|| bad(&names.join(", ")))?.to_owned(),
        Colour => return set_pref_colour(w, part, &t, &ix, i, attr, value),
    };
    let edit = attr_edit(&ix, &t, i, attr, &text);
    w.splice(part, vec![edit])
}

/// The point the rulers call 0 (`Document/@ZeroPoint`): `[x, y]` in the ruler origin's own coordinates. Only what the rulers, the
/// Control panel and the Info panel show moves; items and guides keep their coordinates (InDesign 2025: `docinput-c3.ko.md`).
fn set_zero_point(w: &mut Work<'_>, value: &serde_json::Value) -> Result<(), EditError> {
    let bad = || EditError::new("invalid", "zeroPoint: two finite numbers [x, y]");
    let pair: Vec<f64> = value.as_array().ok_or_else(bad)?.iter().map(|x| x.as_f64().filter(|x| x.is_finite())).collect::<Option<_>>().ok_or_else(bad)?;
    let [x, y] = pair[..] else { return Err(bad()) };
    let part = "designmap.xml";
    let t = w.text(part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Document").ok_or_else(|| EditError::new("xml", "missing Document"))?;
    let edit = attr_edit(&ix, &t, i, "ZeroPoint", &format!("{} {}", geom::num(x), geom::num(y)));
    w.splice(part, vec![edit])
}

/// An interface colour of a preference element, a `<Properties>` child (`GridColor`, `RulerGuidesColor`, …): an
/// enumeration name, or a list of three numbers for a custom colour (oracle: basic-grid-prefs, basic-units-prefs). A
/// new child goes first for `GridColor` and last otherwise, where InDesign writes it.
pub(super) fn set_pref_colour(w: &mut Work<'_>, part: &str, t: &str, ix: &Index, owner: usize, tag: &str, value: &serde_json::Value) -> Result<(), EditError> {
    let bad = |what: &str| EditError::new("invalid", format!("{tag}: {what}"));
    let line_indent = |at: usize| -> String { t[..at].rsplit('\n').next().unwrap_or("").chars().take_while(|c| *c == '\t').collect() };
    // the element for `ind`, the indentation of its own line; a list's items are one tab further in
    let element = |ind: &str| -> Result<String, EditError> {
        match value {
            serde_json::Value::String(name) if folio_model::UI_COLORS.contains(&name.as_str()) => Ok(format!("<{tag} type=\"enumeration\">{name}</{tag}>")),
            serde_json::Value::Array(v) if v.len() == 3 => {
                let rgb: Vec<f64> = v.iter().filter_map(|x| x.as_f64()).filter(|x| (0.0..=255.0).contains(x)).collect();
                if rgb.len() != 3 {
                    return Err(bad("three numbers from 0 to 255"));
                }
                let items: String = rgb.iter().map(|c| format!("\n{ind}\t<ListItem type=\"double\">{}</ListItem>", geom::num(*c))).collect();
                Ok(format!("<{tag} type=\"list\">{items}\n{ind}</{tag}>"))
            }
            _ => Err(bad("an interface colour name or three numbers")),
        }
    };
    let node = &ix.nodes[owner];
    let base = line_indent(node.start);
    let ind = format!("{base}\t\t");
    let props = ix.own_descendants(t, owner, "Properties").first().copied();
    let existing = props.and_then(|p| ix.own_descendants(t, p, tag).first().copied());
    let edit = if let Some(c) = existing {
        let n = &ix.nodes[c];
        (n.start..n.end, element(&line_indent(n.start))?)
    } else if let Some(p) = props {
        let (open, last) = (ix.nodes[p].open_end, ix.children(p).last().map(|&k| ix.nodes[k].end));
        match (tag, last) {
            ("GridColor", _) => (open..open, format!("\n{ind}{}", element(&ind)?)),
            (_, Some(end)) => (end..end, format!("\n{ind}{}", element(&ind)?)),
            (_, None) => (open..open, format!("\n{ind}{}\n{base}\t", element(&ind)?)),
        }
    } else if node.empty {
        let head = t[node.start..node.open_end].trim_end_matches('>').trim_end_matches('/').trim_end();
        let name = ix.local(t, owner);
        (node.start..node.end, format!("{head}>\n{base}\t<Properties>\n{ind}{}\n{base}\t</Properties>\n{base}</{name}>", element(&ind)?))
    } else {
        (node.open_end..node.open_end, format!("\n{base}\t<Properties>\n{ind}{}\n{base}\t</Properties>", element(&ind)?))
    };
    w.splice(part, vec![edit])
}
