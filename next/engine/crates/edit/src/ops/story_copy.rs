//! Story parts of duplicated text frames. No text-editing implementation is shared here.
//! Oracle: duplicate-text, duplicate-thread-first/last/group, duplicate-text-group.
//! A single threaded frame copies its composed range; the last frame includes overset.
//! A copied contiguous chain keeps its threads, detached from frames outside the copy.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

pub(super) fn copy_stories(
    w: &mut Work<'_>, source: &super::clipboard::Source, ix: &Index, src: &str, ids: &mut Vec<(String, String)>,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<(), EditError> {
    let frames: BTreeMap<String, usize> = (0..ix.nodes.len())
        // text on a path holds its story as a text frame does (ParentStory, Previous/NextTextFrame)
        .filter(|&i| matches!(ix.local(src, i), "TextFrame" | "TextPath"))
        .filter_map(|i| ix.attr(src, i, "Self").map(|s| (s.to_owned(), i))).collect();
    let mut done = BTreeSet::new();
    // XML order determines story creation order (independent frames in a group).
    let mut ordered: Vec<_> = frames.iter().collect();
    ordered.sort_by_key(|(_, i)| **i);
    for (id, &i) in ordered {
        if done.contains(id) { continue; }
        let story = ix.attr(src, i, "ParentStory").ok_or_else(|| EditError::new("xml", "frame without story"))?;
        // Find the beginning of this copied chain, even when stacking order differs.
        let mut first = id.clone();
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(first.clone()) { return Err(EditError::new("xml", "cyclic text thread")); }
            let p = ix.attr(src, frames[&first], "PreviousTextFrame").unwrap_or("n");
            if !frames.contains_key(p) { break; }
            first = p.to_owned();
        }
        let mut chain = vec![first.clone()];
        loop {
            let next = ix.attr(src, frames[chain.last().unwrap()], "NextTextFrame").unwrap_or("n");
            if !frames.contains_key(next) { break; }
            if chain.iter().any(|s| s == next) { return Err(EditError::new("xml", "cyclic text thread")); }
            chain.push(next.to_owned());
        }
        let first_i = frames[&first];
        let last_i = frames[chain.last().unwrap()];
        let previous = ix.attr(src, first_i, "PreviousTextFrame").unwrap_or("n");
        let next = ix.attr(src, last_i, "NextTextFrame").unwrap_or("n");
        let (part, sx, si) = source.find(story)?;
        let original = source.text(&part)?.to_owned();
        let partial = previous != "n" || next != "n";
        // only a part of a threaded story needs the frames' composed starts; a story copied whole does not
        // (Data Merge copies a spread once per record in one command, after the page layout changed)
        if partial && w.recompose.iter().any(|s| s == story) {
            return Err(EditError::new("invalid", "compose the changed story before duplicating its threaded frames"));
        }
        let mut copy = if partial {
            let len = simple_length(&sx, &original, si)?;
            let start = if previous == "n" { 0 } else { frame_start(source, &first, len)? };
            let end = if next == "n" { len } else { frame_start(source, next, len)? };
            if start > end || end > len { return Err(EditError::new("invalid", "text frame composition is out of date")); }
            clip(&sx, &original, si, start..end)?
        } else { original.clone() };
        let new_story = w.new_id();
        // Story-local IDs (tables, anchored objects etc.) must be independent too.
        let cx = Index::scan(&copy).map_err(|e| EditError::new("xml", e))?;
        let mut story_ids = vec![(story.to_owned(), new_story.clone())];
        for k in 0..cx.nodes.len() {
            if let Some(s) = cx.attr(&copy, k, "Self")
                && s != story && !ids.iter().any(|(old, _)| old == s)
            { story_ids.push((s.to_owned(), w.new_id())); }
        }
        // anchored and inline objects of the story: their own text frames get their own copied stories (oracle anchored-duplicate)
        let mut nested: Vec<(Range<usize>, String)> = Vec::new();
        if (0..cx.nodes.len()).any(|k| matches!(cx.local(&copy, k), "TextFrame" | "TextPath")) {
            let mut inner = story_ids.clone();
            copy_stories(w, source, &cx, &copy, &mut inner, &mut nested)?;
            story_ids.extend(inner.into_iter().filter(|(old, _)| !story_ids.iter().any(|(o, _)| o == old)).collect::<Vec<_>>());
        }
        let mut changes = nested.clone();
        for k in 0..cx.nodes.len() {
            for a in cx.attrs(&copy, k) {
                let v = &copy[a.value.0..a.value.1];
                if nested.iter().any(|(r, _)| r.start == a.value.0) { continue; }
                if let Some((_, new)) = story_ids.iter().chain(ids.iter()).find(|(old, _)| old == v) {
                    changes.push((a.value.0..a.value.1, new.clone()));
                }
            }
        }
        changes.sort_by_key(|c| std::cmp::Reverse(c.0.start));
        for (r, s) in changes { copy.replace_range(r, &s); }
        w.add_part(format!("Stories/Story_{new_story}.xml"), copy)?;
        register(w, &new_story)?;
        for member in &chain {
            let k = frames[member];
            edits.push(attr_edit(ix, src, k, "ParentStory", &new_story));
            done.insert(member.clone());
        }
        if previous != "n" { edits.push(attr_edit(ix, src, first_i, "PreviousTextFrame", "n")); }
        if next != "n" { edits.push(attr_edit(ix, src, last_i, "NextTextFrame", "n")); }
        // Keep only the local IDs: each disconnected chain gets a separate story.
        ids.extend(story_ids.into_iter().filter(|(old, _)| old != story));
        w.recompose(&new_story);
    }
    Ok(())
}

