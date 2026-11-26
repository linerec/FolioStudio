//! Placed PDFs whose simple fonts are not embedded (e.g. Helvetica referenced by name): PDF/X
//! requires every font embedded, so the font is replaced by an embedded, metric-compatible
//! substitute (Helvetica → Arial, Times → Times New Roman, Courier → Courier New, an
//! installed face of the same name otherwise), written as a Type0 font whose embedded CMap
//! keeps the page's 1-byte codes and whose widths are the original ones. The substitution is
//! reported; a print shop will see a different face.

use crate::encodings::{MAC_ROMAN, STANDARD, WIN_ANSI, glyph_unicode};
use folio_text::FontStore;
use lopdf::Object;
use pdf_writer::{Chunk, Name, Ref, Str};

/// What replaced a font.
pub struct Substituted {
    pub requested: String,
    pub used: String,
}

fn get<'a>(doc: &'a lopdf::Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// Whether a font dictionary is a simple font without an embedded program.
pub fn needs_substitute(doc: &lopdf::Document, font: &lopdf::Dictionary) -> bool {
    let sub = font.get(b"Subtype").ok().and_then(|s| s.as_name().ok());
    if !matches!(sub, Some(b"Type1" | b"TrueType" | b"MMType1")) {
        return false;
    }
    match font.get(b"FontDescriptor").ok().map(|d| get(doc, d)) {
        Some(Object::Dictionary(fd)) => !(fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3")),
        _ => true,
    }
}

/// The face standing in for `base` (a PostScript name, maybe with a subset prefix).
fn choose(store: &FontStore, base: &str) -> Option<(u32, String)> {
    let name = base.split_once('+').map_or(base, |(tag, rest)| if tag.len() == 6 { rest } else { base });
    let lower = name.to_ascii_lowercase();
    let bold = ["bold", "black", "heavy", "semibold", "demi"].iter().any(|k| lower.contains(k));
    let italic = lower.contains("italic") || lower.contains("oblique");
    let style = match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    let family = if lower.contains("helvetica") || lower.contains("arial") {
        "Arial".to_owned()
    } else if lower.contains("times") {
        "Times New Roman".to_owned()
    } else if lower.contains("courier") {
        "Courier New".to_owned()
    } else if lower.starts_with("symbol") {
        "Symbol".to_owned()
    } else {
        name.split([',', '-']).next().unwrap_or(name).to_owned()
    };
    let r = store.resolve(&family, style, &[]);
    let r = if store.is_synthetic(r.font) { store.resolve("Arial", style, &[]) } else { r };
    if store.is_synthetic(r.font) {
        return None;
    }
    let used = store.face_info(r.font).map(|f| f.postscript.clone()).unwrap_or(family);
    Some((r.font, used))
}

/// Code → Unicode of a simple font from its encoding (base + Differences).
fn code_unicode(doc: &lopdf::Document, font: &lopdf::Dictionary, symbolic_builtin: bool) -> [Option<char>; 256] {
    let mut base: &[u16; 256] = &STANDARD;
    let mut out: [Option<char>; 256] = [None; 256];
    let enc = font.get(b"Encoding").ok().map(|e| get(doc, e));
    let pick = |n: &[u8]| -> Option<&'static [u16; 256]> {
        match n {
            b"WinAnsiEncoding" => Some(&WIN_ANSI),
            b"MacRomanEncoding" => Some(&MAC_ROMAN),
            b"StandardEncoding" => Some(&STANDARD),
            _ => None,
        }
    };
    let mut diffs: Option<&Vec<Object>> = None;
    match enc {
        Some(Object::Name(n)) => {
            if let Some(t) = pick(n) {
                base = t;
            }
        }
        Some(Object::Dictionary(d)) => {
            if let Some(t) = d.get(b"BaseEncoding").ok().and_then(|b| b.as_name().ok()).and_then(pick) {
                base = t;
            }
            diffs = d.get(b"Differences").ok().and_then(|x| x.as_array().ok());
        }
        _ => {}
    }
    if !symbolic_builtin {
        for (i, u) in base.iter().enumerate() {
            out[i] = (*u != 0).then(|| char::from_u32(*u as u32)).flatten();
        }
    }
    if let Some(d) = diffs {
        let mut code = 0usize;
        for x in d {
            match x {
                Object::Integer(i) => code = (*i).clamp(0, 255) as usize,
                Object::Name(n) => {
                    if code < 256 {
                        out[code] = glyph_unicode(&String::from_utf8_lossy(n));
                    }
                    code += 1;
                }
                _ => {}
            }
        }
    }
    out
}

