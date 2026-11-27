//! Colour, font and transparency information from PDF objects and content streams.

use std::collections::HashSet;

use super::reader::Pdf;
use super::syntax::{Dict, Lexer, Obj, decode_name, find, get};
use crate::probe::{ColorSpace, PlacedFont, VectorColors};

/// Content bytes tokenised per file.
const MAX_CONTENT: usize = 96 << 20;

const PROCESS_NAMES: [&str; 4] = ["Cyan", "Magenta", "Yellow", "Black"];

struct Colors {
    spaces: HashSet<ColorSpace>,
    spots: HashSet<String>,
    images: HashSet<ColorSpace>,
    transparency: bool,
}

impl Colors {
    fn add(&mut self, s: ColorSpace) {
        self.spaces.insert(s);
    }
}

/// Colour space of a /ColorSpace value (name or array), recording spots.
fn color_space(pdf: &mut Pdf<'_>, o: &Obj, c: &mut Colors, depth: usize) -> Option<ColorSpace> {
    if depth > 8 {
        return None;
    }
    let o = pdf.resolve(o)?;
    match &o {
        Obj::Name(n) => match n.as_str() {
            "DeviceRGB" | "CalRGB" | "RGB" => Some(ColorSpace::Rgb),
            "DeviceCMYK" | "CMYK" => Some(ColorSpace::Cmyk),
            "DeviceGray" | "CalGray" | "G" => Some(ColorSpace::Gray),
            "Lab" => Some(ColorSpace::Lab),
            "Pattern" => None,
            _ => None,
        },
        Obj::Array(a) => {
            let kind = a.first()?.name()?;
            match kind {
                "ICCBased" => {
                    let s = pdf.resolve(a.get(1)?)?;
                    let d = s.dict()?.clone();
                    match get(&d, "N").and_then(Obj::num).map(|n| n as u32) {
                        Some(1) => Some(ColorSpace::Gray),
                        Some(3) => {
                            // Lab-based ICC profiles are rare; treat N=3 as RGB unless the
                            // alternate says Lab.
                            match get(&d, "Alternate").and_then(Obj::name) {
                                Some("Lab") => Some(ColorSpace::Lab),
                                _ => Some(ColorSpace::Rgb),
                            }
                        }
                        Some(4) => Some(ColorSpace::Cmyk),
                        _ => get(&d, "Alternate").cloned().and_then(|alt| color_space(pdf, &alt, c, depth + 1)),
                    }
                }
                "Indexed" | "I" => {
                    let base = a.get(1).cloned().and_then(|b| color_space(pdf, &b, c, depth + 1));
                    if let Some(b) = base {
                        c.add(b);
                    }
                    Some(ColorSpace::Indexed)
                }
                "Separation" => {
                    let name = a.get(1).and_then(Obj::name).unwrap_or("").to_owned();
                    if PROCESS_NAMES.contains(&name.as_str()) {
                        return Some(ColorSpace::Cmyk);
                    }
                    if name != "All" && name != "None" && !name.is_empty() {
                        c.spots.insert(name);
                    }
                    Some(ColorSpace::Other)
                }
                "DeviceN" | "NChannel" => {
                    let names = a.get(1).cloned().and_then(|n| pdf.resolve(&n));
                    let mut process = false;
                    if let Some(Obj::Array(n)) = names {
                        for x in n.iter().filter_map(Obj::name) {
                            if PROCESS_NAMES.contains(&x) {
                                process = true;
                            } else if x != "None" && x != "All" {
                                c.spots.insert(x.to_owned());
                            }
                        }
                    }
                    Some(if process { ColorSpace::Cmyk } else { ColorSpace::Other })
                }
                "Pattern" => a.get(1).cloned().and_then(|b| color_space(pdf, &b, c, depth + 1)),
                "CalRGB" => Some(ColorSpace::Rgb),
                "CalGray" => Some(ColorSpace::Gray),
                "Lab" => Some(ColorSpace::Lab),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Colour operators of a content stream (`rg`, `k`, `g`, `/DeviceRGB cs`, inline image
/// `/CS /RGB`).
fn scan_content(data: &[u8], c: &mut Colors) {
    let mut lx = Lexer { b: data, pos: 0 };
    let mut last_name: Option<String> = None;
    let named = |n: &str| match n {
        "DeviceRGB" | "RGB" => Some(ColorSpace::Rgb),
        "DeviceCMYK" | "CMYK" => Some(ColorSpace::Cmyk),
        "DeviceGray" | "G" => Some(ColorSpace::Gray),
        _ => None,
    };
    loop {
        lx.skip_ws();
        let Some(&ch) = data.get(lx.pos) else { break };
        let mut name = None;
        match ch {
            b'(' => {
                lx.literal();
            }
            b'<' if data.get(lx.pos + 1) == Some(&b'<') => lx.pos += 2,
            b'>' if data.get(lx.pos + 1) == Some(&b'>') => lx.pos += 2,
            b'<' => {
                while lx.pos < data.len() && data[lx.pos] != b'>' {
                    lx.pos += 1;
                }
                lx.pos += 1;
            }
            b'[' | b']' | b'{' | b'}' | b')' | b'>' => lx.pos += 1,
            b'/' => {
                lx.pos += 1;
                let n = decode_name(lx.word());
                // inline image: /CS /RGB
                if matches!(last_name.as_deref(), Some("CS" | "ColorSpace"))
                    && let Some(s) = named(&n)
                {
                    c.add(s);
                }
                name = Some(n);
            }
            _ => {
                let w = lx.word();
                if w.is_empty() {
                    lx.pos += 1;
                } else {
                    match w {
                        b"rg" | b"RG" => c.add(ColorSpace::Rgb),
                        b"k" | b"K" => c.add(ColorSpace::Cmyk),
                        b"g" | b"G" => c.add(ColorSpace::Gray),
                        b"cs" | b"CS" => {
                            if let Some(s) = last_name.as_deref().and_then(named) {
                                c.add(s);
                            }
                        }
                        b"ID" => match find(data, b"EI", lx.pos) {
                            // inline image data: skip to EI
                            Some(p) => lx.pos = p + 2,
                            None => break,
                        },
                        _ => {}
                    }
                }
            }
        }
        last_name = name;
    }
}

pub(super) fn inspect(
    pdf: &mut Pdf<'_>,
    contents: &[Obj],
    page_transparency: bool,
    output_intent: Option<String>,
) -> (VectorColors, Vec<PlacedFont>) {
    let mut colors = Colors {
        spaces: HashSet::new(),
        spots: HashSet::new(),
        images: HashSet::new(),
        transparency: page_transparency,
    };
    // Colour spaces of every object (resources, images, shadings), bounded.
    let mut content_budget = MAX_CONTENT;
    let mut content_ok = !pdf.is_encrypted();
    let mut form_streams: Vec<(Dict, usize, usize)> = Vec::new();
    let mut fonts: Vec<PlacedFont> = Vec::new();
    for n in pdf.object_numbers().into_iter().take(500_000) {
        let Some(o) = pdf.get_obj(n) else { continue };
        let Some(d) = o.dict().cloned() else { continue };
        if get(&d, "Type").and_then(Obj::name) == Some("Font")
            && let Some(f) = placed_font(pdf, &d)
            && !fonts.contains(&f)
        {
            fonts.push(f);
        }
        examine(pdf, &d, &mut colors, 0);
        if let Obj::Stream(sd, s, e) = &o
            && (get(sd, "Subtype").and_then(Obj::name) == Some("Form") || get(sd, "PatternType").and_then(Obj::num) == Some(1.0))
        {
            form_streams.push((sd.clone(), *s, *e));
        }
    }
    // Content streams of pages and forms.
    let mut streams: Vec<(Dict, usize, usize)> = form_streams;
    for c in contents {
        let items = match pdf.resolve(c) {
            Some(Obj::Array(a)) => a,
            Some(other) => vec![other],
            None => Vec::new(),
        };
        for it in items {
            if let Some(Obj::Stream(d, s, e)) = pdf.resolve(&it) {
                streams.push((d, s, e));
            }
        }
    }
    for (d, s, e) in streams {
        if content_budget == 0 {
            content_ok = false;
            break;
        }
        match pdf.decode(&d, s, e) {
            Some(data) => {
                content_budget = content_budget.saturating_sub(data.len());
                scan_content(&data, &mut colors);
            }
            None => content_ok = false,
        }
    }
    let mut spaces: Vec<ColorSpace> = colors.spaces.into_iter().collect();
    spaces.sort_by_key(|s| *s as u8);
    let mut image_spaces: Vec<ColorSpace> = colors.images.into_iter().collect();
    image_spaces.sort_by_key(|s| *s as u8);
    let mut spots: Vec<String> = colors.spots.into_iter().collect();
    spots.sort();
    fonts.sort_by(|a, b| a.name.cmp(&b.name));
    let vector_colors = VectorColors {
        spaces,
        spots,
        image_spaces,
        content_scanned: content_ok,
        output_intent,
        transparency: colors.transparency,
    };
    (vector_colors, fonts)
}

/// Colour spaces and transparency of one dictionary, and of the dictionaries written
/// inline in it (a page's `Resources`, their `ExtGState` entries, a `Group`); referenced
/// objects are examined on their own.
fn examine(pdf: &mut Pdf<'_>, d: &Dict, colors: &mut Colors, depth: usize) {
    if depth > 4 {
        return;
    }
    if let Some(cs) = get(d, "ColorSpace").cloned() {
        match pdf.resolve(&cs) {
            // resource dictionary of named colour spaces
            Some(Obj::Dict(named)) => {
                for (_, v) in named {
                    if let Some(s) = color_space(pdf, &v, colors, 0) {
                        colors.add(s);
                    }
                }
            }
            Some(v) => {
                let s = color_space(pdf, &v, colors, 0);
                if get(d, "Subtype").and_then(Obj::name) == Some("Image") {
                    if let Some(s) = s {
                        colors.images.insert(s);
                    }
                } else if let Some(s) = s {
                    colors.add(s);
                }
            }
            None => {}
        }
    }
    if get(d, "SMask").is_some_and(|m| m.name() != Some("None")) || get(d, "S").and_then(Obj::name) == Some("Transparency") {
        colors.transparency = true;
    }
    for k in ["CA", "ca"] {
        if get(d, k).and_then(Obj::num).is_some_and(|v| v < 1.0) {
            colors.transparency = true;
        }
    }
    for k in ["Resources", "ExtGState", "Group", "Shading", "Pattern"] {
        if let Some(Obj::Dict(inner)) = get(d, k) {
            let inner = inner.clone();
            examine(pdf, &inner, colors, depth + 1);
            // dictionaries of named resources (ExtGState << /GS0 << … >> >>)
            for (_, v) in &inner {
                if let Obj::Dict(x) = v {
                    examine(pdf, &x.clone(), colors, depth + 2);
                }
            }
        }
    }
}

/// A font dictionary: name, subtype and whether its program is in the file. CID fonts
/// are reached through their Type0 parent; Type3 glyphs are always in the file.
fn placed_font(pdf: &mut Pdf<'_>, d: &Dict) -> Option<PlacedFont> {
    let kind = get(d, "Subtype").and_then(Obj::name)?.to_owned();
    if kind.starts_with("CIDFontType") {
        return None;
    }
    let raw = get(d, "BaseFont").and_then(Obj::name).unwrap_or(if kind == "Type3" { "Type3" } else { "" });
    let name = match raw.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|c| c.is_ascii_uppercase()) => rest.to_owned(),
        _ => raw.to_owned(),
    };
    let descriptor_of = |pdf: &mut Pdf<'_>, f: &Dict| -> Option<Dict> {
        let fd = get(f, "FontDescriptor").cloned()?;
        pdf.resolve(&fd).and_then(|o| o.dict().cloned())
    };
    let embedded = if kind == "Type3" {
        true
    } else {
        let desc = if kind == "Type0" {
            let desc_fonts = get(d, "DescendantFonts").cloned().and_then(|x| pdf.resolve(&x));
            match desc_fonts {
                Some(Obj::Array(a)) => a.first().cloned().and_then(|f| pdf.resolve(&f)).and_then(|f| f.dict().cloned()).and_then(|f| descriptor_of(pdf, &f)),
                _ => None,
            }
        } else {
            descriptor_of(pdf, d)
        };
        desc.is_some_and(|fd| ["FontFile", "FontFile2", "FontFile3"].iter().any(|k| get(&fd, k).is_some()))
    };
    Some(PlacedFont { name, kind, embedded })
}

/// Select the display colour space from the vector and image colours in priority order.
pub(super) fn dominant(spaces: &[ColorSpace], images: &[ColorSpace]) -> ColorSpace {
    let all: Vec<ColorSpace> = spaces.iter().chain(images).copied().collect();
    for s in [ColorSpace::Rgb, ColorSpace::Lab, ColorSpace::Cmyk, ColorSpace::Gray, ColorSpace::Indexed] {
        if all.contains(&s) {
            return s;
        }
    }
    ColorSpace::Other
}
