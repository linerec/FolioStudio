//! Attached arguments for the document editor's older (non-registry) text operations.
use serde_json::{Value, json};
use std::sync::OnceLock;

pub fn legacy(name: &str) -> Option<&'static Value> {
    static DEFINITIONS: OnceLock<Value> = OnceLock::new();
    DEFINITIONS.get_or_init(|| serde_json::from_str(include_str!("legacy_text_tools.json")).unwrap()).get(name)
}

pub fn example(name: &str) -> Option<&'static Value> {
    static EXAMPLES: OnceLock<Value> = OnceLock::new();
    EXAMPLES.get_or_init(|| serde_json::from_str(include_str!("text_examples.json")).unwrap()).get(name)
}

pub fn find_query_schema() -> &'static Value {
    &legacy("edit.findReplace").expect("literal find tool")["inputSchema"]["properties"]["query"]
}

pub fn admit_find_query(query: &Value) -> Result<(), String> {
    let fields = &find_query_schema()["properties"];
    for key in query.as_object().ok_or("query must be an object")?.keys() {
        if fields.get(key).is_none() {
            return Err(format!("literal text search does not accept query.{key}"));
        }
    }
    Ok(())
}

/// The host computes these from its composed document. A client cannot provide book data or opt into other documents.
pub fn host_fields(name: &str) -> &'static [&'static str] {
    match name {
        "edit.createTOC" | "edit.updateTOC" => &["sources"],
        "edit.convertListsToText" | "edit.convertStyleListsToText" => &["labels"],
        "edit.generateIndex" => &["locations", "include_book_documents", "book_documents"],
        _ => &[],
    }
}

pub fn admit(name: &str, command: &Value) -> Result<(), String> {
    if name == "edit.findReplace" { admit_find_query(&command["query"])?; }
    for field in host_fields(name) {
        if command.get(field).is_some() { return Err(format!("{name}.{field} is supplied by the open document")); }
    }
    if name == "edit.saveTOCStyle" && command["style"]["includeBookDocuments"] == true {
        return Err("Attached TOCs use only the open document".into());
    }
    Ok(())
}

pub fn prepare(request: &mut Value) {
    let mut toc = false;
    if let Some(commands) = request["commands"].as_array_mut() {
        for command in commands {
            match command["op"].as_str() {
                Some("insertCrossReference" | "updateCrossReference" | "setCrossReference" | "saveCrossReferenceFormat") =>
                    command["locations"] = json!([]),
                Some("createOutlinesRange" | "createOutlinesKeepText") => command["shapes"] = json!(""),
                Some("fillPlaceholderText") => command["text"] = json!(""),
                Some("createTOC" | "updateTOC") => {
                    toc = true;
                    command["sources"] = json!([]);
                }
                Some("generateIndex") => {
                    command["locations"] = json!([]);
                    command["include_book_documents"] = json!(false);
                }
                Some("convertListsToText" | "convertStyleListsToText") => command["labels"] = json!({}),
                _ => {}
            }
        }
    }
    if toc { request["documentOnlyTOC"] = json!(true); }
}
