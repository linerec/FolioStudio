//! Presentation metadata travels alongside the unchanged legacy Korean message.
use serde_json::{Value, json};

pub(crate) struct Message {
    pub id: String,
    pub text: String,
    pub args: Value,
}

pub(crate) fn msg(id: &str, text: String, args: Value) -> Message {
    Message { id: id.into(), text, args }
}

pub(crate) fn term(id: &str) -> Value {
    json!({"messageId": id, "messageArgs": {}})
}

pub(crate) fn part(name: &str) -> Value {
    let terms = [("fill", "preflight.part.fill"), ("stroke", "preflight.part.stroke"),
        ("text", "preflight.part.text"), ("textStroke", "preflight.part.textStroke")];
    terms.iter().find(|(key, _)| *key == name).map(|(_, id)| term(id)).unwrap_or_else(|| json!(name))
}

pub(crate) fn edge_terms(values: &[Value]) -> Vec<Value> {
    let terms = [("top", "preflight.edge.top"), ("bottom", "preflight.edge.bottom"),
        ("left", "preflight.edge.left"), ("right", "preflight.edge.right")];
    values.iter().map(|v| {
        terms.iter().find(|(key, _)| v["edge"] == *key).map(|(_, id)| term(id)).unwrap_or_else(|| v["edge"].clone())
    }).collect()
}

pub(crate) fn font_face(face: &str) -> Value {
    // This reserved sentinel is emitted by font_facts when no fallback is installed.
    if face == "(대체 글꼴 없음)" { term("preflight.font.noFallback") } else { json!(face) }
}

pub(crate) fn transparency(opacity: f64, blend: &str, effects: &[String]) -> Vec<Value> {
    let mut args = Vec::new();
    if opacity < 0.999 {
        args.push(json!({"messageId":"preflight.opacity","messageArgs":{"value":format!("{:.0}",opacity * 100.0)}}));
    }
    if blend != "Normal" {
        args.push(json!({"messageId":"preflight.blendMode","messageArgs":{"value":blend}}));
    }
    args.extend(effects.iter().map(|e| json!(e)));
    args
}
