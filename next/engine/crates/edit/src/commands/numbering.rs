use crate::{EditError, Work, xml::{Index, escape}};
use std::collections::BTreeMap;
use serde_json::Value;
const PART: &str = "designmap.xml";
fn error(s: impl ToString) -> EditError { EditError::new("invalid", s.to_string()) }
impl crate::Editor {
    pub fn numbering_lists(&self) -> Result<Vec<folio_model::numbering::NumberingList>, EditError> {
        folio_idml::numbering::read(self.part(PART).ok_or_else(|| error("missing designmap"))?).map_err(error)
    }
}
fn references(w: &mut Work<'_>, old: &str, new: &str) -> Result<(), EditError> {
    let names: Vec<_> = w.ed.parts.keys().cloned().collect();
    for part in names {
        let t = w.text(&part)?.to_owned(); let ix = Index::scan(&t).map_err(error)?;
        let mut edits = Vec::new();
        for i in 0..ix.nodes.len() {
            if ix.value(&t,i,"AppliedNumberingList").as_deref() == Some(old) {
                edits.push(crate::ops::attr_edit(&ix,&t,i,"AppliedNumberingList",new));
            }
            let n = &ix.nodes[i];
            if ix.local(&t,i)=="AppliedNumberingList" && !n.empty && crate::xml::unescape(&t[n.open_end..n.close_start])==old {
                edits.push((n.open_end..n.close_start,escape(new)));
            }
        }
        if !edits.is_empty() { w.splice(&part,edits)?; }
        if let Some(story)=part.strip_prefix("Stories/Story_").and_then(|s|s.strip_suffix(".xml")) { w.recompose(story); }
    }
    Ok(())
}
pub(crate) fn set(w: &mut Work<'_>, id: &str, name: &str, props: &BTreeMap<String,Value>) -> Result<(),EditError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {return Err(error("list name required"));}
    let fields=[("acrossStories","ContinueNumbersAcrossStories"),("acrossDocuments","ContinueNumbersAcrossDocuments")];
    if props.iter().any(|(k,v)|!fields.iter().any(|(a,_)|a==k)||!v.is_boolean()) {return Err(error("invalid list option"));}
    let lists=w.ed.numbering_lists()?;
    if id==folio_idml::numbering::DEFAULT {return Err(error("default list cannot be renamed"));}
    if !id.is_empty() && !lists.iter().any(|l|l.id==id) {return Err(error("list not found"));}
    if lists.iter().any(|l|l.id!=id&&l.name==name) {return Err(error("list name already exists"));}
    let new_id=format!("NumberingList/{name}");
    let t=w.text(PART)?.to_owned();let ix=Index::scan(&t).map_err(error)?;
    if id.is_empty() {
        let mut attributes=String::new();
        for (api,xml) in fields {attributes.push_str(&format!(" {xml}=\"{}\"",props.get(api).and_then(Value::as_bool).unwrap_or(true)));}
        let at=(0..ix.nodes.len()).filter(|&i|ix.local(&t,i)=="NumberingList").map(|i|ix.nodes[i].end).next_back().unwrap_or(ix.nodes[0].close_start);
        w.splice(PART,vec![(at..at,format!("<NumberingList Self=\"{}\" Name=\"{}\"{attributes}/>\n",escape(&new_id),escape(name)))])?;
        w.created.push(new_id);
    } else {
        let i=(0..ix.nodes.len()).find(|&i| ix.value(&t,i,"Self").as_deref()==Some(id)).ok_or_else(||error("list not found"))?;
        // Update each attribute against the latest source, preserving unknown fields.
        let mut values=vec![("Self",new_id.clone()),("Name",name.to_owned())];
        for (api,xml) in fields {if let Some(value)=props.get(api) {values.push((xml,value.to_string()));}}
        for (key,value) in values {
            let t=w.text(PART)?.to_owned();let ix=Index::scan(&t).map_err(error)?;
            w.splice(PART,vec![crate::ops::attr_edit(&ix,&t,i,key,&value)])?;
        }
        if id!=new_id {references(w,id,&new_id)?;}
        else {
            let stories: Vec<_> = w.ed.parts.keys().filter_map(|part| part.strip_prefix("Stories/Story_").and_then(|s|s.strip_suffix(".xml")).map(str::to_owned)).collect();
            for story in stories { w.recompose(&story); }
        }
    }
    Ok(())
}
pub(crate) fn delete(w:&mut Work<'_>,id:&str)->Result<(),EditError>{
    if id==folio_idml::numbering::DEFAULT {return Err(error("default list cannot be deleted"));}
    let t=w.text(PART)?.to_owned();let ix=Index::scan(&t).map_err(error)?;
    let i=(0..ix.nodes.len()).find(|&i| ix.value(&t,i,"Self").as_deref()==Some(id)).filter(|&i|ix.local(&t,i)=="NumberingList").ok_or_else(||error("list not found"))?;
    let n=&ix.nodes[i];w.splice(PART,vec![(n.start..n.end,String::new())])?;
    references(w,id,folio_idml::numbering::DEFAULT)
}
