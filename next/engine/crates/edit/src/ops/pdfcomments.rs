//! PDF comments stored in the document (File ▸ Import PDF Comments…, the PDF Comments panel). Measured with InDesign 2025
//! (`next/oracle/rules/pdfcomments.ko.md`, scenarios `pdfcomments3-*`):
//! - a comment is a `PDFComment` of the designmap (replies inside as `Reply`), pointing by `CommentDestination` at a destination
//!   element of the designmap: the destinations come first, grouped by kind (page, page item, text), then the comments; all of them
//!   in front of the `IndexingSortOption`s;
//! - the only states are `OpenStatus` and `ResolvedStatus` (a reply has none); deleting a comment deletes its replies;
//! - Accept on a strikethrough comment deletes the commented text (a space that stood on both sides of it goes with it, as the
//!   adjusting of spacing does) and leaves one U+FEFF in its place; the comment is then applied, resolved and an orphan.
//!   The host reads the PDF (`importPdfComments` gets the comments ready; the edit crate reads no files) and keeps the file as a
//!   project reference (principle 1): `source` is what `CommentFilePath` gets, `$ID/folio:path-ref/<id>`.
use crate::{EditError, Work, xml::Index};
use folio_idml::pdfcomments as dm;
use folio_model::pdfcomments::{PdfComment, PdfDestination};
use std::ops::Range;

const DM: &str = "designmap.xml";
/// The order InDesign writes them in (a second import adds to each group).
const GROUPS: [&str; 4] = ["PDFCommentPageDestination", "PDFCommentPageItemDestination", "PDFCommentTextDestination", "PDFComment"];

fn error(m: impl Into<String>) -> EditError { EditError::new("invalid", m) }
fn xml(e: String) -> EditError { EditError::new("xml", e) }

fn read(w: &Work<'_>) -> Result<Vec<PdfComment>, EditError> { dm::read(&w.ed.parts).map_err(xml) }

impl crate::Editor {
    /// The comments of the document (File ▸ Import PDF Comments…), in the order of the designmap.
    pub fn pdf_comments(&self) -> Result<Vec<PdfComment>, EditError> { dm::read(&self.parts).map_err(xml) }
}

fn top_level(ix: &Index, t: &str, name: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == name && ix.nodes[i].parent.is_some_and(|p| ix.nodes[p].parent.is_none())).collect()
}

/// Where each group's new elements go: after the last element of the group, else after the last one of an earlier group, else in
/// front of the first `IndexingSortOption` (InDesign's place for them), else at the end of the designmap. `(offset, after)`.
fn anchor(ix: &Index, t: &str, group: usize) -> Result<(usize, bool), EditError> {
    for g in (0..=group).rev() {
        if let Some(&last) = top_level(ix, t, GROUPS[g]).last() { return Ok((ix.nodes[last].end, true)); }
    }
    if let Some(&first) = top_level(ix, t, "IndexingSortOption").first() { return Ok((ix.nodes[first].start, false)); }
    let root = ix.roots().next().ok_or_else(|| xml("designmap has no root".into()))?;
    Ok((ix.nodes[root].close_start, false))
}

pub(crate) fn import(w: &mut Work<'_>, path: &str, again: &bool, source: &Option<String>, comments: &Option<Vec<PdfComment>>) -> Result<(), EditError> {
    let source = source.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| error(format!("{path}: the PDF's file reference is missing (the host reads the PDF)")))?;
    let comments = comments.as_ref().ok_or_else(|| error(format!("{path}: the PDF's comments are missing (the host reads the PDF)")))?;
    if comments.is_empty() { return Err(EditError::new("noComments", "PDF has no comments. Select another PDF to continue.")); }
    let existing = read(w)?;
    if !*again && existing.iter().any(|c| c.file == source) {
        return Err(EditError::new("duplicate", "PDF with the same file name is already imported. Re-import can lead to duplicate comments."));
    }
    // each comment's elements, ids in InDesign's order: destination, comment, replies
    let mut blocks: [Vec<String>; 4] = Default::default();
    for c in comments {
        let mut c = c.clone();
        if let Some(d) = c.destination.as_mut() {
            d.id = w.new_id();
            let group = match d.kind.as_str() { dm::PAGE => 0, dm::PAGE_ITEM => 1, dm::TEXT => 2, other => return Err(error(format!("unknown destination kind {other}"))) };
            blocks[group].push(dm::destination_xml(d));
        }
        c.id = w.new_id();
        for r in &mut c.replies { r.id = w.new_id(); }
        c.file = source.to_owned();
        if c.status.is_empty() { c.status = dm::OPEN.into(); }
        if c.kind.is_empty() { return Err(error("a comment needs its type")); }
        blocks[3].push(dm::comment_xml(&c));
        w.created.push(c.id.clone());
    }
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(xml)?;
    // one text per place, so elements that go to the same offset keep their order
    let mut edits: Vec<(usize, String)> = Vec::new();
    for (group, elements) in blocks.iter().enumerate().filter(|(_, e)| !e.is_empty()) {
        let (at, after) = anchor(&ix, &t, group)?;
        let text = if after { elements.iter().map(|e| format!("\n\t{e}")).collect::<String>() } else { elements.iter().map(|e| format!("{e}\n\t")).collect::<String>() };
        match edits.iter_mut().find(|(offset, _)| *offset == at) {
            Some((_, joined)) => joined.push_str(&text),
            None => edits.push((at, text)),
        }
    }
    w.splice(DM, edits.into_iter().map(|(at, text)| (at..at, text)).collect())
}

