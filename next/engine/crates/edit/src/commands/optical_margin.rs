use crate::{EditError, Work, xml::Index};
use std::collections::BTreeMap;
use serde_json::Value;

fn error(message: impl ToString) -> EditError { EditError::new("invalid",message.to_string()) }

fn owner<'a>(parts: &'a BTreeMap<String,String>, id: &str) -> Result<(&'a str, Index, usize), EditError> {
    let id=folio_idml::footnotes::address(id).map_or(id, |(story,_)|story);
    owner_inner(parts, id, &mut std::collections::BTreeSet::new())
}

fn owner_inner<'a>(parts: &'a BTreeMap<String,String>, id: &str, visited: &mut std::collections::BTreeSet<String>) -> Result<(&'a str, Index, usize), EditError> {
    if !visited.insert(id.to_owned()) { return Err(error("cyclic story reference")); }
    for (part,t) in parts {
        let ix=Index::scan(t).map_err(error)?;
        let Some(i)=ix.by_self(t,id) else {continue};
        if let Some(story)=ix.attr(t,i,"ParentStory") { return owner_inner(parts,story,visited); }
        let story = std::iter::once(i).chain(ix.ancestors(i)).find(|&n|ix.local(t,n)=="Story");
        if let Some(story)=story {
            return Ok((part,ix,story));
        }
        return Err(error("select a story or text frame"));
    }
    Err(EditError::new("notFound",id))
}

impl crate::Editor {
    pub fn optical_margin(&self, id: &str) -> Result<folio_model::optical_margin::OpticalMargin, EditError> {
        let (part,ix,i)=owner(self.parts(),id)?;
        let t=&self.parts()[part];
        let pref=ix.children(i).iter().copied().find(|&n|ix.local(t,n)=="StoryPreference");
        Ok(folio_idml::optical_margin::read(|key|pref.and_then(|n|ix.attr(t,n,key))))
    }
}

pub(crate) fn set(w: &mut Work<'_>, id: &str, props: &BTreeMap<String,Value>) -> Result<(), EditError> {
    let values=props.iter().map(|(k,v)|folio_idml::optical_margin::encode(k,v).ok_or_else(||error(format!("invalid optical margin {k}"))))
        .collect::<Result<Vec<_>,_>>()?;
    let (part,ix,i)=owner(w.ed.parts(),id)?;
    let part=part.to_owned();
    let t=w.text(&part)?.to_owned();
    let story=ix.attr(&t,i,"Self").ok_or_else(||error("story has no id"))?.to_owned();
    if values.is_empty() {return Ok(());}
    if !ix.children(i).iter().any(|&n|ix.local(&t,n)=="StoryPreference") {
        let node=&ix.nodes[i];
        let at=node.open_end;
        let change=if node.empty {
            let name=&t[node.name.clone()];
            (at-2..at,format!("><StoryPreference/></{name}>"))
        } else {
            (at..at,"<StoryPreference/>".into())
        };
        w.splice(&part,vec![change])?;
    }
    for (key,value) in values {
        let t=w.text(&part)?.to_owned();let ix=Index::scan(&t).map_err(error)?;
        let i=ix.by_self(&t,&story).ok_or_else(||error("story missing"))?;
        let n=ix.children(i).iter().copied().find(|&n|ix.local(&t,n)=="StoryPreference")
            .ok_or_else(||EditError::new("xml",format!("{part}: Story {story} has no StoryPreference")))?;
        w.splice(&part,vec![crate::ops::attr_edit(&ix,&t,n,key,&value)])?;
    }
    w.recompose(&story);
    Ok(())
}

/// Type ▸ Type Orientation (vtext-c1): `StoryPreference/@StoryOrientation` of the story becomes `orientation`
/// ("Vertical" or "Horizontal"). The story preference is shared with the optical margin settings above.
pub(crate) fn set_orientation(w: &mut Work<'_>, id: &str, orientation: &str) -> Result<(), EditError> {
    let value = match orientation {
        "Vertical" | "vertical" | "VERTICAL" => "Vertical",
        "Horizontal" | "horizontal" | "HORIZONTAL" => "Horizontal",
        other => return Err(error(format!("invalid story orientation {other}"))),
    };
    // an empty story: the frame made earlier in the same edit (the Vertical Type tool's drag: one undo)
    let made;
    let id = if id.is_empty() { made = w.created.last().cloned().ok_or_else(|| error("no story: give a story or frame id"))?; made.as_str() } else { id };
    let (part,ix,i)=owner(w.ed.parts(),id)?;
    let part=part.to_owned();
    let t=w.text(&part)?.to_owned();
    let story=ix.attr(&t,i,"Self").ok_or_else(||error("story has no id"))?.to_owned();
    if !ix.children(i).iter().any(|&n|ix.local(&t,n)=="StoryPreference") {
        let node=&ix.nodes[i];
        let at=node.open_end;
        let change=if node.empty {
            let name=&t[node.name.clone()];
            (at-2..at,format!("><StoryPreference/></{name}>"))
        } else {
            (at..at,"<StoryPreference/>".into())
        };
        w.splice(&part,vec![change])?;
    }
    let t=w.text(&part)?.to_owned();let ix=Index::scan(&t).map_err(error)?;
    let i=ix.by_self(&t,&story).ok_or_else(||error("story missing"))?;
    let n=ix.children(i).iter().copied().find(|&n|ix.local(&t,n)=="StoryPreference")
        .ok_or_else(||EditError::new("xml",format!("{part}: Story {story} has no StoryPreference")))?;
    if ix.attr(&t,n,"StoryOrientation")==Some(value) { return Ok(()); }
    w.splice(&part,vec![crate::ops::attr_edit(&ix,&t,n,"StoryOrientation",value)])?;
    w.recompose(&story);
    Ok(())
}

impl crate::Editor {
    /// Whether the story of `id` (a story or one of its frames) is vertical (`StoryOrientation="Vertical"`).
    pub fn story_vertical(&self, id: &str) -> Result<bool, EditError> {
        let (part,ix,i)=owner(self.parts(),id)?;
        let t=&self.parts()[part];
        let pref=ix.children(i).iter().copied().find(|&n|ix.local(t,n)=="StoryPreference");
        Ok(pref.and_then(|n|ix.attr(t,n,"StoryOrientation"))==Some("Vertical"))
    }
}
