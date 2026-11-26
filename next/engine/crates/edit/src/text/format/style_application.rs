//! Apply character and paragraph styles to story ranges.

use super::*;
use super::selection::{caret_character, set_caret_character};

pub(crate) fn apply_style(
    w: &mut Work<'_>,
    id: &str,
    start: usize,
    end: usize,
    name: &str,
    paragraph: bool,
    clear: bool,
) -> Result<(), EditError> {
    apply_style_in(w, id, start, end, name, paragraph, clear, false, None)
}

/// The paragraph style context menu's variants (Apply X, Clear Overrides / Clear Character Styles / Clear All, and the same "then Next
/// Style"): `clear` is `none`, `overrides` (the local character formatting goes, the character styles stay: Alt+click), `characterStyles`
/// (the character styles of the paragraphs go to `[No character style]`, the local formatting stays) or `all` (both); `next` applies the
/// style to the first paragraph and the next style of the one before to each later paragraph of the range (the chain measured for the object
/// style's apply-next, oracle os-next-*). Folio's reading of the menu names: InDesign cannot be asked (the actions need the panel row).
pub(crate) fn apply_style_variant(w: &mut Work<'_>, id: &str, start: &usize, end: &usize, name: &str, clear: &str, next: &bool) -> Result<(), EditError> {
    let (overrides, characters) = match clear {
        "none" | "" => (false, false),
        "overrides" => (true, false),
        "characterStyles" => (false, true),
        "all" => (true, true),
        other => return Err(EditError::new("invalid", format!("clear must be none, overrides, characterStyles or all, not {other}"))),
    };
    apply_style_in(w, id, *start, *end, name, true, overrides, characters, if *next { Some(true) } else { None })
}

/// A paragraph style over a whole story, as an object style gives it (styles-c1: ObjectStyle.applyNextParagraphStyle, measured in
/// InDesign 2025: oracle os-next-*). With `next` the first paragraph takes `name` and every later paragraph the next style of the
/// style the paragraph before it took (a style without a next style is its own next); overrides are cleared as for any paragraph
/// style application of an object style. An empty paragraph after the story's final return takes its place in the chain too.
pub(crate) fn apply_style_to_story(w: &mut Work<'_>, id: &str, name: &str, next: bool) -> Result<(), EditError> {
    let end = crate::text::contents_len(w, id)?;
    apply_style_in(w, id, 0, end, name, true, true, false, Some(next))
}

/// The style a paragraph takes after one in `style`: its next style (its own when it has none or the catalog lacks it).
fn next_style(catalog: &Catalog, style: &str) -> String {
    let next = catalog.styles.get(style).map_or("", |a| val(a, "NextStyle"));
    let next = if next.starts_with("$ID/") { format!("{}/{}", style.split('/').next().unwrap_or("ParagraphStyle"), next) } else { next.to_owned() };
    if !next.is_empty() && catalog.styles.contains_key(&next) { next } else { style.to_owned() }
}

