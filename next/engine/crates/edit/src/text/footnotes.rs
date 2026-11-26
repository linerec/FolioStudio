//! Footnote editing uses the same atoms, ranges and undo path as ordinary text.
use super::*;
use crate::xml::escape;
use std::collections::BTreeMap;

pub(super) fn node(t: &str, ix: &Index, story: usize, index: usize) -> Result<usize, EditError> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Footnote"
        && ix.ancestors(i).any(|a| a == story)
        && !ix.ancestors(i).take_while(|&a| a != story).any(|a| matches!(ix.local(t, a), "Table" | "Footnote")))
        .nth(index).ok_or_else(|| EditError::new("notFound", format!("footnote {index}")))
}

fn settings(w: &Work<'_>) -> Result<BTreeMap<String, String>, EditError> {
    folio_idml::footnotes::settings(w.text("Resources/Preferences.xml")?)
        .map_err(|e| EditError::new("unsupported", e))
}

/// The separator that follows a footnote's number (document footnote options).
pub(crate) fn separator(w: &Work<'_>) -> Result<String, EditError> {
    Ok(settings(w)?.get("SeparatorText").cloned().unwrap_or_else(|| "	".into()))
}

/// A footnote element as Insert Footnote makes it, holding `text` (already XML-escaped) after the number and separator.
pub(crate) fn footnote_xml(w: &Work<'_>, text: &str) -> Result<String, EditError> {
    let opts = settings(w)?;
    let ps = opts.get("FootnoteTextStyle").map_or("ParagraphStyle/$ID/NormalParagraphStyle", String::as_str);
    let separator = opts.get("SeparatorText").map_or("	", String::as_str);
    Ok(format!("<Footnote><ParagraphStyleRange AppliedParagraphStyle=\"{}\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content><?ACE 4?>{}{text}</Content></CharacterStyleRange></ParagraphStyleRange></Footnote>", escape(ps), crate::ops::endnotes::text_escape(separator)))
}

/// The paragraph style and the separator a footnote has (the document's footnote options), for a footnote a Word file brought.
pub(crate) fn footnote_wrapper(w: &Work<'_>) -> Result<(String, String), EditError> {
    let opts = settings(w)?;
    let ps = opts.get("FootnoteTextStyle").map_or("ParagraphStyle/$ID/NormalParagraphStyle", String::as_str).to_owned();
    let separator = opts.get("SeparatorText").map_or("\t", String::as_str).to_owned();
    Ok((ps, separator))
}

/// What a footnote marker looks like in a story: its `Position` and its character style (the document's footnote options).
pub(crate) fn marker_look(w: &mut Work<'_>) -> Result<(Option<String>, Option<String>), EditError> {
    ensure_options(w)?;
    let opts = settings(w)?;
    let position = folio_idml::footnotes::marker_position(&opts).map_err(|e| EditError::new("unsupported", e))?;
    let style = opts.get("FootnoteMarkerStyle").filter(|cs| !cs.ends_with("/[No character style]")).cloned();
    Ok((position.map(|p| p.to_owned()), style))
}

pub(crate) fn insert(w: &mut Work<'_>, id: &String, at: &usize) -> Result<(), EditError> {
    ensure_options(w)?;
    let opts = settings(w)?;
    let (part, ix, owner, story) = locate(w, id)?;
    if ix.local(w.text(&part)?, owner) != "Story" {
        return Err(unsupported("footnotes are supported in main stories only"));
    }
    let ps = opts.get("FootnoteTextStyle").map_or("ParagraphStyle/$ID/NormalParagraphStyle", String::as_str);
    let marker_style = opts.get("FootnoteMarkerStyle").map(String::as_str);
    let pos = folio_idml::footnotes::marker_position(&opts).map_err(|e| EditError::new("unsupported", e))?;
    let separator = opts.get("SeparatorText").map_or("\t", String::as_str);
    let raw = format!("<Footnote><ParagraphStyleRange AppliedParagraphStyle=\"{}\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content><?ACE 4?>{}</Content></CharacterStyleRange></ParagraphStyleRange></Footnote>", escape(ps), escape(separator));
    let s = Story::read(w.text(&part)?, &ix, owner)?;
    let ordinal = s.atoms.iter().take(*at).filter(|a| a.raw.starts_with("<Footnote")).count();
    insert_with(w, id, *at, &|s, p, c| {
        let mut style = s.characters[c].clone();
        if let Some(pos) = pos { style = format::with_attr(&style, "Position", pos); }
        if let Some(cs) = marker_style.filter(|cs| !cs.ends_with("/[No character style]")) {
            style = format::with_attr(&style, "AppliedCharacterStyle", cs);
        }
        s.characters.push(style);
        Ok(vec![Atom { p, c: s.characters.len()-1, ch:'\u{4}', content:false, raw:raw.clone() }])
    })?;
    w.created.push(folio_idml::footnotes::text_id(&story, ordinal));
    Ok(())
}

fn address(w: &mut Work<'_>, story: &str, index: usize) -> Result<String, EditError> {
    let story = story_of(w, story)?;
    if folio_idml::footnotes::address(&story).is_some() { return Err(unsupported("nested footnotes")); }
    Ok(folio_idml::footnotes::text_id(&story, index))
}

pub(crate) fn insert_marker(w: &mut Work<'_>, story: &String, index: &usize, at: &usize) -> Result<(), EditError> {
    let id = address(w, story, *index)?;
    restore_marker(w, &id, *at)
}

