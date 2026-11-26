//! The footnote oracle and print checker share the wire format and this adapter.
use super::RegisteredCommand;
use crate::{Command, EditError};
use serde_json::Value;

pub fn command(step: &Value, target: Option<&str>) -> Result<Option<Command>, EditError> {
    let Some(op) = step["op"].as_str().filter(|op| RegisteredCommand::recognizes(op)) else {
        return Ok(None);
    };
    let mut value = step.clone();
    let obj = value.as_object_mut().ok_or_else(|| EditError::new("invalid", "expected step object"))?;
    obj.remove("target");
    if RegisteredCommand::has_field(op, "ids") && !obj.contains_key("ids") {
        obj.insert("ids".into(), serde_json::json!([target.ok_or_else(|| EditError::new("invalid", "target required"))?]));
    }
    if RegisteredCommand::has_field(op, "story") {
        obj.insert("story".into(), Value::String(target.ok_or_else(|| EditError::new("invalid", "target required"))?.into()));
    }
    if matches!(step["op"].as_str(), Some("insertFootnoteText" | "deleteFootnoteText")) {
        obj.entry("index").or_insert(Value::from(0));
    }
    serde_json::from_value::<RegisteredCommand>(value).map(|c| Some(c.into())).map_err(|e| EditError::new("invalid", e.to_string()))
}