/// Writes a substitute for `font` at `at` in `chunk`; `next` allocates local references.
pub fn write_substitute(
    doc: &lopdf::Document,
    font: &lopdf::Dictionary,
    store: &FontStore,
    chunk: &mut Chunk,
    at: Ref,
    next: &mut dyn FnMut() -> Ref,
) -> Result<Substituted, String> {
    let base = font.get(b"BaseFont").ok().and_then(|b| b.as_name().ok()).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_else(|| "Unnamed".into());
    let (fid, used) = choose(store, &base).ok_or("no substitute face installed")?;
    let faces = store.snapshot();
    let face = faces.get(fid).ok_or("substitute not loaded")?;
    let ttf = face.ttf().ok_or("substitute does not parse")?;
    let info = store.face_info(fid).ok_or("no face info")?;
    let symbolic = base.to_ascii_lowercase().contains("symbol") || base.to_ascii_lowercase().contains("dingbat");
    let unicode = code_unicode(doc, font, symbolic);
    // code → original glyph of the substitute
    let mut gid = [0u16; 256];
    for c in 0..256 {
        if let Some(ch) = unicode[c] {
            gid[c] = ttf.glyph_index(ch).map_or(0, |g| g.0);
        } else if symbolic {
            // symbol fonts address glyphs through the F0xx private range
            gid[c] = ttf.glyph_index(char::from_u32(0xF000 + c as u32).unwrap_or('\0')).map_or(0, |g| g.0);
        }
    }
    let mut glyphs: Vec<u16> = gid.iter().copied().filter(|g| *g != 0).collect();
    glyphs.sort_unstable();
    glyphs.dedup();
    let mut order = vec![0u16];
    order.extend(glyphs);
    let remap = subsetter::GlyphRemapper::new_from_glyphs(&order);
    let coords: Vec<(subsetter::Tag, f32)> = info.coords.iter().map(|(t, v)| (subsetter::Tag::new(t), *v)).collect();
    let data = face.bytes().ok_or("no font data")?;
    let sub = if coords.is_empty() {
        subsetter::subset(data, info.index, &remap)
    } else {
        subsetter::subset_with_variations(data, info.index, &coords, &remap)
    }
    .map_err(|e| format!("subsetting {used}: {e}"))?;
    let cff = crate::fonts::table(&sub, b"CFF ").map(<[u8]>::to_vec);
    let upem = f32::from(ttf.units_per_em().max(1));
    let first = font.get(b"FirstChar").ok().and_then(num).unwrap_or(0.0) as usize;
    let widths: Vec<f64> = font
        .get(b"Widths")
        .ok()
        .map(|w| get(doc, w))
        .and_then(|w| w.as_array().ok())
        .map(|a| a.iter().map(|x| num(get(doc, x)).unwrap_or(0.0)).collect())
        .unwrap_or_default();
    let width_of = |c: usize| -> f32 {
        if c >= first && c - first < widths.len() {
            return widths[c - first] as f32;
        }
        f32::from(ttf.glyph_hor_advance(ttf_parser::GlyphId(gid[c])).unwrap_or(0)) * 1000.0 / upem
    };
    // CMap: code → CID (= subset glyph id); widths and ToUnicode per CID.
    let mut cmap = String::from(
        "%!PS-Adobe-3.0 Resource-CMap\n/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n/CMapName /Folio-Simple def\n/CMapType 1 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    let mapped: Vec<(usize, u16)> = (0..256).filter(|c| gid[*c] != 0).filter_map(|c| remap.get(gid[c]).map(|n| (c, n))).collect();
    for chunk_codes in mapped.chunks(100) {
        cmap.push_str(&format!("{} begincidchar\n", chunk_codes.len()));
        for (c, n) in chunk_codes {
            cmap.push_str(&format!("<{c:02X}> {n}\n"));
        }
        cmap.push_str("endcidchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let mut tu = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    let uni: Vec<(usize, char)> = (0..256).filter_map(|c| unicode[c].map(|u| (c, u))).collect();
    for part in uni.chunks(100) {
        tu.push_str(&format!("{} beginbfchar\n", part.len()));
        for (c, u) in part {
            let mut buf = [0u16; 2];
            let hex: String = u.encode_utf16(&mut buf).iter().map(|x| format!("{x:04X}")).collect();
            tu.push_str(&format!("<{c:02X}> <{hex}>\n"));
        }
        tu.push_str("endbfchar\n");
    }
    tu.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");

    let ps = format!("FOLIOS+{}", used.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>());
    let (cid, desc, file, cmap_ref, tu_ref) = (next(), next(), next(), next(), next());
    {
        let mut d = chunk.indirect(at).dict();
        d.pair(Name(b"Type"), Name(b"Font"));
        d.pair(Name(b"Subtype"), Name(b"Type0"));
        d.pair(Name(b"BaseFont"), Name(ps.as_bytes()));
        d.pair(Name(b"Encoding"), cmap_ref);
        d.insert(Name(b"DescendantFonts")).array().item(cid);
        d.pair(Name(b"ToUnicode"), tu_ref);
    }
    {
        let mut c = chunk.indirect(cid).dict();
        c.pair(Name(b"Type"), Name(b"Font"));
        c.pair(Name(b"Subtype"), Name(if cff.is_some() { b"CIDFontType0".as_slice() } else { b"CIDFontType2".as_slice() }));
        c.pair(Name(b"BaseFont"), Name(ps.as_bytes()));
        {
            let mut si = c.insert(Name(b"CIDSystemInfo")).dict();
            si.pair(Name(b"Registry"), Str(b"Adobe"));
            si.pair(Name(b"Ordering"), Str(b"Identity"));
            si.pair(Name(b"Supplement"), 0);
        }
        c.pair(Name(b"FontDescriptor"), desc);
        c.pair(Name(b"DW"), 0);
        {
            let mut w = c.insert(Name(b"W")).array();
            for (code, n) in &mapped {
                w.item(*n as i32);
                w.push().array().item(width_of(*code));
            }
        }
        if cff.is_none() {
            c.pair(Name(b"CIDToGIDMap"), Name(b"Identity"));
        }
    }
    {
        let s = |v: i16| f32::from(v) * 1000.0 / upem;
        let bb = ttf.global_bounding_box();
        let mut d = chunk.indirect(desc).dict();
        d.pair(Name(b"Type"), Name(b"FontDescriptor"));
        d.pair(Name(b"FontName"), Name(ps.as_bytes()));
        d.pair(Name(b"Flags"), 4 | if ttf.is_italic() { 64 } else { 0 });
        d.insert(Name(b"FontBBox")).array().items([s(bb.x_min), s(bb.y_min), s(bb.x_max), s(bb.y_max)]);
        d.pair(Name(b"ItalicAngle"), ttf.italic_angle());
        d.pair(Name(b"Ascent"), s(ttf.ascender()));
        d.pair(Name(b"Descent"), s(ttf.descender()));
        d.pair(Name(b"CapHeight"), ttf.capital_height().map(s).unwrap_or(700.0));
        d.pair(Name(b"StemV"), 80);
        d.pair(Name(if cff.is_some() { b"FontFile3".as_slice() } else { b"FontFile2".as_slice() }), file);
    }
    let program = cff.clone().unwrap_or_else(|| sub.clone());
    let packed = crate::images::flate(&program, 9);
    {
        let mut st = chunk.stream(file, &packed);
        st.filter(pdf_writer::Filter::FlateDecode);
        if cff.is_some() {
            st.pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
        } else {
            st.pair(Name(b"Length1"), program.len() as i32);
        }
    }
    let cm = crate::images::flate(cmap.as_bytes(), 6);
    {
        let mut st = chunk.stream(cmap_ref, &cm);
        st.filter(pdf_writer::Filter::FlateDecode);
        st.pair(Name(b"Type"), Name(b"CMap"));
        st.pair(Name(b"CMapName"), Name(b"Folio-Simple"));
        let mut si = st.insert(Name(b"CIDSystemInfo")).dict();
        si.pair(Name(b"Registry"), Str(b"Adobe"));
        si.pair(Name(b"Ordering"), Str(b"Identity"));
        si.pair(Name(b"Supplement"), 0);
    }
    let t = crate::images::flate(tu.as_bytes(), 6);
    chunk.stream(tu_ref, &t).filter(pdf_writer::Filter::FlateDecode);
    Ok(Substituted { requested: base, used })
}
