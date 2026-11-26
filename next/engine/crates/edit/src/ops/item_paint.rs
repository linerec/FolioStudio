//! Page item appearance and style overrides.

use super::*;

/// A graphic attribute as the item's object style gives it (following `BasedOn`); `enable`
/// is the style's switch for that group of attributes (`EnableFill`, `EnableStroke`).
pub(super) fn style_attr(w: &Work<'_>, ix: &Index, t: &str, i: usize, name: &str, enable: &str) -> Option<String> {
    objectstyle::from_style(w, ix, t, i, &[], name, enable)
}

pub(super) fn same_number(a: &str, b: &str) -> bool {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

/// InDesign writes a graphic attribute only where it differs from the object style; one set
/// back to the style's value goes away (oracle: fill-*, stroke-*). Giving a stroke a colour
/// where its weight is 0 brings the weight back (to the style's, or 1 pt); taking the
/// colour away sets the weight to 0.
pub(super) fn set_paint(w: &mut Work<'_>, id: &str, fill: Option<&str>, fill_tint: Option<f64>, stroke: Option<&str>, stroke_tint: Option<f64>, weight: Option<f64>) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let mut wanted: Vec<(&str, &str, String)> = Vec::new();
    if let Some(f) = fill {
        wanted.push(("FillColor", "EnableFill", f.to_owned()));
    }
    if let Some(v) = fill_tint {
        wanted.push(("FillTint", "EnableFill", geom::num(v)));
    }
    let mut weight = weight;
    if let Some(s) = stroke {
        wanted.push(("StrokeColor", "EnableStroke", s.to_owned()));
        let current: f64 = stroke_attr(w, &ix, &t, i, "StrokeWeight").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if s == "Swatch/None" {
            weight.get_or_insert(0.0);
        } else if current <= 0.0 && weight.is_none() {
            let style: f64 = style_attr(w, &ix, &t, i, "StrokeWeight", "EnableStroke").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            weight = Some(if style > 0.0 { style } else { 1.0 });
        }
    }
    // a weight given to an object without a stroke colour paints it in InDesign's hidden black
    // (a document without one keeps its stroke colour)
    if stroke.is_none()
        && weight.is_some_and(|v| v > 0.0)
        && stroke_attr(w, &ix, &t, i, "StrokeColor").is_none_or(|c| c == "Swatch/None")
        && let Some(black) = swatches::hidden_black(w)?
    {
        wanted.push(("StrokeColor", "EnableStroke", black));
    }
    if let Some(v) = stroke_tint {
        wanted.push(("StrokeTint", "EnableStroke", geom::num(v)));
    }
    if let Some(v) = weight {
        wanted.push(("StrokeWeight", "EnableStroke", geom::num(v)));
    }
    let mut edits = Vec::new();
    for (name, enable, value) in wanted {
        let style = style_attr(w, &ix, &t, i, name, enable);
        if style.as_deref().is_some_and(|s| same_number(s, &value)) {
            if let Some(e) = attr_removal(&ix, &t, i, name) {
                edits.push(e);
            }
        } else {
            edits.push(attr_edit(&ix, &t, i, name, &value));
        }
    }
    w.splice(&part, edits)
}

/// The Effects dialog's view of an object (effects.rs).
pub(crate) fn effects_of(w: &mut Work<'_>, id: Option<&str>) -> Result<serde_json::Value, EditError> {
    effects::effects_of(w, id)
}
