//! The endnote story: where an endnote's body goes and how it leaves again.
//!
//! Rules measured in InDesign 20.5 (artifacts/notes3-20261023/measurement, next/oracle/rules/notes3-*.ko.md):
//! the bodies are `EndnoteRange`s in reference order, separated by a `Br`; a body goes in before
//! the range of the next reference (its own `Br` after it) or after the last range (a `Br` before
//! it); the title's own `Br` stays when the last body leaves.
use crate::xml::Index;
use crate::{EditError, Work};
use std::collections::BTreeMap;

pub(super) const NO_STYLE: &str = "CharacterStyle/$ID/[No character style]";

/// Where a new range goes among the story's ranges.
pub(super) enum At<'a> {
    /// In front of the range with this `Self` (the next reference's).
    Before(&'a str),
    /// After the range with this `Self` (the last one).
    After(&'a str),
    /// No range yet: after the title.
    First,
}

fn unsupported(why: &str) -> EditError {
    EditError::new("unsupported", format!("endnote story: {why}"))
}

/// Document order of the stories: designmap `StoryList` first, any other story part after.
pub(super) fn story_parts(w: &Work<'_>) -> Result<Vec<String>, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let root = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document"))?;
    let mut out: Vec<String> = Vec::new();
    for id in ix.attr(t, root, "StoryList").unwrap_or("").split_whitespace() {
        let part = format!("Stories/Story_{id}.xml");
        if w.ed.parts.contains_key(&part) && !out.contains(&part) { out.push(part); }
    }
    for part in w.parts_named("Stories/Story_") {
        if !out.contains(&part) { out.push(part); }
    }
    Ok(out)
}

/// The part of the document's endnote story, if it has one.
pub(super) fn endnote_part(w: &Work<'_>) -> Result<Option<String>, EditError> {
    let mut found = Vec::new();
    for part in story_parts(w)? {
        if w.text(&part)?.contains("IsEndnoteStory=\"true\"") { found.push(part); }
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err(unsupported("more than one endnote story")),
    }
}

/// The story part of `id` (a story, or one of its text frames).
pub(super) fn source_part(w: &mut Work<'_>, id: &str) -> Result<String, EditError> {
    let part = format!("Stories/Story_{id}.xml");
    if w.ed.parts.contains_key(&part) { return Ok(part); }
    let (p, ix, i) = w.find(id)?;
    let story = ix.value(w.text(&p)?, i, "ParentStory").ok_or_else(|| EditError::new("invalid", "not a story or a text frame"))?;
    Ok(format!("Stories/Story_{story}.xml"))
}

/// With the scope Story: the endnote story that holds the bodies of the references in `source` (none before its first endnote).
pub(super) fn endnote_part_for(w: &Work<'_>, source: &str) -> Result<Option<String>, EditError> {
    let ranges: Vec<String> = {
        let t = w.text(source)?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Endnote").filter_map(|i| ix.value(t, i, "EndnoteTextRange")).collect()
    };
    if ranges.is_empty() { return Ok(None); }
    for part in story_parts(w)? {
        let t = w.text(&part)?;
        if t.contains("IsEndnoteStory=\"true\"") && ranges.iter().any(|r| t.contains(&format!("Self=\"{r}\""))) { return Ok(Some(part)); }
    }
    Ok(None)
}

/// With the scope Story: the page in front of which the story's endnote page goes, the page after the one that holds the story's last
/// text frame (none: the story ends on the last page, or has no frame, so the new page is the last).
pub(super) fn page_after_story(w: &Work<'_>, source: &str) -> Result<Option<String>, EditError> {
    let story = source.trim_start_matches("Stories/Story_").trim_end_matches(".xml");
    let dm = w.text("designmap.xml")?.to_owned();
    let spreads: Vec<String> = dm.match_indices("<idPkg:Spread src=\"").filter_map(|(i, m)| dm[i + m.len()..].split('"').next().map(str::to_owned)).collect();
    // every page in document order, and the spread that holds the story's last frame (the one with no next frame)
    let mut pages: Vec<(String, usize)> = Vec::new();
    let mut last: Option<usize> = None;
    for (k, part) in spreads.iter().enumerate() {
        let Ok(t) = w.text(part) else { continue };
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        for i in 0..ix.nodes.len() {
            match ix.local(t, i) {
                "Page" => if let Some(id) = ix.attr(t, i, "Self") { pages.push((id.to_owned(), k)); },
                "TextFrame" if ix.attr(t, i, "ParentStory") == Some(story) && ix.attr(t, i, "NextTextFrame").is_none_or(|n| n == "n") => last = Some(k),
                _ => {}
            }
        }
    }
    let Some(k) = last else { return Ok(None) };
    Ok(pages.iter().find(|(_, s)| *s > k).map(|(id, _)| id.clone()))
}

pub(super) fn options(w: &Work<'_>) -> Result<BTreeMap<String, String>, EditError> {
    folio_idml::endnotes::settings(w.text("designmap.xml")?).map_err(|e| EditError::new("xml", e))
}

/// Element text as InDesign writes it: only `&`, `<` and `>` become entities (a tab stays a tab).
pub(crate) fn text_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// An endnote body (`text` already escaped): the number marker and the separator are the options'.
pub(super) fn range_xml(range: &str, reference: &str, text: &str, separator: &str) -> String {
    format!("<EndnoteRange Self=\"{range}\" SourceEndnote=\"{reference}\"><Content>\u{feff}<?ACE 4?>{}{text}\u{feff}</Content></EndnoteRange>", text_escape(separator))
}

/// One run of an endnote body with character formatting: its text, the id of its character style and the attributes of its range.
pub(crate) type RichRun = (String, String, Vec<(String, String)>);

/// An endnote body whose runs have character formatting (oracle docxtext3-q04): a `CharacterStyleRange` per run inside the `EndnoteRange`;
/// the number marker and the separator open the first range's content and the closing mark ends the last one's. A return in a run is a `Br`.
pub(super) fn rich_range_xml(range: &str, reference: &str, runs: &[RichRun], separator: &str) -> String {
    let lead = format!("\u{feff}<?ACE 4?>{}", text_escape(separator));
    let mut body = String::new();
    for (k, (text, style, attrs)) in runs.iter().enumerate() {
        let attrs: String = attrs.iter().map(|(key, value)| format!(" {key}=\"{}\"", crate::xml::escape(value))).collect();
        body.push_str(&format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\"{attrs}>", crate::xml::escape(style)));
        let parts: Vec<&str> = text.split('\r').collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                body.push_str("<Br />");
            }
            let before = if k == 0 && i == 0 { lead.as_str() } else { "" };
            let after = if k + 1 == runs.len() && i + 1 == parts.len() { "\u{feff}" } else { "" };
            if !part.is_empty() || !before.is_empty() || !after.is_empty() {
                body.push_str(&format!("<Content>{before}{}{after}</Content>", text_escape(part)));
            }
        }
        body.push_str("</CharacterStyleRange>");
    }
    format!("<EndnoteRange Self=\"{range}\" SourceEndnote=\"{reference}\">{body}</EndnoteRange>")
}

