//! Message ids for the Korean sentences the engine sends to the screen (errors and notes).
//!
//! The engine writes these sentences in many crates (`edit`, `project`, `package`, `textimport`, this one). Every response goes
//! out through `send` (main.rs), and `annotate` gives each failure `messageId` + `messageArgs` next to the unchanged Korean `error`,
//! the same fields the preflight, import and package code already send. The screen translates by id (`engineMessage`, with the
//! English templates of `engine-messages.en.ts`) and fills the arguments in as they are: names, paths and other user text are
//! arguments, never translated.
//!
//! `engine_messages.tsv` lists the sentences: id, the source files that write the sentence, and the Korean template with
//! `{name}` arguments. It is generated from `artifacts/i18n-c3-20261023/catalogue.tsv` together with the English side. A sentence is
//! recognised by its fixed text, so a test keeps every template tied to the source that writes it: changing a Korean sentence in a
//! crate without changing its row fails here instead of silently losing its translation.
use serde_json::{Map, Value, json};
use std::sync::OnceLock;

const TABLE: &str = include_str!("engine_messages.tsv");

/// Only text of at least this many bytes may carry an argument: a template of mostly arguments would match any text.
const MIN_FIXED_BYTES: usize = 8;

struct Entry {
    id: &'static str,
    sources: &'static str,
    template: &'static str,
    /// The fixed text between the arguments (one more than there are names).
    chunks: Vec<&'static str>,
    names: Vec<&'static str>,
}

impl Entry {
    fn parse(line: &'static str) -> Entry {
        let mut columns = line.split('\t');
        let (id, sources, template) = (columns.next().unwrap_or(""), columns.next().unwrap_or(""), columns.next().unwrap_or(""));
        let (mut chunks, mut names, mut rest) = (Vec::new(), Vec::new(), template);
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else { break };
            chunks.push(&rest[..open]);
            names.push(&rest[open + 1..open + close]);
            rest = &rest[open + close + 1..];
        }
        chunks.push(rest);
        Entry { id, sources, template, chunks, names }
    }

    fn fixed_bytes(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum()
    }

    /// The text of each argument when `text` is this sentence.
    fn capture<'t>(&self, text: &'t str) -> Option<Vec<&'t str>> {
        let mut rest = text.strip_prefix(self.chunks[0])?;
        if self.names.is_empty() {
            return rest.is_empty().then(Vec::new);
        }
        let mut values = Vec::with_capacity(self.names.len());
        for chunk in &self.chunks[1..self.chunks.len() - 1] {
            let at = rest.find(chunk)?;
            values.push(&rest[..at]);
            rest = &rest[at + chunk.len()..];
        }
        values.push(rest.strip_suffix(self.chunks[self.chunks.len() - 1])?);
        values.iter().all(|v| !v.is_empty()).then_some(values)
    }
}

fn table() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| TABLE.lines().filter(|l| !l.trim().is_empty()).map(Entry::parse).collect())
}

/// A recognised sentence: its id and its arguments (an argument that is itself a sentence of the table is a message object).
#[derive(Debug, PartialEq)]
pub struct Recognised {
    pub id: &'static str,
    pub args: Map<String, Value>,
}

/// The id and arguments of `text`, when it is one of the sentences of the table (the one with the most fixed text wins).
pub fn recognise(text: &str) -> Option<Recognised> {
    recognise_at(text, 0)
}

fn recognise_at(text: &str, depth: usize) -> Option<Recognised> {
    let (entry, values) = table().iter()
        .filter(|e| e.names.is_empty() || e.fixed_bytes() >= MIN_FIXED_BYTES)
        .filter_map(|e| e.capture(text).map(|v| (e, v)))
        .max_by_key(|(e, _)| e.fixed_bytes())?;
    let args = entry.names.iter().zip(values).map(|(name, value)| {
        let inner = if depth < 4 { recognise_at(value, depth + 1) } else { None };
        ((*name).to_owned(), match inner { Some(m) => message(value, &m), None => Value::String(value.to_owned()) })
    }).collect();
    Some(Recognised { id: entry.id, args })
}

fn message(text: &str, m: &Recognised) -> Value {
    json!({"message": text, "messageId": m.id, "messageArgs": m.args})
}

/// Adds the message id and arguments to a response that carries a table sentence: `error` of a failure (the answer or an object in it that says `ok: false`), and the notes of
/// `warnings` / `limitations` lists (parallel `warningMessages` / `limitationMessages`, like the ones the importers send). Fields
/// that are already there are kept; the Korean text is never changed.
pub fn annotate(header: &mut Value) {
    annotate_at(header, 0);
}

