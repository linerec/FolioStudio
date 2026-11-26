//! Import warnings retain their legacy text alongside structured presentation metadata.
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct Warnings {
    pub text: Vec<String>,
    pub messages: Vec<Value>,
}

impl Warnings {
    pub fn push(&mut self, id: &str, message: String, args: Value) {
        self.messages.push(json!({"message":message,"messageId":id,"messageArgs":args}));
        self.text.push(message);
    }
}
