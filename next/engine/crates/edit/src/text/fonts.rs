use crate::{EditError, Work, xml::{Index, escape, unescape}};
use std::collections::BTreeMap;
use serde_json::Value;

pub(super) fn register(w: &mut Work<'_>, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    let (Some(family), Some(style)) = (props.get("appliedFont").and_then(Value::as_str), props.get("fontStyle").and_then(Value::as_str)) else { return Ok(()) };
    let Some(record) = w.ed.font_records.iter().find(|r| r.get("FontFamily").is_some_and(|f| f == family)
        && r.get("FontStyleName").is_some_and(|s| s == style)).cloned() else { return Ok(()) };
    let part = "Resources/Fonts.xml";
    let Some(text) = w.ed.parts.get(part) else { return Ok(()) };
    let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    if (0..ix.nodes.len()).any(|i| ix.local(text,i) == "Font"
        && ix.attr(text,i,"FontFamily").is_some_and(|v| unescape(v)==family)
        && ix.attr(text,i,"FontStyleName").is_some_and(|v| unescape(v)==style)) { return Ok(()); }
    let existing = (0..ix.nodes.len()).find(|&i| ix.local(text,i)=="FontFamily" && ix.attr(text,i,"Name").is_some_and(|v| unescape(v)==family));
    let family_id = existing.and_then(|i| ix.attr(text,i,"Self")).map(str::to_owned);
    let at = existing.map(|i| ix.nodes[i].close_start).unwrap_or_else(||
        (0..ix.nodes.len()).find(|&i| ix.local(text,i)=="CompositeFont").map(|i|ix.nodes[i].start).unwrap_or(ix.nodes[0].close_start));
    let family_id = family_id.unwrap_or_else(|| w.new_id());
    let name = record.get("Name").ok_or_else(|| EditError::new("invalid", "font name required"))?;
    let attrs = record.iter().map(|(k,v)|format!(" {k}=\"{}\"",escape(v))).collect::<String>();
    let font = format!("<Font Self=\"{}\"{attrs} />\n", escape(&format!("{family_id}Fontn{name}")));
    let xml = if existing.is_some() { font } else {
        format!("<FontFamily Self=\"{}\" Name=\"{}\">\n{font}</FontFamily>\n",escape(&family_id),escape(family))
    };
    w.splice(part, vec![(at..at, xml)])
}

/// A font the document does not know yet comes in with every style of its family (InDesign: a Word or RTF file naming Times New Roman adds its
/// Regular, Italic, Bold and Bold Italic to the document's fonts; oracle placetext-rtf-format). The host supplied the records.
pub(crate) fn register_family(w: &mut Work<'_>, family: &str) -> Result<(), EditError> {
    for style in ["Regular", "Italic", "Bold", "Bold Italic"] {
        let props = BTreeMap::from([("appliedFont".to_owned(), Value::String(family.to_owned())), ("fontStyle".to_owned(), Value::String(style.to_owned()))]);
        register(w, &props)?;
    }
    Ok(())
}
