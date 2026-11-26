use super::{DocumentFrames, listplan::Plan};
use folio_model::Document;
use std::collections::BTreeMap;

/// Number changes can invalidate other stories and their saved line widths.
pub fn changed(old: &Document, new: &Document) -> bool {
    super::listplan::Plan::new(old, &DocumentFrames::new(old)).0
        != super::listplan::Plan::new(new, &DocumentFrames::new(new)).0
}

/// Prefixes for a story or cell, using the same document plan as composition.
pub fn labels(doc: &Document, id: &str) -> Result<BTreeMap<String,String>,String> {
    let frames=DocumentFrames::new(doc);
    let id=frames.frame(id).map_or(id,|f|f.frame.story.as_str());
    Plan::new(doc,&frames).0.remove(id)
        .map(|labels| labels.into_iter().map(|(at,label)| (at.to_string(),label)).collect())
        .ok_or_else(|| "list conversion requires a story or cell".into())
}

pub fn document_labels(doc:&Document)->Result<BTreeMap<String,BTreeMap<String,String>>,String>{
    Ok(Plan::new(doc,&DocumentFrames::new(doc)).0.into_iter().filter(|(_,labels)|!labels.is_empty())
        .map(|(id,labels)| (id,labels.into_iter().map(|(at,label)| (at.to_string(),label)).collect())).collect())
}
