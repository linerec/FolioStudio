//! Starting and submitted values for Clipping Path Options.

use super::{Defaults, graphic_of};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    // the frame's graphic's clipping path options, as the reader has them (the path's name without InDesign's `$ID/`)
    let id = ids[0].as_str().ok_or("dialog needs a frame id")?;
    let options = graphic_of(s, id).and_then(|g| g.clipping_options.as_ref()).ok_or_else(|| format!("{id} has no clipping path options"))?;
    for (key, value) in serde_json::to_value(options).map_err(|e| e.to_string())?.as_object().into_iter().flatten() { values.insert(key.clone(), value.clone()); }
    let name = options.path_name.trim_start_matches("$ID/").to_owned();
    values.insert("pathName".into(), json!(name));
    // the channel and the path the lists start on: the one the picture uses, else the file's first
    let found = crate::clippaths::sources(s, id);
    let pick = |list: &[String], used: bool| if used && list.contains(&name) { name.clone() } else { list.first().cloned().unwrap_or_default() };
    values.insert("alphaName".into(), json!(pick(&found.alpha_channels, options.kind == "AlphaChannel")));
    values.insert("photoshopPath".into(), json!(pick(&found.paths, options.kind == "PhotoshopPath")));
    Ok((values, json!({})))
}

// the clipping path's name: the alpha channel or the Photoshop path chosen for the kind
pub(super) fn prepare(values: &mut Value) {
    values["pathName"] = match values["kind"].as_str() { Some("AlphaChannel") => values["alphaName"].clone(), Some("PhotoshopPath") => values["photoshopPath"].clone(), _ => json!("") };
}
