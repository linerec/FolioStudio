//! Starting values for Type on a Path Options.

use super::Defaults;
use crate::Session;
use serde_json::{Map, Value, json};

// a text on a path's options as IDML writes them (numbers as text, the flip as Flipped/NotFlipped) and whether Folio edits that path
pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a text path id")?;
    let found = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.text_path_options(id).map_err(|e| e.to_string())?;
    for (key, value) in found["props"].as_object().into_iter().flatten() {
        values.insert(key.clone(), if key == "FlipPathEffect" { json!(value == "Flipped") } else { value.clone() });
    }
    values.insert("supported".into(), found["supported"].clone());
    Ok((values, json!({})))
}
