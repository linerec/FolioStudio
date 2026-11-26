//! Find story ranges and query the formatting in a selection.

use super::*;

pub(super) fn paragraph_range(s: &Story, start: usize, end: usize) -> Range<usize> {
    let first = s.atoms[..start]
        .iter()
        .rposition(|a| a.ch == '\r')
        .map_or(0, |i| i + 1);
    let last = if end > start { end - 1 } else { start };
    let last = s
        .atoms
        .get(last..)
        .and_then(|a| a.iter().position(|a| a.ch == '\r'))
        .map_or(s.atoms.len(), |i| last + i + 1);
    first..last
}
pub(super) fn inherit(s: &Story, at: usize) -> (usize, usize) {
    if at == s.atoms.len() && s.trailing.is_some() {
        return s.empty;
    }
    (if at > 0 && s.atoms[at - 1].ch != '\r' {
        s.atoms.get(at - 1)
    } else {
        s.atoms.get(at).or_else(|| s.atoms.last())
    })
    .map(|a| (a.p, a.c))
    .unwrap_or(s.empty)
}
pub(super) fn validate(s: &Story, start: usize, end: usize) -> Result<(), EditError> {
    if start > end || end > s.atoms.len() {
        Err(EditError::new("invalid", "format range outside story"))
    } else {
        Ok(())
    }
}

pub(super) fn caret_character<'a>(
    w: &'a Work<'_>,
    story: &str,
    at: usize,
    inherited: &'a Style,
) -> &'a Style {
    w.ed.text_state
        .get(story)
        .filter(|state| state.at == at)
        .map(|state| &state.character)
        .unwrap_or(inherited)
}

pub(super) fn set_caret_character(w: &mut Work<'_>, story: String, at: usize, character: Style) {
    w.ed.text_state.insert(story, InsertionStyle { at, character });
}

pub(crate) fn info(
    w: &mut Work<'_>,
    id: &str,
    start: usize,
    end: usize,
) -> Result<Value, EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let (part, ix, i, id) = locate(w, id)?;
    let original = w.text(&part)?;
    let plain = crate::hyperlinks::query_xml(original)?;
    let s = if let Some(plain) = plain {
        let plain_ix = Index::scan(&plain).map_err(|e| EditError::new("xml", e))?;
        let story = plain_ix.by_self(&plain, &id).ok_or_else(|| EditError::new("xml", "story not found"))?;
        changes::visible(super::query::read(&plain, &plain_ix, story, true)?)?
    } else {
        changes::visible(super::query::read(original, &ix, i, true)?)?
    };
    validate(&s, start, end)?;
    let mut rows = Vec::new();
    let positions: Vec<_> = if start == end {
        vec![inherit(&s, start)]
    } else {
        s.atoms[start..end].iter().map(|a| (a.p, a.c)).collect()
    };
    for (p, c) in positions {
        let pa = style_attrs(&s.paragraphs[p]);
        let cs = if start == end {
            caret_character(w, &id, start, &s.characters[c])
        } else {
            &s.characters[c]
        };
        let ca = style_attrs(cs);
        let mut resolved = catalog.character(&pa, &ca)?;
        resolved.extend(ca.clone());
        let mut row = serde_json::Map::new();
        row.insert(
            "kerningManual".into(),
            Value::Bool(resolved.contains_key("KerningValue")),
        );
        resolved
            .entry("KerningValue".into())
            .or_insert_with(|| attr("0"));
        for p in folio_idml::character_lines::PROPERTIES {
            resolved.entry(p.xml.into()).or_insert_with(|| attr(p.default()));
        }
        for p in folio_idml::opentype::PROPERTIES {
            resolved.entry(p.xml.into()).or_insert_with(|| attr(p.default));
        }
        resolved.entry("Composer".into()).or_insert_with(|| attr(folio_idml::parcomp::DEFAULT));
        for p in folio_idml::listprops::PROPERTIES {
            resolved.entry(p.xml.into()).or_insert_with(|| attr(p.default));
        }
        for k in controls() {
            let xml = control_key(k);
            row.insert(k.into(), api_value(k, val(&resolved, &xml)));
        }
        for (api, xml) in paint::KEYS { row.insert(api.into(), api_value(api, val(&resolved, xml))); }
        let sets = row.get("otfStylisticSets").and_then(Value::as_u64).unwrap_or(0);
        for n in 1..=20 { row.insert(format!("otfStylisticSet{n}"), Value::Bool(sets & (1 << n) != 0)); }
        row.insert(
            "paragraphStyle".into(),
            Value::String(val(&pa, "AppliedParagraphStyle").into()),
        );
        row.insert(
            "characterStyle".into(),
            Value::String(val(&ca, "AppliedCharacterStyle").into()),
        );
        row.insert(
            "paragraphOverride".into(),
            Value::Bool(ranges::has_override(&pa, "AppliedParagraphStyle")),
        );
        row.insert(
            "characterOverride".into(),
            Value::Bool(ranges::has_override(&ca, "AppliedCharacterStyle")),
        );
        rows.push(row);
    }
    let mut mixed = rows.first().cloned().unwrap_or_default();
    for row in &rows[1..] {
        for (k, v) in &mut mixed {
            if row.get(k) != Some(v) {
                *v = Value::Null;
            }
        }
    }
    Ok(Value::Object(mixed))
}

impl crate::Editor {
    pub fn text_format(
        &mut self,
        story: &str,
        start: usize,
        end: usize,
    ) -> Result<Value, EditError> {
        info(&mut query_work(self), story, start, end)
    }
}

pub(super) fn query_work(editor: &mut crate::Editor) -> Work<'_> {
    Work {
        ed: editor,
        splices: Vec::new(),
        recompose: Vec::new(),
        created: Vec::new(),
    }
}
