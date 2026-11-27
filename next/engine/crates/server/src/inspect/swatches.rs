//! Colour and gradient descriptions shared by document and item replies.

use folio_color::{BlendSpace, Colors};
use folio_model::{ColorModel, ColorSpace, Document};
use serde_json::{Value, json};

fn model_name(m: ColorModel) -> &'static str {
    match m {
        ColorModel::Process => "process",
        ColorModel::Spot => "spot",
        ColorModel::Registration => "registration",
        ColorModel::None => "none",
        ColorModel::Paper => "paper",
        ColorModel::Mixed => "mixedInk",
    }
}

fn space_name(s: ColorSpace) -> &'static str {
    match s {
        ColorSpace::Cmyk => "CMYK",
        ColorSpace::Rgb => "RGB",
        ColorSpace::Lab => "LAB",
        ColorSpace::Gray => "Gray",
    }
}

/// The colour halfway along a gradient's ramp.
pub(super) fn gradient_middle(doc: &Document, colors: &Colors, g: &folio_model::Gradient) -> [u8; 3] {
    let mut stops: Vec<&folio_model::GradientStop> = g.stops.iter().collect();
    stops.sort_by(|a, b| a.location.total_cmp(&b.location));
    let at = stops.iter().position(|s| s.location >= 50.0);
    let pair = match at {
        Some(0) => stops.first().map(|s| (*s, *s)),
        Some(i) => Some((stops[i - 1], stops[i])),
        None => stops.last().map(|s| (*s, *s)),
    };
    pair.and_then(|(a, b)| {
        let (sa, sb) = (doc.swatches.get(&a.color)?, doc.swatches.get(&b.color)?);
        let span = b.location - a.location;
        let t = if span > 1e-9 { (50.0 - a.location) / span } else { 0.0 };
        colors.swatch_mix(sa, sb, folio_color::GradientCurve::new(b.midpoint).sample(t), BlendSpace::Rgb)
    })
    .or_else(|| colors.gradient_flat(g, &doc.swatches, BlendSpace::Rgb))
    .unwrap_or([128, 128, 128])
}

pub(super) fn swatch_json(doc: &Document, colors: &Colors, id: &str) -> Option<Value> {
    if let Some(s) = doc.swatches.get(id) {
        let none = s.model == ColorModel::None;
        return Some(json!({
            "id": s.id, "name": s.name, "model": if s.base.is_some() { "tint" } else { model_name(s.model) },
            "space": if none { "" } else { space_name(s.space) },
            "values": s.values, "tint": s.tint, "base": s.base,
            "rgb": colors.swatch_rgb(s, 100.0, BlendSpace::Rgb).unwrap_or([255, 255, 255]),
        }));
    }
    let g = doc.gradients.get(id)?;
    let kind = match g.kind {
        folio_model::GradientKind::Linear => "Linear",
        folio_model::GradientKind::Radial => "Radial",
    };
    Some(json!({"id": g.id, "name": g.name, "model": "gradient", "space": kind, "values": [], "tint": null, "base": null,
                "rgb": gradient_middle(doc, colors, g), "stops": g.stops}))
}
