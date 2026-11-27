use crate::agent::{Input, Provider, Step};
use serde_json::Value;
use std::collections::VecDeque;

pub struct Scripted(VecDeque<Step>);
impl Scripted {
    pub fn from_json(script: &Value) -> Result<Self, String> {
        let steps = serde_json::from_value(script.clone()).map_err(|_| "invalid scripted steps")?;
        Ok(Self(steps))
    }
}
impl Provider for Scripted {
    fn next(&mut self, input: Input<'_>) -> Result<Step, String> {
        let mut step = self.0.pop_front().ok_or("script ended without end")?;
        if let Step::Tool { args, .. } = &mut step {
            bind(args, input.messages)?;
        }
        Ok(step)
    }
}

// A fixture can use the ID the actual editor allocated in an earlier tool result.
fn bind(value: &mut Value, messages: &[Value]) -> Result<(), String> {
    match value {
        Value::Object(o) if o.contains_key("$result") => {
            let id = o["$result"].as_str().ok_or("invalid scripted reference")?;
            let result = messages
                .iter()
                .rev()
                .find(|m| m["kind"] == "tool_result" && m["toolUseId"] == id)
                .ok_or("missing scripted result")?;
            let mut found = result["content"]
                .pointer(o.get("pointer").and_then(Value::as_str).unwrap_or(""))
                .ok_or("missing scripted pointer")?;
            if let Some(mut id) = o.get("itemId").cloned() {
                bind(&mut id, messages)?;
                found = found
                    .as_array()
                    .and_then(|a| a.iter().find(|item| item["id"] == id))
                    .ok_or("missing scripted item")?;
            }
            *value = found
                .pointer(o.get("field").and_then(Value::as_str).unwrap_or(""))
                .ok_or("missing scripted field")?
                .clone();
        }
        Value::Object(o) => {
            for child in o.values_mut() {
                bind(child, messages)?;
            }
        }
        Value::Array(a) => {
            for child in a {
                bind(child, messages)?;
            }
        }
        _ => {}
    }
    Ok(())
}
