//! Atomic replacement of an ordered GREP list, preserving imported record data.
use super::*;
use folio_model::grep::GrepStyle;
use std::collections::BTreeSet;

fn record(raw: &str, rule: &GrepStyle, style: &str) -> Result<String, EditError> {
    let ix = Index::scan(raw).map_err(|e| EditError::new("xml", e))?;
    let mut result = raw.to_owned();
    let mut edits = Vec::new();
    for (name, value) in [("AppliedCharacterStyle", style), ("GrepExpression", rule.expression.as_str())] {
        let i = ix.children(0).iter().copied().find(|&i| ix.local(raw, i) == name)
            .ok_or_else(|| EditError::new("unsupported", "incomplete GREP record"))?;
        let n = &ix.nodes[i];
        edits.push(if n.empty {
            (n.open_end-2..n.end, format!(">{}</{name}>", escape(value)))
        } else { (n.open_end..n.close_start, escape(value)) });
    }
    edits.sort_by_key(|(r,_)| std::cmp::Reverse(r.start));
    for (r,v) in edits { result.replace_range(r, &v); }
    Ok(result)
}

pub(crate) fn set(w: &mut Work<'_>, style: &str, rules: &[GrepStyle]) -> Result<(), EditError> {
    if rules.len() > 256 { return Err(EditError::new("invalid", "at most 256 GREP styles")); }
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id("ParagraphStyle", style)?;
    if root(&id) { return Err(EditError::new("invalid", "Invalid request on a root style.")); }
    let mut attrs = cat.styles[&id].clone();
    let raw = attrs.get("AllGREPStyles").and_then(|v|v.xml.as_deref()).unwrap_or("<AllGREPStyles type=\"list\"/>");
    let ix = Index::scan(raw).map_err(|e|EditError::new("xml",e))?;
    let records: Vec<_> = ix.children(0).iter().copied().filter(|&i|ix.local(raw,i)=="ListItem").map(|i|&raw[ix.nodes[i].start..ix.nodes[i].end]).collect();
    let mut used = BTreeSet::new();
    let mut content = String::new();
    for rule in rules {
        folio_idml::grep::compile(&rule.expression).map_err(|e|EditError::new("unsupported",e))?;
        let character = cat.id("CharacterStyle", &rule.style)?;
        let template = if let Some(source) = rule.source {
            if !used.insert(source) { return Err(EditError::new("invalid", "duplicate GREP source index")); }
            *records.get(source).ok_or_else(||EditError::new("invalid", "GREP source index out of bounds"))?
        } else { "<ListItem type=\"record\"><AppliedCharacterStyle type=\"object\"></AppliedCharacterStyle><GrepExpression type=\"string\"></GrepExpression></ListItem>" };
        content.push_str(&record(template, rule, &character)?);
    }
    let root = &ix.nodes[0];
    let mut list = raw.to_owned();
    if root.empty {
        list.replace_range(root.open_end-2..root.end, &format!(">{content}</AllGREPStyles>"));
    } else {
        list.insert_str(root.close_start, &content);
        for &i in ix.children(0).iter().rev() {
            if ix.local(raw,i)=="ListItem" { list.replace_range(ix.nodes[i].start..ix.nodes[i].end, ""); }
        }
    }
    let remove = rules.is_empty() && ix.children(0).iter().all(|&i|ix.local(raw,i)=="ListItem") && ix.attrs(raw,0).len() <= 1;
    attrs.insert("EmptyGrepStyles".into(), attr(if rules.is_empty() {"true"} else {"false"}));
    if remove { attrs.remove("AllGREPStyles"); }
    else { attrs.insert("AllGREPStyles".into(), Property {value: String::new(), xml: Some(list)}); }
    put(w, &id, "ParagraphStyle", &attrs)?;
    refresh_stories(w)
}
