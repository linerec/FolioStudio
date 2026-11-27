use super::{
    Reply, history,
    stream::{Decoder, push},
};
use crate::{agent::Input, connection::Connection};
use serde_json::{Value, json};

pub fn body(c: &Connection, input: &Input) -> Value {
    let mut tools = input.tools.to_vec();
    for tool in &mut tools {
        tool["eager_input_streaming"] = json!(true)
    }
    let mut body = json!({"model":c.model,"stream":true,"max_tokens":8192,"tools":tools,
        "system":[{"type":"text","text":input.system,"cache_control":{"type":"ephemeral"}}],
        "messages":history::messages(input.messages,c.send_images)});
    if !matches!(
        c.model.as_str(),
        "claude-haiku-4-5" | "claude-haiku-4-5-20251001"
    ) {
        body["output_config"] = json!({"effort":c.effort});
    }
    body
}
#[derive(Default)]
pub struct Messages {
    reply: Reply,
    started: bool,
    open: Option<usize>,
    partial: String,
    stopping: bool,
}
impl Decoder for Messages {
    fn event(&mut self, data: &str) -> Result<bool, String> {
        let e: Value = serde_json::from_str(data).map_err(|_| "invalid Messages event")?;
        let index = e["index"].as_u64().and_then(|i| usize::try_from(i).ok());
        match e["type"].as_str() {
            Some("ping") => {}
            Some("error") => {
                return Err(if e["error"]["type"] == "overloaded_error" {
                    "aiOverloaded".into()
                } else {
                    e["error"]["message"]
                        .as_str()
                        .unwrap_or("AI stream error")
                        .into()
                });
            }
            Some("message_start") if !self.started => {
                self.started = true;
                self.reply.usage = e["message"]["usage"].clone()
            }
            Some("content_block_start")
                if self.started
                    && !self.stopping
                    && self.open.is_none()
                    && index == Some(self.reply.blocks.len()) =>
            {
                self.open = index;
                self.partial.clear();
                self.reply.blocks.push(e["content_block"].clone());
            }
            Some("content_block_delta") if self.open.is_some() && self.open == index => {
                let block = self.reply.blocks.last_mut().unwrap();
                let d = &e["delta"];
                match d["type"].as_str() {
                    Some("text_delta") => push(block, "text", &d["text"])?,
                    Some("thinking_delta") => push(block, "thinking", &d["thinking"])?,
                    Some("signature_delta") => push(block, "signature", &d["signature"])?,
                    Some("input_json_delta") => self
                        .partial
                        .push_str(d["partial_json"].as_str().ok_or("invalid tool fragment")?),
                    _ => {}
                }
            }
            Some("content_block_stop") if self.open.is_some() && self.open == index => {
                if !self.partial.is_empty() {
                    self.reply.blocks.last_mut().unwrap()["input"] =
                        serde_json::from_str(&self.partial).unwrap_or(Value::Null);
                }
                self.open = None;
            }
            Some("message_delta") if self.started && self.open.is_none() => {
                self.reply.reason = e["delta"]["stop_reason"]
                    .as_str()
                    .ok_or("missing stop reason")?
                    .into();
                self.stopping = true;
                for (k, v) in e["usage"].as_object().into_iter().flatten() {
                    self.reply.usage[k] = v.clone()
                }
            }
            Some("message_stop") if self.stopping => return Ok(true),
            Some(
                "message_start"
                | "message_delta"
                | "message_stop"
                | "content_block_start"
                | "content_block_delta"
                | "content_block_stop",
            ) => return Err("out-of-order Messages event".into()),
            _ => {}
        }
        Ok(false)
    }
    fn preview(&self) -> Option<(usize, &str, &str)> {
        let index = self.open?;
        let block = self.reply.blocks.get(index)?;
        let kind = block["type"].as_str()?;
        if !matches!(kind, "text" | "thinking") {
            return None;
        }
        Some((index, kind, block[kind].as_str()?))
    }
    fn finish(self) -> Result<Reply, String> {
        Ok(self.reply)
    }
}
pub fn cost(model: &str, u: &Value) -> Option<f64> {
    let read = |k: &str| u[k].as_u64().unwrap_or(0) as f64;
    let input = u["input_tokens"].as_u64()? as f64;
    let output = u["output_tokens"].as_u64()? as f64;
    let cache = read("cache_read_input_tokens");
    let writes = read("cache_creation_input_tokens");
    let (i, o, r) = match model {
        "claude-opus-5-5" => (4., 20., 0.2),
        "claude-sonnet-5-5" => (2., 10., 0.1),
        "claude-fable-5-2" | "claude-mythos-5-2" => (8., 40., 0.2),
        "claude-haiku-5-5" if input + cache + writes <= 100000. => (0.1, 0.5, 0.01),
        "claude-haiku-5-5" => (0.5, 2.5, 0.05),
        "claude-opus-5" | "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" => {
            (5., 25., 0.5)
        }
        "claude-fable-5-1" | "claude-mythos-5-1" => (10., 50., 0.25),
        "claude-fable-5" | "claude-mythos-5" => (10., 50., 1.),
        "claude-sonnet-5" => (2., 10., 0.2),
        "claude-sonnet-4-6" => (3., 15., 0.3),
        "claude-haiku-4-5" | "claude-haiku-4-5-20251001" => (1., 5., 0.1),
        _ => return None,
    };
    Some((input * i + output * o + writes * 1.25 * i + cache * r) / 1e6)
}
