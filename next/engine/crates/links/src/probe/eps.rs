//! EPS (and PostScript Illustrator files): DSC comments (bounding box, process and custom
//! colours), Photoshop EPS image data and resources, the DOS binary header.

use super::{ColorSpace, ImageFormat, ImageInfo, Result, VectorColors, le32, malformed, photoshop_resources, read_at};

/// PostScript scanned for colour operators (bytes).
const SCAN: usize = 8 << 20;

pub(crate) fn probe(head: &[u8], file: &mut std::fs::File, len: u64) -> Result<ImageInfo> {
    let (ps_start, ps_len) = if head.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        (le32(head, 4).unwrap_or(0) as u64, le32(head, 8).unwrap_or(0) as u64)
    } else {
        (0, len)
    };
    if ps_len == 0 || ps_start >= len {
        return malformed("EPS PostScript section");
    }
    let take = ps_len.min(SCAN as u64) as usize;
    let ps = read_at(file, ps_start, take)?;
    let complete = ps_len as usize <= SCAN;
    let text = String::from_utf8_lossy(&ps);

    let mut bbox: Option<[f64; 4]> = None;
    let mut hires: Option<[f64; 4]> = None;
    let mut process: Vec<String> = Vec::new();
    let mut custom: Vec<String> = Vec::new();
    let mut image_data: Option<(u32, u32, u8, u32)> = None;
    let mut ps_res = ImageInfo::raster(ImageFormat::Eps, 0, 0, ColorSpace::Other, 0, 0);
    // (supplied, value) of the DSC font comments
    let mut fonts_dsc: Vec<(bool, String)> = Vec::new();
    let mut res_dsc: Vec<(bool, String)> = Vec::new();
    let mut last_key = "";
    let mut lines = text.lines().take(200_000);
    while let Some(line) = lines.next() {
        let line = line.trim_end_matches('\r');
        let (key, value) = if let Some(rest) = line.strip_prefix("%%+") {
            (last_key, rest)
        } else if let Some(rest) = line.strip_prefix("%%") {
            let (k, v) = rest.split_once(':').unwrap_or((rest, ""));
            last_key = match k {
                "DocumentProcessColors" => "DocumentProcessColors",
                "DocumentCustomColors" => "DocumentCustomColors",
                "CMYKCustomColor" => "CMYKCustomColor",
                "RGBCustomColor" => "RGBCustomColor",
                "DocumentNeededResources" => "DocumentNeededResources",
                "DocumentSuppliedResources" => "DocumentSuppliedResources",
                "DocumentNeededFonts" => "DocumentNeededFonts",
                "DocumentSuppliedFonts" => "DocumentSuppliedFonts",
                _ => "",
            };
            (k, v)
        } else if let Some(rest) = line.strip_prefix("%ImageData:") {
            let n: Vec<&str> = rest.split_whitespace().collect();
            if n.len() >= 4 {
                let p = |i: usize| n[i].parse::<u32>().unwrap_or(0);
                image_data = Some((p(0), p(1), p(2) as u8, p(3)));
            }
            continue;
        } else {
            continue;
        };
        let nums = |v: &str| -> Option<[f64; 4]> {
            let n: Vec<f64> = v.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            (n.len() == 4).then(|| [n[0], n[1], n[2], n[3]])
        };
        match key {
            "BoundingBox" if !value.contains("atend") => bbox = bbox.or(nums(value)),
            "HiResBoundingBox" => hires = hires.or(nums(value)),
            "DocumentProcessColors" => process.extend(value.split_whitespace().map(str::to_owned)),
            "DocumentCustomColors" => custom.extend(paren_names(value)),
            "CMYKCustomColor" | "RGBCustomColor" => custom.extend(paren_names(value)),
            // "font A B" (resources) or plain names (fonts)
            "DocumentNeededResources" | "DocumentNeededFonts" | "DocumentSuppliedResources" | "DocumentSuppliedFonts" => {
                let list = if key.ends_with("Fonts") { &mut fonts_dsc } else { &mut res_dsc };
                let supplied = key.contains("Supplied");
                list.push((supplied, value.to_owned()));
            }
            "BeginPhotoshop" => {
                // hex lines "% 3842494D…" follow
                let want: usize = value.trim().parse().unwrap_or(0);
                let mut hex = Vec::with_capacity(want);
                for l in lines.by_ref() {
                    if l.starts_with("%EndPhotoshop") || hex.len() >= want {
                        break;
                    }
                    let h = l.trim_start_matches('%').trim();
                    let b = h.as_bytes();
                    for pair in b.chunks(2) {
                        if pair.len() == 2
                            && let (Some(a), Some(c)) = ((pair[0] as char).to_digit(16), (pair[1] as char).to_digit(16))
                        {
                            hex.push((a * 16 + c) as u8);
                        }
                    }
                }
                photoshop_resources(&hex, &mut ps_res);
            }
            _ => {}
        }
    }
    // Fonts: needed but not supplied are not in the file.
    let mut needed: Vec<String> = Vec::new();
    let mut supplied: Vec<String> = Vec::new();
    for (sup, v) in fonts_dsc {
        let names = v.split_whitespace().filter(|n| *n != "(atend)").map(str::to_owned);
        if sup { supplied.extend(names) } else { needed.extend(names) }
    }
    for (sup, v) in res_dsc {
        let mut it = v.split_whitespace();
        while let Some(w) = it.next() {
            if w == "font" {
                for n in it.by_ref() {
                    if matches!(n, "procset" | "file" | "form" | "pattern" | "encoding") {
                        break;
                    }
                    if sup { supplied.push(n.to_owned()) } else { needed.push(n.to_owned()) }
                }
            }
        }
    }
    needed.sort();
    needed.dedup();
    let eps_fonts: Vec<super::PlacedFont> =
        needed.into_iter().map(|n| super::PlacedFont { embedded: supplied.contains(&n), name: n, kind: "PostScript".into() }).collect();
    let bb = hires.or(bbox);
    let (bw, bh) = bb.map(|b| (b[2] - b[0], b[3] - b[1])).unwrap_or((0.0, 0.0));
    if let Some((w, h, depth, mode)) = image_data.filter(|d| d.0 > 0 && d.1 > 0) {
        let (space, ch) = match (mode, depth) {
            (_, 1) => (ColorSpace::Gray, 1),
            (1, _) => (ColorSpace::Gray, 1),
            (2, _) => (ColorSpace::Lab, 3),
            (3, _) => (ColorSpace::Rgb, 3),
            (4, _) => (ColorSpace::Cmyk, 4),
            _ => (ColorSpace::Other, 1),
        };
        let mut info = ImageInfo::raster(ImageFormat::Eps, w, h, space, depth.max(1), ch);
        info.icc = ps_res.icc;
        info.ppi = ps_res.ppi.or_else(|| (bw > 0.0 && bh > 0.0).then(|| (w as f64 * 72.0 / bw, h as f64 * 72.0 / bh)));
        info.fonts = eps_fonts;
        return Ok(info);
    }
    if bb.is_none() {
        return malformed("EPS without BoundingBox");
    }
    // DSC colour comments are what the producer declares; operator names alone also appear
    // in procsets that are never called, so they are used only when no comment exists.
    let declared = !process.is_empty() || !custom.is_empty();
    let mut colors = VectorColors { content_scanned: !declared && complete, ..VectorColors::default() };
    if declared {
        if process.iter().any(|p| matches!(p.as_str(), "Cyan" | "Magenta" | "Yellow")) {
            colors.spaces.push(ColorSpace::Cmyk);
        } else if process.iter().any(|p| p == "Black") {
            colors.spaces.push(ColorSpace::Gray);
        }
    } else {
        let has = |s: &str| text.contains(s);
        if has("setcmykcolor") || has("/DeviceCMYK") {
            colors.spaces.push(ColorSpace::Cmyk);
        }
        if has("setrgbcolor") || has("/DeviceRGB") {
            colors.spaces.push(ColorSpace::Rgb);
        }
        if has("setgray") || has("/DeviceGray") {
            colors.spaces.push(ColorSpace::Gray);
        }
    }
    custom.sort();
    custom.dedup();
    colors.spots = custom;
    let mut info = ImageInfo::raster(ImageFormat::Eps, bw.max(0.0).ceil() as u32, bh.max(0.0).ceil() as u32, colors.spaces.first().copied().unwrap_or(ColorSpace::Other), 0, 0);
    info.vector = true;
    info.vector_colors = Some(colors);
    info.fonts = eps_fonts;
    Ok(info)
}

/// `(PANTONE 280 C) (Other)` → names.
fn paren_names(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = v;
    while let Some(s) = rest.find('(') {
        let Some(e) = rest[s + 1..].find(')') else { break };
        let name = rest[s + 1..s + 1 + e].trim();
        if !name.is_empty() {
            out.push(name.to_owned());
        }
        rest = &rest[s + 2 + e..];
    }
    if out.is_empty() {
        // unparenthesised list (some writers)
        out.extend(v.split_whitespace().filter(|s| *s != "(atend)").map(str::to_owned));
    }
    out
}
