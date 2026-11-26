//! Apply direct formatting to a story range or insertion point.

use super::*;
use super::selection::{caret_character, set_caret_character};

pub(crate) fn format(
    w: &mut Work<'_>,
    id: &str,
    start: usize,
    end: usize,
    props: &BTreeMap<String, Value>,
) -> Result<(), EditError> {
    format_story_range(w, id, Some((start, end)), props)
}

pub(super) fn format_story_range(
    w: &mut Work<'_>,
    id: &str,
    range: Option<(usize, usize)>,
    props: &BTreeMap<String, Value>,
) -> Result<(), EditError> {
    if let Some((start, end)) = range
        && start == end
        && (props.contains_key("kerningValue") || props.contains_key("kerningMethod"))
    {
        return format_caret_kerning(w, id, start, props);
    }
    let sets = opentype::changes(props)?;
    let changes: Vec<_> = props
        .iter()
        .filter(|(k, _)| opentype::set_index(k).is_none())
        .map(|(k, v)| direct(k, v))
        .collect::<Result<_, _>>()?;
    let catalog = Catalog::read(&w.ed.parts)?;
    catalog.list_ref(props)?;
    paint::check(&w.ed.parts, props)?;
    for key in ["bulletsCharacterStyle","numberingCharacterStyle"] {
        if let Some(value)=props.get(key) {
            catalog.id("CharacterStyle",value.as_str().ok_or_else(||EditError::new("invalid","character style reference required"))?)?;
        }
    }
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    let (start, end) = match range {
        Some(range) => range,
        None if s.atoms.is_empty() => return Ok(()),
        None => (0, s.atoms.len()),
    };
    validate(&s, start, end)?;
    let pr = paragraph_range(&s, start, end);
    let apply =
        |p: &Style, c: &Style, para: bool, chr: bool| -> Result<(Style, Style), EditError> {
            let mut pa = style_attrs(p);
            let mut ca = style_attrs(c);
            for (is_para, k, v) in &changes {
                if *is_para && para {
                    pa.insert(k.clone(), v.clone());
                }
                if !*is_para && chr {
                    ca.insert(k.clone(), v.clone());
                }
                if !*is_para && chr && k == "KerningMethod" {
                    ca.remove("KerningValue");
                }
            }
            let base = catalog.para(&pa)?;
            if para { columns::normalize(&mut pa, &base, props)?; }
            if para && props.keys().any(|k| folio_idml::paragraph_options::property(k).is_some()) {
                validate_paragraph_options(&pa, &base)?;
            }
            normalize(&mut pa, &base);
            let base = catalog.character(&pa, &ca)?;
            if chr { opentype::apply(&sets, &base, &mut ca); }
            normalize(&mut ca, &base);
            Ok((make_style(p, &pa), make_style(c, &ca)))
        };
    let mut cache = BTreeMap::new();
    for at in pr.clone() {
        let a = &s.atoms[at];
        let chr = at >= start && at < end;
        let key = (a.p, a.c, chr);
        let (p, c) = if let Some(pair) = cache.get(&key) {
            *pair
        } else {
            let (p, c) = apply(&s.paragraphs[a.p], &s.characters[a.c], true, chr)?;
            let pair = (s.paragraphs.len(), s.characters.len());
            s.paragraphs.push(p);
            s.characters.push(c);
            cache.insert(key, pair);
            pair
        };
        s.atoms[at].p = p;
        s.atoms[at].c = c;
    }
    if s.atoms.is_empty() {
        let (p, c) = s.empty;
        let (np, nc) = apply(&s.paragraphs[p], &s.characters[c], true, true)?;
        s.paragraphs[p] = np;
        s.characters[c] = nc;
    } else if start == end && start == s.atoms.len() && s.atoms.last().is_some_and(|a| a.ch == '\r')
    {
        let (p, c) = inherit(&s, start);
        let (np, _) = apply(&s.paragraphs[p], &s.characters[c], true, false)?;
        s.trailing = Some(s.paragraphs.len());
        s.empty.0 = s.paragraphs.len();
        s.paragraphs.push(np);
    }
    if start == end {
        let (p, c) = inherit(&s, start);
        let old = caret_character(w, &id, start, &s.characters[c]);
        let (_, c) = apply(&s.paragraphs[p], old, false, true)?;
        set_caret_character(w, id.clone(), start, c);
    } else {
        w.ed.text_state.remove(&id);
    }
    write(w, &part, &id, &s)?;
    super::fonts::register(w, props)?;
    super::languages::register(w, props)
}

fn format_caret_kerning(
    w: &mut Work<'_>,
    id: &str,
    start: usize,
    props: &BTreeMap<String, Value>,
) -> Result<(), EditError> {
    // format-kern-caret(-insert): a manual pair value belongs to the character
    // before the caret. Newly inserted text does not inherit a manual pair value.
    let pair: BTreeMap<String, Value> = props
        .iter()
        .filter(|(k, _)| matches!(k.as_str(), "kerningValue" | "kerningMethod"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (k, v) in &pair {
        direct(k, v)?;
    }
    if start > 0 {
        format(w, id, start - 1, start, &pair)?;
    }
    let mut rest = props.clone();
    rest.remove("kerningValue");
    rest.remove("kerningMethod");
    if rest.is_empty() {
        Ok(())
    } else {
        format(w, id, start, start, &rest)
    }
}
