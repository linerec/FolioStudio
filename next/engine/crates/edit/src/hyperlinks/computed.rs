//! Data Merge computed fields in the document: the Folio-only document label `folio.datamerge` (a JSON object in the designmap's
//! `Label`, which survives InDesign), key `computed` = `[{name, expr}]` (the language: `folio_idml::datamerge_expr`).
//! The label is written key by key: the other keys (the database query of `dbmerge-a`) are read and written back as they are.
use super::*;
use folio_idml::datamerge_expr as expr;
use folio_model::datamerge::ComputedField;
use serde_json::{Map, Value};

const DM: &str = "designmap.xml";
/// The label the data merge keeps its Folio-only state in.
pub const LABEL: &str = "folio.datamerge";
const KEY: &str = "computed";

fn find_pair(t: &str, ix: &Index) -> Option<usize> {
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some(LABEL))
}

pub(crate) fn read_map(t: &str) -> Result<Map<String, Value>, EditError> {
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let Some(i) = find_pair(t, &ix) else { return Ok(Map::new()) };
    match serde_json::from_str::<Value>(&ix.value(t, i, "Value").unwrap_or_default()) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(EditError::new("invalid", "the folio.datamerge label of the document is not a JSON object")),
    }
}

impl crate::Editor {
    /// The keys of the document's `folio.datamerge` label (empty when there is none or it cannot be read).
    pub fn data_merge_label(&self) -> Map<String, Value> {
        self.part(DM).and_then(|t| read_map(t).ok()).unwrap_or_default()
    }

    /// The computed fields of the document, in the order they were defined.
    pub fn data_merge_computed(&self) -> Vec<ComputedField> {
        self.data_merge_label().get(KEY).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }
}

fn bad(name: &str, e: &expr::ExprError) -> EditError {
    EditError::new("invalid", format!("{name}: {} (character {})", e.message, e.pos + 1))
}

/// Setting the computed fields: every name must be a field name no other computed field has, and every expression must be
/// well formed. Whether it names a column of the data source is the host's to check (it has the source).
pub(crate) fn set(w: &mut Work<'_>, fields: &Vec<ComputedField>) -> Result<(), EditError> {
    if fields.len() > expr::MAX_COMPUTED_FIELDS { return Err(invalid(&format!("at most {} computed fields", expr::MAX_COMPUTED_FIELDS))); }
    let mut seen: Vec<(String, String)> = Vec::new();
    for cf in fields {
        let field = expr::field_of_name(&cf.name).map_err(|e| bad(&cf.name, &e))?;
        let key = (field.kind, field.name);
        if seen.contains(&key) { return Err(bad(&cf.name, &expr::ExprError { code: "duplicateName", pos: 0, end: 0, args: vec![], message: "another computed field has this name".into() })); }
        seen.push(key);
        expr::compile(&cf.expr, None, &[]).map_err(|e| bad(&cf.name, &e))?;
    }
    let mut map = read_map(w.text(DM)?)?;
    let before = map.clone();
    if fields.is_empty() { map.remove(KEY); } else { map.insert(KEY.into(), serde_json::to_value(fields).map_err(|e| invalid(&e.to_string()))?); }
    if map == before { return Ok(()); }
    write_map(w, &map)
}

/// The label with `map` as its JSON (taken away when `map` is empty), in the designmap's document `Label`.
pub(crate) fn write_map(w: &mut Work<'_>, map: &Map<String, Value>) -> Result<(), EditError> {
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let existing = find_pair(&t, &ix);
    if map.is_empty() {
        return match existing { Some(i) => w.splice(DM, vec![(ix.nodes[i].start..ix.nodes[i].end, String::new())]), None => Ok(()) };
    }
    let json = serde_json::to_string(&Value::Object(map.clone())).map_err(|e| invalid(&e.to_string()))?;
    let pair = format!("<KeyValuePair Key=\"{LABEL}\" Value=\"{}\" />", esc(&json));
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
