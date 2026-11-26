//! Starting values for File Info.

use super::Defaults;
use crate::Session;
use serde_json::{Map, json};

pub(super) fn defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    // the document's metadata (XMP) as text, by field
    let info = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.file_info().map_err(|e| e.to_string())?;
    for (key, value) in info { values.insert(key, json!(value)); }
    Ok((values, json!({})))
}
