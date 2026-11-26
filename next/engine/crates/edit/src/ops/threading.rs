//! Text threads measured in basic/thread-* and basic-create-type/thread-*-defaults.
//! Linking retains the upstream Story, appends the incoming text (no added return),
//! and inserts its frame chain before the old successor. Unlinking leaves ALL text
//! upstream and gives the detached frames an empty Story, not a composed text slice.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone)]
struct Frame { id: String, story: String, previous: String, next: String }

fn frame(w: &mut Work<'_>, id: &str) -> Result<Frame, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    if !part.starts_with("Spreads/") || ix.local(t, i) != "TextFrame" {
        return Err(EditError::new("unsupported", "threading requires a document text frame"));
    }
    for k in std::iter::once(i).chain(ix.ancestors(i)) {
        if ix.attr(t, k, "Locked") == Some("true") || ix.attr(t, k, "Visible") == Some("false") {
            return Err(EditError::new("locked", "text frame or its parent is locked or hidden"));
        }
        if let Some(layer) = ix.attr(t, k, "ItemLayer") {
            let dm = w.text("designmap.xml")?;
            let dx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
            if let Some(li) = dx.by_self(dm, layer)
                && (dx.attr(dm, li, "Locked") == Some("true") || dx.attr(dm, li, "Visible") == Some("false"))
            { return Err(EditError::new("locked", "text frame layer is locked or hidden")); }
        }
    }
    Ok(Frame {
        id: id.into(),
        story: ix.attr(t,i,"ParentStory").ok_or_else(||EditError::new("xml","frame without story"))?.into(),
        previous: ix.attr(t,i,"PreviousTextFrame").unwrap_or("n").into(),
        next: ix.attr(t,i,"NextTextFrame").unwrap_or("n").into(),
    })
}

fn chain(w: &mut Work<'_>, first: &Frame) -> Result<Vec<Frame>, EditError> {
    let mut frames=vec![first.clone()];
    let mut seen=BTreeSet::from([first.id.clone()]);
    while frames.last().unwrap().next != "n" {
        let last=frames.last().unwrap();
        if !seen.insert(last.next.clone()) { return Err(EditError::new("invalid","cyclic text thread")); }
        let next=frame(w,&last.next)?;
        if next.previous != last.id || next.story != first.story {
            return Err(EditError::new("xml","inconsistent text thread"));
        }
        frames.push(next);
    }
    Ok(frames)
}

pub(super) fn run(w: &mut Work<'_>, id: &str, next: Option<&str>) -> Result<(), EditError> {
    let from=frame(w,id)?;
    if next.unwrap_or("n") == from.next { return Ok(()); }
    let following=chain(w,&from)?;
    if let Some(next)=next {
        let first=frame(w,next)?;
        if first.story == from.story {
            return Err(EditError::new("invalid","choose a frame of a different story"));
        }
        let mut head=first.clone();
        let mut seen=BTreeSet::new();
        while head.previous!="n" {
            if !seen.insert(head.id.clone()) { return Err(EditError::new("invalid","cyclic text thread")); }
            let previous=frame(w,&head.previous)?;
            if previous.next!=head.id || previous.story!=first.story {
                return Err(EditError::new("xml","inconsistent text thread"));
            }
            head=previous;
        }
        let mut incoming=chain(w,&head)?;
        // thread-target-middle: rotate the incoming chain to start at the clicked
        // frame; Story text still appends in its original order.
        let pivot=incoming.iter().position(|f|f.id==first.id).unwrap();
        incoming.rotate_left(pivot);
        let (sp,_,_)=w.find(&first.story)?;
        let text=crate::text::contents(w.text(&sp)?,&first.story)?;
        if !text.is_empty() {
            let (dp,_,_)=w.find(&from.story)?;
            let at=crate::text::contents(w.text(&dp)?,&from.story)?.chars().count();
            let fragment=crate::text::format::copy(w,&first.story,0,text.chars().count())?;
            crate::text::format::paste(w,&from.story,at,&fragment)?;
        }
        // thread-insert-text: its text appends to the Story even when its frame is
        // inserted in the middle; existing text is not split at the clicked port.
        set_attrs(w,id,&[("NextTextFrame",first.id.clone())])?;
        if let Some(old)=following.get(1) { set_attrs(w,&old.id,&[("PreviousTextFrame",incoming.last().unwrap().id.clone())])?; }
        for (i,f) in incoming.iter().enumerate() {
            set_attrs(w,&f.id,&[
                ("ParentStory",from.story.clone()),
                ("PreviousTextFrame",if i==0 {id.into()} else {incoming[i-1].id.clone()}),
                ("NextTextFrame",incoming.get(i+1).map(|f|f.id.clone()).unwrap_or_else(||from.next.clone())),
            ])?;
        }
        delete_story(w,&first.story)?;
        w.ed.text_state.remove(&first.story);
        w.recompose(&first.story);
    } else {
        // thread-unlink/defaults: detached frames are empty and use the document's
        // text insertion defaults, not the source Story's first/last character style.
        let story=w.new_id();
        let empty=create::empty_story(w,&story)?;
        w.add_part(format!("Stories/Story_{story}.xml"),empty)?;
        story_copy::register(w,&story)?;
        set_attrs(w,id,&[("NextTextFrame","n".into())])?;
        for (i,f) in following.iter().skip(1).enumerate() {
            let mut attrs=vec![("ParentStory",story.clone())];
            if i==0 { attrs.push(("PreviousTextFrame","n".into())); }
            set_attrs(w,&f.id,&attrs)?;
        }
        w.recompose(&story);
    }
    w.ed.text_state.remove(&from.story);
    w.recompose(&from.story);
    Ok(())
}