pub(crate) fn restore_marker(w: &mut Work<'_>, id: &str, at: usize) -> Result<(), EditError> {
    track_edit::require_untracked(w, id)?;
    let (part, ix, owner, _) = locate(w, id)?;
    let story = Story::read(w.text(&part)?, &ix, owner)?;
    if ix.local(w.text(&part)?, owner) != "Footnote" || at != 0 || story.atoms.iter().any(|a| a.raw == "<?ACE 4?>") {
        return Err(EditError::new("invalid", "place the cursor at the beginning of a footnote without a number marker"));
    }
    insert_with(w, id, at, &|_, p, c| Ok(vec![Atom { p, c, ch:'\u{fffc}', content:true, raw:"<?ACE 4?>".into() }]))
}

pub(crate) fn insert_text(w: &mut Work<'_>, story: &String, index: &usize, at: &usize, text: &String) -> Result<(), EditError> {
    let id = address(w, story, *index)?;
    super::insert(w, &id, *at, text)
}

pub(crate) fn delete_text(w: &mut Work<'_>, story: &String, index: &usize, start: &usize, end: &usize) -> Result<(), EditError> {
    let id = address(w, story, *index)?;
    super::delete(w, &id, *start, *end)
}

pub(crate) fn set_options(w: &mut Work<'_>, props: &BTreeMap<String, serde_json::Value>) -> Result<(), EditError> {
    if props.is_empty() { return Ok(()); }
    ensure_options(w)?;
    let before = settings(w)?;
    let changes = props.iter().map(|(k, v)| {
        let value = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
        folio_idml::footnotes::option_value(k, &value).map_err(|e| EditError::new("invalid", e))
    }).collect::<Result<Vec<_>, _>>()?;
    const PART: &str = "Resources/Preferences.xml";
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let n = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "FootnoteOption").ok_or_else(|| unsupported("missing FootnoteOption"))?;
    let mut edits = Vec::new();
    for (name, value, property) in changes {
        if property {
            let child = ix.path(&t, n, &["Properties", name]).ok_or_else(|| unsupported(format!("missing footnote option {name}")))?;
            edits.push((ix.nodes[child].open_end..ix.nodes[child].close_start, escape(&value)));
        } else {
            let attr = ix.attrs(&t, n).into_iter().find(|a| &t[a.name.0..a.name.1] == name)
                .ok_or_else(|| unsupported(format!("missing footnote option {name}")))?;
            edits.push((attr.value.0..attr.value.1, escape(&value)));
        }
    }
    w.splice(PART, edits)?;
    let after = settings(w)?;
    update_existing(w, &before, &after)?;
    let stories: Vec<_> = w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
    for story in stories { w.recompose(&story); }
    Ok(())
}

/// A newly created Folio document may omit the default preferences element.
fn ensure_options(w: &mut Work<'_>) -> Result<(), EditError> {
    const PART: &str = "Resources/Preferences.xml";
    let t = w.text(PART)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    if (0..ix.nodes.len()).any(|i| ix.local(t, i) == "FootnoteOption") { return Ok(()); }
    let root = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Preferences").ok_or_else(|| unsupported("missing Preferences"))?;
    let at = ix.nodes[root].close_start;
    w.splice(PART, vec![(at..at, folio_idml::footnotes::DEFAULT_OPTIONS.into())])
}

fn update_existing(w: &mut Work<'_>, before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Result<(), EditError> {
    let changed = |key: &str| before.get(key) != after.get(key);
    if !["MarkerPositioning", "FootnoteTextStyle", "FootnoteMarkerStyle", "SeparatorText"].iter().any(|k| changed(k)) { return Ok(()); }
    let stories: Vec<_> = w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
    for id in stories {
        let (part, ix, i, _) = locate(w, &id)?;
        if !w.text(&part)?.contains("<Footnote>") { continue; }
        let mut story = Story::read(w.text(&part)?, &ix, i)?;
        let mut touched = false;
        for atom in &mut story.atoms {
            if !atom.raw.starts_with("<Footnote") { continue; }
            touched = true;
            let mut style = story.characters[atom.c].clone();
            if changed("MarkerPositioning") {
                style = format::without_attr(&style, "Position");
                if let Some(pos) = folio_idml::footnotes::marker_position(after).map_err(unsupported)? {
                    style = format::with_attr(&style, "Position", pos);
                }
            }
            if changed("FootnoteMarkerStyle") && let Some(cs) = after.get("FootnoteMarkerStyle") {
                style = format::with_attr(&style, "AppliedCharacterStyle", cs);
            }
            story.characters.push(style); atom.c = story.characters.len()-1;
            let ix = Index::scan(&atom.raw).map_err(|e| EditError::new("xml", e))?;
            let mut note = Story::read(&atom.raw, &ix, 0)?;
            if changed("FootnoteTextStyle") && let Some(ps) = after.get("FootnoteTextStyle") {
                for p in &mut note.paragraphs { *p = format::with_attr(p, "AppliedParagraphStyle", ps); }
            }
            if changed("SeparatorText") && note.atoms.first().is_some_and(|a| a.raw == "<?ACE 4?>") {
                let old = before.get("SeparatorText").map_or("\t", String::as_str);
                let n = old.chars().count();
                if note.atoms.iter().skip(1).take(n).map(|a| a.ch).collect::<String>() == old {
                    let first = &note.atoms[0];
                    let (p, c) = (first.p, first.c);
                    let text = after.get("SeparatorText").map_or("\t", String::as_str);
                    let atoms = text.chars().map(|ch| Atom { p, c, ch, content: true, raw: raw_of(ch) }).collect::<Vec<_>>();
                    note.atoms.splice(1..1+n, atoms);
                }
            }
            atom.raw.replace_range(note.range.clone(), &note.serialize_ranges(true));
        }
        if touched { write(w, &part, &id, &story)?; }
    }
    Ok(())
}
