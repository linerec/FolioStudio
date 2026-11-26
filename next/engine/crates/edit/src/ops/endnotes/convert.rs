//! Type ▸ Convert Footnotes and Endnotes, whole document (InDesign 20.5, notes3-convert.ko.md).
//!
//! Measured: every footnote of a direction goes, in reference order, to the other kind; the body
//! moves as plain text after the target kind's own number marker and separator (the local bold of
//! two measured bodies did not survive); the endnote bodies stay in reference order whatever the
//! kind they had; the endnote story stays, title only, when its last body leaves. Selection scope,
//! bodies of several paragraphs or with character styles, and tracked changes were not measured:
//! the command refuses them.
use super::story::{self, At, NO_STYLE};
use crate::xml::{escape, Index};
use crate::{EditError, Work};

struct Slot {
    part: String,
    footnote: bool,
    /// Offset of the element in its part (the parts are scanned again after every conversion).
    start: usize,
}

pub(crate) fn run(w: &mut Work<'_>, direction: &str, scope: &String) -> Result<(), EditError> {
    if scope != "document" {
        return Err(EditError::new("unsupported", "converting footnotes and endnotes is measured for the whole document only"));
    }
    let forward = match direction {
        "footnotesToEndnotes" => true,
        "endnotesToFootnotes" => false,
        other => return Err(EditError::new("invalid", format!("unknown direction {other}"))),
    };
    let mut converted = 0;
    while let Some(i) = slots(w)?.iter().position(|s| s.footnote == forward) {
        if forward { to_endnote(w, i)?; } else { to_footnote(w, i)?; }
        converted += 1;
    }
    if converted == 0 {
        return Err(EditError::new("invalid", if forward { "the document has no footnotes" } else { "the document has no endnotes" }));
    }
    Ok(())
}

/// Footnote and endnote references of the main stories in reading order.
fn slots(w: &Work<'_>) -> Result<Vec<Slot>, EditError> {
    let mut out = Vec::new();
    for part in story::story_parts(w)? {
        let t = w.text(&part)?;
        if !(t.contains("<Footnote") || t.contains("<Endnote ")) || t.contains("IsEndnoteStory=\"true\"") { continue; }
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        for i in 0..ix.nodes.len() {
            let footnote = match ix.local(t, i) { "Footnote" => true, "Endnote" => false, _ => continue };
            if ix.ancestors(i).any(|a| matches!(ix.local(t, a), "Table" | "Footnote" | "Cell")) {
                return Err(EditError::new("unsupported", "notes inside tables were not measured"));
            }
            if ix.ancestors(i).any(|a| ix.local(t, a) == "Change") {
                return Err(EditError::new("unsupported", "converting notes under tracked changes was not measured"));
            }
            out.push(Slot { part: part.clone(), footnote, start: ix.nodes[i].start });
        }
    }
    Ok(out)
}

/// The plain text of a note body: the Content of its ranges without number marker, separator and
/// the U+FEFF fences. Anything but one paragraph of unstyled runs is refused.
fn body_text(t: &str, ix: &Index, node: usize, separator: &str) -> Result<String, EditError> {
    let unsupported = |why: &str| EditError::new("unsupported", format!("converting this note was not measured: {why}"));
    let mut out = String::new();
    let mut paragraphs = 0;
    for i in node + 1..ix.nodes.len() {
        if ix.nodes[i].start >= ix.nodes[node].end { break; }
        match ix.local(t, i) {
            "ParagraphStyleRange" => paragraphs += 1,
            "CharacterStyleRange" => {
                for a in ix.attrs(t, i) {
                    let (name, value) = (&t[a.name.0..a.name.1], &t[a.value.0..a.value.1]);
                    let ok = (name == "AppliedCharacterStyle" && value == NO_STYLE) || name == "FontStyle";
                    if !ok { return Err(unsupported("character style or formatting")); }
                }
            }
            "Content" => {
                let raw = &t[ix.nodes[i].open_end..ix.nodes[i].close_start];
                if raw.contains("<![CDATA[") { return Err(unsupported("content")); }
                out.push_str(&raw.replace("<?ACE 4?>", "").replace('\u{feff}', ""));
            }
            "Properties" => {
                if !ix.children(i).is_empty() { return Err(unsupported("character properties")); }
            }
            "Footnote" | "Endnote" | "EndnoteRange" => {}
            _ => return Err(unsupported("note contents other than text")),
        }
    }
    if paragraphs > 1 || (paragraphs == 0 && ix.local(t, node) == "Footnote") { return Err(unsupported("a body of several paragraphs")); }
    // the separator after the marker is not part of the text (written as it is, or as an entity)
    for sep in [separator.to_owned(), story::text_escape(separator), escape(separator)] {
        if let Some(rest) = out.strip_prefix(&sep) { return Ok(rest.to_owned()); }
    }
    Ok(out)
}

