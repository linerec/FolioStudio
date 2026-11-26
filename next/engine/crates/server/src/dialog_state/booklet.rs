//! Starting values and preview for Print Booklet.

use super::{Defaults, Refusal};
use crate::Session;
use serde_json::{Map, Value, json};

// Print Booklet: the printer chosen first is the computer's default, else the first one listed
pub(super) fn defaults() -> Defaults {
    let mut values = Map::new();
    let listed = crate::printing::list_printers();
    let printers = listed["printers"].as_array().cloned().unwrap_or_default();
    let first = printers.iter().find(|p| p["isDefault"] == true).or(printers.first()).and_then(|p| p["name"].as_str()).unwrap_or("");
    values.insert("printer".into(), json!(first));
    Ok((values, json!({})))
}

// Print Booklet: the sheets and which page sits where on them (the plan the output and the printer take)
pub(super) fn preview(args: Value, session: Option<&Session>) -> Result<Value, Refusal> {
    let plan = crate::printing::booklet(&args, session)?;
    Ok(json!({"ok": true, "sheets": plan["sheets"]}))
}
