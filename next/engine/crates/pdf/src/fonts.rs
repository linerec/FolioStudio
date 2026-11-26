//! Embedded font subsets: every face the layout used becomes a Type0 font with Identity-H
//! encoding. CFF outlines are embedded as CIDFontType0C (the subset's bare CFF table),
//! TrueType outlines as CIDFontType2 with an identity CIDToGIDMap; variable fonts are
//! instanced at the face's coordinates (CFF2 instances become TrueType). Codes are the
//! subset's glyph ids, so glyphs are placed exactly as `folio-text` laid them out. ToUnicode
//! comes from the font's own cmap, reversed, extended through GSUB ligatures and single
//! substitutions.

use folio_model::layout::{DocumentLayout, FontId};
use folio_text::FontStore;
use std::collections::{BTreeMap, HashMap};
use subsetter::GlyphRemapper;

pub struct FontEntry {
    pub font: FontId,
    pub remap: GlyphRemapper,
    /// Advance width per original glyph, 1/1000 em.
    pub widths: HashMap<u16, f32>,
    /// Glyphs (original ids) in subset order (index = new id).
    pub order: Vec<u16>,
    /// The face has data to embed (not the synthetic stand-in).
    pub embeddable: bool,
    pub why_not: Option<String>,
    pub drawn: bool,
    pub notdef_drawn: usize,
    /// Characters the layout showed with each glyph (lines whose glyphs and characters pair
    /// one to one): the ToUnicode text when the font's own tables disagree or are silent.
    pub observed: HashMap<u16, HashMap<char, u32>>,
    /// The subsetted font file, made before any content is written (see [`subset_file`]).
    pub file: Option<Vec<u8>>,
}

#[derive(Default)]
pub struct FontRegistry {
    pub entries: BTreeMap<FontId, FontEntry>,
    /// The space glyph of each font (written where the layout leaves word gaps, so text
    /// extracts with its spaces; it has no outline).
    space: HashMap<FontId, u16>,
}

impl FontRegistry {
    /// Builds the subsets' glyph numbering from every glyph the layout placed, so content
    /// streams can be written in any order and the output stays deterministic.
    pub fn prepare(layout: &DocumentLayout, store: &FontStore, extra: &[(FontId, u16)]) -> FontRegistry {
        let mut used: BTreeMap<FontId, std::collections::BTreeSet<u16>> = BTreeMap::new();
        let frames = layout.frames.values().chain(layout.page_frames.iter().map(|p| &p.frame));
        for fr in frames {
            for line in &fr.lines {
                for g in &line.glyphs {
                    used.entry(g.font).or_default().insert(g.glyph);
                }
            }
        }
        for (f, g) in extra {
            used.entry(*f).or_default().insert(*g);
        }
        let faces = store.snapshot();
        let mut space = HashMap::new();
        for (font, glyphs) in used.iter_mut() {
            if let Some(sp) = faces.get(*font).and_then(|f| f.ttf()).and_then(|t| t.glyph_index(' ')) {
                glyphs.insert(sp.0);
                space.insert(*font, sp.0);
            }
        }
        let mut entries = BTreeMap::new();
        for (font, glyphs) in used {
            let mut order: Vec<u16> = vec![0];
            order.extend(glyphs.iter().copied().filter(|g| *g != 0));
            let remap = GlyphRemapper::new_from_glyphs(&order);
            let face = faces.get(font);
            let (mut embeddable, mut why_not) = (true, None);
            let mut widths = HashMap::new();
            match face.and_then(|f| f.ttf()) {
                Some(ttf) => {
                    let upem = f32::from(ttf.units_per_em().max(1));
                    for g in &order {
                        let adv = ttf.glyph_hor_advance(ttf_parser::GlyphId(*g)).unwrap_or(0);
                        widths.insert(*g, f32::from(adv) * 1000.0 / upem);
                    }
                    if let Some(perm) = ttf.permissions()
                        && perm == ttf_parser::Permissions::Restricted
                    {
                        embeddable = false;
                        why_not = Some("the font's licence forbids embedding (OS/2 fsType restricted)".into());
                    }
                }
                None => {
                    embeddable = false;
                    why_not = Some("no font file (synthetic stand-in)".into());
                }
            }
            entries.insert(font, FontEntry { font, remap, widths, order, embeddable, why_not, drawn: false, notdef_drawn: 0, observed: HashMap::new(), file: None });
        }
        FontRegistry { entries, space }
    }

    /// Code and width of the font's space glyph.
    pub fn space(&self, font: FontId) -> Option<(u16, f32)> {
        self.glyph(font, *self.space.get(&font)?)
    }

