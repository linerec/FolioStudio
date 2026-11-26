//! Starting values for document footnote and endnote options.

use super::Defaults;
use crate::Session;
use serde_json::{Map, json};

pub(super) fn footnote_defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    // the settings as the document writes them (text); `opened` reads them as the fields' types
    let options = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.footnote_options().map_err(|e| e.to_string())?;
    for (key, value) in options { values.insert(key, json!(value)); }
    Ok((values, json!({})))
}

pub(super) fn endnote_defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    let options = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.endnote_options().map_err(|e| e.to_string())?;
    for (key, value) in options { values.insert(key, json!(value)); }
    Ok((values, json!({})))
}
