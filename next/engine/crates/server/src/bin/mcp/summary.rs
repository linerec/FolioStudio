//! A compact view of the document model for LLM clients.
//!
//! The full model of a small page is over 150 KB of JSON, more than a client's tool-output limit (Claude Code stops at about
//! 25,000 tokens), so `open` and `documentInfo` answer with this summary unless `detail: "full"` is asked for. It carries what the
//! next call needs: the ids of layers, spreads, pages and items (for `edit.createByDrag` and the like), the page boxes and the counts.
use serde_json::{Value, json};

const MAX_ITEMS_PER_SPREAD: usize = 100;

fn count(model: &Value, key: &str) -> usize {
    model[key].as_array().map_or_else(|| model[key].as_object().map_or(0, |o| o.len()), Vec::len)
}

/// The name of an item's kind: `{"Rectangle":{…}}` → `Rectangle`, `"GraphicLine"` → `GraphicLine`.
fn kind(item: &Value) -> Value {
    match &item["kind"] {
        Value::String(name) => json!(name),
        Value::Object(map) => map.keys().next().map_or(Value::Null, |name| json!(name)),
        _ => Value::Null,
    }
}

fn item(value: &Value) -> Value {
    let mut out = json!({"id": value["id"], "kind": kind(value)});
    if let Some(layer) = value["layer"].as_str() { out["layer"] = json!(layer); }
    if let Some(children) = value["kind"]["Group"]["children"].as_array() { out["children"] = json!(children.len()); }
    out
}

pub fn summarize(model: &Value) -> Value {
    let layers: Vec<Value> = model["layers"].as_array().map_or_else(Vec::new, |layers| layers.iter()
        .map(|l| json!({"id": l["id"], "name": l["name"], "visible": l["visible"], "locked": l["locked"]})).collect());
    let spreads: Vec<Value> = model["spreads"].as_array().map_or_else(Vec::new, |spreads| spreads.iter().map(|spread| {
        let pages: Vec<Value> = spread["pages"].as_array().map_or_else(Vec::new, |pages| pages.iter()
            .map(|p| json!({"id": p["id"], "name": p["name"], "bounds": p["bounds"], "margins": p["margins"]})).collect());
        let items = spread["items"].as_array().map_or(&[][..], Vec::as_slice);
        let listed: Vec<Value> = items.iter().take(MAX_ITEMS_PER_SPREAD).map(item).collect();
        json!({"id": spread["id"], "name": spread["name"], "pages": pages, "itemCount": items.len(),
            "items": listed, "itemsTruncated": items.len() > MAX_ITEMS_PER_SPREAD})
    }).collect());
    json!({
        "layers": layers,
        "spreads": spreads,
        "masterSpreads": count(model, "master_spreads"),
        "stories": count(model, "stories"),
        "swatches": count(model, "swatches"),
        "styles": count(model, "styles"),
        "warnings": model["warnings"],
        "note": "Compact summary. Ask for detail \"full\" for the complete model (over 100 KB)."
    })
}
