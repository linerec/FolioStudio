//! Remove zero-width markers and source wrappers only for read-only text queries.
use super::*;

impl crate::Editor {
    pub fn hyperlink_target(&self, id: &str) -> Result<folio_idml::hyperlinks::Target, EditError> {
        let data = folio_idml::hyperlinks::read(self.parts()).map_err(|e| EditError::new("xml", e))?;
        Ok(folio_idml::hyperlinks::target(&data, id))
    }

    pub fn cross_reference_targets(&self) -> Result<serde_json::Value, EditError> {
        let mut rows = Vec::new();
        for (part, text) in self.parts().iter().filter(|(p,_)|p.starts_with("Stories/")) {
            let ix = Index::scan(text).map_err(|e|EditError::new("xml",e))?;
            let Some(i) = (0..ix.nodes.len()).find(|&i|ix.local(text,i)=="Story"&&ix.value(text,i,"Self").is_some()) else {continue};
            let Some(id) = ix.value(text,i,"Self") else {continue};
            let contents = folio_idml::editorial::contents(text,&id).map_err(|e|EditError::new("xml",format!("{part}: {e}")))?;
            let mut at = 0;
            for paragraph in contents.split('\r') {
                if !paragraph.trim().is_empty() && !paragraph.chars().any(|c|c.is_control()&&c!='\t') {
                    rows.push(serde_json::json!({"story":id,"at":at,"text":paragraph}));
                }
                at += paragraph.chars().count()+1;
            }
        }
        Ok(serde_json::json!(rows))
    }
}

pub(crate) fn query_xml(t: &str) -> Result<Option<String>, EditError> {
    if !(t.contains("<Hyperlink") || t.contains("<CrossReferenceSource")) {
        return Ok(None);
    }
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let mut ranges = Vec::new();
    for i in 0..ix.nodes.len() {
        let n = &ix.nodes[i];
        match ix.local(t, i) {
            "HyperlinkTextSource" | "CrossReferenceSource" => {
                if ix.value(t, i, "AppliedCharacterStyle").is_some_and(|v| v != "n" && !v.is_empty()) {
                    return Err(EditError::new("unsupported", "hyperlink source character style query is not supported yet"));
                }
                ranges.push(n.start..n.open_end);
                ranges.push(n.close_start..n.end);
            }
            "HyperlinkTextDestination" | "ParagraphDestination" => ranges.push(n.start..n.end),
            _ => {}
        }
    }
    ranges.sort_by_key(|r| r.start);
    let mut result = t.to_owned();
    for range in ranges.into_iter().rev() { result.replace_range(range, ""); }
    Ok(Some(result))
}
