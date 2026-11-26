//! Starting values for document Preflight Options.

use super::Defaults;
use crate::Session;
use serde_json::{Map, json};

pub(super) fn defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    // the document's preflight settings, and its profile list (carried whole, to be sent back with the chosen working profile)
    let settings = serde_json::to_value(&s.doc.preflight_settings).map_err(|e| e.to_string())?;
    for (key, value) in settings.as_object().into_iter().flatten() { values.insert(key.clone(), value.clone()); }
    values.insert("profiles".into(), json!(s.doc.preflight_profiles.profiles));
    values.insert("active".into(), json!(s.doc.preflight_profiles.active));
    Ok((values, json!({})))
}
