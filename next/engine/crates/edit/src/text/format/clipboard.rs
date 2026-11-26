//! Copy and paste story fragments with their local formatting.

use super::*;
use super::selection::{query_work, set_caret_character};

fn validate_clipboard_inheritance(
    from: &Attrs,
    into: &Attrs,
    direct: &Attrs,
) -> Result<(), EditError> {
    for key in from.keys().chain(into.keys()) {
        if key == "AppliedParagraphStyle"
            || direct.contains_key(key)
            || controls().any(|name| control_key(name) == *key)
            || from
                .get(key)
                .zip(into.get(key))
                .is_some_and(|(a, b)| equal(a, b))
        {
            continue;
        }
        // Only the measured character/paragraph properties may change inheritance.
        // For example an inherited fill, language, or nested style must not vanish
        // silently when the destination supplies a different paragraph style.
        return Err(unsupported(format!(
            "clipboard cannot yet preserve inherited {key} across these paragraph styles; original XML kept"
        )));
    }
    Ok(())
}

/// An internal plain-text clipboard fragment. This is range XML, never a story part.
/// Only the app's own clipboard token may use it; system clipboard input stays plain.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextFragment {
    pub(crate) xml: String,
    pub text: String,
}
pub(crate) fn copy(
    w: &mut Work<'_>,
    id: &str,
    start: usize,
    end: usize,
) -> Result<TextFragment, EditError> {
    let (part, ix, i, _) = locate(w, id)?;
    let t = w.text(&part)?;
    // a story with change records is copied as the text a reader sees (deleted text is not copied)
    let mut s = if t.contains("<Change ") { super::changes::visible(Story::read_mode(t, &ix, i, true)?)? } else { Story::read(t, &ix, i)? };
    validate(&s, start, end)?;
    s.atoms = s.atoms[start..end].to_vec();
    s.trailing = None;
    if s.atoms
        .iter()
        .any(|a| !a.content && a.ch != '\r' || a.raw.starts_with("<?"))
    {
        return Err(unsupported(
            "clipboard supports plain text only (no inline objects)",
        ));
    }
    Ok(TextFragment {
        text: s.atoms.iter().map(|a| a.ch).collect(),
        xml: s.serialize(),
    })
}
pub(crate) fn paste(
    w: &mut Work<'_>,
    id: &str,
    at: usize,
    fragment: &TextFragment,
) -> Result<(), EditError> {
    if fragment.text.is_empty() {
        return Ok(());
    }
    let t = format!("<Story Self=\"clipboard\">{}</Story>", fragment.xml);
    let ix = Index::scan(&t).map_err(|e| EditError::new("invalid", e))?;
    let source = Story::read(&t, &ix, 0)?;
    if source
        .atoms
        .iter()
        .any(|a| !a.content && a.ch != '\r' || a.raw.starts_with("<?"))
        || source.atoms.iter().map(|a| a.ch).collect::<String>() != fragment.text
    {
        return Err(unsupported("invalid plain-text clipboard fragment"));
    }
    let catalog = Catalog::read(&w.ed.parts)?;
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, at, at)?;
    let (dest, _) = inherit(&s, at);
    if at == s.atoms.len() {
        s.trailing = None;
    }
    let last_break = source.atoms.iter().rposition(|a| a.ch == '\r');
    let first_break = source.atoms.iter().position(|a| a.ch == '\r');
    let starts_paragraph = at == 0 || s.atoms[at - 1].ch == '\r';
    let mut added = Vec::new();
    let mut cache = BTreeMap::new();
    for (offset, a) in source.atoms.iter().enumerate() {
        // Native text transfer (format-copy-*): a complete first paragraph that joins
        // the destination adopts its paragraph style; the final partial paragraph
        // preserves source character appearance as direct overrides.
        let tail = last_break.is_none_or(|last| offset > last);
        let joins = !s.atoms.is_empty()
            && (tail || !starts_paragraph && first_break.is_some_and(|first| offset <= first));
        let key = (a.p, a.c, tail, joins);
        let (p, c) = if let Some(pair) = cache.get(&key) {
            *pair
        } else {
            let para = if joins {
                s.paragraphs[dest].clone()
            } else {
                source.paragraphs[a.p].clone()
            };
            let mut ca = style_attrs(&source.characters[a.c]);
            if tail {
                let from = catalog.character(&style_attrs(&source.paragraphs[a.p]), &ca)?;
                let into = catalog.character(&style_attrs(&para), &ca)?;
                validate_clipboard_inheritance(&from, &into, &ca)?;
                // The inherited character properties that differ between paragraphs
                // become overrides. Paragraph properties never enter a character range.
                for name in controls() {
                    if folio_idml::paragraph_options::property(name).is_some() { continue; }
                    if matches!(
                        name,
                        "justification"
                            | "leftIndent"
                            | "rightIndent"
                            | "firstLineIndent"
                            | "spaceBefore"
                            | "spaceAfter"
                    ) {
                        continue;
                    }
                    let k = control_key(name);
                    if !ca.contains_key(&k)
                        && let Some(v) = from.get(&k)
                        && !into.get(&k).is_some_and(|b| equal(v, b)) {
                        ca.insert(k, v.clone());
                    }
                }
            }
            let base = catalog.character(&style_attrs(&para), &ca)?;
            normalize(&mut ca, &base);
            let chr = make_style(&source.characters[a.c], &ca);
            let pair = (s.paragraphs.len(), s.characters.len());
            s.paragraphs.push(para);
            s.characters.push(chr);
            cache.insert(key, pair);
            pair
        };
        let mut a = a.clone();
        a.p = p;
        a.c = c;
        added.push(a);
    }
    let style = added.last().map(|a| s.characters[a.c].clone()).unwrap();
    s.atoms.splice(at..at, added);
    write(w, &part, &id, &s)?;
    set_caret_character(w, id, at + source.atoms.len(), style);
    Ok(())
}

impl crate::Editor {
    pub fn copy_text(
        &mut self,
        story: &str,
        start: usize,
        end: usize,
    ) -> Result<TextFragment, EditError> {
        copy(&mut query_work(self), story, start, end)
    }
}
