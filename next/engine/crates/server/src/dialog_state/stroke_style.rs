//! Starting values and patterns for stroke style options.

use super::{Defaults, refuse};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, requested: &Value) -> Defaults {
    let mut values = Map::new();
    // the stroke style the picker names (none = a new one): its name, kind, pattern (typed: "12 6"), corner adjustment and cap
    let id = requested["style"].as_str().unwrap_or("");
    if id.is_empty() {
        for (key, value) in [("name", json!("")), ("kind", json!("DashedStrokeStyle")), ("values", json!("12 6")), ("adjustment", json!("DashesAndGaps")), ("cap", json!("ButtEndCap")), ("existing", json!(false))] { values.insert(key.into(), value); }
    } else {
        let style = s.doc.stroke_styles.iter().find(|x| x.id == id).ok_or_else(|| refuse("style", "unknownItem", format!("unknown stroke style: {id}")))?;
        values.insert("name".into(), json!(style.name));
        values.insert("kind".into(), json!(style.kind));
        values.insert("values".into(), json!(style.values.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ")));
        values.insert("adjustment".into(), json!(style.adjustment));
        values.insert("cap".into(), json!(style.cap));
        values.insert("existing".into(), json!(true));
    }
    Ok((values, json!({})))
}

// a new stroke style starts the pattern of the kind it is changed to (an existing one keeps its kind)
pub(super) fn link_kind(values: &mut Value) {
    values["values"] = json!(match values["kind"].as_str() { Some("StripedStrokeStyle") => "0 20 50 100", Some("DottedStrokeStyle") => "10", _ => "12 6" });
}
