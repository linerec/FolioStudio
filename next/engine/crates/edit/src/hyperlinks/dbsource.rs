//! Data Merge ▸ a database as the data source (dbmerge DESIGN, `artifacts/dbmerge-20261119/DESIGN.ko.md`).
//!
//! The host runs the query (`folio-dbsource`), writes the rows as a snapshot CSV into the project and prepares its reference
//! (`server/src/dbmerge.rs`); this edit then does what `selectDataSource` does with that snapshot (`DataMerge/@DataSourceFile`
//! = `folio:path-ref/<id>`, so InDesign reads the snapshot after an IDML export) and keeps the database side in the document
//! label `folio.datamerge` (Folio only; a document label survives InDesign), key `database`:
//! `{query, paramValues, fetchedAt, rows, fields, sha256, lastRefresh?}` (`sha256`: the snapshot's, which its project reference also has).
//! The label is shared key by key: the other keys (`computed`, the computed fields) are read and written back as they are.
//! Both happen in one edit, so one Undo puts the document back.
use crate::{EditError, Work, xml::{self, Index}};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const DM: &str = "designmap.xml";
const LABEL: &str = "folio.datamerge";
const KEY: &str = "database";

fn invalid(m: impl Into<String>) -> EditError { EditError::new("invalid", m) }

fn find_pair(t: &str, ix: &Index) -> Option<usize> {
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some(LABEL))
}

fn read_map(t: &str) -> Result<Map<String, Value>, EditError> {
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let Some(i) = find_pair(t, &ix) else { return Ok(Map::new()) };
    match serde_json::from_str::<Value>(&ix.value(t, i, "Value").unwrap_or_default()) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(invalid("the folio.datamerge label of the document is not a JSON object")),
    }
}

/// The label with `map` as its JSON (taken away when `map` is empty), in the designmap's document `Label`.
fn write_map(w: &mut Work<'_>, map: &Map<String, Value>) -> Result<(), EditError> {
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let existing = find_pair(&t, &ix);
    if map.is_empty() {
        return match existing { Some(i) => w.splice(DM, vec![(ix.nodes[i].start..ix.nodes[i].end, String::new())]), None => Ok(()) };
    }
    let json = serde_json::to_string(&Value::Object(map.clone())).map_err(|e| invalid(e.to_string()))?;
    let pair = format!("<KeyValuePair Key=\"{LABEL}\" Value=\"{}\" />", xml::escape(&json));
    if let Some(i) = existing { return w.splice(DM, vec![(ix.nodes[i].start..ix.nodes[i].end, pair)]); }
    let doc = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document element"))?;
    let props = ix.named(&t, doc, "Properties").first().copied();
    let label = props.and_then(|p| ix.named(&t, p, "Label").first().copied());
    let (node, child) = match (label, props) {
        (Some(label), _) => (label, pair),
        (None, Some(p)) => (p, format!("<Label>{pair}</Label>")),
        (None, None) => (doc, format!("<Properties><Label>{pair}</Label></Properties>")),
    };
    let n = &ix.nodes[node];
    if t[n.start..n.end].trim_end().ends_with("/>") {
        let open = t[n.start..n.end].trim_end().trim_end_matches("/>");
        return w.splice(DM, vec![(n.start..n.end, format!("{open}>{child}</{}>", ix.local(&t, node)))]);
    }
    let at = t[..n.end].rfind("</").ok_or_else(|| EditError::new("xml", "missing closing tag"))?;
    w.splice(DM, vec![(at..at, child)])
}

impl crate::Editor {
    /// The database side of the document's data source (the `database` key of `folio.datamerge`), if it has one.
    pub fn data_merge_database(&self) -> Option<Value> {
        let t = self.part(DM)?;
        read_map(t).ok()?.get(KEY).cloned()
    }
}

fn store(w: &mut Work<'_>, source: &Option<String>, file_type: &Option<String>, state: &Option<Value>, query: Option<&Value>) -> Result<(), EditError> {
    let mut state = match state { Some(Value::Object(m)) => m.clone(), _ => return Err(invalid("the host runs the query and gives the state (state is missing)")) };
    // the host gives the query as it ran (with the connection hint); a state without one takes the command's
    if let Some(q) = query && !state.contains_key("query") { state.insert("query".into(), q.clone()); }
    if !state.get("query").and_then(|q| q.get("connection")).is_some_and(Value::is_string) {
        return Err(invalid("the query names no connection"));
    }
    // no reference in the label: an IDML export writes references as paths of this computer, and the snapshot is told by its
    // content (`sha256` = the SHA-256 of the snapshot, as its project reference has it)
    // the snapshot is the data source InDesign and the merge read (datasource.rs writes the DataMerge element)
    crate::hyperlinks::datasource::select(w, "database snapshot", source, file_type, &None, &None, &None, &None)?;
    let mut map = read_map(w.text(DM)?)?;
    map.insert(KEY.into(), Value::Object(state));
    write_map(w, &map)
}

// The registry passes each wire field separately; `param_values` and `today` are what the host ran the query with.
#[allow(clippy::too_many_arguments)]
pub(crate) fn select(w: &mut Work<'_>, query: &Value, _param_values: &Option<BTreeMap<String, Value>>, _today: &Option<String>, source: &Option<String>, file_type: &Option<String>, state: &Option<Value>) -> Result<(), EditError> {
    if !query.is_object() { return Err(invalid("the query is a JSON object (folio_dbsource::model::Query)")); }
    store(w, source, file_type, state, Some(query))
}

pub(crate) fn refresh(w: &mut Work<'_>, _param_values: &Option<BTreeMap<String, Value>>, _today: &Option<String>, source: &Option<String>, file_type: &Option<String>, state: &Option<Value>) -> Result<(), EditError> {
    let map = read_map(w.text(DM)?)?;
    let Some(before) = map.get(KEY) else { return Err(invalid("the document's data source is not a database")) };
    // the query stays what the document remembers unless the host gives it again
    let query = before.get("query").cloned();
    let mut state = state.clone();
    if let Some(Value::Object(s)) = &mut state && !s.contains_key("query") && let Some(q) = query { s.insert("query".into(), q); }
    store(w, source, file_type, &state, None)
}

/// How the document's database source is refreshed when the document opens (`manual`, `askOnOpen`, `autoOnOpen`): the `refresh` of the
/// remembered query, changed alone (no query is run; the connection need not be on this computer). One Undo goes back.
pub(crate) fn set_refresh_mode(w: &mut Work<'_>, mode: &str) -> Result<(), EditError> {
    if !matches!(mode, "manual" | "askOnOpen" | "autoOnOpen") { return Err(invalid("the refresh mode is manual, askOnOpen or autoOnOpen")); }
    let mut map = read_map(w.text(DM)?)?;
    let Some(Value::Object(db)) = map.get_mut(KEY) else { return Err(invalid("the document's data source is not a database")) };
    let Some(Value::Object(query)) = db.get_mut("query") else { return Err(invalid("the database source has no query")) };
    if query.get("refresh").and_then(Value::as_str).unwrap_or("manual") == mode { return Ok(()); }
    query.insert("refresh".into(), Value::String(mode.into()));
    write_map(w, &map)
}
