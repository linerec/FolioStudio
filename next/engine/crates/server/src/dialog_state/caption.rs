//! Starting values for Caption Setup.

use super::Defaults;
use crate::Session;
use serde_json::{Map, json};

pub(super) fn defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    let setup = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.caption_setup().map_err(|e| e.to_string())?;
    for (key, value) in setup.as_object().into_iter().flatten() { values.insert(key.clone(), value.clone()); }
    Ok((values, json!({})))
}