fn annotate_at(value: &mut Value, depth: usize) {
    let Some(object) = value.as_object_mut() else { return };
    // a failure at any level (`plan.dir` of the package plan is one); a `null` id is no id (a crate that has none yet sends `messageId: null`)
    if object.get("ok") == Some(&Value::Bool(false)) && object.get("messageId").is_none_or(Value::is_null) {
        if let Some(found) = object.get("error").and_then(Value::as_str).and_then(recognise) {
            object.insert("messageId".into(), json!(found.id));
            object.insert("messageArgs".into(), Value::Object(found.args));
        }
    }
    for (list, parallel) in [("warnings", "warningMessages"), ("limitations", "limitationMessages")] {
        annotate_list(object, list, parallel);
    }
    if depth < 3 {
        for child in object.values_mut().filter(|v| v.is_object()) {
            annotate_at(child, depth + 1);
        }
    }
}

fn annotate_list(object: &mut Map<String, Value>, list: &str, parallel: &str) {
    let Some(Value::Array(notes)) = object.get(list) else { return };
    let known = |i: usize| object.get(parallel).and_then(|p| p.get(i)).filter(|m| m.get("messageId").is_some()).cloned();
    let mut found = false;
    let messages: Vec<Value> = notes.iter().enumerate().map(|(i, note)| match (known(i), note.as_str()) {
        (Some(existing), _) => existing,
        (None, Some(text)) => match recognise(text) {
            Some(m) => { found = true; message(text, &m) }
            None => object.get(parallel).and_then(|p| p.get(i)).cloned().unwrap_or_else(|| json!({})),
        },
        (None, None) => json!({}),
    }).collect();
    if found {
        object.insert(parallel.into(), Value::Array(messages));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// What a source file writes for a Korean literal: its escapes undone, so the fixed text can be searched for.
    fn source_text(file: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(file);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\\\"", "\"").replace("\\\\", "\\")
    }

    fn filled(entry: &Entry) -> String {
        let mut text = String::new();
        for (i, chunk) in entry.chunks.iter().enumerate() {
            text.push_str(chunk);
            if let Some(name) = entry.names.get(i) {
                text.push_str(&format!("<{name}>"));
            }
        }
        text
    }

    #[test]
    fn the_table_is_well_formed() {
        let mut ids = std::collections::BTreeSet::new();
        assert!(table().len() > 200, "{} rows", table().len());
        for entry in table() {
            assert!(ids.insert(entry.id), "duplicate id {}", entry.id);
            assert!(entry.id.contains('.') && !entry.sources.is_empty() && !entry.template.is_empty(), "{}", entry.id);
            assert!(entry.names.iter().collect::<std::collections::BTreeSet<_>>().len() == entry.names.len(), "{}: repeated name", entry.id);
            assert!(entry.chunks.iter().skip(1).take(entry.names.len().saturating_sub(1)).all(|c| !c.is_empty()), "{}: two arguments next to each other", entry.id);
            assert!(entry.names.is_empty() || entry.fixed_bytes() >= MIN_FIXED_BYTES, "{}: too little fixed text for an argument", entry.id);
        }
    }

    #[test]
    fn every_template_is_still_written_by_the_source_it_names() {
        for entry in table() {
            for file in entry.sources.split(';') {
                let source = source_text(file);
                // a helper may add the ": " in front of an argument (`io("보고서를 쓸 수 없습니다", e)`)
                for chunk in entry.chunks.iter().map(|c| c.trim().trim_end_matches(':')).filter(|c| c.len() > 1) {
                    assert!(source.contains(chunk), "{}: {file} no longer has the text {chunk:?}; update the Korean template (and the English one) in artifacts/i18n-c3-20261023/catalogue.tsv and run gen-catalogue.py", entry.id);
                }
            }
        }
    }

    #[test]
    fn every_sentence_is_recognised_as_itself_with_its_arguments() {
        for entry in table() {
            let text = filled(entry);
            let found = recognise(&text).unwrap_or_else(|| panic!("{} is not recognised: {text}", entry.id));
            assert_eq!(found.id, entry.id, "{text}");
            for name in &entry.names {
                assert_eq!(found.args[*name], json!(format!("<{name}>")), "{}: {name}", entry.id);
            }
        }
    }

    #[test]
    fn a_failure_keeps_its_korean_text_and_gains_an_id_and_the_user_text_as_arguments() {
        let mut header = json!({"ok": false, "code": "invalid", "error": "문단 스타일을 찾을 수 없습니다: 본문 {스타일}"});
        annotate(&mut header);
        assert_eq!(header["error"], "문단 스타일을 찾을 수 없습니다: 본문 {스타일}");
        assert_eq!(header["messageId"], "edit.style.paragraphNotFound");
        assert_eq!(header["messageArgs"], json!({"id": "본문 {스타일}"}));
        let mut path = json!({"ok": false, "error": "원본 문서를 읽을 수 없습니다: D:\\잡지\\원본.indd: 지정된 파일을 찾을 수 없습니다. (os error 2)"});
        annotate(&mut path);
        assert_eq!(path["messageId"], "package.source.unreadable");
        assert_eq!(path["messageArgs"]["path"], "D:\\잡지\\원본.indd");
        assert_eq!(path["messageArgs"]["e"], "지정된 파일을 찾을 수 없습니다. (os error 2)");
    }

    #[test]
    fn a_sentence_inside_a_sentence_becomes_a_nested_message() {
        let inner = "그림 내용을 읽을 수 없습니다";
        let mut header = json!({"ok": false, "error": format!("그림 한 개를 가져오지 않았습니다: {inner}")});
        annotate(&mut header);
        assert_eq!(header["messageId"], "edit.inline.skipped");
        assert_eq!(header["messageArgs"]["why"], json!({"message": inner, "messageId": "edit.inline.badContent", "messageArgs": {}}));
    }

    #[test]
    fn texts_that_are_not_sentences_of_the_table_are_left_alone() {
        for error in ["no document open", "bad request: expected value", "파일 이름이 올바르지 않습니다", "", "{}"] {
            let mut header = json!({"ok": false, "error": error});
            annotate(&mut header);
            assert_eq!(header, json!({"ok": false, "error": error}), "{error}");
        }
        let mut success = json!({"ok": true, "error": "취소했습니다"});
        annotate(&mut success);
        assert_eq!(success, json!({"ok": true, "error": "취소했습니다"}));
    }

    #[test]
    fn a_failure_inside_an_answer_and_an_empty_id_are_handled() {
        // the package plan: the destination check is an object of the plan; the package crate sends `null` for a sentence it has no id for
        let mut header = json!({"ok": true, "plan": {"dir": {"ok": false, "code": "space", "error": "디스크 공간이 부족합니다: 필요 약 12 MB, 남은 공간 3 MB", "messageId": null, "messageArgs": {}}}});
        annotate(&mut header);
        let dir = &header["plan"]["dir"];
        assert_eq!(dir["messageId"], "package.diskSpace");
        assert_eq!(dir["messageArgs"], json!({"need": "12", "free": "3"}));
        assert_eq!(dir["code"], "space");
        assert_eq!(dir["error"], "디스크 공간이 부족합니다: 필요 약 12 MB, 남은 공간 3 MB");
    }

    #[test]
    fn sentences_the_other_crates_really_write_are_recognised() {
        use folio_textimport::{Options, read_bytes};
        let id = |text: &str| recognise(text).unwrap_or_else(|| panic!("not recognised: {text}")).id;
        assert_eq!(id(&read_bytes(b"", &Options::default()).unwrap_err()), "textimport.error.empty");
        assert_eq!(id(&read_bytes(b"a\x01b", &Options::default()).unwrap_err()), "textimport.error.control");
        let word = read_bytes(b"PK\x03\x04rest of a zip", &Options::default()).unwrap_err();
        assert_eq!(id(&word), "textimport.error.docxOpen");
        let absolute = folio_project::paths::RelPath::parse("/abs/파일.png").unwrap_err().to_string();
        let found = recognise(&absolute).unwrap();
        assert_eq!((found.id, &found.args["path"]), ("project.path.absolute", &json!("/abs/파일.png")));
        assert_eq!(id(&folio_project::paths::check_name("a:b").unwrap_err().to_string()), "project.path.forbiddenChar");
    }

    #[test]
    fn answers_that_already_have_an_id_are_not_changed() {
        let mut header = json!({"ok": false, "error": "취소했습니다", "messageId": "package.destination.invalid", "messageArgs": {"x": 1}});
        let before = header.clone();
        annotate(&mut header);
        assert_eq!(header, before);
    }

    #[test]
    fn notes_get_parallel_messages_and_keep_the_ones_the_importers_sent() {
        let sent = json!({"message": "a", "messageId": "textimport.color", "messageArgs": {}});
        let mut header = json!({"ok": true, "info": {
            "warnings": ["a", "그림 한 개를 가져오지 않았습니다: 그림의 크기가 올바르지 않습니다", "문서 이름"],
            "warningMessages": [sent.clone()]}});
        annotate(&mut header);
        let messages = header["info"]["warningMessages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], sent);
        assert_eq!(messages[1]["messageId"], "edit.inline.skipped");
        assert_eq!(messages[1]["messageArgs"]["why"]["messageId"], "edit.inline.badSize");
        assert_eq!(messages[2], json!({}));
        assert_eq!(header["info"]["warnings"][1], "그림 한 개를 가져오지 않았습니다: 그림의 크기가 올바르지 않습니다");
        let mut plain = json!({"ok": true, "warnings": ["이 문서는 다른 프로젝트(abc)에서 만들어졌습니다. 참조는 지금 프로젝트 폴더 기준으로 찾습니다."]});
        annotate(&mut plain);
        assert_eq!(plain["warningMessages"][0]["messageId"], "project.open.otherProject");
        assert_eq!(plain["warningMessages"][0]["messageArgs"], json!({"project": "abc"}));
        let mut quiet = json!({"ok": true, "warnings": ["그 밖의 글"]});
        annotate(&mut quiet);
        assert_eq!(quiet, json!({"ok": true, "warnings": ["그 밖의 글"]}));
    }
}
