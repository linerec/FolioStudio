use super::*;
use serde_json::{Value, json};

/// Read-only note context in the same scalar index space as textStory.
pub fn context(t: &str, id: &str, at: usize) -> Result<Value, EditError> {
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let (owner, index) = folio_idml::footnotes::address(id).map_or((id, None), |(owner, index)| (owner, Some(index)));
    let mut node = ix.by_self(t, owner).ok_or_else(|| EditError::new("notFound", id))?;
    if let Some(index) = index { node = footnotes::node(t, &ix, node, index)?; }
    let story = changes::visible(query::read(t, &ix, node, true)?)?;
    if at > story.atoms.len() { return Err(EditError::new("invalid", "cursor outside story")); }
    let mut ordinal = 0;
    let mut target = None;
    let mut endnote = None;
    for (offset, atom) in story.atoms.iter().enumerate() {
        if atom.raw.starts_with("<Footnote") {
            if at == offset || at == offset + 1 { target = Some(folio_idml::footnotes::text_id(owner, ordinal)); }
            ordinal += 1;
        }
        if atom.raw.starts_with("<Endnote ") && (at == offset || at == offset + 1) {
            let index = Index::scan(&atom.raw).map_err(|e|EditError::new("xml",e))?;
            endnote = index.value(&atom.raw,0,"EndnoteTextRange");
        }
    }
    Ok(json!({"footnoteTarget":target,"endnoteRange":endnote,"canInsertFootnoteMarker":index.is_some() && at == 0 && !story.atoms.iter().any(|a| a.raw == "<?ACE 4?>")}))
}

/// Resolve an IDML endnote range to its first insertion point in its owning story.
pub fn endnote_target(t: &str, range: &str) -> Result<Value, EditError> {
    let ix = Index::scan(t).map_err(|e|EditError::new("xml",e))?;
    let node = ix.by_self(t,range).ok_or_else(||EditError::new("notFound",range))?;
    if ix.local(t,node) != "EndnoteRange" {return Err(unsupported("expected EndnoteRange"));}
    let mut owner = node;
    while ix.local(t,owner) != "Story" {
        owner = ix.nodes[owner].parent.ok_or_else(||unsupported("endnote story absent"))?;
    }
    let root = &ix.nodes[owner];
    let ranges: Vec<_> = ix.nodes.iter().enumerate().filter(|(i,n)|n.start>=root.start && n.end<=root.end && ix.local(t,*i)=="EndnoteRange").collect();
    if ranges.iter().any(|(_,n)|t[n.open_end..n.close_start].matches("<?ACE 4?>").count()!=1) {
        return Err(unsupported("endnote range requires one number marker"));
    }
    let story = query::read(t,&ix,owner,false)?;
    let markers: Vec<_> = story.atoms.iter().enumerate().filter(|(_,a)|a.raw=="<?ACE 4?>").map(|(i,_)|i).collect();
    if markers.len()!=ranges.len() {return Err(unsupported("ambiguous endnote number markers"));}
    let ordinal = ranges.iter().position(|(i,_)|*i==node).ok_or_else(||unsupported("endnote range absent"))?;
    Ok(json!({"story":ix.value(t,owner,"Self"),"at":markers[ordinal]}))
}
