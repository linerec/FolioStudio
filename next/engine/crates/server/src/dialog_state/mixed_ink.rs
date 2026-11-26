//! Starting values and preview for mixed ink swatches and groups.

use super::{Defaults, Refusal, document_info};
use crate::Session;
use serde_json::{Map, Value, json};

// the document's inks (the Ink Manager's plates), one row each with the first spot ink ticked, and a swatch name no swatch has (`namePrefix`, then a number)
// (a new mixed ink group: the same rows with nothing ticked; its cells are the group's initial, repeat and increment)
pub(super) fn new_defaults(provider: &str, s: &Session, requested: &Value) -> Defaults {
    let mut values = Map::new();
    let swatch = provider == "mixedInk";
    let inks = crate::sep::inks_view(s.doc_inks());
    let first_spot = inks["inks"].as_array().into_iter().flatten().position(|k| k["kind"] == "spot");
    let rows: Vec<Value> = inks["inks"].as_array().into_iter().flatten().enumerate().map(|(i, k)| {
        let name = k["name"].as_str().unwrap_or("");
        let id = if k["kind"] == "process" { format!("Ink/$ID/Process {name}") } else { format!("Ink/{name}") };
        let mut row = json!({"ink": id, "kind": k["kind"], "name": name, "on": swatch && Some(i) == first_spot});
        if swatch { row["percent"] = json!(100); }
        row
    }).collect();
    values.insert("inks".into(), Value::Array(rows));
    let info = document_info(s)?;
    let taken: Vec<&str> = info["swatches"].as_array().into_iter().flatten().filter_map(|x| x["name"].as_str()).collect();
    let prefix = requested["namePrefix"].as_str().filter(|p| !p.is_empty()).unwrap_or(if swatch { "Mixed Ink Swatch" } else { "Mixed Ink Group" });
    let mut name = prefix.to_owned();
    for k in 2.. { if !taken.contains(&name.as_str()) { break; } name = format!("{prefix} {k}"); }
    values.insert("name".into(), json!(name));
    Ok((values, json!({})))
}

pub(super) fn group_defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a mixed ink group id")?;
    let members = s.doc.print.mixed_ink_groups.get(id).ok_or_else(|| format!("not a mixed ink group: {id}"))?;
    values.insert("name".into(), json!(mixed_group_name(id)));
    // the replacement starts from Black when it may (as the screen did), else the first one offered
    values.insert("replace".into(), json!("Color/Black"));
    Ok((values, json!({"0": mixed_group_name(id), "1": members.len()})))
}

/// `MixedInkGroup/<name>` (a % written %25) as the name the panel shows.
pub(super) fn mixed_group_name(id: &str) -> String { id.trim_start_matches("MixedInkGroup/").replace("%25", "%") }

// the swatches a new mixed ink group would make (their names and percentages), as the group dialog lists them
pub(super) fn preview(args: Value) -> Result<Value, Refusal> {
    let group = serde_json::from_value(args).map_err(|e| e.to_string())?;
    Ok(json!({"ok": true, "swatches": folio_edit::mixedink_groups::preview(&group).map_err(|e| e.to_string())?}))
}
