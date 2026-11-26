//! Data Merge ▸ Update Content in Data Fields and Show Log of Update Results (build-c2), Folio's own way.
//!
//! InDesign's merged document keeps no field links (measured, datamerge-c3), so these menu items cannot follow InDesign's data.
//! Folio's merged document remembers, in the Folio-only document label `folio.datamerge` (it survives InDesign), key `merged`:
//! the template and the data source (project-relative path, SHA-256, how the source is read) and, for every story a record
//! filled, the fields in story order with the text each one got. Updating reads the source again (the host does: it has the
//! file) and, story by story, finds each field's text from where the one before it ended: only text still as the merge (or the
//! last update) left it is replaced, in the formatting of its first character; text a person changed stays and is logged.
//! The log keeps what changed and when (the last `LOG_MAX` entries).
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DM: &str = "designmap.xml";
const KEY: &str = "merged";
const LOG_MAX: usize = 1000;
/// What an empty value is set as in a merged story (two zero-width no-break spaces, as InDesign's merge writes it).
const EMPTY: &str = "\u{FEFF}\u{FEFF}";

/// The fields one record put into one story: (field, text it got), in story order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Filled {
    pub story: String,
    /// The record's index in the data source (0 = the first record).
    pub record: usize,
    pub fields: Vec<(String, String)>,
}

/// The QR code one record put into one frame (rest-c1): the frame, the field and the text the code holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeFilled {
    pub frame: String,
    pub field: String,
    pub record: usize,
    pub value: String,
}

/// After Create Merged Document: the merged document remembers `link` (the template and the source, JSON from the host), the
/// field texts and the QR codes.
pub(crate) fn remember(w: &mut Work<'_>, link: &str, filled: &[Filled], codes: &[CodeFilled]) -> Result<(), EditError> {
    let mut merged: Value = serde_json::from_str(link).map_err(|e| invalid(&format!("link is not JSON: {e}")))?;
    if !merged.is_object() { return Err(invalid("link is a JSON object")); }
    merged["stories"] = json!(filled);
    if !codes.is_empty() { merged["codes"] = json!(codes); }
    merged["log"] = json!([]);
    let mut map = super::computed::read_map(w.text(DM)?)?;
    map.insert(KEY.into(), merged);
    super::computed::write_map(w, &map)
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() { return None; }
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| hay[i..].starts_with(needle))
}

fn shown(s: &str) -> &str {
    if s == EMPTY { "" } else { s }
}

