//! Copies the resource graph and content of a placed page into a PDF form.

use super::{FileChunk, ImportCache, Imported};
use crate::spotconv::{CsAction, Conversion, InkRules};
use folio_model::Rect;
use lopdf::{Object, ObjectId};
use pdf_writer::{Name, Ref, Str};
use std::collections::HashMap;
use std::path::Path;

/// Largest decompressed page content accepted from a placed PDF.
const MAX_CONTENT: usize = 256 << 20;

/// Content operators that paint in DeviceRGB (`rg`, `RG`, `/DeviceRGB cs`) or inline images
/// in RGB.
fn content_has_rgb(data: &[u8]) -> bool {
    let mut prev: &[u8] = b"";
    for tok in data.split(|b| b.is_ascii_whitespace() || b"[]()<>{}".contains(b)) {
        if tok.is_empty() {
            continue;
        }
        if tok == b"rg" || tok == b"RG" || tok == b"/DeviceRGB" {
            return true;
        }
        if (prev == b"/CS" || prev == b"/ColorSpace") && (tok == b"/RGB" || tok == b"/DeviceRGB" || tok == b"/CalRGB") {
            return true;
        }
        prev = tok;
    }
    false
}

struct Copier<'a> {
    doc: &'a lopdf::Document,
    st: &'a mut FileChunk,
    todo: Vec<(ObjectId, Ref)>,
    dropped: Vec<String>,
    transparency: bool,
    fonts: &'a folio_text::FontStore,
    substituted: Vec<String>,
    rules: &'a InkRules,
    /// Converted Separations of the Resources dictionary converted last, by resource name.
    captured: HashMap<Vec<u8>, Conversion>,
    /// Ink Manager effects (ink → what happened), for the report.
    inks: Vec<String>,
    /// Add DefaultRGB to Resources.
    default_rgb: bool,
    /// CMYK-only output: DeviceRGB converted with these colours.
    rgb: Option<&'a folio_color::Colors>,
    /// PDF 1.3 output (see [`ImportCache::pdf13`]).
    pdf13: bool,
}

const DROP_KEYS: &[&[u8]] = &[b"TR", b"TR2", b"OPI", b"AA", b"JS", b"JavaScript", b"PieceInfo", b"Metadata"];

