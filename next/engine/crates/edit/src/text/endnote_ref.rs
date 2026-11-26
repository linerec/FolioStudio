//! The reference of a new endnote in the text (Type ▸ Insert Endnote, notes3-insert.ko.md): one
//! character at the insertion point in a character range of its own; the selected text stays.
use super::*;

pub(crate) fn insert(w: &mut Work<'_>, id: &str, at: usize, reference: &str, range: &str) -> Result<(), EditError> {
    track_edit::require_untracked(w, id)?;
    let opts = folio_idml::endnotes::settings(w.text("designmap.xml")?).map_err(|e| EditError::new("xml", e))?;
    let (part, ix, owner, _) = locate(w, id)?;
    let t = w.text(&part)?;
    if ix.local(t, owner) != "Story" || ix.attr(t, owner, "IsEndnoteStory") == Some("true") {
        return Err(unsupported("endnotes are supported in main stories only"));
    }
    let superscript = opts.get("EndnoteMarkerPositioning").is_none_or(|p| p == "SuperscriptMarker");
    let marker_style = opts.get("EndnoteMarkerStyle").filter(|cs| !cs.ends_with("/[No character style]")).cloned();
    let raw = format!("<Endnote Self=\"{reference}\" EndnoteTextRange=\"{range}\" />");
    insert_with(w, id, at, &|s, p, c| {
        let mut style = s.characters[c].clone();
        if superscript { style = format::with_attr(&style, "Position", "Superscript"); }
        if let Some(cs) = &marker_style { style = format::with_attr(&style, "AppliedCharacterStyle", cs); }
        s.characters.push(style);
        Ok(vec![Atom { p, c: s.characters.len() - 1, ch: '\u{4}', content: false, raw: raw.clone() }])
    })
}