fn to_endnote(w: &mut Work<'_>, slot: usize) -> Result<(), EditError> {
    let all = slots(w)?;
    let here = &all[slot];
    let opts = story::options(w)?;
    let separator = opts.get("EndnoteSeparatorText").cloned().unwrap_or_else(|| "\t".into());
    let footnote_separator = crate::text::footnotes::separator(w)?;
    let t = w.text(&here.part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let node = ix.nodes.iter().position(|n| n.start == here.start).ok_or_else(|| EditError::new("xml", "footnote moved"))?;
    let text = body_text(t, &ix, node, &footnote_separator)?;
    let reference = w.new_id();
    let range = w.new_id();
    let (span, part) = (ix.nodes[node].start..ix.nodes[node].end, here.part.clone());
    // the reference order decides where the body goes among the endnote bodies
    let next = all[slot + 1..].iter().find(|s| !s.footnote).map(|s| endnote_range(w, s)).transpose()?;
    let previous = all[..slot].iter().rev().find(|s| !s.footnote).map(|s| endnote_range(w, s)).transpose()?;
    let story_part = match story::endnote_part(w)? { Some(p) => p, None => super::insert::create_story(w, None)? };
    let body = story::range_xml(&range, &reference, &text, &separator);
    let at = match (&next, &previous) {
        (Some(id), _) => At::Before(id),
        (None, Some(id)) => At::After(id),
        (None, None) => At::First,
    };
    story::place_range(w, &story_part, &body, at)?;
    w.splice(&part, vec![(span, format!("<Endnote Self=\"{reference}\" EndnoteTextRange=\"{range}\" />"))])?;
    if let Some(owner) = story_of(w, &part) { w.recompose(&owner); }
    w.created.push(reference);
    w.created.push(range);
    Ok(())
}

fn to_footnote(w: &mut Work<'_>, slot: usize) -> Result<(), EditError> {
    let all = slots(w)?;
    let here = &all[slot];
    let opts = story::options(w)?;
    let separator = opts.get("EndnoteSeparatorText").cloned().unwrap_or_else(|| "\t".into());
    let t = w.text(&here.part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let node = ix.nodes.iter().position(|n| n.start == here.start).ok_or_else(|| EditError::new("xml", "endnote moved"))?;
    let range = ix.value(t, node, "EndnoteTextRange").ok_or_else(|| EditError::new("invalid", "endnote without body"))?;
    let reference = ix.value(t, node, "Self").unwrap_or_default();
    let span = ix.nodes[node].start..ix.nodes[node].end;
    let part = here.part.clone();
    let story_part = story::endnote_part(w)?.ok_or_else(|| EditError::new("invalid", "endnote story missing"))?;
    let st = w.text(&story_part)?;
    let six = Index::scan(st).map_err(|e| EditError::new("xml", e))?;
    let r = six.by_self(st, &range).filter(|&r| six.local(st, r) == "EndnoteRange" && six.attr(st, r, "SourceEndnote") == Some(reference.as_str()))
        .ok_or_else(|| EditError::new("invalid", "endnote reference and body do not match"))?;
    let text = body_text(st, &six, r, &separator)?;
    let footnote = crate::text::footnotes::footnote_xml(w, &text)?;
    story::remove_range(w, &story_part, &range)?;
    let at = span.start;
    w.splice(&part, vec![(span, footnote)])?;
    if let Some(owner) = story_of(w, &part) {
        // the new footnote's text address: its story and its place among the story's footnotes
        let t = w.text(&part)?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        let ordinal = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Footnote" && ix.nodes[i].start < at).count();
        w.recompose(&owner);
        w.created.push(folio_idml::footnotes::text_id(&owner, ordinal));
    }
    Ok(())
}

fn endnote_range(w: &Work<'_>, slot: &Slot) -> Result<String, EditError> {
    let t = w.text(&slot.part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let node = ix.nodes.iter().position(|n| n.start == slot.start).ok_or_else(|| EditError::new("xml", "endnote moved"))?;
    ix.value(t, node, "EndnoteTextRange").ok_or_else(|| EditError::new("invalid", "endnote without body"))
}

fn story_of(w: &Work<'_>, part: &str) -> Option<String> {
    let t = w.text(part).ok()?;
    let ix = Index::scan(t).ok()?;
    let story = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Story" && ix.attr(t, i, "Self").is_some())?;
    ix.value(t, story, "Self")
}
