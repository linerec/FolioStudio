//! Starting values for color groups, gradient swatches and tints.

use super::{Defaults, Refusal, document_info, refuse};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn group_defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a colour group id")?;
    let group = color_groups(s)?.into_iter().find(|g| g["id"] == id && g["root"] != true).ok_or_else(|| format!("not a colour group: {id}"))?;
    values.insert("name".into(), group["name"].clone());
    Ok((values, json!({})))
}

pub(super) fn gradient_defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a gradient swatch id")?;
    let info = document_info(s)?;
    let swatch = info["swatches"].as_array().and_then(|l| l.iter().find(|w| w["id"] == id && w["model"] == "gradient"))
        .ok_or_else(|| format!("not a gradient swatch: {id}"))?;
    values.insert("name".into(), swatch["name"].clone());
    values.insert("kind".into(), swatch["space"].clone());
    values.insert("stops".into(), swatch["stops"].clone());
    Ok((values, json!({})))
}

pub(super) fn tint_defaults(s: &Session, requested: &Value) -> Defaults {
    let values = Map::new();
    let id = requested["base"].as_str().ok_or_else(|| refuse("base", "required", "base is required".into()))?;
    let info = document_info(s)?;
    let swatch = info["swatches"].as_array().and_then(|l| l.iter().find(|w| w["id"] == id))
        .ok_or_else(|| refuse("base", "unknownItem", format!("unknown swatch: {id}")))?;
    Ok((values, json!({"baseName": swatch["name"]})))
}

pub(super) fn color_groups(s: &crate::Session) -> Result<Vec<Value>, Refusal> {
    let edit = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation");
    Ok(folio_edit::color_groups(edit.editor.parts()).into_iter().map(|g| json!(g)).collect())
}

// a gradient's stops stay in the order of their places on the ramp (as the stop editor sorts them)
pub(super) fn link_stops(values: &mut Value) {
    if let Some(rows) = values["stops"].as_array_mut() {
        rows.sort_by(|a, b| a["location"].as_f64().unwrap_or(0.).total_cmp(&b["location"].as_f64().unwrap_or(0.)));
    }
}