/// `updateMergedFields {fields, records, sha256, at}`: the field texts of the merged document from the data source as it is now.
pub(crate) fn update(w: &mut Work<'_>, fields: &[String], records: &[Vec<String>], sha256: &String, at: &String) -> Result<(), EditError> {
    let mut map = super::computed::read_map(w.text(DM)?)?;
    let Some(mut merged) = map.get(KEY).cloned().filter(Value::is_object) else {
        return Err(EditError::new("notMerged", "this document was not made by Create Merged Document: it has no data fields to update"));
    };
    let mut stories: Vec<Filled> = serde_json::from_value(merged["stories"].clone()).map_err(|e| invalid(&format!("merged stories: {e}")))?;
    let mut log: Vec<Value> = merged["log"].as_array().cloned().unwrap_or_default();
    let (mut updated, mut left) = (0usize, 0usize);
    for entry in &mut stories {
        let record = records.get(entry.record);
        let story = entry.story.clone();
        let mut note = |field: &str, from: &str, to: &str, result: &str| {
            log.push(json!({"at": at, "record": entry.record + 1, "story": story, "field": field, "from": shown(from), "to": shown(to), "result": result}));
        };
        let chars: Option<Vec<char>> = w.find(&entry.story).ok().and_then(|(part, _, _)| w.text(&part).ok().and_then(|t| crate::text::contents(t, &entry.story).ok())).map(|t| t.chars().collect());
        let Some(mut chars) = chars else {
            for (field, old) in &entry.fields { note(field, old, old, "noStory"); left += 1; }
            continue;
        };
        let mut cursor = 0;
        for (field, old) in entry.fields.iter_mut() {
            let Some(record) = record else { note(field, old, old, "noRecord"); left += 1; continue };
            let Some(i) = fields.iter().position(|f| f == field) else { note(field, old, old, "noField"); left += 1; continue };
            let value = record.get(i).map(String::as_str).unwrap_or("");
            let new = if value.is_empty() { EMPTY.to_owned() } else { value.to_owned() };
            let was: Vec<char> = old.chars().collect();
            let Some(pos) = find(&chars, &was, cursor) else {
                // not as the merge left it (a person changed it, or a blank line holding it was removed): left alone
                if *old != new { note(field, old, &new, "edited"); left += 1; }
                continue;
            };
            if *old == new { cursor = pos + was.len(); continue; }
            crate::text::replace_keeping_format(w, &entry.story, pos, pos + was.len(), &new)?;
            let now: Vec<char> = new.chars().collect();
            cursor = pos + now.len();
            chars.splice(pos..pos + was.len(), now);
            note(field, old, &new, "updated");
            *old = new;
            updated += 1;
        }
    }
    // QR code fields (rest-c1): a frame still holding the code the merge (or the last update) made gets the new value's code; a frame a
    // person emptied or filled with something else, or that is gone, is left alone and logged
    let mut codes: Vec<CodeFilled> = merged.get("codes").map(|c| serde_json::from_value(c.clone())).transpose().map_err(|e| invalid(&format!("merged codes: {e}")))?.unwrap_or_default();
    for code in &mut codes {
        let mut note = |from: &str, to: &str, result: &str| {
            log.push(json!({"at": at, "record": code.record + 1, "frame": code.frame, "field": code.field, "from": from, "to": to, "result": result}));
        };
        let Some(record) = records.get(code.record) else { note(&code.value, &code.value, "noRecord"); left += 1; continue };
        let Some(i) = fields.iter().position(|f| *f == code.field) else { note(&code.value, &code.value, "noField"); left += 1; continue };
        let value = record.get(i).cloned().unwrap_or_default();
        if value == code.value { continue; }
        match replace_code(w, &code.frame, &code.value, &value)? {
            Some(result) => { note(&code.value, &value, result); left += 1; }
            None => { note(&code.value, &value, "updated"); code.value = value; updated += 1; }
        }
    }
    if !codes.is_empty() { merged["codes"] = json!(codes); }
    log.push(json!({"at": at, "result": "summary", "updated": updated, "left": left, "sha256": sha256}));
    if log.len() > LOG_MAX { log.drain(..log.len() - LOG_MAX); }
    merged["stories"] = json!(stories);
    merged["source"]["sha256"] = json!(sha256);
    merged["updatedAt"] = json!(at);
    merged["log"] = json!(log);
    map.insert(KEY.into(), merged);
    super::computed::write_map(w, &map)
}

/// The QR code in `frame` becomes the code of `to` (an empty value empties the frame). `Some(why)` when the frame is left alone: it is
/// gone (noFrame) or does not hold the code of `from` any more (edited).
fn replace_code(w: &mut Work<'_>, frame: &str, from: &str, to: &str) -> Result<Option<&'static str>, EditError> {
    let Ok((part, ix, i)) = w.find(frame) else { return Ok(Some("noFrame")) };
    let t = w.text(&part)?.to_owned();
    let graphics: Vec<usize> = ix.children(i).iter().copied().filter(|&n| matches!(ix.local(&t, n), "Image" | "PDF" | "EPS" | "ImportedPage")).collect();
    // what the merge left: the code of `from` (an EPS with its PlainTextQRCode), or nothing for an empty value
    let holds = |n: usize| ix.local(&t, n) == "EPS" && t[ix.nodes[n].start..ix.nodes[n].end].contains(&format!("PlainText=\"{}\"", crate::xml::escape(from)));
    let as_left = if from.is_empty() { graphics.is_empty() } else { graphics.len() == 1 && holds(graphics[0]) };
    if !as_left { return Ok(Some("edited")); }
    if let Some(&n) = graphics.first() {
        w.splice(&part, vec![(ix.nodes[n].start..ix.nodes[n].end, String::new())])?;
    }
    if !to.is_empty() { super::qr::place(w, frame, to)?; }
    Ok(None)
}

impl crate::Editor {
    /// What a merged document remembers (`None`: not made by Create Merged Document in Folio): template, source, stories, log.
    pub fn merged_document(&self) -> Option<Value> {
        self.data_merge_label().get(KEY).cloned().filter(Value::is_object)
    }
}
