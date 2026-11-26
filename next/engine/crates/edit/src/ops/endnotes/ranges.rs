use crate::{EditError, Work};

pub(crate) fn remove_inserted(w: &mut Work<'_>, reference: &str, range: &str) -> Result<(), EditError> {
    let (part, ix, node) = w.find(range)?;
    let t = w.text(&part)?;
    let unsupported = || EditError::new("unsupported", "endnote range structure cannot be removed safely");
    if ix.local(t, node) != "EndnoteRange" || ix.attr(t, node, "SourceEndnote") != Some(reference) {
        return Err(EditError::new("invalid", "endnote reference and body do not match"));
    }
    let parent = ix.nodes[node].parent.ok_or_else(unsupported)?;
    let paragraph = match ix.local(t, parent) {
        "CharacterStyleRange" => ix.nodes[parent].parent.ok_or_else(unsupported)?,
        "ParagraphStyleRange" => parent,
        _ => return Err(unsupported()),
    };
    if ix.local(t, paragraph) != "ParagraphStyleRange" { return Err(unsupported()); }
    let story = ix.nodes[paragraph].parent.ok_or_else(unsupported)?;
    if ix.local(t, story) != "Story" || ix.attr(t, story, "IsEndnoteStory") != Some("true")
        || ix.named(t, story, "ParagraphStyleRange").len() != 1 {
        return Err(unsupported());
    }
    let siblings = ix.children(parent);
    let pos = siblings.iter().position(|&i| i == node).unwrap();
    let mut end = ix.nodes[node].end;
    let mut removed = 1;
    if let Some(&next) = siblings.get(pos + 1) {
        if ix.local(t, next) != "Br" { return Err(unsupported()); }
        end = ix.nodes[next].end;
        removed += 1;
    }
    if siblings.len() == removed { return Err(unsupported()); }
    let owner = ix.value(t, story, "Self").ok_or_else(unsupported)?;
    w.splice(&part, vec![(ix.nodes[node].start..end, String::new())])?;
    w.recompose(&owner);
    Ok(())
}