fn find<'a>(comments: &'a [PdfComment], id: &str) -> Result<&'a PdfComment, EditError> {
    comments.iter().find(|c| c.id == id).ok_or_else(|| EditError::new("notFound", format!("PDF comment {id}")))
}

/// Mark Resolved / Mark Unresolved (`status`: `resolved` or `open`).
pub(crate) fn set_status(w: &mut Work<'_>, ids: &[String], status: &str) -> Result<(), EditError> {
    let value = match status { "resolved" => dm::RESOLVED, "open" => dm::OPEN, other => return Err(error(format!("unknown comment status {other} (open or resolved)"))) };
    let comments = read(w)?;
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(xml)?;
    let mut edits = Vec::new();
    for id in ids {
        find(&comments, id)?;
        let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("PDF comment {id}")))?;
        edits.push(super::attr_edit(&ix, &t, i, "CommentStatus", value));
    }
    w.splice(DM, edits)
}

/// Delete Comment / Delete All Comments: the comments with their replies and their destination elements.
pub(crate) fn remove(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    let comments = read(w)?;
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(xml)?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut cut = |i: usize| {
        let n = &ix.nodes[i];
        let mut start = n.start;
        while start > 0 && matches!(t.as_bytes()[start - 1], b'\t' | b' ') { start -= 1; }
        if start > 0 && t.as_bytes()[start - 1] == b'\n' { start -= 1; }
        edits.push((start..n.end, String::new()));
    };
    for id in ids {
        let comment = find(&comments, id)?;
        let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("PDF comment {id}")))?;
        cut(i);
        // a destination that no other comment points at goes with its comment
        if let Some(d) = &comment.destination
            && !comments.iter().any(|o| o.id != comment.id && !ids.contains(&o.id) && o.destination.as_ref().is_some_and(|x| x.id == d.id))
            && let Some(j) = ix.by_self(&t, &d.id) { cut(j); }
    }
    w.splice(DM, edits)
}

/// The characters of a story, in InDesign's index space.
fn story_chars(w: &mut Work<'_>, story: &str) -> Result<Vec<char>, EditError> {
    let (part, _, _) = w.find(story)?;
    Ok(crate::text::contents(w.text(&part)?, story)?.chars().collect())
}

/// Accept (apply the comment on the layout): only a strikethrough comment that is tied to its text. The text goes, one U+FEFF stays
/// (measured, rules/pdfcomments.ko.md); the comment is applied, resolved and an orphan.
pub(crate) fn accept(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let comments = read(w)?;
    let comment = find(&comments, id)?;
    if !dm::deletes_text(&comment.kind) { return Err(EditError::new("unsupported", "Accept is measured for strikethrough comments only")); }
    if comment.orphan || comment.applied == Some(true) { return Err(error("The comment is not tied to the layout (already accepted, or unmapped)")); }
    let d: &PdfDestination = comment.destination.as_ref().filter(|d| d.kind == dm::TEXT).ok_or_else(|| error("The comment has no text to apply to"))?;
    let story = d.target.clone();
    let chars = story_chars(w, &story)?;
    if d.start >= d.end || d.end > chars.len() { return Err(error("The comment's text is outside the story (the text was edited)")); }
    // a space on both sides: one goes with the word (the spacing adjusts as it does when a word is cut)
    let space = |at: Option<&char>| at == Some(&' ');
    let end = if space(d.start.checked_sub(1).and_then(|i| chars.get(i))) && space(chars.get(d.end)) { d.end + 1 } else { d.end };
    crate::text::delete(w, &story, d.start, end)?;
    crate::text::insert(w, &story, d.start, "\u{feff}")?;
    // the text moved: the other text destinations of the story follow; one that overlapped the removed text is no longer tied
    let delta = 1 - (end - d.start) as i64;
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(xml)?;
    let mut edits = Vec::new();
    for other in comments.iter().filter(|o| o.id != comment.id) {
        let Some(od) = other.destination.as_ref().filter(|x| x.kind == dm::TEXT && x.target == story) else { continue };
        let shift = |v: usize| (v as i64 + delta).max(0) as usize;
        let j = ix.by_self(&t, &od.id).ok_or_else(|| EditError::new("notFound", format!("destination {}", od.id)))?;
        if od.start >= end {
            edits.push(super::attr_edit(&ix, &t, j, "CommentTextDestinationStartIndex", &shift(od.start).to_string()));
            edits.push(super::attr_edit(&ix, &t, j, "CommentTextDestinationEndIndex", &shift(od.end).to_string()));
        } else if od.end > d.start {
            let i = ix.by_self(&t, &other.id).ok_or_else(|| EditError::new("notFound", format!("PDF comment {}", other.id)))?;
            edits.push(super::attr_edit(&ix, &t, i, "CommentIsOrphan", "true"));
        }
    }
    let t = {
        w.splice(DM, edits)?;
        w.text(DM)?.to_owned()
    };
    let ix = Index::scan(&t).map_err(xml)?;
    let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("PDF comment {id}")))?;
    let edits = vec![
        super::attr_edit(&ix, &t, i, "CommentStatus", dm::RESOLVED),
        super::attr_edit(&ix, &t, i, "CommentIsOrphan", "true"),
        super::attr_edit(&ix, &t, i, "CommentIsApplied", "true"),
    ];
    w.splice(DM, edits)
}
