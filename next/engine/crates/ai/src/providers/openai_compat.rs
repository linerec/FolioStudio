use super::{
    Reply, history,
    stream::{Decoder, push},
};
use crate::{agent::Input, connection::Connection};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn parts(blocks: &[Value]) -> Vec<Value> {
    blocks.iter().filter_map(|b| match b["type"].as_str(){
        Some("text")=>Some(b.clone()),
        Some("image")=>Some(json!({"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",b["source"]["data"].as_str()?)}})),
        _=>None,
    }).collect()
}
pub fn body(c: &Connection, input: &Input, tools: bool) -> Value {
    let mut messages = vec![json!({"role":"system","content":input.system})];
    for m in history::messages(input.messages, c.send_images) {
        let blocks = m["content"].as_array().unwrap();
        if m["role"] == "assistant" {
            let mut calls = Vec::new();
            let mut text = String::new();
            for b in blocks {
                if b["type"] == "tool_use" {
                    calls.push(json!({"id":b["id"],"type":"function","function":{"name":b["name"],"arguments":b["input"].to_string()}}))
                }
                if let Some(t) = b["text"].as_str() {
                    text.push_str(t)
                }
            }
            let mut a = json!({"role":"assistant","content":text});
            if !calls.is_empty() {
                a["tool_calls"] = json!(calls)
            }
            messages.push(a)
        } else if blocks.first().is_some_and(|b| b["type"] == "tool_result") {
            let mut images = Vec::new();
            for b in blocks {
                let mut text = String::new();
                for p in parts(b["content"].as_array().unwrap()) {
                    if let Some(t) = p["text"].as_str() {
                        text.push_str(t)
                    } else {
                        images.push(p)
                    }
                }
                messages
                    .push(json!({"role":"tool","tool_call_id":b["tool_use_id"],"content":text}));
            }
            if !images.is_empty() {
                messages.push(json!({"role":"user","content":images}))
            }
        } else {
            messages.push(json!({"role":"user","content":parts(blocks)}))
        }
    }
    let mut body = json!({"model":c.model,"messages":messages,"stream":true,"stream_options":{"include_usage":true}});
    if tools && !input.tools.is_empty() {
        body["tools"]=json!(input.tools.iter().map(|t|
        json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["input_schema"]}})).collect::<Vec<_>>())
    }
    body
}
pub fn unsupported_tools(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    (m.contains("tool") || m.contains("function calling"))
        && (m.contains("not support") || m.contains("unsupported") || m.contains("doesn't support"))
}
#[derive(Default)]
pub struct Chat {
    text: String,
    calls: BTreeMap<usize, Value>,
    reason: String,
    usage: Value,
}
impl Decoder for Chat {
    fn event(&mut self, data: &str) -> Result<bool, String> {
        if data == "[DONE]" {
            return Ok(true);
        }
        let e: Value = serde_json::from_str(data).map_err(|_| "invalid chat completion chunk")?;
        if let Some(err) = e.get("error") {
            return Err(err["message"].as_str().unwrap_or("AI stream error").into());
        }
        if !e["usage"].is_null() {
            self.usage = e["usage"].clone()
        }
        for c in e["choices"].as_array().into_iter().flatten() {
            if c["index"].as_u64().unwrap_or(0) != 0 {
                continue;
            }
            if let Some(reason) = c["finish_reason"].as_str() {
                self.reason = reason.into()
            }
            let delta = &c["delta"];
            if let Some(text) = delta["content"].as_str() {
                self.text.push_str(text)
            }
            for call in delta["tool_calls"].as_array().into_iter().flatten() {
                let idx = call["index"]
                    .as_u64()
                    .filter(|i| *i < 1024)
                    .ok_or("invalid tool call index")? as usize;
                let b = self.calls.entry(idx).or_insert_with(|| json!({}));
                for (k, v) in [
                    ("id", &call["id"]),
                    ("name", &call["function"]["name"]),
                    ("args", &call["function"]["arguments"]),
                ] {
                    if !v.is_null() {
                        push(b, k, v)?
                    }
                }
            }
        }
        Ok(false)
    }
    fn preview(&self) -> Option<(usize, &str, &str)> {
        Some((0, "text", &self.text))
    }
    fn finish(self) -> Result<Reply, String> {
        if self.reason.is_empty() {
            return Err("missing completion finish reason".into());
        }
        let mut blocks = vec![];
        if !self.text.is_empty() {
            blocks.push(history::text(&self.text))
        }
        for c in self.calls.into_values() {
            let args: Value =
                serde_json::from_str(c["args"].as_str().unwrap_or("")).unwrap_or(Value::Null);
            blocks.push(json!({"type":"tool_use","id":c["id"],"name":c["name"],"input":args}));
        }
        Ok(Reply {
            blocks,
            reason: self.reason,
            usage: self.usage,
        })
    }
}
