use crate::{EditError, Work, xml::Index};
use serde_json::Value;
use std::collections::BTreeMap;

const PART: &str = "META-INF/metadata.xml";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RIGHTS: &str = "http://ns.adobe.com/xap/1.0/rights/";
const EMPTY: &str = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\"></rdf:Description></rdf:RDF></x:xmpmeta>";
struct Field { key: &'static str, local: &'static str, container: &'static str }
const FIELDS: &[Field] = &[
    Field { key: "documentTitle", local: "title", container: "Alt" },
    Field { key: "author", local: "creator", container: "Seq" },
    Field { key: "description", local: "description", container: "Alt" },
    Field { key: "keywords", local: "subject", container: "Bag" },
    Field { key: "copyrightNotice", local: "rights", container: "Alt" },
    Field { key: "copyrightStatus", local: "Marked", container: "" },
];
fn err(e: impl ToString) -> EditError { EditError::new("xml", e.to_string()) }
fn named(ix: &Index, t: &str, i: usize, ns: &str, local: &str) -> bool {
    if ix.local(t, i) != local { return false; }
    let name = &t[ix.nodes[i].name.clone()];
    let key = name.split_once(':').map(|(p, _)| format!("xmlns:{p}")).unwrap_or_else(|| "xmlns".into());
    let mut ancestor = Some(i);
    while let Some(n) = ancestor {
        if let Some(value) = ix.attr(t, n, &key) { return value == ns; }
        ancestor = ix.nodes[n].parent;
    }
    false
}
fn field_node(ix: &Index, t: &str, f: &Field) -> Option<usize> {
    (0..ix.nodes.len()).find(|&i| named(ix, t, i, if f.container.is_empty() { RIGHTS } else { DC }, f.local))
}
fn text(ix: &Index, t: &str, i: usize) -> Result<String, EditError> {
    let n = &ix.nodes[i];
    let mut reader = quick_xml::Reader::from_str(&t[n.start..n.end]);
    let mut result = String::new();
    loop {
        use quick_xml::events::Event;
        match reader.read_event().map_err(err)? {
            Event::Text(e) => result.push_str(&e.xml10_content()),
            Event::CData(e) => result.push_str(&e.xml10_content()),
            Event::GeneralRef(e) => result.push_str(&crate::xml::unescape(&format!("&{};", e.xml10_content()))),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}
fn list(ix: &Index, t: &str, i: usize) -> Vec<usize> {
    ix.children(i).iter().flat_map(|&c| ix.children(c)).copied().filter(|&c| named(ix, t, c, RDF, "li")).collect()
}
fn values(t: &str) -> Result<BTreeMap<String, String>, EditError> {
    let ix = Index::scan(t).map_err(err)?;
    FIELDS.iter().map(|f| {
        let value = if let Some(i) = field_node(&ix, t, f) {
            if f.container.is_empty() { text(&ix, t, i)? }
            else {
                let items = list(&ix, t, i);
                if f.container == "Alt" {
                    items.iter().find(|&&c| ix.attr(t, c, "xml:lang") == Some("x-default")).or(items.first()).map(|&c| text(&ix, t, c)).transpose()?.unwrap_or_default()
                } else { items.iter().map(|&c| text(&ix, t, c)).collect::<Result<Vec<_>, _>>()?.join("\n") }
            }
        } else if f.container.is_empty() { "Unknown".into() } else { String::new() };
        Ok((f.key.into(), value))
    }).collect()
}
fn element(f: &Field, value: &str) -> String {
    let escaped = crate::xml::escape(value);
    if f.container.is_empty() { return format!("<xmpRights:Marked xmlns:xmpRights=\"{RIGHTS}\">{escaped}</xmpRights:Marked>"); }
    let entries = if f.container == "Alt" { format!("<rdf:li xml:lang=\"x-default\">{escaped}</rdf:li>") }
        else { value.lines().filter(|s| !s.is_empty()).map(|s| format!("<rdf:li>{}</rdf:li>", crate::xml::escape(s))).collect() };
    format!("<dc:{n} xmlns:dc=\"{DC}\" xmlns:rdf=\"{RDF}\"><rdf:{c}>{entries}</rdf:{c}></dc:{n}>", n=f.local, c=f.container)
}
pub(crate) fn set(w: &mut Work<'_>, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    for (key, value) in props {
        if !FIELDS.iter().any(|f| f.key == key) || !value.is_string() || value.as_str().is_some_and(|v| v.chars().any(|c| c < ' ' && !matches!(c, '\n'|'\r'|'\t'))) {
            return Err(EditError::new("invalid", format!("invalid metadata {key}")));
        }
        if key == "copyrightStatus" && value.as_str().is_some_and(|v| !["True", "False", "Unknown"].contains(&v)) { return Err(EditError::new("invalid", "copyrightStatus")); }
    }
    if props.is_empty() { return Ok(()); }
    if w.ed.part(PART).is_none() { w.add_part(PART.into(), EMPTY.into())?; }
    for f in FIELDS {
        let Some(value) = props.get(f.key).and_then(Value::as_str) else { continue; };
        let t = w.text(PART)?.to_owned();
        if values(&t)?.get(f.key).is_some_and(|s| s == value) { continue; }
        let ix = Index::scan(&t).map_err(err)?;
        let existing = field_node(&ix, &t, f);
        let replacement = if value.is_empty() || value == "Unknown" && f.container.is_empty() { String::new() } else { element(f, value) };
        let edit = if let Some(i) = existing {
            let n = &ix.nodes[i];
            // A dialog edits the default language without discarding translated values.
            let default = if f.container == "Alt" { list(&ix, &t, i).into_iter().find(|&c| ix.attr(&t, c, "xml:lang") == Some("x-default")) } else { None };
            if let Some(c) = default {
                let n = &ix.nodes[c];
                (n.start..n.end, format!("<rdf:li xmlns:rdf=\"{RDF}\" xml:lang=\"x-default\">{}</rdf:li>", crate::xml::escape(value)))
            } else if f.container == "Alt" && !list(&ix, &t, i).is_empty() {
                let container = ix.children(i)[0]; let at = ix.nodes[container].close_start;
                (at..at, format!("<rdf:li xmlns:rdf=\"{RDF}\" xml:lang=\"x-default\">{}</rdf:li>", crate::xml::escape(value)))
            } else { (n.start..n.end, replacement) }
        } else {
            if replacement.is_empty() { continue; }
            let i = (0..ix.nodes.len()).find(|&i| named(&ix, &t, i, RDF, "Description")).ok_or_else(|| err("missing RDF Description"))?;
            let n = &ix.nodes[i];
            if n.empty {
                let name = &t[n.name.clone()];
                (n.open_end-2..n.end, format!(">{replacement}</{name}>"))
            } else { (n.close_start..n.close_start, replacement) }
        };
        w.splice(PART, vec![edit])?;
    }
    Ok(())
}
impl crate::Editor {
    pub fn file_info(&self) -> Result<BTreeMap<String, String>, EditError> { values(self.part(PART).unwrap_or(EMPTY)) }
}
