//! Starting values for Math Expression Options.

use super::Defaults;
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs an equation id")?;
    let equations = crate::mathml::expressions(Some(s))?;
    let item = equations["items"].as_array().and_then(|l| l.iter().find(|x| x["id"] == id)).ok_or_else(|| format!("unknown equation: {id}"))?;
    values.insert("mathml".into(), item["mathml"].clone());
    values.insert("size".into(), item["size"].clone());
    let channel = |at: usize| item["color"][at].as_f64().unwrap_or(0.).round().clamp(0., 255.) as u8;
    values.insert("color".into(), json!(format!("#{:02x}{:02x}{:02x}", channel(0), channel(1), channel(2))));
    Ok((values, json!({})))
}