impl Copier<'_> {
    fn alloc(&mut self) -> Ref {
        let r = Ref::new(self.st.next);
        self.st.next += 1;
        r
    }

    fn reference(&mut self, id: ObjectId) -> Ref {
        if let Some(&r) = self.st.map.get(&id) {
            return r;
        }
        let r = self.alloc();
        self.st.map.insert(id, r);
        self.todo.push((id, r));
        r
    }

    /// Rewrites a content stream (converted spot tints, DeviceRGB in CMYK-only output) and
    /// notes RGB left in it. `None`: unchanged.
    fn content(&mut self, raw: &[u8]) -> Option<Vec<u8>> {
        let conv = self.rgb.map(|c| move |rgb: [f32; 3]| folio_render::inks::cmyk8_of_rgb(c, rgb));
        let rgb_fn = conv.as_ref().map(|f| f as crate::spotconv::RgbToCmyk<'_>);
        let new = crate::spotconv::rewrite_content(raw, &self.captured, rgb_fn);
        let had_rgb = content_has_rgb(raw);
        if content_has_rgb(new.as_deref().unwrap_or(raw)) {
            self.st.non_cmyk = true;
        } else if had_rgb {
            self.st.rgb_converted = true;
        }
        new
    }

    fn drain(&mut self) {
        while let Some((id, r)) = self.todo.pop() {
            let obj = self.doc.get_object(id).cloned().unwrap_or(Object::Null);
            if let Object::Dictionary(d) = &obj
                && crate::fontsub::needs_substitute(self.doc, d)
            {
                let mut next = self.st.next;
                let mut alloc = || {
                    let r = Ref::new(next);
                    next += 1;
                    r
                };
                let res = crate::fontsub::write_substitute(self.doc, d, self.fonts, &mut self.st.chunk, r, &mut alloc);
                self.st.next = next;
                match res {
                    Ok(s) => {
                        let msg = format!("{} → {}", s.requested, s.used);
                        if !self.substituted.contains(&msg) {
                            self.substituted.push(msg);
                        }
                        continue;
                    }
                    Err(e) => self.dropped.push(format!("font not embedded and not substituted ({e})")),
                }
            }
            match obj {
                Object::Stream(s) => {
                    let image = s.dict.get(b"Subtype").ok().and_then(|x| x.as_name().ok()) == Some(b"Image");
                    // PDF 1.3: a soft mask it can hold without transparency
                    let (s, extra) = if image && self.pdf13 { self.pdf13_mask(s) } else { (s, Vec::new()) };
                    let subtype = s.dict.get(b"Subtype").ok().and_then(|x| x.as_name().ok());
                    if image && (self.convert_image(&s, r, &extra) || self.convert_rgb_image(&s, r, &extra)) {
                        continue;
                    }
                    self.captured.clear();
                    let mut dict = self.dict_entries(&s.dict, false);
                    dict.extend(extra);
                    // a form or tiling pattern: tints of converted spot inks become their
                    // alternates, DeviceRGB becomes CMYK in CMYK-only output
                    let content_stream = subtype == Some(b"Form") || s.dict.get(b"PatternType").ok().and_then(|x| x.as_i64().ok()) == Some(1);
                    let mut content = std::borrow::Cow::Borrowed(&s.content);
                    if content_stream || !self.captured.is_empty() {
                        match s.decompressed_content() {
                            Ok(raw) => {
                                if let Some(new) = self.content(&raw) {
                                    content = std::borrow::Cow::Owned(crate::images::flate(&new, 6));
                                    dict.retain(|(k, _)| k.as_slice() != b"Filter" && k.as_slice() != b"DecodeParms");
                                    dict.push((b"Filter".to_vec(), Val::Name(b"FlateDecode".to_vec())));
                                }
                            }
                            // colour not known: not copied as it is in CMYK-only output
                            Err(_) if self.rgb.is_some() => self.st.non_cmyk = true,
                            Err(_) => {}
                        }
                    }
                    self.captured.clear();
                    // Flate data whose zlib trailer is damaged (truncated by its producer):
                    // re-encoded so every reader inflates it cleanly.
                    if let std::borrow::Cow::Borrowed(raw) = &content
                        && matches!(s.dict.get(b"Filter").ok(), Some(Object::Name(n)) if n == b"FlateDecode")
                        && let Some(fixed) = repair_flate(raw)
                    {
                        content = std::borrow::Cow::Owned(fixed);
                        self.note_ink("damaged Flate streams re-encoded".into());
                    }
                    let mut st = self.st.chunk.stream(r, &content);
                    for (k, v) in dict {
                        if k.as_slice() == b"Length" {
                            continue;
                        }
                        write_obj(st.insert(Name(&k)), &v);
                    }
                }
                other => {
                    let v = self.convert(&other, false);
                    write_obj(self.st.chunk.indirect(r), &v);
                }
            }
        }
    }

    /// An image in a Separation the Ink Manager converts, written in the alternate space.
    /// `false` when it is not such an image (or cannot be converted).
    fn convert_image(&mut self, s: &lopdf::Stream, at: Ref, extra: &[(Vec<u8>, Val)]) -> bool {
        let Ok(cs) = s.dict.get(b"ColorSpace") else { return false };
        let CsAction::Convert(cv) = crate::spotconv::classify(self.doc, cs, self.rules) else { return false };
        let bpc = s.dict.get(b"BitsPerComponent").ok().and_then(|b| b.as_i64().ok()).unwrap_or(8);
        let dct = matches!(s.dict.get(b"Filter").ok(), Some(Object::Name(n)) if n == b"DCTDecode");
        let samples = if bpc == 8 && !dct { s.decompressed_content().ok().or_else(|| s.dict.get(b"Filter").is_err().then(|| s.content.clone())) } else { None };
        let Some(samples) = samples else {
            let m = format!("image in converted ink {} kept as a spot (not 8-bit Flate)", cv.ink);
            if !self.inks.contains(&m) {
                self.inks.push(m);
            }
            return false;
        };
        let inverted = matches!(s.dict.get(b"Decode").ok().and_then(|d| d.as_array().ok()), Some(a) if a.first().and_then(|x| x.as_float().ok()) == Some(1.0));
        let data = crate::images::flate(&crate::spotconv::convert_samples(&cv, &samples, inverted), 6);
        let alt = self.convert(&cv.alternate.clone(), false);
        let mut dict = Vec::new();
        for (k, v) in s.dict.iter() {
            if matches!(k.as_slice(), b"ColorSpace" | b"Filter" | b"DecodeParms" | b"Decode" | b"Length") {
                continue;
            }
            if k.as_slice() == b"SMask" && !matches!(v, Object::Name(n) if n == b"None") {
                self.transparency = true;
            }
            dict.push((k.clone(), self.convert(v, false)));
        }
        dict.extend(extra.iter().cloned());
        dict.push((b"ColorSpace".to_vec(), alt));
        dict.push((b"Filter".to_vec(), Val::Name(b"FlateDecode".to_vec())));
        {
            let mut st = self.st.chunk.stream(at, &data);
            for (k, v) in dict {
                write_obj(st.insert(Name(&k)), &v);
            }
        }
        self.note_ink(format!("{} converted to process (Ink Manager)", cv.ink));
        true
    }

    /// An 8-bit DeviceRGB image in CMYK-only output, written in DeviceCMYK with the document
    /// conversion (the one vector paint and the flattener use). `false` when it is not such an
    /// image or is not decoded here: copied as it is, its DeviceRGB then marks the file
    /// non-CMYK (flattened from the preview). A soft mask leaves it to the flattener too.
    fn convert_rgb_image(&mut self, s: &lopdf::Stream, at: Ref, extra: &[(Vec<u8>, Val)]) -> bool {
        let Some(colors) = self.rgb else { return false };
        let rgb = s.dict.get(b"ColorSpace").ok().and_then(|cs| self.doc.dereference(cs).ok()).is_some_and(|(_, cs)| matches!(cs, Object::Name(n) if n == b"DeviceRGB"));
        let colour_key = matches!(s.dict.get(b"Mask").ok(), Some(Object::Array(_)));
        if !rgb
            || colour_key
            || s.dict.get(b"BitsPerComponent").ok().and_then(|b| b.as_i64().ok()) != Some(8)
            || s.dict.has(b"Decode")
            || s.dict.has(b"DecodeParms")
            || s.dict.has(b"SMask")
        {
            return false;
        }
        let dim = |k: &[u8]| s.dict.get(k).ok().and_then(|v| v.as_i64().ok()).filter(|v| (1..=65_535).contains(v)).map(|v| v as u32);
        let (Some(w), Some(h)) = (dim(b"Width"), dim(b"Height")) else { return false };
        let px = w as usize * h as usize;
        let filter = s.dict.get(b"Filter").ok().and_then(|f| f.as_name().ok());
        let samples = match (s.dict.get(b"Filter").ok(), filter) {
            (None, _) => Some(s.content.clone()),
            (_, Some(b"FlateDecode")) => s.decompressed_content().ok(),
            (_, Some(b"DCTDecode")) => image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg)
                .ok()
                .filter(|i| i.width() == w && i.height() == h)
                .map(|i| i.into_rgb8().into_raw()),
            _ => None,
        };
        let Some(samples) = samples.filter(|v| v.len() >= px * 3) else { return false };
        let mut cmyk = Vec::with_capacity(px * 4);
        let mut last: Option<([u8; 3], [u8; 4])> = None;
        for p in samples[..px * 3].chunks_exact(3) {
            let rgb = [p[0], p[1], p[2]];
            let c = match last {
                Some((i, o)) if i == rgb => o,
                _ => {
                    let o = colors.rgb_to_cmyk8(rgb);
                    last = Some((rgb, o));
                    o
                }
            };
            cmyk.extend_from_slice(&c);
        }
        // a JPEG stays a JPEG (CMYK, as InDesign writes converted ones)
        let jpeg = if filter == Some(b"DCTDecode") { crate::images::cmyk_jpeg(&cmyk, w, h) } else { None };
        let mut d = s.dict.clone();
        for k in [&b"ColorSpace"[..], b"Filter", b"DecodeParms", b"Decode", b"Length"] {
            d.remove(k);
        }
        let mut dict = self.dict_entries(&d, false);
        dict.extend(extra.iter().cloned());
        dict.push((b"ColorSpace".to_vec(), Val::Name(b"DeviceCMYK".to_vec())));
        let data = match jpeg {
            Some(j) => {
                dict.push((b"Filter".to_vec(), Val::Name(b"DCTDecode".to_vec())));
                dict.push((b"Decode".to_vec(), Val::Array([1, 0, 1, 0, 1, 0, 1, 0].iter().map(|v| Val::Int(*v)).collect())));
                j
            }
            None => {
                dict.push((b"Filter".to_vec(), Val::Name(b"FlateDecode".to_vec())));
                crate::images::flate(&cmyk, 6)
            }
        };
        let mut st = self.st.chunk.stream(at, &data);
        for (k, v) in dict {
            write_obj(st.insert(Name(&k)), &v);
        }
        self.st.rgb_converted = true;
        true
    }

    /// PDF 1.3 output: an image's soft mask that is all opaque is dropped, and a cut-out one
    /// (opaque and clear areas with only anti-aliased edges between them) becomes a 1-bit
    /// explicit mask at up to twice its resolution, as our own images' alpha does. A fading
    /// mask stays (transparency: the flattener draws the placement). Returns the image to
    /// write and the keys to add to it.
    fn pdf13_mask(&mut self, mut s: lopdf::Stream) -> (lopdf::Stream, Vec<(Vec<u8>, Val)>) {
        let Some(id) = s.dict.get(b"SMask").ok().and_then(|o| o.as_reference().ok()) else { return (s, Vec::new()) };
        let Ok(Object::Stream(m)) = self.doc.get_object(id) else { return (s, Vec::new()) };
        let int = |d: &lopdf::Dictionary, k: &[u8]| d.get(k).ok().and_then(|v| v.as_i64().ok());
        let (Some(w), Some(h)) = (int(&m.dict, b"Width"), int(&m.dict, b"Height")) else { return (s, Vec::new()) };
        let gray = m.dict.get(b"ColorSpace").ok().and_then(|cs| self.doc.dereference(cs).ok()).is_some_and(|(_, cs)| matches!(cs, Object::Name(n) if n == b"DeviceGray"));
        // Matte: colour premultiplied with the mask, wrong without it
        if !(1..=20_000).contains(&w)
            || !(1..=20_000).contains(&h)
            || !gray
            || int(&m.dict, b"BitsPerComponent") != Some(8)
            || m.dict.has(b"Matte")
            || m.dict.has(b"Decode")
            || s.dict.has(b"Mask")
        {
            return (s, Vec::new());
        }
        let (w, h) = (w as usize, h as usize);
        let a = match m.dict.get(b"Filter").ok() {
            None => Some(m.content.clone()),
            Some(Object::Name(n)) if n == b"FlateDecode" => m.decompressed_content().ok(),
            _ => None,
        };
        let Some(a) = a.filter(|a| a.len() == w * h) else { return (s, Vec::new()) };
        match crate::images::classify_alpha(&a, w, h) {
            crate::images::AlphaClass::Opaque => {
                s.dict.remove(b"SMask");
                (s, Vec::new())
            }
            crate::images::AlphaClass::Cutout => {
                let f = if w.max(h) <= 4000 { 2 } else { 1 };
                let (mw, mh, data) = crate::images::explicit_mask(&a, w as u32, h as u32, f, 6);
                let at = self.alloc();
                let mut x = self.st.chunk.image_xobject(at, &data);
                x.width(mw as i32);
                x.height(mh as i32);
                x.image_mask(true);
                x.bits_per_component(1);
                x.filter(pdf_writer::Filter::FlateDecode);
                drop(x);
                s.dict.remove(b"SMask");
                (s, vec![(b"Mask".to_vec(), Val::Ref(at))])
            }
            crate::images::AlphaClass::Soft => (s, Vec::new()),
        }
    }

    fn note_ink(&mut self, m: String) {
        if !self.inks.contains(&m) {
            self.inks.push(m);
        }
    }

    /// A Resources ColorSpace dictionary with the Ink Manager applied.
    fn color_spaces(&mut self, v: &Object) -> Val {
        let d = match v {
            Object::Reference(id) => self.doc.get_object(*id).ok().and_then(|o| o.as_dict().ok()).cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        };
        let Some(d) = d else { return self.convert(v, false) };
        let mut entries = Vec::new();
        for (name, cs) in d.iter() {
            match crate::spotconv::classify(self.doc, cs, self.rules) {
                CsAction::Keep => {
                    if let Ok(Object::Array(a)) = self.doc.dereference(cs).map(|x| x.1)
                        && a.first().and_then(|x| x.as_name().ok()) == Some(b"Separation")
                        && let Some(n) = a.get(1).and_then(|x| x.as_name().ok())
                        && !matches!(n, b"All" | b"None" | b"Cyan" | b"Magenta" | b"Yellow" | b"Black")
                    {
                        self.note_ink(format!("{} kept as its own plate (placed PDF)", String::from_utf8_lossy(n)));
                    }
                    entries.push((name.clone(), self.convert(cs, false)));
                }
                CsAction::Rename(to) => {
                    let arr = self.doc.dereference(cs).map(|x| x.1.clone()).unwrap_or(Object::Null);
                    let mut val = self.convert(&arr, false);
                    if let Val::Array(items) = &mut val
                        && items.len() > 1
                    {
                        items[1] = Val::Name(to.clone().into_bytes());
                    }
                    self.note_ink(format!("alias of {to} (Ink Manager)"));
                    entries.push((name.clone(), val));
                }
                CsAction::Convert(cv) => {
                    let alt = self.convert(&cv.alternate.clone(), false);
                    self.note_ink(format!("{} converted to process (Ink Manager)", cv.ink));
                    entries.push((name.clone(), alt));
                    self.captured.insert(name.clone(), cv);
                }
            }
        }
        Val::Dict(entries)
    }

    /// A shading in a converted Separation: the alternate space, and its function composed
    /// with the tint transform (sampled).
    fn shading(&mut self, d: &lopdf::Dictionary) -> Option<Vec<(Vec<u8>, Val)>> {
        let cs = d.get(b"ColorSpace").ok()?;
        let CsAction::Convert(cv) = crate::spotconv::classify(self.doc, cs, self.rules) else { return None };
        let f = crate::spotconv::parse_function(self.doc, d.get(b"Function").ok()?)?;
        let domain: Vec<f64> = d
            .get(b"Domain")
            .ok()
            .and_then(|x| x.as_array().ok())
            .map(|a| a.iter().filter_map(|v| v.as_float().ok().map(f64::from)).collect())
            .filter(|v: &Vec<f64>| v.len() == 2)
            .unwrap_or_else(|| vec![0.0, 1.0]);
        let lab = matches!(&cv.alternate, Object::Array(a) if a.first().and_then(|x| x.as_name().ok()) == Some(b"Lab"));
        let range: Vec<f32> = if lab { vec![0.0, 100.0, -128.0, 127.0, -128.0, 127.0] } else { [0.0f32, 1.0].repeat(cv.components) };
        const N: usize = 256;
        let mut samples = Vec::with_capacity(N * cv.components * 2);
        for i in 0..N {
            let t = domain[0] + (domain[1] - domain[0]) * i as f64 / (N - 1) as f64;
            let tint = f.eval(&[t]).and_then(|v| v.first().copied()).unwrap_or(0.0);
            for (k, v) in cv.values(tint).iter().enumerate() {
                let (lo, hi) = (range[2 * k] as f64, range[2 * k + 1] as f64);
                let q = (((v - lo) / (hi - lo)).clamp(0.0, 1.0) * 65535.0).round() as u16;
                samples.extend_from_slice(&q.to_be_bytes());
            }
        }
        let fr = self.alloc();
        {
            let data = crate::images::flate(&samples, 6);
            let mut sf = self.st.chunk.sampled_function(fr, &data);
            sf.domain([domain[0] as f32, domain[1] as f32]);
            sf.range(range.iter().copied());
            sf.size([N as i32]);
            sf.bits_per_sample(16);
            sf.filter(pdf_writer::Filter::FlateDecode);
        }
        let mut out = Vec::new();
        for (k, v) in d.iter() {
            match k.as_slice() {
                b"ColorSpace" => out.push((k.clone(), self.convert(&cv.alternate.clone(), false))),
                b"Function" => out.push((k.clone(), Val::Ref(fr))),
                _ => out.push((k.clone(), self.convert(v, false))),
            }
        }
        self.note_ink(format!("{} converted to process (Ink Manager)", cv.ink));
        Some(out)
    }

    fn dict_entries(&mut self, d: &lopdf::Dictionary, resources: bool) -> Vec<(Vec<u8>, Val)> {
        if d.has(b"ShadingType")
            && let Some(sh) = self.shading(d)
        {
            return sh;
        }
        let mut out = Vec::new();
        for (k, v) in d.iter() {
            if resources && k.as_slice() == b"ColorSpace" {
                let cs = self.color_spaces(v);
                out.push((k.clone(), cs));
                continue;
            }
            if DROP_KEYS.contains(&k.as_slice()) {
                if !self.dropped.contains(&String::from_utf8_lossy(k).into_owned()) {
                    self.dropped.push(String::from_utf8_lossy(k).into_owned());
                }
                continue;
            }
            // PDF 1.3 has no transparency groups. Content without soft masks, alpha below 1 or
            // blend modes draws the same without its group (any of those makes the placement
            // transparency the flattener draws), so the group is left out.
            if k.as_slice() == b"Group" && self.pdf13 {
                continue;
            }
            if (k.as_slice() == b"SMask" || k.as_slice() == b"Group") && !matches!(v, Object::Name(n) if n == b"None") {
                self.transparency = true;
            }
            if k.as_slice() == b"BM" && !matches!(v, Object::Name(n) if n == b"Normal" || n == b"Compatible") {
                self.transparency = true;
            }
            if (k.as_slice() == b"CA" || k.as_slice() == b"ca") && matches!(v, Object::Real(a) if *a < 1.0) {
                self.transparency = true;
            }
            if (k.as_slice() == b"BitsPerComponent" && matches!(v, Object::Integer(16))) || k.as_slice() == b"OC" || k.as_slice() == b"OCProperties" {
                self.st.newer_than_13 = true;
            }
            let is_res = k.as_slice() == b"Resources";
            out.push((k.clone(), self.convert(v, is_res)));
        }
        if resources && self.default_rgb {
            // DeviceRGB inside the placed page uses the document RGB profile.
            let cs_pos = out.iter().position(|(k, _)| k.as_slice() == b"ColorSpace");
            match cs_pos {
                Some(p) => {
                    if let Val::Dict(entries) = &mut out[p].1
                        && !entries.iter().any(|(k, _)| k.as_slice() == b"DefaultRGB")
                    {
                        entries.push((b"DefaultRGB".to_vec(), Val::Ref(self.st.default_rgb)));
                    }
                }
                None => out.push((b"ColorSpace".to_vec(), Val::Dict(vec![(b"DefaultRGB".to_vec(), Val::Ref(self.st.default_rgb))]))),
            }
        }
        out
    }

    /// `resources`: the object is (or refers to) a Resources dictionary.
    fn convert(&mut self, o: &Object, resources: bool) -> Val {
        match o {
            Object::Null => Val::Null,
            Object::Boolean(b) => Val::Bool(*b),
            Object::Integer(i) => Val::Int(*i),
            Object::Real(r) => Val::Real(*r),
            Object::Name(n) => {
                if matches!(n.as_slice(), b"DeviceRGB" | b"CalRGB" | b"CalGray" | b"Lab" | b"ICCBased") {
                    self.st.non_cmyk = true;
                }
                if matches!(n.as_slice(), b"JPXDecode" | b"JBIG2Decode") {
                    self.st.newer_than_13 = true;
                }
                Val::Name(n.clone())
            }
            Object::String(s, _) => Val::Str(s.clone()),
            Object::Array(a) => Val::Array(a.iter().map(|x| self.convert(x, false)).collect()),
            Object::Dictionary(d) => Val::Dict(self.dict_entries(d, resources)),
            Object::Stream(_) => Val::Null,
            Object::Reference(id) => {
                if resources {
                    // Resources are inlined so DefaultRGB can be added without touching the
                    // shared source object.
                    if let Ok(Object::Dictionary(d)) = self.doc.get_object(*id) {
                        let d = d.clone();
                        return Val::Dict(self.dict_entries(&d, true));
                    }
                }
                Val::Ref(self.reference(*id))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum Val {
    Null,
    Bool(bool),
    Int(i64),
    Real(f32),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Array(Vec<Val>),
    Dict(Vec<(Vec<u8>, Val)>),
    Ref(Ref),
}

fn write_obj(obj: pdf_writer::Obj<'_>, v: &Val) {
    match v {
        Val::Null => obj.primitive(pdf_writer::Null),
        Val::Bool(b) => obj.primitive(*b),
        Val::Int(i) => obj.primitive((*i).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        Val::Real(r) => obj.primitive(*r),
        Val::Name(n) => obj.primitive(Name(n)),
        Val::Str(s) => obj.primitive(Str(s)),
        Val::Ref(r) => obj.primitive(*r),
        Val::Array(a) => {
            let mut arr = obj.array();
            for x in a {
                write_obj(arr.push(), x);
            }
        }
        Val::Dict(d) => {
            let mut dict = obj.dict();
            for (k, x) in d {
                write_obj(dict.insert(Name(k)), x);
            }
        }
    }
}

/// `None` when the zlib data inflates cleanly; else the data re-encoded from what a
/// lenient raw inflate recovers (a missing or wrong Adler-32 trailer is the usual damage).
fn repair_flate(raw: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    // A complete zlib stream ends with StreamEnd after its Adler-32 check.
    let mut d = flate2::Decompress::new(true);
    let mut out: Vec<u8> = Vec::with_capacity(raw.len() * 4 + 64);
    loop {
        let before = (d.total_in(), d.total_out());
        if out.capacity() - out.len() < 64 * 1024 {
            out.reserve(out.len().max(64 * 1024));
        }
        let consumed = d.total_in() as usize;
        match d.decompress_vec(&raw[consumed.min(raw.len())..], &mut out, flate2::FlushDecompress::Finish) {
            Ok(flate2::Status::StreamEnd) => return None,
            Ok(_) if (d.total_in(), d.total_out()) == before => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    let mut out = Vec::new();
    let body = raw.get(2..)?;
    let mut dec = flate2::read::DeflateDecoder::new(body);
    // Keep what decodes; an empty but well-formed body (e.g. `03 00`) is an empty stream.
    let complete = dec.read_to_end(&mut out).is_ok();
    (complete || !out.is_empty()).then(|| crate::images::flate(&out, 6))
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// A page attribute, inherited through the page tree.
pub(super) fn inherited<'a>(doc: &'a lopdf::Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut cur = doc.get_object(page).ok()?.as_dict().ok()?;
    for _ in 0..32 {
        if let Ok(v) = cur.get(key) {
            return Some(v);
        }
        let parent = cur.get(b"Parent").ok()?.as_reference().ok()?;
        cur = doc.get_object(parent).ok()?.as_dict().ok()?;
    }
    None
}

pub(super) fn rect_of(doc: &lopdf::Document, o: &Object) -> Option<[f64; 4]> {
    let o = match o {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        o => o,
    };
    let a = o.as_array().ok()?;
    if a.len() != 4 {
        return None;
    }
    let v: Vec<f64> = a.iter().filter_map(num).collect();
    (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

impl ImportCache {
    /// Page `page` (1-based) of the PDF, showing `bounds` (graphic coordinates).
    #[cfg(test)]
    pub fn import(&mut self, path: &Path, page: u32, bounds: &Rect, fonts: &folio_text::FontStore, rules: &InkRules) -> Result<Imported, String> {
        self.import_background(path, page, bounds, fonts, rules, true)
    }

    #[cfg(test)]
    pub fn import_background(&mut self, path: &Path, page: u32, bounds: &Rect, fonts: &folio_text::FontStore, rules: &InkRules, transparent: bool) -> Result<Imported, String> {
        self.import_layers(path,page,bounds,fonts,rules,transparent,None)
    }

    // Keep the existing import boundary and its independent PDF placement options.
    #[allow(clippy::too_many_arguments)]
    pub fn import_layers(&mut self, path: &Path, page: u32, bounds: &Rect, fonts: &folio_text::FontStore, rules: &InkRules, transparent: bool, layers: Option<&folio_model::graphic_layers::Options>) -> Result<Imported, String> {
        let (doc, file) = self.load(path,layers)?;
        if doc.is_encrypted() {
            return Err("encrypted PDF".into());
        }
        let pages = doc.get_pages();
        let pid = *pages.get(&page).ok_or_else(|| format!("page {page} not in the file ({} pages)", pages.len()))?;
        let media = inherited(&doc, pid, b"MediaBox").and_then(|o| rect_of(&doc, o)).ok_or("page without MediaBox")?;
        let crop = inherited(&doc, pid, b"CropBox").and_then(|o| rect_of(&doc, o)).unwrap_or(media);
        let rotate = inherited(&doc, pid, b"Rotate").and_then(num).unwrap_or(0.0) as i64;
        let mut warnings = Vec::new();
        // a page turned by something other than a quarter turn is placed unturned (the placement refuses it; this is a file placed before)
        let rotate = if rotate.rem_euclid(90) == 0 { rotate.rem_euclid(360) as i32 } else {
            warnings.push(format!("placed PDF page rotated {rotate}°: placed unrotated"));
            0
        };
        let content = doc
            .get_page_content_with_limit(pid, MAX_CONTENT)
            .map_err(|e| format!("page content: {e}"))?;
        let mut cp = Copier {
            doc: &doc,
            st: &mut self.files[file],
            todo: Vec::new(),
            dropped: Vec::new(),
            transparency: false,
            fonts,
            substituted: Vec::new(),
            rules,
            captured: HashMap::new(),
            inks: Vec::new(),
            default_rgb: self.cmyk_output.is_none(),
            rgb: self.cmyk_output.as_ref(),
            pdf13: self.pdf13,
        };
        let form = cp.alloc();
        let resources = inherited(&doc, pid, b"Resources").cloned();
        cp.captured.clear();
        let res_val = match &resources {
            Some(o) => cp.convert(o, true),
            None if cp.default_rgb => Val::Dict(vec![(b"ColorSpace".to_vec(), Val::Dict(vec![(b"DefaultRGB".to_vec(), Val::Ref(cp.st.default_rgb))]))]),
            None => Val::Dict(Vec::new()),
        };
        // the page's own tints in converted inks (before forms reuse `captured`)
        let page_captured = std::mem::take(&mut cp.captured);
        let group = match doc.get_object(pid).ok().and_then(|o| o.as_dict().ok()).and_then(|d| d.get(b"Group").ok()) {
            // PDF 1.3: left out as forms' groups are (see `dict_entries`)
            Some(_) if cp.pdf13 => None,
            Some(g) => {
                cp.transparency = true;
                Some(cp.convert(g, false))
            }
            None => None,
        };
        cp.drain();
        // The shown box in the page's user space (bounds measured from the CropBox's
        // bottom-left, y up); it maps onto the bounds right side up: x = pdf x − crop.x0,
        // y = bounds.top + (box.y1 − pdf y).
        let crop4 = crop;
        let bbox = folio_links::pdfplace::unturn(rotate, crop4, [bounds.left, bounds.top, bounds.right, bounds.bottom]);
        cp.captured = page_captured;
        let mut content = cp.content(&content).unwrap_or(content);
        if !transparent {
            let mut background = format!("q 1 g {} {} {} {} re f Q\n", bbox[0], bbox[1], bbox[2]-bbox[0], bbox[3]-bbox[1]).into_bytes();
            background.extend(content);
            content = background;
        }
        cp.captured.clear();
        let data = crate::images::flate(&content, 6);
        {
            let mut f = cp.st.chunk.form_xobject(form, &data);
            f.filter(pdf_writer::Filter::FlateDecode);
            f.bbox(pdf_writer::Rect::new(bbox[0] as f32, bbox[1] as f32, bbox[2] as f32, bbox[3] as f32));
            f.matrix(folio_links::pdfplace::form_matrix(rotate, crop4, [bounds.left, bounds.top, bounds.right, bounds.bottom]).map(|v| v as f32));
            write_obj(f.insert(Name(b"Resources")), &res_val);
            if let Some(g) = &group {
                write_obj(f.insert(Name(b"Group")), g);
            }
        }
        cp.substituted.retain(|m| !cp.st.reported.contains(m));
        cp.dropped.retain(|m| !cp.st.reported.contains(m));
        let (subs, drops) = (cp.substituted.clone(), cp.dropped.clone());
        cp.st.reported.extend(subs);
        cp.st.reported.extend(drops);
        if !cp.substituted.is_empty() {
            warnings.push(format!(
                "placed PDF uses fonts that are not embedded: embedded substitutes ({}); check the type",
                cp.substituted.join(", ")
            ));
        }
        cp.inks.retain(|m| !cp.st.reported.contains(m));
        let inks = cp.inks.clone();
        cp.st.reported.extend(inks.iter().cloned());
        for m in inks {
            warnings.push(format!("placed PDF ink: {m}"));
        }
        if !cp.dropped.is_empty() {
            warnings.push(format!("placed PDF: keys not allowed in PDF/X dropped ({})", cp.dropped.join(", ")));
        }
        cp.st.transparency |= cp.transparency;
        let transparency = cp.st.transparency;
        Ok(Imported { file, form, transparency, warnings })
    }
}
