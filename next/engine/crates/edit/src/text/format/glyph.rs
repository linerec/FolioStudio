//! Edit ▸ Find/Change ▸ Glyph: find characters by font face and glyph id, change them to another glyph
//! (oracle glyphfind-*, rules/glyphfind.ko.md). The host supplies the cmap of the faces involved
//! (`Editor::set_glyph_fonts`); the editor has no font files.
use super::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One face's glyph table: the Unicode scalars it maps and the glyph each reaches.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphFont {
    pub family: String,
    pub style: String,
    pub count: usize,
    forward: BTreeMap<u32, u16>,
    reverse: BTreeMap<u16, u32>,
}
impl GlyphFont {
    /// `rows`: (glyph id, the Unicode scalars mapped to it).
    pub fn new(family: String, style: String, count: usize, rows: impl IntoIterator<Item = (u16, Vec<u32>)>) -> Self {
        let (mut forward, mut reverse) = (BTreeMap::new(), BTreeMap::new());
        for (id, scalars) in rows {
            for cp in &scalars {
                forward.insert(*cp, id);
            }
            // Change to a glyph makes the lowest Unicode scalar it is mapped to (measured:
            // glyphfind-story-probe, gid 3 of Arial becomes U+0020); controls are never typed.
            if let Some(cp) = scalars.iter().copied().filter(|cp| char::from_u32(*cp).is_some_and(|c| !c.is_control())).min() {
                reverse.insert(id, cp);
            }
        }
        GlyphFont { family, style, count, forward, reverse }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlyphQuery {
    pub family: String,
    pub style: String,
    pub glyph_id: usize,
    pub scope: String,
    pub story: Option<String>,
    pub start: usize,
    pub end: Option<usize>,
}

/// The glyph a character shows: the custom glyph it carries, else the face's cmap entry.
fn glyph_of(font: &GlyphFont, ch: char, custom: Option<usize>) -> Option<usize> {
    custom.or_else(|| font.forward.get(&(ch as u32)).map(|&g| usize::from(g)))
}
/// `$ID/g155` (a glyph without a Unicode value, stored as U+FFFD plus this property).
fn custom_id(value: &str) -> Option<usize> {
    value.strip_prefix("$ID/g")?.parse().ok()
}

fn font<'a>(w: &'a Work<'_>, family: &str, style: &str, what: &str) -> Result<&'a Arc<GlyphFont>, EditError> {
    if family.is_empty() || style.is_empty() {
        return Err(EditError::new("invalid", format!("Invalid {what} Font")));
    }
    w.ed.glyph_fonts.iter().find(|f| f.family == family && f.style == style)
        .ok_or_else(|| EditError::new("invalid", format!("Invalid {what} Font")))
}

pub(crate) fn search(w: &mut Work<'_>, q: &GlyphQuery) -> Result<Vec<FoundText>, EditError> {
    if !matches!(q.scope.as_str(), "" | "story" | "document" | "selection") {
        return Err(EditError::new("invalid", "invalid find scope"));
    }
    let face = font(w, &q.family, &q.style, "Find")?.clone();
    // measured: a glyph id outside the face is "Invalid Find Glyph"; id 0 (.notdef) is a valid search
    if q.glyph_id >= face.count {
        return Err(EditError::new("invalid", "Invalid Find Glyph"));
    }
    let cat = Catalog::read(&w.ed.parts)?;
    let ids: Vec<String> = if q.scope == "document" {
        let mut ids = Vec::new();
        for (part, t) in &w.ed.parts {
            if !part.starts_with("Stories/") {
                continue;
            }
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            ids.extend((0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Story").filter_map(|i| ix.value(t, i, "Self")));
        }
        ids
    } else {
        vec![q.story.clone().ok_or_else(|| EditError::new("invalid", "story required"))?]
    };
    let mut found = Vec::new();
    for id in ids {
        let (part, ix, i, id) = locate(w, &id)?;
        let s = Story::read(w.text(&part)?, &ix, i)?;
        let (start, end) = if q.scope == "selection" {
            (q.start, q.end.ok_or_else(|| EditError::new("invalid", "selection end required"))?)
        } else {
            (0, s.atoms.len())
        };
        validate(&s, start, end)?;
        // (shows the wanted face, custom glyph id) per style pair
        let mut cache: BTreeMap<(usize, usize), (bool, Option<usize>)> = BTreeMap::new();
        for at in start..end {
            let a = &s.atoms[at];
            let (same, custom) = match cache.get(&(a.p, a.c)) {
                Some(v) => *v,
                None => {
                    let pa = style_attrs(&s.paragraphs[a.p]);
                    let ca = style_attrs(&s.characters[a.c]);
                    let mut effective = cat.character(&pa, &ca)?;
                    effective.extend(ca.clone());
                    let v = (val(&effective, "AppliedFont") == q.family && val(&effective, "FontStyle") == q.style,
                        ca.get("CustomGlyph").and_then(|p| custom_id(&p.value)));
                    cache.insert((a.p, a.c), v);
                    v
                }
            };
            if same && glyph_of(&face, a.ch, custom) == Some(q.glyph_id) {
                found.push(FoundText { story: id.clone(), start: at, end: at + 1, text: a.ch.to_string() });
            }
        }
    }
    Ok(found)
}

/// Replaces the glyph at one story index: the character the glyph is mapped to (U+FFFD with a
/// `CustomGlyph` property when it has none), in the change face.
fn put_glyph(w: &mut Work<'_>, story: &str, at: usize, ch: char, custom: Option<usize>) -> Result<(), EditError> {
    let (part, ix, i, id) = locate(w, story)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, at, at + 1)?;
    let mut ca = style_attrs(&s.characters[s.atoms[at].c]);
    match custom {
        Some(g) => { ca.insert("CustomGlyph".into(), property("CustomGlyph", "string", &format!("$ID/g{g}"))); }
        None => { ca.remove("CustomGlyph"); }
    }
    let style = make_style(&s.characters[s.atoms[at].c], &ca);
    s.atoms[at].ch = ch;
    s.atoms[at].raw = raw_of(ch);
    s.atoms[at].c = s.characters.len();
    s.characters.push(style);
    write(w, &part, &id, &s)?;
    w.ed.text_state.remove(&id);
    Ok(())
}

/// Change Glyph: every glyph `find` reaches becomes `change` (measured: the face and the
/// glyph are both required; the matched characters keep the rest of their formatting).
pub(crate) fn replace(w: &mut Work<'_>, find: &GlyphQuery, change_family: &str, change_style: &str, change_glyph: usize) -> Result<(), EditError> {
    let to = font(w, change_family, change_style, "Replace")?.clone();
    // measured: glyph 0 and ids beyond the face are "Invalid Replace Glyph"
    if change_glyph == 0 || change_glyph >= to.count {
        return Err(EditError::new("invalid", "Invalid Replace Glyph"));
    }
    let (ch, custom) = match u16::try_from(change_glyph).ok().and_then(|g| to.reverse.get(&g)).and_then(|cp| char::from_u32(*cp)) {
        Some(c) => (c, None),
        None => ('\u{FFFD}', Some(change_glyph)),
    };
    let mut props = BTreeMap::from([("appliedFont".to_string(), Value::from(change_family)), ("fontStyle".to_string(), Value::from(change_style))]);
    if custom.is_none() {
        // measured (glyphfind-same-face …): a glyph that is a character of the face is pinned: InDesign turns
        // ligatures and contextual alternates off on it; a custom glyph (CustomGlyph) is not.
        props.insert("ligatures".into(), Value::Bool(false));
        props.insert("otfContextualAlternate".into(), Value::Bool(false));
    }
    for m in search(w, find)?.into_iter().rev() {
        put_glyph(w, &m.story, m.start, ch, custom)?;
        format(w, &m.story, m.start, m.end, &props)?;
    }
    Ok(())
}

/// Inserts one glyph of a face at a story index (the Glyphs panel's insert, by glyph id): the character the glyph is
/// mapped to in the face, or U+FFFD with a `CustomGlyph` property when it has none (Unicode-less glyphs, alternates). The
/// result is what Find/Change ▸ Glyph makes of a typed character (oracle glyphs-insert-*): the face is applied, a
/// glyph that is a character of the face is pinned (ligatures and contextual alternates off), a custom glyph is not.
pub(crate) fn insert_glyph(w: &mut Work<'_>, story: &str, at: usize, family: &str, style: &str, glyph: usize) -> Result<(), EditError> {
    let face = font(w, family, style, "Insert")?.clone();
    if glyph == 0 || glyph >= face.count {
        return Err(EditError::new("invalid", "Invalid Insert Glyph"));
    }
    let (ch, custom) = match u16::try_from(glyph).ok().and_then(|g| face.reverse.get(&g)).and_then(|cp| char::from_u32(*cp)) {
        Some(c) => (c, None),
        None => ('\u{FFFD}', Some(glyph)),
    };
    let mut props = BTreeMap::from([("appliedFont".to_string(), Value::from(family)), ("fontStyle".to_string(), Value::from(style))]);
    if custom.is_none() {
        props.insert("ligatures".into(), Value::Bool(false));
        props.insert("otfContextualAlternate".into(), Value::Bool(false));
    }
    crate::text::insert(w, story, at, &ch.to_string())?;
    format(w, story, at, at + 1, &props)?;
    if custom.is_some() {
        put_glyph(w, story, at, ch, custom)?;
    }
    Ok(())
}

/// What the character at a story index shows, for "Load Selected Glyph in Find / Change".
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphAt {
    pub family: String,
    pub style: String,
    /// `None` when the face is not installed or has no glyph for the character.
    pub glyph_id: Option<usize>,
    pub text: String,
    /// The document's default text face ([Basic Paragraph]): Load Selected Glyph in Find gives it to an empty Change side (measured).
    pub default_family: String,
    pub default_style: String,
}

pub(crate) fn glyph_at(w: &mut Work<'_>, story: &str, at: usize) -> Result<GlyphAt, EditError> {
    let cat = Catalog::read(&w.ed.parts)?;
    let (part, ix, i, _) = locate(w, story)?;
    let s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, at, at + 1)?;
    let a = &s.atoms[at];
    let pa = style_attrs(&s.paragraphs[a.p]);
    let ca = style_attrs(&s.characters[a.c]);
    let mut effective = cat.character(&pa, &ca)?;
    effective.extend(ca.clone());
    let (family, style) = (val(&effective, "AppliedFont").to_string(), val(&effective, "FontStyle").to_string());
    let glyph_id = w.ed.glyph_fonts.iter().find(|f| f.family == family && f.style == style)
        .and_then(|f| glyph_of(f, a.ch, ca.get("CustomGlyph").and_then(|p| custom_id(&p.value))));
    let base = cat.resolved("ParagraphStyle/$ID/NormalParagraphStyle").unwrap_or_default();
    let (default_family, default_style) = match (val(&base, "AppliedFont"), val(&base, "FontStyle")) {
        ("", _) | (_, "") => (family.clone(), style.clone()),
        (f, s) => (f.to_string(), s.to_string()),
    };
    Ok(GlyphAt { family, style, glyph_id, text: a.ch.to_string(), default_family, default_style })
}

impl crate::Editor {
    /// The faces whose glyph tables the next glyph command may use.
    pub fn set_glyph_fonts(&mut self, fonts: Vec<GlyphFont>) {
        self.glyph_fonts = fonts.into_iter().map(Arc::new).collect();
    }
    pub fn find_glyph(&mut self, q: &GlyphQuery) -> Result<Vec<FoundText>, EditError> {
        search(&mut Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() }, q)
    }
    pub fn glyph_at(&mut self, story: &str, at: usize) -> Result<GlyphAt, EditError> {
        glyph_at(&mut Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() }, story, at)
    }
    /// The faces the characters of `story` at `start..end` show (the host builds their glyph tables).
    pub fn glyph_faces_in(&mut self, story: &str, start: usize, end: usize) -> Result<Vec<(String, String)>, EditError> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        let cat = Catalog::read(&w.ed.parts)?;
        let (part, ix, i, _) = locate(&mut w, story)?;
        let s = Story::read(w.text(&part)?, &ix, i)?;
        validate(&s, start, end)?;
        let mut faces = std::collections::BTreeSet::new();
        for a in &s.atoms[start..end] {
            let pa = style_attrs(&s.paragraphs[a.p]);
            let ca = style_attrs(&s.characters[a.c]);
            let mut effective = cat.character(&pa, &ca)?;
            effective.extend(ca);
            faces.insert((val(&effective, "AppliedFont").to_string(), val(&effective, "FontStyle").to_string()));
        }
        Ok(faces.into_iter().collect())
    }
}
