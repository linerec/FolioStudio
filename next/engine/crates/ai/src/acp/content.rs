use crate::agent::Step;
use serde_json::{Value, json};

pub(super) fn prompt(messages: &[Value], system: &str, images: bool, replay: bool) -> Vec<Value> {
    let mut blocks = Vec::new();
    if !system.is_empty() {
        blocks.push(json!({"type":"text","text":system}));
    }
    let current = messages.iter().rposition(|m| m["kind"] == "user");
    if replay {
        for m in messages.iter().take(current.unwrap_or(0)) {
            if matches!(m["kind"].as_str(), Some("user" | "text")) {
                blocks.push(json!({"type":"text","text":format!("{}: {}",m["kind"],m["text"].as_str().unwrap_or(""))}));
            }
        }
    }
    if let Some(user) = current.map(|i| &messages[i]) {
        blocks.push(json!({"type":"text","text":user["text"]}));
        for a in user["attachments"].as_array().into_iter().flatten() {
            let mut summary = a.clone();
            let image = summary.as_object_mut().and_then(|a| a.remove("image"));
            let text = summary.to_string();
            let hash = folio_project::hash::sha256_bytes(text.as_bytes());
            blocks.push(json!({"type":"resource_link","name":a["kind"],"uri":format!("folio://context/{hash}")}));
            blocks.push(json!({"type":"text","text":text}));
            if images && let Some(png) = image.as_ref().and_then(|v| v["png"].as_str()) {
                blocks.push(json!({"type":"image","mimeType":"image/png","data":png}));
            }
        }
    }
    blocks
}
pub(super) fn update(u: &Value) -> Option<Step> {
    match u["sessionUpdate"].as_str()? {
        "agent_message_chunk" | "agent_thought_chunk" if u["content"]["type"] == "text" => {
            let text = u["content"]["text"].as_str()?.to_owned();
            Some(if u["sessionUpdate"] == "agent_message_chunk" {
                Step::Text { text }
            } else {
                Step::Thinking { text }
            })
        }
        "plan" => {
            let entries = u["entries"]
                .as_array()?
                .iter()
                .filter_map(|e| {
                    Some(json!({"content":e["content"].as_str()?,"status":e["status"].as_str()}))
                })
                .collect::<Vec<_>>();
            Some(Step::Response {
                record: json!({"kind":"plan","entries":entries}),
            })
        }
        "tool_call" | "tool_call_update" => Some(Step::Response {
            record: json!({"kind":"agentTool","toolCallId":u["toolCallId"].as_str()?,"title":u["title"].as_str(),"status":u["status"].as_str()}),
        }),
        _ => None,
    }
}

// A credential may cross stdout frames. Hold only a possible suffix until it is resolved.
#[derive(Default)]
pub(super) struct StreamText {
    pub key: String,
    tail: String,
    thinking: bool,
}
impl StreamText {
    fn step(thinking: bool, text: String) -> Step {
        if thinking {
            Step::Thinking { text }
        } else {
            Step::Text { text }
        }
    }
    pub fn filter(&mut self, step: Step) -> (Step, Option<Step>) {
        let thinking = matches!(step, Step::Thinking { .. });
        let is_chunk = thinking || matches!(step, Step::Text { .. });
        let flush = (!self.tail.is_empty() && (!is_chunk || thinking != self.thinking))
            .then(|| Self::step(self.thinking, std::mem::take(&mut self.tail)));
        let filtered = match step {
            Step::Text { text } | Step::Thinking { text } if !self.key.is_empty() => {
                self.thinking = thinking;
                self.tail.push_str(&text);
                let mut safe = self.tail.replace(&self.key, "[redacted]");
                self.tail.clear();
                let first = safe.len().saturating_sub(self.key.len() - 1);
                if let Some((at, _)) = safe
                    .char_indices()
                    .skip_while(|(i, _)| *i < first)
                    .find(|(i, _)| self.key.starts_with(&safe[*i..]))
                {
                    self.tail = safe.split_off(at);
                }
                Self::step(thinking, safe)
            }
            other => other,
        };
        match flush {
            Some(pending) => (pending, Some(filtered)),
            None => (filtered, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_session_replays_text_and_respects_disabled_images() {
        let history = [
            json!({"kind":"user","text":"first"}),
            json!({"kind":"text","text":"reply"}),
            json!({"kind":"user","text":"next","attachments":[{"kind":"objects","count":2,"image":{"png":"fixture"}}]}),
        ];
        let blocks = prompt(&history, "instructions", false, true);
        assert_eq!(blocks.len(), 6);
        assert!(blocks[1]["text"].as_str().unwrap().contains("first"));
        assert!(blocks[2]["text"].as_str().unwrap().contains("reply"));
        assert_eq!(blocks[4]["type"], "resource_link");
        assert!(blocks[5]["text"].as_str().unwrap().contains("count"));
        assert!(!serde_json::to_string(&blocks).unwrap().contains("fixture"));
    }
}