/// Puts `range` (an `EndnoteRange` element) in its place among the story's ranges.
pub(super) fn place_range(w: &mut Work<'_>, part: &str, range: &str, at: At<'_>) -> Result<(), EditError> {
    let t = w.text(part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let find = |id: &str| -> Result<usize, EditError> {
        let n = ix.by_self(t, id).ok_or_else(|| EditError::new("notFound", format!("endnote range {id}")))?;
        if ix.local(t, n) != "EndnoteRange" { return Err(EditError::new("invalid", format!("{id} is not an endnote range"))); }
        Ok(n)
    };
    let edit = match at {
        At::Before(id) => {
            let n = find(id)?;
            let parent = ix.nodes[n].parent.ok_or_else(|| unsupported("range without parent"))?;
            let at = match ix.local(t, parent) {
                "CharacterStyleRange" => ix.nodes[n].start,
                "ParagraphStyleRange" => {
                    let sibs = ix.children(parent);
                    let pos = sibs.iter().position(|&s| s == n).unwrap();
                    let prev = pos.checked_sub(1).map(|p| sibs[p]).filter(|&p| ix.local(t, p) == "CharacterStyleRange" && !ix.nodes[p].empty);
                    ix.nodes[prev.ok_or_else(|| unsupported("range not preceded by a character range"))?].close_start
                }
                _ => return Err(unsupported("range structure")),
            };
            (at..at, format!("{range}<Br />"))
        }
        At::After(id) => {
            let n = find(id)?;
            let parent = ix.nodes[n].parent.ok_or_else(|| unsupported("range without parent"))?;
            if ix.local(t, parent) != "CharacterStyleRange" { return Err(unsupported("last range outside a character range")); }
            let at = ix.nodes[n].end;
            (at..at, format!("<Br />{range}"))
        }
        At::First => {
            let story = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Story" && ix.attr(t, i, "Self").is_some()).ok_or_else(|| unsupported("no Story"))?;
            let paragraphs = ix.named(t, story, "ParagraphStyleRange");
            let last = *paragraphs.last().ok_or_else(|| unsupported("no paragraph"))?;
            let opts = options(w)?;
            let body_style = opts.get("EndnoteTextStyle").map_or("ParagraphStyle/$ID/NormalParagraphStyle", String::as_str);
            let runs = ix.named(t, last, "CharacterStyleRange");
            let tail = *runs.last().ok_or_else(|| unsupported("no character range"))?;
            let leaves: Vec<usize> = ix.children(tail).iter().copied().filter(|&c| ix.local(t, c) != "Properties").collect();
            if ix.nodes[tail].empty || leaves.last().map(|&c| ix.local(t, c)) != Some("Br") || ix.children(last).last() != Some(&tail) {
                return Err(unsupported("the title does not end the story"));
            }
            if ix.attr(t, last, "AppliedParagraphStyle") == Some(body_style) {
                let at = ix.nodes[tail].close_start;
                (at..at, range.to_owned())
            } else {
                let at = ix.nodes[last].close_start;
                (at..at, format!("<ParagraphStyleRange AppliedParagraphStyle=\"{}\"><CharacterStyleRange AppliedCharacterStyle=\"{NO_STYLE}\">{range}</CharacterStyleRange></ParagraphStyleRange>", crate::xml::escape(body_style)))
            }
        }
    };
    let owner = story_id(t, &ix);
    w.splice(part, vec![edit])?;
    if let Some(owner) = owner { w.recompose(&owner); }
    Ok(())
}

/// The bodies of the endnote story in a character range of their own with this style (measured: the last paragraph of a Word file ends with a
/// note reference, so the bodies take the character style of that paragraph's return; docxtext2-en3, docxtext3-n01 .. n05).
pub(super) fn wrap_bodies(w: &mut Work<'_>, part: &str, style: &str) -> Result<(), EditError> {
    let t = w.text(part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let Some(story) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Story" && ix.attr(t, i, "Self").is_some()) else { return Ok(()) };
    let Some(&last) = ix.named(t, story, "ParagraphStyleRange").last() else { return Ok(()) };
    let runs = ix.named(t, last, "CharacterStyleRange");
    // only the plain state: the title and the bodies in one range
    let [only] = runs.as_slice() else { return Ok(()) };
    let Some(first) = ix.children(*only).iter().copied().find(|&c| ix.local(t, c) == "EndnoteRange") else { return Ok(()) };
    let (from, to) = (ix.nodes[first].start, ix.nodes[*only].close_start);
    let rest = t[from..to].to_owned();
    let edit = (from..to, format!("</CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"{}\">{rest}", crate::xml::escape(style)));
    let owner = story_id(t, &ix);
    w.splice(part, vec![edit])?;
    if let Some(owner) = owner { w.recompose(&owner); }
    Ok(())
}

fn story_id(t: &str, ix: &Index) -> Option<String> {
    let story = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Story" && ix.attr(t, i, "Self").is_some())?;
    ix.value(t, story, "Self")
}

/// Takes the range out with the `Br` that separated it from its neighbour: the one after it, else
/// (when another range comes before) the one in front. A character range left with nothing goes too.
pub(super) fn remove_range(w: &mut Work<'_>, part: &str, id: &str) -> Result<(), EditError> {
    let t = w.text(part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let n = ix.by_self(t, id).ok_or_else(|| EditError::new("notFound", format!("endnote range {id}")))?;
    if ix.local(t, n) != "EndnoteRange" { return Err(EditError::new("invalid", format!("{id} is not an endnote range"))); }
    let leaf = |i: usize| matches!(ix.local(t, i), "Content" | "Br" | "EndnoteRange");
    // leaves are never inside an EndnoteRange for this purpose: skip the range's own descendants
    let outside = |i: usize| !ix.ancestors(i).any(|a| ix.local(t, a) == "EndnoteRange");
    let after = (n + 1..ix.nodes.len()).find(|&i| ix.nodes[i].start >= ix.nodes[n].end && leaf(i) && outside(i));
    let before: Vec<usize> = (0..n).filter(|&i| ix.nodes[i].end <= ix.nodes[n].start && leaf(i) && outside(i)).collect();
    let mut gone = vec![n];
    match after.filter(|&i| ix.local(t, i) == "Br") {
        Some(br) => gone.push(br),
        None => {
            if let [.., range, br] = before.as_slice() && ix.local(t, *br) == "Br" && ix.local(t, *range) == "EndnoteRange" {
                gone.push(*br);
            }
        }
    }
    let story = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Story" && ix.attr(t, i, "Self").is_some());
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut whole: Vec<usize> = Vec::new();
    for &g in &gone {
        let parent = ix.nodes[g].parent.ok_or_else(|| unsupported("range without parent"))?;
        let emptied = ix.local(t, parent) == "CharacterStyleRange" && Some(parent) != story
            && ix.children(parent).iter().all(|c| gone.contains(c));
        if emptied {
            if !whole.contains(&parent) { whole.push(parent); }
        } else {
            edits.push((ix.nodes[g].start..ix.nodes[g].end, String::new()));
        }
    }
    for p in whole { edits.push((ix.nodes[p].start..ix.nodes[p].end, String::new())); }
    let owner = story_id(t, &ix);
    w.splice(part, edits)?;
    if let Some(owner) = owner { w.recompose(&owner); }
    Ok(())
}
