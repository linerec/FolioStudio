//! Break Link to Style (Paragraph Styles and Character Styles panel menus; oracle text-format tstyle-break-link-*).
//! The text keeps how it looks and no longer follows the style: the style applied becomes `[No paragraph style]` /
//! `[No character style]`, and what the style gave (its own settings over those of the styles it is based on) becomes the
//! text's own settings, except what the new base gives anyway (`PointSize` 12 of a Body style is not written). What the text
//! set itself stays. Only the selected text changes: all the paragraphs it touches, or the characters themselves.
use super::*;

pub(crate) fn run(w: &mut Work<'_>, kind: &String, story: &String, start: &usize, end: &usize) -> Result<(), EditError> {
    let paragraph = match kind.as_str() {
        "paragraph" => true,
        "character" => false,
        _ => return Err(EditError::new("invalid", "style kind must be paragraph or character")),
    };
    let (start, end) = (*start, *end);
    if !paragraph && start == end {
        return Err(unsupported("breaking the link of a character style at an insertion point is not measured"));
    }
    let catalog = Catalog::read(&w.ed.parts)?;
    let (part, ix, i, id) = locate(w, story)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, start, end)?;
    let range = if paragraph { paragraph_range(&s, start, end) } else { start..end };
    let affected: Vec<usize> = range.clone().filter(|&at| at < s.atoms.len()).collect();
    if affected.is_empty() {
        return Ok(());
    }
    break_atoms(&catalog, &mut s, paragraph, &affected)?;
    w.ed.text_state.remove(&id);
    write(w, &part, &id, &s)
}

/// The change itself, for the atoms `affected` of a story: the style applied becomes the `[No ... style]` and what it gave becomes the text's own.
fn break_atoms(catalog: &Catalog, s: &mut Story, paragraph: bool, affected: &[usize]) -> Result<(), EditError> {
    if affected.iter().any(|&at| style_attrs(&s.characters[s.atoms[at].c]).contains_key("AppliedConditions")) {
        return Err(unsupported("breaking the link of conditional text is not measured; original XML kept"));
    }
    let (key, none) = if paragraph {
        ("AppliedParagraphStyle", "ParagraphStyle/$ID/[No paragraph style]")
    } else {
        ("AppliedCharacterStyle", "CharacterStyle/$ID/[No character style]")
    };
    let change = |p: &Style, c: &Style| -> Result<(Style, Style), EditError> {
        let mut pa = style_attrs(p);
        let mut ca = style_attrs(c);
        let old = val(if paragraph { &pa } else { &ca }, key).to_owned();
        if old.is_empty() || old == none {
            return Ok((make_style(p, &pa), make_style(c, &ca)));
        }
        // what the style gave becomes the text's own, the text's own settings winning; the paragraph style's character settings
        // (font, size, leading) go to the characters, as InDesign writes them (tstyle-break-link-paragraph)
        let mut given = catalog.resolved(&old)?;
        given.retain(|k, _| !META.contains(&k.as_str()));
        // a character style applied to the text outranks the paragraph style: what it sets is not given again (tsg-delete-based: the Bold of
        // Format Bold over the Regular of the paragraph style)
        let over = if paragraph { catalog.resolved(val(&ca, "AppliedCharacterStyle"))? } else { Attrs::new() };
        for (k, v) in given {
            if paragraph && crate::ops::paragraph_attr(&k) { pa.entry(k).or_insert(v); } else if !over.contains_key(&k) { ca.entry(k).or_insert(v); }
        }
        if paragraph {
            pa.insert(key.into(), attr(none));
            let base = catalog.para(&pa)?;
            normalize(&mut pa, &base);
        } else {
            ca.insert(key.into(), attr(none));
        }
        let base = catalog.character(&pa, &ca)?;
        normalize(&mut ca, &base);
        Ok((make_style(p, &pa), make_style(c, &ca)))
    };
    let mut cache = BTreeMap::new();
    for &at in affected {
        let a = &s.atoms[at];
        let key = (a.p, a.c);
        let (p, c) = if let Some(pair) = cache.get(&key) {
            *pair
        } else {
            let (p, c) = change(&s.paragraphs[a.p], &s.characters[a.c])?;
            let pair = (s.paragraphs.len(), s.characters.len());
            s.paragraphs.push(p);
            s.characters.push(c);
            cache.insert(key, pair);
            pair
        };
        s.atoms[at].p = p;
        s.atoms[at].c = c;
    }
    Ok(())
}

/// Delete Style Group: every text in the document that has one of the styles `ids` applied breaks the link to it (the same change as
/// `run`, over all the stories; oracle text-format tsg-delete-*).
pub(crate) fn break_all(w: &mut Work<'_>, paragraph: bool, ids: &std::collections::BTreeSet<String>) -> Result<(), EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let key = if paragraph { "AppliedParagraphStyle" } else { "AppliedCharacterStyle" };
    let stories: Vec<(String, String)> = w.ed.parts.iter().filter(|(p, _)| p.starts_with("Stories/")).map(|(p, t)| (p.clone(), t.clone())).collect();
    for (part, text) in stories {
        // a story that never names one of the styles has nothing to change
        if !ids.iter().any(|id| text.contains(&escape(id))) {
            continue;
        }
        let ix = Index::scan(&text).map_err(|e| EditError::new("xml", e))?;
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&text, i) == "Story" && ix.attr(&text, i, "Self").is_some()) else { continue };
        let id = ix.value(&text, i, "Self").unwrap_or_default();
        let mut s = Story::read(&text, &ix, i)?;
        let affected: Vec<usize> = (0..s.atoms.len()).filter(|&at| {
            let a = &s.atoms[at];
            let style = if paragraph { &s.paragraphs[a.p] } else { &s.characters[a.c] };
            ids.contains(val(&style_attrs(style), key))
        }).collect();
        if affected.is_empty() {
            continue;
        }
        break_atoms(&catalog, &mut s, paragraph, &affected)?;
        w.ed.text_state.remove(&id);
        write(w, &part, &id, &s)?;
    }
    Ok(())
}