    /// Code (new glyph id) and width of a glyph, when the font can be embedded.
    pub fn glyph(&self, font: FontId, glyph: u16) -> Option<(u16, f32)> {
        let e = self.entries.get(&font)?;
        if !e.embeddable {
            return None;
        }
        Some((e.remap.get(glyph)?, e.widths.get(&glyph).copied().unwrap_or(0.0)))
    }

    /// Subsets every embeddable font before content is written: a font that cannot be
    /// subset is not embeddable, so its glyphs are reported as not drawn instead of being
    /// shown with a font the file never defines. `files` holds one result per font, in
    /// [`FontRegistry::embeddable`] order.
    pub fn take_files(&mut self, files: Vec<Result<Vec<u8>, String>>) {
        let ids: Vec<FontId> = self.embeddable();
        for (id, f) in ids.into_iter().zip(files) {
            let e = self.entries.get_mut(&id).expect("font entry");
            match f {
                Ok(bytes) => e.file = Some(bytes),
                Err(why) => {
                    e.embeddable = false;
                    e.why_not = Some(format!("the font could not be embedded ({why})"));
                }
            }
        }
    }

    /// Fonts with data to embed, in id order.
    pub fn embeddable(&self) -> Vec<FontId> {
        self.entries.values().filter(|e| e.embeddable).map(|e| e.font).collect()
    }

    pub fn observe(&mut self, font: FontId, glyph: u16, ch: char) {
        if let Some(e) = self.entries.get_mut(&font) {
            *e.observed.entry(glyph).or_default().entry(ch).or_default() += 1;
        }
    }

    pub fn mark_drawn(&mut self, font: FontId, notdef: usize) {
        if let Some(e) = self.entries.get_mut(&font) {
            e.drawn = true;
            e.notdef_drawn += notdef;
        }
    }
}

/// A subset ready to write.
pub struct Subset {
    pub postscript: String,
    pub file: String,
    /// The bytes for FontFile2 (TrueType) or FontFile3 (bare CFF).
    pub data: Vec<u8>,
    pub cff: bool,
    pub widths: Vec<f32>,
    /// Unicode text per new glyph id (may be empty).
    pub unicode: Vec<String>,
    pub bbox: [f32; 4],
    pub ascent: f32,
    pub descent: f32,
    pub cap_height: f32,
    pub italic_angle: f32,
    pub flags: u32,
    pub stem_v: f32,
    pub tag: String,
}

pub(crate) fn table<'a>(otf: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let n = u16::from_be_bytes([*otf.get(4)?, *otf.get(5)?]) as usize;
    for i in 0..n {
        let rec = otf.get(12 + 16 * i..12 + 16 * i + 16)?;
        if &rec[0..4] == tag {
            let off = u32::from_be_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
            let len = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]) as usize;
            return otf.get(off..off.checked_add(len)?);
        }
    }
    None
}

fn sanitize_ps(s: &str) -> String {
    let out: String = s.chars().filter(|c| c.is_ascii_graphic() && !"[](){}<>/%#".contains(*c)).collect();
    if out.is_empty() { "Font".into() } else { out.chars().take(100).collect() }
}