fn frame_start(source: &super::clipboard::Source, frame: &str, end: usize) -> Result<usize, EditError> {
    let mut id = frame.to_owned();
    let mut seen = BTreeSet::new();
    while id != "n" {
        if !seen.insert(id.clone()) { return Err(EditError::new("xml", "cyclic text thread")); }
        match source.frame_start(&id) {
            Some(Some(start)) => return Ok(start),
            Some(None) => {}
            None => return Err(EditError::new("unsupported", "threaded text frame copy needs current composition")),
        }
        let (p, ix, i) = source.find(&id)?;
        id = ix.attr(source.text(&p)?, i, "NextTextFrame").unwrap_or("n").to_owned();
    }
    Ok(end)
}

pub(super) fn register(w: &mut Work<'_>, story: &str) -> Result<(), EditError> {
    let t = w.text("designmap.xml")?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let root = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document"))?;
    // A new story goes before the list's last entry, part refs newest first (oracle:
    // duplicate-text/twice/group in basic, parent-duplicate in pages, where the list is not
    // sorted: "… u1c4 u20a u222 u98").
    let mut list: Vec<_> = ix.attr(&t, root, "StoryList").unwrap_or("").split_whitespace().map(str::to_owned).collect();
    if !list.iter().any(|s| s == story) {
        let at = list.len().saturating_sub(1);
        list.insert(at, story.to_owned());
    }
    let at = (0..ix.nodes.len()).find(|&i| &t[ix.nodes[i].name.clone()] == "idPkg:Story")
        .map(|i| ix.nodes[i].start).unwrap_or(ix.nodes[root].close_start);
    w.splice("designmap.xml", vec![
        attr_edit(&ix, &t, root, "StoryList", &list.join(" ")),
        (at..at, format!("<idPkg:Story src=\"Stories/Story_{story}.xml\" />\n\t")),
    ])
}

/// The inline elements that are one character of the story whatever they hold (folio_idml text.rs `unit`): a table (U+0016), a
/// footnote (U+0004), a text variable (U+0018) and an anchored or inline object (U+FFFC). A part of a threaded story keeps each one
/// whole when its character is in the part (and drops it when not); what is inside them is not counted in the story's characters.
const ATOMS: [&str; 10] = ["Table", "Footnote", "TextVariableInstance", "Rectangle", "Oval", "Polygon", "GraphicLine", "Group", "TextFrame", "TextPath"];

