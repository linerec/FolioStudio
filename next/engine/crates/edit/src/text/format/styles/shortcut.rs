//! A style's keyboard shortcut (`KeyboardShortcut` + `ExtendedKeyboardShortcut` of the style element) and the document's list of
//! assigned shortcuts. The encoding is `folio_idml::style_shortcut` (measured in InDesign, rules/stylekeys.ko.md).
use super::*;
use folio_idml::style_shortcut::{Shortcut, NONE};

/// Sets (`shortcut` like `ctrl+alt+num1`) or clears (empty) the shortcut of a style. Two styles may share one shortcut (InDesign only warns).
pub(crate) fn set(w: &mut Work<'_>, k: &str, style: &str, shortcut: &str) -> Result<(), EditError> {
    let tag = kind(k)?;
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id(tag, style)?;
    if root(&id) { return Err(EditError::new("invalid", "Invalid request on a root style.")); }
    let (short, ext) = if shortcut.trim().is_empty() { (NONE.0.to_owned(), NONE.1.to_owned()) } else {
        Shortcut::parse(shortcut).map_err(|e| EditError::new("invalid", e))?.to_idml()
    };
    let t = w.text(PART)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let i = ix.by_self(t, &id).filter(|&i| ix.local(t, i) == tag).ok_or_else(|| EditError::new("notFound", id.clone()))?;
    let n = &ix.nodes[i];
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut missing = String::new();
    for (name, value) in [("KeyboardShortcut", &short), ("ExtendedKeyboardShortcut", &ext)] {
        match ix.attrs(t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == name) {
            Some(a) => if unescape(&t[a.value.0..a.value.1]) != *value { edits.push((a.value.0..a.value.1, escape(value))); },
            None => missing += &format!(" {name}=\"{}\"", escape(value)),
        }
    }
    if !missing.is_empty() {
        let at = if n.empty { n.open_end - 2 } else { n.open_end - 1 };
        edits.push((at..at, missing));
    }
    if edits.is_empty() { return Ok(()); }
    w.splice(PART, edits)?;
    // the last style given a shortcut wins a key press among styles sharing it (measured: rules/stylekeys-c2.ko.md); a cleared shortcut has no turn
    if shortcut.trim().is_empty() {
        w.ed.shortcut_order.remove(&id);
    } else {
        let next = w.ed.shortcut_order.values().max().map_or(1, |m| m + 1);
        w.ed.shortcut_order.insert(id, next);
    }
    Ok(())
}

/// A style shortcut pressed with text frames selected (InDesign 2025, measured in m6, rules/stylekeys-c2.ko.md): the style goes over the whole story of
/// each selected text frame, once per story, overset text included -- as a click in the panel does with the whole story selected (overrides stay unless
/// `clear`). Items without a story (rectangles) are left alone; none with text is an error. Table cells are not part of the story's range.
pub(crate) fn apply_to_frames(w: &mut Work<'_>, k: &str, style: &str, ids: &[String], clear: &bool) -> Result<(), EditError> {
    let paragraph = k == "paragraph";
    kind(k)?;
    let stories = super::super::frame_text::stories(w, ids)?;
    if stories.is_empty() {
        return Err(EditError::new("notText", "none of the items holds text"));
    }
    for story in stories {
        let end = crate::text::contents_len(w, &story)?;
        super::super::apply_style(w, &story, 0, end, style, paragraph, *clear)?;
    }
    Ok(())
}

/// A style shortcut pressed with nothing selected: the document's default paragraph (or character) style changes -- what new text gets -- and no text
/// does (`TextDefault/@AppliedParagraphStyle` or `@AppliedCharacterStyle` of Preferences.xml, as InDesign's `textDefaults` do).
pub(crate) fn set_text_default(w: &mut Work<'_>, k: &str, style: &str) -> Result<(), EditError> {
    let tag = kind(k)?;
    let attr = if tag == "ParagraphStyle" { "AppliedParagraphStyle" } else { "AppliedCharacterStyle" };
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id(tag, style)?;
    const PREFS: &str = "Resources/Preferences.xml";
    let t = w.text(PREFS)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "TextDefault").ok_or_else(|| EditError::new("xml", "no TextDefault"))?;
    let edit = match ix.attrs(&t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == attr) {
        Some(a) if unescape(&t[a.value.0..a.value.1]) == id => return Ok(()),
        Some(a) => (a.value.0..a.value.1, escape(&id)),
        None => {
            let n = &ix.nodes[i];
            let at = if n.empty { n.open_end - 2 } else { n.open_end - 1 };
            (at..at, format!(" {attr}=\"{}\"", escape(&id)))
        }
    };
    w.splice(PREFS, vec![edit])
}

impl Editor {
    /// Every style that has a usable shortcut, the one that wins a key press first. InDesign: of styles sharing a shortcut the one given it last
    /// wins, paragraph and character styles alike; a reloaded document (IDML) has lost that order: then a paragraph style beats a character style
    /// and of two styles of one kind the later one of the document wins (measured, rules/stylekeys-c2.ko.md). Folio keeps the order of this session
    /// (`shortcut_order`, gone when the document is reloaded) and falls back to that rule -- the order of this list.
    pub fn style_shortcuts(&self) -> Result<Vec<Value>, EditError> {
        let cat = Catalog::read(&self.parts)?;
        let mut rows = Vec::new();
        for (kind_name, tag) in [("character", "CharacterStyle"), ("paragraph", "ParagraphStyle")] {
            let t = self.parts.get(PART).map(String::as_str).unwrap_or_default();
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            for i in 0..ix.nodes.len() {
                if ix.local(t, i) != tag { continue; }
                let Some(id) = ix.value(t, i, "Self") else { continue };
                let (Some(a), true) = (cat.styles.get(&id), !root(&id)) else { continue };
                let Some(s) = Shortcut::from_idml(val(a, "KeyboardShortcut"), val(a, "ExtendedKeyboardShortcut")) else { continue };
                rows.push(serde_json::json!({"kind": kind_name, "id": id, "name": shown_name(val(a, "Name")), "shortcut": s.spec(), "label": s.label()}));
            }
        }
        // rows are character styles then paragraph styles, each in document order: the last row wins unless the session gave an earlier turn
        rows.reverse();
        rows.sort_by_key(|r| std::cmp::Reverse(r["id"].as_str().and_then(|id| self.shortcut_order.get(id)).copied().unwrap_or(0)));
        Ok(rows)
    }
}

impl Editor {
    /// What a key combination means as a style shortcut: `valid`, the canonical `shortcut`, its `label` and the `owners` (styles that use it
    /// already, the one that wins a key press first). An unusable combination (a bare key) is `valid: false` with the reason.
    pub fn style_shortcut_info(&self, spec: &str) -> Result<Value, EditError> {
        match Shortcut::parse(spec) {
            Ok(s) => {
                let spec = s.spec();
                let owners: Vec<Value> = self.style_shortcuts()?.into_iter().filter(|r| r["shortcut"] == spec.as_str()).collect();
                Ok(serde_json::json!({"valid": true, "shortcut": spec, "label": s.label(), "owners": owners}))
            }
            Err(error) => Ok(serde_json::json!({"valid": false, "error": error, "owners": []})),
        }
    }
}
