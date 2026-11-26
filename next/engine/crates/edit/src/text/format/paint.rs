//! Character fill, stroke, tints and stroke weight, and where a gradient fill or stroke sits (`gradientFillStart` [x, y],
//! `gradientFillLength`, `gradientFillAngle`, and the `gradientStroke…` three: InDesign 2025 DOM, the text range's
//! `GradientFillStart/Length/Angle`; see `folio_model::text_gradient`). The text-format command and query share this
//! property registry and value validation.
use super::EditError;
use serde_json::Value;
use std::collections::BTreeMap;

/// The API name and the IDML attribute of each property.
pub(super) const KEYS: [(&str, &str); 11] = [
    ("fillColor", "FillColor"), ("fillTint", "FillTint"), ("strokeColor", "StrokeColor"), ("strokeTint", "StrokeTint"), ("strokeWeight", "StrokeWeight"),
    ("gradientFillStart", "GradientFillStart"), ("gradientFillLength", "GradientFillLength"), ("gradientFillAngle", "GradientFillAngle"),
    ("gradientStrokeStart", "GradientStrokeStart"), ("gradientStrokeLength", "GradientStrokeLength"), ("gradientStrokeAngle", "GradientStrokeAngle"),
];

/// A gradient start (`[x, y]`, or "x y") as the attribute's `x y`.
fn start_of(v: &Value) -> Option<String> {
    let (x, y) = match v {
        Value::Array(a) if a.len() == 2 => (a[0].as_f64()?, a[1].as_f64()?),
        Value::String(s) => {
            let mut it = s.split_ascii_whitespace().map(|n| n.parse::<f64>().ok());
            let (x, y) = (it.next()??, it.next()??);
            if it.next().is_some() { return None; }
            (x, y)
        }
        _ => return None,
    };
    (x.is_finite() && y.is_finite() && x.abs() <= 1.0e6 && y.abs() <= 1.0e6).then(|| format!("{} {}", crate::geom::num(x), crate::geom::num(y)))
}

pub(super) fn xml(api: &str) -> Option<&'static str> {
    KEYS.iter().find(|(k, _)| *k == api).map(|(_, x)| *x)
}

pub(super) fn decode(api: &str, value: &str) -> Option<Value> {
    // an unset gradient placement is InDesign's default: start 0 0, no length (automatic), angle 0
    if api.starts_with("gradient") {
        return Some(match (api.ends_with("Start"), api.ends_with("Length")) {
            (true, _) => {
                let n: Vec<f64> = value.split_ascii_whitespace().filter_map(|n| n.parse().ok()).collect();
                if n.len() == 2 { Value::from(n) } else { Value::from(vec![0.0, 0.0]) }
            }
            (_, true) => Value::from(value.parse::<f64>().unwrap_or(-1.0)),
            _ => Value::from(value.parse::<f64>().unwrap_or(0.0)),
        });
    }
    if api != "strokeWeight" { return None; }
    Some(if value.is_empty() {
        Value::from(folio_model::CharacterAttrs::default().stroke_weight)
    } else {
        value.parse::<f64>().map(Value::from).unwrap_or(Value::Null)
    })
}

/// The attribute value of a property value: a swatch reference, or a tint of 0..=100 (−1 is no tint).
pub(super) fn encode(api: &str, v: &Value) -> Option<String> {
    if api.starts_with("gradient") {
        return if api.ends_with("Start") {
            start_of(v)
        } else {
            // a length is −1 (automatic) or points; an angle degrees
            v.as_f64().filter(|n| n.is_finite() && n.abs() <= 1.0e6 && (!api.ends_with("Length") || *n >= -1.0)).map(crate::geom::num)
        };
    }
    if api.ends_with("Color") {
        v.as_str().filter(|s| !s.is_empty() && !s.chars().any(char::is_control)).map(str::to_owned)
    } else if api == "strokeWeight" {
        v.as_f64().filter(|n| n.is_finite() && (0.0..=1000.0).contains(n)).map(|n| n.to_string())
    } else {
        v.as_f64().filter(|n| n.is_finite() && (*n == -1.0 || (0.0..=100.0).contains(n))).map(|n| n.to_string())
    }
}

/// A swatch reference must name a swatch of the document (`Swatch/None` always exists).
pub(super) fn check(parts: &BTreeMap<String, String>, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    let graphic = parts.get("Resources/Graphic.xml").map(String::as_str).unwrap_or("");
    for (api, _) in KEYS.iter().filter(|(k, _)| k.ends_with("Color")) {
        let Some(value) = props.get(*api) else { continue };
        let id = value.as_str().unwrap_or("");
        if id != "Swatch/None" && !graphic.contains(&format!(" Self=\"{}\"", crate::xml::escape(id))) {
            return Err(EditError::new("notFound", format!("swatch {id}")));
        }
    }
    Ok(())
}