/// Reverse cmap plus GSUB: text for each glyph in `glyphs`, and the glyphs the cmap itself
/// maps (the rest were reached through substitutions).
fn unicode_of(face: &ttf_parser::Face<'_>, glyphs: &[u16]) -> (HashMap<u16, String>, std::collections::HashSet<u16>) {
    let wanted: std::collections::HashSet<u16> = glyphs.iter().copied().collect();
    let mut map: HashMap<u16, String> = HashMap::new();
    if let Some(cmap) = face.tables().cmap {
        for st in cmap.subtables {
            if !st.is_unicode() {
                continue;
            }
            st.codepoints(|cp| {
                // control codes some fonts map to their space glyph are not text
                if cp < 0x20 || (0x7F..0xA0).contains(&cp) {
                    return;
                }
                if let Some(g) = st.glyph_index(cp)
                    && wanted.contains(&g.0)
                    && let Some(ch) = char::from_u32(cp)
                {
                    // Prefer the lowest code point (U+0020 over U+00A0, ASCII over fullwidth).
                    match map.get(&g.0) {
                        Some(prev) if prev.chars().next().is_some_and(|p| (p as u32) <= cp) => {}
                        _ => {
                            map.insert(g.0, ch.to_string());
                        }
                    }
                }
            });
        }
    }
    // Presentation-form ligatures extract as their letters (as InDesign writes them).
    for t in map.values_mut() {
        let lig = match t.as_str() {
            "\u{FB00}" => Some("ff"),
            "\u{FB01}" => Some("fi"),
            "\u{FB02}" => Some("fl"),
            "\u{FB03}" => Some("ffi"),
            "\u{FB04}" => Some("ffl"),
            "\u{FB05}" | "\u{FB06}" => Some("st"),
            _ => None,
        };
        if let Some(l) = lig {
            *t = l.to_owned();
        }
    }
    let direct: std::collections::HashSet<u16> = map.keys().copied().collect();
    // Glyphs reached only through substitutions (ligatures, small caps, alternates).
    if let Some(gsub) = face.tables().gsub {
        use ttf_parser::gsub::SubstitutionSubtable as S;
        let mut from_sub: HashMap<u16, String> = HashMap::new();
        for _round in 0..3 {
            let mut added = false;
            for li in 0..gsub.lookups.len() {
                let Some(lookup) = gsub.lookups.get(li) else { continue };
                for st in lookup.subtables.into_iter::<S>() {
                    match st {
                        S::Single(single) => {
                            type Substitute<'a> = Box<dyn Fn(u16, u16) -> Option<u16> + 'a>;
                            let (cov, sub): (ttf_parser::opentype_layout::Coverage, Substitute<'_>) = match single {
                                ttf_parser::gsub::SingleSubstitution::Format1 { coverage, delta } => {
                                    (coverage, Box::new(move |g: u16, _i: u16| Some((g as i32 + delta as i32) as u16)))
                                }
                                ttf_parser::gsub::SingleSubstitution::Format2 { coverage, substitutes } => {
                                    (coverage, Box::new(move |_g: u16, i: u16| substitutes.get(i).map(|s| s.0)))
                                }
                            };
                            for (g, i) in coverage_glyphs(&cov) {
                                if let Some(out) = sub(g, i)
                                    && wanted.contains(&out)
                                    && let Some(t) = map.get(&g).cloned()
                                {
                                    // several sources: the lowest code point (".", not "Ų")
                                    let better = match (from_sub.get(&out), map.get(&out)) {
                                        (_, None) => true,
                                        (Some(prev), Some(_)) => t.chars().next() < prev.chars().next(),
                                        (None, Some(_)) => false,
                                    };
                                    if better {
                                        map.insert(out, t.clone());
                                        from_sub.insert(out, t);
                                        added = true;
                                    }
                                }
                            }
                        }
                        S::Alternate(alt) => {
                            for (g, i) in coverage_glyphs(&alt.coverage) {
                                let Some(set) = alt.alternate_sets.get(i) else { continue };
                                let Some(t) = map.get(&g).cloned() else { continue };
                                for a in set.alternates {
                                    if wanted.contains(&a.0) && !map.contains_key(&a.0) {
                                        map.insert(a.0, t.clone());
                                        added = true;
                                    }
                                }
                            }
                        }
                        S::Ligature(lig) => {
                            for (g, i) in coverage_glyphs(&lig.coverage) {
                                let Some(set) = lig.ligature_sets.get(i) else { continue };
                                for k in 0..set.len() {
                                    let Some(l) = set.get(k) else { continue };
                                    if !wanted.contains(&l.glyph.0) || map.contains_key(&l.glyph.0) {
                                        continue;
                                    }
                                    let mut text = match map.get(&g) {
                                        Some(t) => t.clone(),
                                        None => continue,
                                    };
                                    let mut ok = true;
                                    for c in l.components {
                                        match map.get(&c.0) {
                                            Some(t) => text.push_str(t),
                                            None => {
                                                ok = false;
                                                break;
                                            }
                                        }
                                    }
                                    if ok {
                                        map.insert(l.glyph.0, text);
                                        added = true;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !added {
                break;
            }
        }
    }
    (map, direct)
}

fn coverage_glyphs(c: &ttf_parser::opentype_layout::Coverage<'_>) -> Vec<(u16, u16)> {
    use ttf_parser::opentype_layout::Coverage;
    let mut out = Vec::new();
    match c {
        Coverage::Format1 { glyphs } => {
            for (i, g) in glyphs.into_iter().enumerate() {
                out.push((g.0, i as u16));
            }
        }
        Coverage::Format2 { records } => {
            for r in records.into_iter() {
                for g in r.start.0..=r.end.0 {
                    out.push((g, r.value.saturating_add(g - r.start.0)));
                }
            }
        }
    }
    out
}

/// The subsetted font file of one font (run on a worker thread).
pub fn subset_file(entry: &FontEntry, store: &FontStore) -> Result<Vec<u8>, String> {
    let faces = store.snapshot();
    let face = faces.get(entry.font).ok_or("font not loaded")?;
    let data = face.bytes().ok_or("no font data")?;
    let info = store.face_info(entry.font).ok_or("no face info")?;
    let ttf = face.ttf().ok_or("font does not parse")?;
    let coords: Vec<(subsetter::Tag, f32)> = info.coords.iter().map(|(t, v)| (subsetter::Tag::new(t), *v)).collect();
    // CFF2 (variable CFF) outlines have no PDF 1.6 embedding; the variations path instances
    // them (at the default coordinates when none are set) and converts them to TrueType.
    if coords.is_empty() && ttf.tables().cff2.is_none() {
        subsetter::subset(data, info.index, &entry.remap)
    } else {
        subsetter::subset_with_variations(data, info.index, &coords, &entry.remap)
    }
    .map_err(|e| format!("subsetting failed: {e}"))
}

/// Font objects' data of one font whose file [`FontRegistry::take_files`] holds (run on a
/// worker thread, after the content recorded what text each glyph showed).
pub fn build_subset(entry: &FontEntry, store: &FontStore) -> Result<Subset, String> {
    let faces = store.snapshot();
    let face = faces.get(entry.font).ok_or("font not loaded")?;
    let info = store.face_info(entry.font).ok_or("no face info")?;
    let ttf = face.ttf().ok_or("font does not parse")?;
    let sub = entry.file.clone().ok_or("font was not subset")?;
    let cff_table = table(&sub, b"CFF ");
    let (cff, file_data) = match cff_table {
        Some(c) => (true, c.to_vec()),
        None => (false, sub.clone()),
    };
    let upem = f32::from(ttf.units_per_em().max(1));
    let s = |v: i16| f32::from(v) * 1000.0 / upem;
    let bb = ttf.global_bounding_box();
    let (unicode_map, direct) = unicode_of(&ttf, &entry.order);
    // What the text showed wins over the tables (glyph variants, CID fonts without cmap).
    let unicode: Vec<String> = entry
        .order
        .iter()
        .map(|g| {
            let seen = entry.observed.get(g).and_then(|m| m.iter().max_by_key(|(c, n)| (**n, std::cmp::Reverse(**c))).map(|(c, _)| *c));
            // Arabic contextual forms are mapped by many fonts' cmaps from their presentation
            // form code points; the text shows the letter itself (as InDesign's PDF has it).
            let presentation = |t: &str| t.chars().all(|c| matches!(c, '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'));
            match (seen, unicode_map.get(g)) {
                (Some(c), Some(t)) if presentation(t) => c.to_string(),
                // Yu Gothic maps 日 and the compatibility radical ⽇ to one
                // glyph. Preserve the source alias, without undoing case
                // changes or replacing a ligature with its first character.
                (Some(c), Some(_)) if ttf.glyph_index(c).is_some_and(|id|id.0==*g) => c.to_string(),
                // the cmap's own character first (case changes show other characters)
                (_, Some(t)) if direct.contains(g) => t.clone(),
                (Some(c), Some(t)) if t.chars().count() > 1 && t.starts_with(c) => t.clone(),
                (Some(c), _) => c.to_string(),
                (None, Some(t)) => t.clone(),
                (None, None) => String::new(),
            }
        })
        .collect();
    // InDesign's PDF/X W entries use integer thousandths (wrap-box,
    // style-ligatures-22). Keep layout/kerning widths precise, but serialize the
    // same integer advances: Ghostscript honors fractional W while MuPDF rounds
    // it, otherwise a text-position probe can hide different printed advances.
    let widths: Vec<f32> = entry.order.iter().map(|g| entry.widths.get(g).copied().unwrap_or(0.0).round()).collect();
    let italic = ttf.is_italic() || ttf.italic_angle() != 0.0;
    let mut flags = 4; // symbolic: glyphs addressed by CID, not a standard encoding
    if ttf.is_monospaced() {
        flags |= 1;
    }
    if italic {
        flags |= 64;
    }
    let ps = if info.postscript.is_empty() { format!("{}-{}", info.family, info.style) } else { info.postscript.clone() };
    // Subset tag: six capitals from the glyph set.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for g in &entry.order {
        h = (h ^ *g as u64).wrapping_mul(0x0100_0000_01b3);
    }
    h ^= entry.font as u64;
    let tag: String = (0..6).map(|i| (b'A' + ((h >> (i * 5)) % 26) as u8) as char).collect();
    let weight = ttf.weight().to_number() as f32;
    Ok(Subset {
        postscript: sanitize_ps(&ps),
        file: info.path.display().to_string(),
        data: file_data,
        cff,
        widths,
        unicode,
        bbox: [s(bb.x_min), s(bb.y_min), s(bb.x_max), s(bb.y_max)],
        ascent: s(ttf.ascender()),
        descent: s(ttf.descender()),
        cap_height: ttf.capital_height().map(s).unwrap_or(s(ttf.ascender()) * 0.7),
        italic_angle: ttf.italic_angle(),
        flags,
        stem_v: 10.0 + 220.0 * ((weight - 50.0) / 900.0).clamp(0.0, 1.0),
        tag,
    })
}