fn atom(ix: &Index, t: &str, i: usize) -> bool {
    ATOMS.contains(&ix.local(t, i)) && ix.nodes[i].parent.is_some_and(|p| ix.local(t, p) == "CharacterStyleRange")
}

fn simple_length(ix: &Index, t: &str, story: usize) -> Result<usize, EditError> {
    let mut len = 0;
    let mut inside = 0;
    for i in story + 1..ix.nodes.len() {
        let n = &ix.nodes[i];
        if n.start < inside { continue; }
        if atom(ix, t, i) {
            len += 1;
            inside = n.end;
            continue;
        }
        if n.parent.is_some_and(|p| ix.local(t, p) == "CharacterStyleRange")
            && !matches!(ix.local(t, i), "Content" | "Br" | "Properties")
        {
            return Err(EditError::new("unsupported", "a part of a threaded story with this inline structure cannot be copied or duplicated (not supported); nothing was changed"));
        }
        match ix.local(t, i) {
            "Content" => len += crate::xml::unescape(&t[n.open_end..n.close_start]).chars().count(),
            "Br" => len += 1,
            _ => {}
        }
    }
    Ok(len)
}

/// Keep raw formatting/property markup and cut only text and paragraph breaks. Containers
/// with no retained text disappear. The original part is never rewritten.
fn clip(ix: &Index, t: &str, story: usize, range: Range<usize>) -> Result<String, EditError> {
    // Empty threaded frames still copy the insertion point's character/paragraph style
    // (duplicate-thread-empty-first/last), without adding an empty Content element.
    let mut cursor = 0;
    let mut empty_style = None;
    let mut inside = 0;
    for k in story + 1..ix.nodes.len() {
        let n = &ix.nodes[k];
        if n.start < inside { continue; }
        let len = if atom(ix, t, k) {
            inside = n.end;
            1
        } else {
            match ix.local(t, k) {
                "Content" => crate::xml::unescape(&t[n.open_end..n.close_start]).chars().count(),
                "Br" => 1,
                _ => 0,
            }
        };
        if len > 0 {
            empty_style = ix.ancestors(k).find(|&a| ix.local(t, a) == "CharacterStyleRange");
            cursor += len;
            if cursor > range.start { break; }
        }
    }
    // An entirely empty story has no Content/Br to identify its insertion style.
    if empty_style.is_none() {
        empty_style = (story + 1..ix.nodes.len()).find(|&k| ix.local(t, k) == "CharacterStyleRange");
    }
    fn node(ix: &Index, t: &str, i: usize, range: &Range<usize>, pos: &mut usize, empty_style: Option<usize>) -> Option<String> {
        let n = &ix.nodes[i];
        if atom(ix, t, i) {
            let at = *pos; *pos += 1;
            return range.contains(&at).then(|| t[n.start..n.end].to_owned());
        }
        match ix.local(t, i) {
            "Content" => {
                let text = crate::xml::unescape(&t[n.open_end..n.close_start]);
                let len = text.chars().count(); let start = *pos; *pos += len;
                if start >= range.end || *pos <= range.start { return None; }
                let kept: String = text.chars().skip(range.start.saturating_sub(start)).take(range.end.min(*pos) - range.start.max(start)).collect();
                return Some(format!("{}{}{}", &t[n.start..n.open_end], crate::xml::escape(&kept), &t[n.close_start..n.end]));
            }
            "Br" => {
                let at = *pos; *pos += 1;
                return (range.contains(&at)).then(|| t[n.start..n.end].to_owned());
            }
            "ParagraphStyleRange" | "CharacterStyleRange" | "Story" => {}
            _ => return Some(t[n.start..n.end].to_owned()),
        }
        let start = *pos;
        let mut out = t[n.start..n.open_end].to_owned();
        let mut prev = n.open_end;
        for &k in ix.children(i) {
            let c = &ix.nodes[k];
            let keep = node(ix, t, k, range, pos, empty_style);
            if let Some(keep) = keep { out.push_str(&t[prev..c.start]); out.push_str(&keep); }
            prev = c.end;
        }
        out.push_str(&t[prev..n.end]);
        let empty_keep = range.is_empty() && empty_style.is_some_and(|c| c == i || ix.ancestors(c).any(|a| a == i));
        if ix.local(t, i) == "Story" || empty_keep || (start < range.end && *pos > range.start) { Some(out) } else { None }
    }
    let n = &ix.nodes[story];
    let mut pos = 0;
    let kept = node(ix, t, story, &range, &mut pos, empty_style).unwrap();
    Ok(format!("{}{}{}", &t[..n.start], kept, &t[n.end..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    const STORY: &str = r#"<idPkg:Story><Story Self="u20"><StoryPreference/><ParagraphStyleRange AppliedParagraphStyle="P"><CharacterStyleRange AppliedCharacterStyle="A"><Properties><AppliedFont>Arial</AppliedFont></Properties><Content>A&amp;한😀</Content><Br/></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle="B"><Content>XYZ</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"#;
    #[test]
    fn clip_uses_characters_and_keeps_styles_and_entities() {
        let ix = Index::scan(STORY).unwrap();
        let cut = clip(&ix, STORY, 1, 1..7).unwrap();
        assert!(cut.contains("<Content>&amp;한😀</Content><Br/>"));
        assert!(cut.contains(r#"AppliedCharacterStyle="B"><Content>XY</Content>"#));
        assert!(!cut.contains("XYZ"));
        assert_eq!(simple_length(&ix, STORY, 1).unwrap(), 8);
    }

    #[test]
    fn inline_elements_count_as_one_character_and_go_whole() {
        let story = r#"<idPkg:Story><Story Self="u30"><ParagraphStyleRange AppliedParagraphStyle="P"><CharacterStyleRange AppliedCharacterStyle="A"><Content>ab</Content><Table Self="t1"><Cell Self="t1c"><ParagraphStyleRange><CharacterStyleRange><Content>cell text</Content></CharacterStyleRange></ParagraphStyleRange></Cell></Table><Content>cd</Content><Br/><Rectangle Self="r1"><Properties/></Rectangle><Footnote Self="f1"><ParagraphStyleRange><CharacterStyleRange><Content>note</Content></CharacterStyleRange></ParagraphStyleRange></Footnote><TextVariableInstance Self="v1" ResultText="12"/><Content>ef</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"#;
        let ix = Index::scan(story).unwrap();
        // a b [table] c d \r [rect] [footnote] [variable] e f
        assert_eq!(simple_length(&ix, story, 1).unwrap(), 11);
        let first = clip(&ix, story, 1, 0..6).unwrap();
        assert!(first.contains("<Table Self=\"t1\">") && first.contains("cell text") && first.contains("<Br/>") && !first.contains("Rectangle") && !first.contains("Footnote"));
        let second = clip(&ix, story, 1, 6..11).unwrap();
        assert!(!second.contains("Table") && second.contains("<Rectangle Self=\"r1\">") && second.contains("note") && second.contains("ResultText=\"12\"") && second.contains("<Content>ef</Content>"));
        assert!(!second.contains("<Content>ab</Content>") && !second.contains("<Content>cd</Content>"));
    }

    #[test]
    fn empty_range_keeps_insertion_style() {
        let ix = Index::scan(STORY).unwrap();
        let cut = clip(&ix, STORY, 1, 8..8).unwrap();
        assert!(cut.contains(r#"AppliedCharacterStyle="B""#));
        assert!(!cut.contains(r#"AppliedCharacterStyle="A""#));
        assert!(!cut.contains("<Content>"));
        assert!(!cut.contains("<Br"));
    }

    #[test]
    fn entirely_empty_story_keeps_its_insertion_style() {
        let story = r#"<idPkg:Story><Story Self="u20"><ParagraphStyleRange AppliedParagraphStyle="P"><CharacterStyleRange AppliedCharacterStyle="A"><Properties><AppliedFont>Arial</AppliedFont></Properties></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"#;
        let ix = Index::scan(story).unwrap();
        assert_eq!(clip(&ix, story, 1, 0..0).unwrap(), story);
    }
}
