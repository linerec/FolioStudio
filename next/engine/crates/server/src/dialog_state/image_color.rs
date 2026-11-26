//! Starting values for Image Color Settings.

use super::{Defaults, graphic_of};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a frame id")?;
    let color = graphic_of(s, id).and_then(|g| g.image_color.as_ref()).ok_or_else(|| format!("{id} has no image colour settings"))?;
    values.insert("profile".into(), json!(color.profile));
    values.insert("intent".into(), json!(color.intent));
    Ok((values, json!({})))
}
