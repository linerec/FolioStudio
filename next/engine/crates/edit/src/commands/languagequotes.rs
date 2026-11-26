use crate::{EditError, Work, xml::Index};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

const PART: &str = "designmap.xml";
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageQuotes {
    id: String,
    name: String,
    single_quotes: String,
    double_quotes: String,
}
fn pair(value: &Value) -> Result<String, EditError> {
    let text = value.as_str().ok_or_else(|| EditError::new("invalid", "expected quotation mark pair"))?;
    if text.chars().count() != 2 { return Err(EditError::new("invalid", "expected two quotation marks")); }
    text.chars().map(|c| {
        char::from_u32(c as u32 & 0xffff).filter(|c| !c.is_control() && *c != '\u{fffe}' && *c != '\u{ffff}')
            .ok_or_else(|| EditError::new("invalid", "invalid quotation mark"))
    }).collect()
}
pub(crate) fn set(w: &mut Work<'_>, language: &String, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    let values = props.iter().map(|(key, value)| {
        if !matches!(key.as_str(), "SingleQuotes" | "DoubleQuotes") { return Err(EditError::new("invalid", "unknown language preference")); }
        Ok((key, pair(value)?))
    }).collect::<Result<Vec<_>, EditError>>()?;
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e.to_string()))?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(&t,i)=="Language" && ix.attr(&t,i,"Self").map(crate::xml::unescape).as_ref()==Some(language))
        .ok_or_else(|| EditError::new("notFound", "document language not found"))?;
    let edits = values.into_iter().filter_map(|(key,value)|
        (ix.attr(&t,i,key).map(crate::xml::unescape).as_deref()!=Some(&value)).then(|| crate::ops::attr_edit(&ix,&t,i,key,&value))
    ).collect();
    w.splice(PART,edits)
}
impl crate::Editor {
    pub fn language_quotes(&self) -> Result<Vec<LanguageQuotes>, EditError> {
        let t=self.part(PART).ok_or_else(|| EditError::new("notFound",PART))?;
        let ix=Index::scan(t).map_err(|e| EditError::new("xml",e.to_string()))?;
        Ok((0..ix.nodes.len()).filter(|&i| ix.local(t,i)=="Language").filter_map(|i| {
            let attr=|key| ix.attr(t,i,key).map(crate::xml::unescape);
            Some(LanguageQuotes { id:attr("Self")?, name:attr("Name")?.trim_start_matches("$ID/").into(),
                single_quotes:attr("SingleQuotes").unwrap_or_default(), double_quotes:attr("DoubleQuotes").unwrap_or_default() })
        }).collect())
    }
}