fn apply_style_in(
    w: &mut Work<'_>,
    id: &str,
    start: usize,
    end: usize,
    name: &str,
    paragraph: bool,
    clear: bool,
    clear_character_styles: bool,
    story_next: Option<bool>,
) -> Result<(), EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let style = catalog.id(
        if paragraph {
            "ParagraphStyle"
        } else {
            "CharacterStyle"
        },
        name,
    )?;
    let new_style = catalog.resolved(&style)?;
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, start, end)?;
    let range = if paragraph {
        paragraph_range(&s, start, end)
    } else {
        start..end
    };
    // Conditions are membership metadata, not formatting overrides. Until their
    // interaction with style clearing is measured, keep the original XML intact.
    let affected = if clear && start == end {
        paragraph_range(&s, start, end)
    } else {
        range.clone()
    };
    let conditional = affected
        .filter_map(|at| s.atoms.get(at))
        .any(|a| style_attrs(&s.characters[a.c]).contains_key("AppliedConditions"));
    let (_, caret_c) = inherit(&s, start);
    if conditional
        || start == end && style_attrs(&s.characters[caret_c]).contains_key("AppliedConditions")
    {
        return Err(unsupported(
            "style application to conditional text is not measured; original XML kept",
        ));
    }
    let change_with = |style: &str, p: &Style, c: &Style| -> Result<(Style, Style), EditError> {
        let mut pa = style_attrs(p);
        let mut ca = style_attrs(c);
        if paragraph {
            // format-paragraph-{keep,clear}: paragraph overrides always clear. Alt
            // additionally clears character overrides, retaining applied character styles.
            pa = BTreeMap::from([("AppliedParagraphStyle".into(), attr(style))]);
            if clear {
                ca.retain(|k, _| k == "AppliedCharacterStyle");
            }
            // Clear Character Styles: the character styles of the paragraphs go
            if clear_character_styles {
                ca.insert("AppliedCharacterStyle".into(), attr("CharacterStyle/$ID/[No character style]"));
            }
        } else {
            // format-character-{italic,none,keep}: old style values become overrides;
            // values defined by the new style replace them. Alt clears all overrides.
            if clear {
                ca.clear();
            } else {
                let mut previous = catalog.resolved(val(&ca, "AppliedCharacterStyle"))?;
                previous.extend(ca);
                ca = previous;
                ca.retain(|k, _| !new_style.contains_key(k));
            }
            ca.insert("AppliedCharacterStyle".into(), attr(style));
        }
        let base = catalog.character(&pa, &ca)?;
        normalize(&mut ca, &base);
        Ok((make_style(p, &pa), make_style(c, &ca)))
    };
    let change = |p: &Style, c: &Style| change_with(&style, p, c);
    if !paragraph && start == end {
        return apply_character_style_to_caret(w, &part, id, &mut s, start, clear, change);
    }
    if s.atoms.is_empty() {
        let (p, c) = s.empty;
        let (np, nc) = change(&s.paragraphs[p], &s.characters[c])?;
        s.paragraphs[p] = np;
        s.characters[c] = nc;
    } else if paragraph
        && start == end
        && start == s.atoms.len()
        && s.atoms.last().is_some_and(|a| a.ch == '\r')
    {
        let (p, c) = inherit(&s, start);
        let (np, _) = change(&s.paragraphs[p], &s.characters[c])?;
        s.trailing = Some(s.paragraphs.len());
        s.empty.0 = s.paragraphs.len();
        s.paragraphs.push(np);
    }
    let mut cache = BTreeMap::new();
    // the style of each paragraph of the range: the one style, or (object style, next style) a chain from the first paragraph
    let chain = story_next == Some(true);
    let mut styles = vec![style.clone()];
    let mut ordinal = 0;
    let chosen = |ordinal: usize, styles: &mut Vec<String>| -> String {
        while chain && styles.len() <= ordinal {
            let next = next_style(&catalog, styles.last().expect("one style"));
            styles.push(next);
        }
        styles[if chain { ordinal } else { 0 }].clone()
    };
    for at in range {
        let a = &s.atoms[at];
        let paragraph_style = chosen(ordinal, &mut styles);
        let key = (a.p, a.c, if chain { ordinal } else { 0 });
        let (p, c) = if let Some(pair) = cache.get(&key) {
            *pair
        } else {
            let (p, c) = change_with(&paragraph_style, &s.paragraphs[a.p], &s.characters[a.c])?;
            let pair = (s.paragraphs.len(), s.characters.len());
            s.paragraphs.push(p);
            s.characters.push(c);
            cache.insert(key, pair);
            pair
        };
        if s.atoms[at].ch == '\r' { ordinal += 1; }
        s.atoms[at].p = p;
        s.atoms[at].c = c;
    }
    // the empty paragraph after the story's final return takes its style as well
    if story_next.is_some() && end == s.atoms.len() && s.atoms.last().is_some_and(|a| a.ch == '\r') {
        let paragraph_style = chosen(ordinal, &mut styles);
        let last = s.atoms.last().expect("a final return");
        let (np, _) = change_with(&paragraph_style, &s.paragraphs[last.p], &s.characters[last.c])?;
        let index = match s.trailing {
            Some(t) => { s.paragraphs[t] = np; t }
            None => { s.paragraphs.push(np); s.paragraphs.len() - 1 }
        };
        s.trailing = Some(index);
        s.empty.0 = index;
    }
    w.ed.text_state.remove(&id);
    write(w, &part, &id, &s)
}

fn apply_character_style_to_caret(
    w: &mut Work<'_>,
    part: &str,
    id: String,
    story: &mut Story,
    at: usize,
    clear: bool,
    change: impl FnOnce(&Style, &Style) -> Result<(Style, Style), EditError>,
) -> Result<(), EditError> {
    let (p, c) = inherit(story, at);
    let old = caret_character(w, &id, at, &story.characters[c]);
    let (_, c) = change(&story.paragraphs[p], old)?;
    if story.atoms.is_empty() {
        let index = story.empty.1;
        story.characters[index] = c.clone();
        write(w, part, &id, story)?;
    }
    // format-character-caret-clear: clearOverrides at an insertion point clears
    // character overrides throughout its paragraph, not just the pending format.
    if clear {
        let mut cache = BTreeMap::new();
        for at in paragraph_range(story, at, at) {
            let old = story.atoms[at].c;
            let new = *cache.entry(old).or_insert_with(|| {
                let mut attrs = style_attrs(&story.characters[old]);
                attrs.retain(|k, _| k == "AppliedCharacterStyle");
                let new = story.characters.len();
                story.characters.push(make_style(&story.characters[old], &attrs));
                new
            });
            story.atoms[at].c = new;
        }
        write(w, part, &id, story)?;
    }
    set_caret_character(w, id, at, c);
    Ok(())
}
