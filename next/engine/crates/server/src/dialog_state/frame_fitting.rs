//! Starting values for Frame Fitting Options.

use super::{Defaults, item};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    // the first frame's options (the screen started from them too); every frame named gets the dialog's values
    let id = ids[0].as_str().ok_or("dialog needs a frame id")?;
    let frame = item(s, id).ok_or_else(|| format!("unknown item: {id}"))?;
    for (key, value) in serde_json::to_value(&frame.fitting).map_err(|e| e.to_string())?.as_object().into_iter().flatten() { values.insert(key.clone(), value.clone()); }
    Ok((values, json!({})))
}
