use serde_json::{Value, json};

pub fn query(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let session = session.ok_or("no document open")?;
    let request = serde_json::from_value(req.clone()).map_err(|e| e.to_string())?;
    let page = folio_text::glyphs::query(&session.fonts, &session.doc.font_table, &request)?;
    Ok(json!({"type":"fontGlyphs", "page":page}))
}

/// Glyphs panel ▸ Alternates for Selection as a query of its own: `family`, `style` and the selected `text`; the page holds the
/// character's own glyph and the glyphs the face's substitution features give it, each with its feature tags.
pub fn alternates(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let mut req = req.clone();
    req["filter"] = json!("alternates");
    query(&req, session).map(|mut v| { v["type"] = json!("glyphAlternates"); v })
}

/// The glyph tables of `faces` (installed faces only: a missing face is left out, so the editor reports it as invalid).
pub fn tables(fonts: &folio_text::FontStore, font_table: &[folio_model::FontInfo], faces: &[(String, String)]) -> Vec<folio_edit::text::GlyphFont> {
    let mut out: Vec<folio_edit::text::GlyphFont> = Vec::new();
    for (family, style) in faces {
        if out.iter().any(|f| &f.family == family && &f.style == style) { continue; }
        if let Ok((count, rows)) = folio_text::glyphs::table(fonts, font_table, family, style) {
            out.push(folio_edit::text::GlyphFont::new(family.clone(), style.clone(), count, rows));
        }
    }
    out
}

/// Edit ▸ Find/Change ▸ Glyph: the matches of a font face + glyph id (the same query as `findGlyph`).
pub fn find(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let session = session.ok_or("no document open")?;
    let query: folio_edit::text::GlyphQuery = serde_json::from_value(req["query"].clone()).map_err(|e| e.to_string())?;
    let shared = session.edit.as_ref().ok_or("text editing requires an editable document")?;
    let mut state = shared.lock().map_err(|_| "edit state unavailable")?;
    state.editor.set_glyph_fonts(tables(&session.fonts, &session.doc.font_table, &[(query.family.clone(), query.style.clone())]));
    state.editor.find_glyph(&query).map(|matches| json!({"type": "glyphFind", "matches": matches})).map_err(|e| e.to_string())
}

/// "Load Selected Glyph in Find / Change": the face and glyph id of the character at a story index.
pub fn at(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let session = session.ok_or("no document open")?;
    let story = req["story"].as_str().ok_or("story required")?;
    let at = req["at"].as_u64().ok_or("at required")? as usize;
    let shared = session.edit.as_ref().ok_or("text editing requires an editable document")?;
    let mut state = shared.lock().map_err(|_| "edit state unavailable")?;
    let faces = state.editor.glyph_faces_in(story, at, at + 1).map_err(|e| e.to_string())?;
    state.editor.set_glyph_fonts(tables(&session.fonts, &session.doc.font_table, &faces));
    state.editor.glyph_at(story, at).map(|glyph| json!({"type": "glyphAt", "glyph": glyph})).map_err(|e| e.to_string())
}
