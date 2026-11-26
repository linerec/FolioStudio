//! Starting values for Cross-Reference Formats.

use super::{Defaults, document_info, refuse};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, requested: &Value) -> Defaults {
    let mut values = Map::new();
    // the format the picker names (none = a new one): its name, style and building blocks, whether a link uses it, and the name a copy would get
    let links = &s.doc.hyperlinks;
    let id = requested["format"].as_str().unwrap_or("");
    let word = |key: &str, fallback: &'static str| requested[key].as_str().filter(|t| !t.is_empty()).map_or_else(|| fallback.to_owned(), str::to_owned);
    let numbered = |prefix: &str| (1..).map(|n| format!("{prefix} {n}")).find(|name| links.formats.iter().all(|f| &f.name != name)).unwrap_or_default();
    let copied = |base: &str| { let (mut name, mut n) = (base.to_owned(), 1); while links.formats.iter().any(|f| f.name == name) { name = format!("{base} {n}"); n += 1; } name };
    if id.is_empty() {
        values.insert("name".into(), json!(numbered(&word("namePrefix", "New Cross-Reference Format"))));
        values.insert("characterStyle".into(), json!("n"));
        values.insert("blocks".into(), json!([{"kind":"ParagraphTextBuildingBlock","text":"","characterStyle":"n","delimiter":"","includeDelimiter":false}]));
        values.insert("used".into(), json!(false));
        values.insert("duplicateName".into(), json!(""));
    } else {
        let format = links.formats.iter().find(|f| f.id == id).ok_or_else(|| refuse("format", "unknownItem", format!("unknown cross-reference format: {id}")))?;
        // a style the document does not offer (a built-in one, or none) is shown as "none"
        let plain: Vec<String> = document_info(s)?["characterStyles"].as_array().into_iter().flatten().filter_map(|x| x["id"].as_str().filter(|i| !i.contains("$ID/")).map(str::to_owned)).collect();
        let known = |style: &str| if style == "n" || plain.iter().any(|p| p == style) { style.to_owned() } else { "n".to_owned() };
        values.insert("name".into(), json!(format.name));
        values.insert("characterStyle".into(), json!(known(&format.character_style)));
        let blocks: Vec<Value> = format.blocks.iter().map(|block| { let mut row = json!(block); row["characterStyle"] = json!(known(&block.character_style)); row }).collect();
        values.insert("blocks".into(), json!(blocks));
        values.insert("used".into(), json!(links.sources.iter().any(|x| x.format == id)));
        values.insert("duplicateName".into(), json!(copied(&format!("{} {}", format.name, word("copySuffix", "copy")))));
    }
    Ok((values, json!({})))
}
