//! The screen preview an EPS file carries (build-a). There is no PostScript interpreter with a licence Folio can ship (Ghostscript is
//! AGPL), so an EPS is shown through the picture its writer stored for that purpose:
//! - the TIFF of a DOS binary EPS (header `C5 D0 D3 C6`: PostScript, WMF and TIFF sections), the usual preview of Windows files;
//! - an EPSI preview (`%%BeginPreview: width height depth lines`, hex lines in comments; 1 is black);
//! - a Photoshop EPS's JPEG thumbnail (image resource 1036, `%BeginPhotoshop`).
//!
//! A file with none of them is shown as a grey box with a cross, the size of its bounding box. The print output uses the same preview,
//! and preflight says so (EPS_PREVIEW); the IDML keeps the EPS link, so InDesign prints the PostScript itself.
use std::path::Path;

/// The preview as RGBA (width, height, pixels), or `None` when the file carries none.
pub fn preview(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        let le = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let (start, len) = (le(20)?, le(24)?);
        if len > 0
            && let Some(tiff) = bytes.get(start..start.checked_add(len)?)
            && let Some((w, h, rgba, _)) = crate::decode_rgba(tiff, &folio_color::Colors::unmanaged(), true)
        {
            return Some((w, h, rgba));
        }
        let (ps, ps_len) = (le(4)?, le(8)?);
        return bytes.get(ps..ps.checked_add(ps_len)?).and_then(text_preview);
    }
    text_preview(bytes)
}

/// The EPSI preview, else the Photoshop thumbnail, of the PostScript text.
fn text_preview(ps: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let head = &ps[..ps.len().min(16 << 20)];
    epsi(head).or_else(|| photoshop_thumbnail(head))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn epsi(ps: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let at = find(ps, b"%%BeginPreview:")?;
    let rest = &ps[at + 15..];
    let line_end = rest.iter().position(|b| *b == b'\n' || *b == b'\r')?;
    let nums: Vec<usize> = std::str::from_utf8(&rest[..line_end]).ok()?.split_whitespace().filter_map(|v| v.parse().ok()).collect();
    let (w, h, depth) = (*nums.first()?, *nums.get(1)?, *nums.get(2)?);
    if w == 0 || h == 0 || !matches!(depth, 1 | 2 | 4 | 8) || w.checked_mul(h)? > 1 << 26 {
        return None;
    }
    let end = find(rest, b"%%EndPreview").unwrap_or(rest.len());
    let hex: Vec<u8> = rest[line_end..end].split(|b| *b == b'\n' || *b == b'\r').filter(|l| l.first() == Some(&b'%')).flat_map(|l| l[1..].iter().copied()).filter(u8::is_ascii_hexdigit).collect();
    let data: Vec<u8> = hex.chunks_exact(2).filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok()).collect();
    let row = (w * depth).div_ceil(8);
    if data.len() < row * h {
        return None;
    }
    let max = (1u32 << depth) - 1;
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let line = &data[y * row..(y + 1) * row];
        for x in 0..w {
            let bit = x * depth;
            let v = (line[bit / 8] >> (8 - depth - bit % 8)) as u32 & max;
            let g = 255 - (v * 255 / max) as u8;
            out.extend_from_slice(&[g, g, g, 255]);
        }
    }
    Some((w as u32, h as u32, out))
}

/// Image resource 1036 (a JFIF thumbnail after a 28-byte header) in the hex lines of `%BeginPhotoshop`.
fn photoshop_thumbnail(ps: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let at = find(ps, b"%BeginPhotoshop:")?;
    let rest = &ps[at..];
    let end = find(rest, b"%EndPhotoshop").unwrap_or(rest.len());
    let hex: Vec<u8> = rest[..end].split(|b| *b == b'\n' || *b == b'\r').skip(1).filter(|l| l.first() == Some(&b'%')).flat_map(|l| l[1..].iter().copied()).filter(u8::is_ascii_hexdigit).collect();
    let res: Vec<u8> = hex.chunks_exact(2).filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok()).collect();
    let mut b = &res[..];
    while b.len() >= 12 && &b[..4] == b"8BIM" {
        let id = u16::from_be_bytes([b[4], b[5]]);
        let mut o = 7 + b[6] as usize;
        o += o % 2;
        let size = u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize;
        let data = b.get(o + 4..o + 4 + size)?;
        if id == 1036 && data.len() > 28 {
            let img = image::load_from_memory_with_format(&data[28..], image::ImageFormat::Jpeg).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            return Some((w, h, img.into_raw()));
        }
        b = b.get(o + 4 + size + size % 2..).unwrap_or(&[]);
    }
    None
}

/// A grey box with a cross for an EPS with no preview, `size` (points) at 72 dpi (at least 8 pixels a side).
pub fn placeholder(size: [f64; 2]) -> (u32, u32, Vec<u8>) {
    let w = size[0].round().clamp(8.0, 2000.0) as u32;
    let h = size[1].round().clamp(8.0, 2000.0) as u32;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64 / (w - 1).max(1) as f64, y as f64 / (h - 1).max(1) as f64);
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            let cross = (fx - fy).abs() * (w.max(h) as f64) < 1.0 || (fx + fy - 1.0).abs() * (w.max(h) as f64) < 1.0;
            let g = if edge || cross { 128 } else { 214 };
            out.extend_from_slice(&[g, g, g, 255]);
        }
    }
    (w, h, out)
}

/// A Photoshop EPS's own picture, what it prints from (build-a2): `%ImageData: columns rows depth mode pad-channels block
/// binary/ascii "start"` and the samples after the start line — binary or hexadecimal, channels interleaved (block 1) or one
/// row per channel, 8-bit grayscale, Lab, RGB or CMYK (1-bit as grayscale), or a JPEG stream (encodings 3–6). Returned as a file
/// the image readers take: a TIFF of the samples (CMYK stays CMYK; Lab becomes sRGB as a CIELab TIFF is shown), or the JPEG.
pub fn photoshop_body(ps: &[u8]) -> Option<(&'static str, Vec<u8>)> {
    let at = find(&ps[..ps.len().min(1 << 20)], b"%ImageData:")?;
    let rest = &ps[at + 11..];
    let line_end = rest.iter().position(|b| *b == b'\n' || *b == b'\r')?;
    let line = std::str::from_utf8(&rest[..line_end]).ok()?;
    let (nums, start) = line.split_once('"').map_or((line, None), |(n, s)| (n, s.split('"').next()));
    let v: Vec<usize> = nums.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let (w, h, depth, mode, pad, block, kind) = (*v.first()?, *v.get(1)?, *v.get(2)?, *v.get(3)?, *v.get(4).unwrap_or(&0), *v.get(5).unwrap_or(&1), *v.get(6).unwrap_or(&1));
    if w == 0 || h == 0 || w.checked_mul(h)? > 1 << 28 {
        return None;
    }
    let colors = match (mode, depth) {
        (_, 1) | (1, _) => 1,
        (2 | 3, 8) => 3,
        (4, 8) => 4,
        _ => return None,
    };
    // the samples begin on the line after the start string (Photoshop: "beginimage"), else after the comment
    let after = &rest[line_end..];
    let data = match start.filter(|s| !s.is_empty()).and_then(|s| find(after, s.as_bytes()).map(|k| k + s.len())) {
        Some(k) => &after[k..],
        None => after,
    };
    let data = match data {
        [b'\r', b'\n', r @ ..] | [b'\n', r @ ..] | [b'\r', r @ ..] => r,
        d => d,
    };
    if (3..=6).contains(&kind) {
        let soi = find(&data[..data.len().min(64)], &[0xFF, 0xD8])?;
        let eoi = find(&data[soi..], &[0xFF, 0xD9]).map_or(data.len(), |e| soi + e + 2);
        return Some(("jpg", data[soi..eoi].to_vec()));
    }
    let channels = colors + pad;
    let row = (w * depth).div_ceil(8);
    let need = row * h * channels;
    let raw: Vec<u8> = if kind == 2 {
        let mut out = Vec::with_capacity(need);
        let mut hi: Option<u8> = None;
        for &c in data {
            let d = match c { b'0'..=b'9' => c - b'0', b'a'..=b'f' => c - b'a' + 10, b'A'..=b'F' => c - b'A' + 10, b'%' | b'>' => break, _ => continue };
            match hi.take() {
                Some(x) => out.push(x << 4 | d),
                None => hi = Some(d),
            }
            if out.len() == need {
                break;
            }
        }
        out
    } else {
        data.get(..need)?.to_vec()
    };
    if raw.len() < need {
        return None;
    }
    // the colour samples of each pixel, interleaved (block 1) or row by row per channel
    let mut px = vec![0u8; w * h * colors];
    for y in 0..h {
        for c in 0..colors {
            for x in 0..w {
                let v = if depth == 1 {
                    let line = &raw[(y * channels + c) * row..];
                    if (line[x / 8] >> (7 - x % 8)) & 1 == 1 { 255 } else { 0 }
                } else if block <= 1 {
                    raw[(y * w + x) * channels + c]
                } else {
                    raw[(y * channels + c) * row + x]
                };
                px[(y * w + x) * colors + c] = v;
            }
        }
    }
    // the PostScript decodes CMYK as Photoshop stores it (255 − ink) when its image dictionary says so
    let head = &ps[..at.min(ps.len())];
    let inverted = mode == 4 && (find(&ps[at..(at + 4096).min(ps.len())], b"[1 0 1 0 1 0 1 0]").is_some() || find(head, b"/Decode [1 0 1 0 1 0 1 0]").is_some());
    if inverted {
        px.iter_mut().for_each(|v| *v = 255 - *v);
    }
    let (photometric, spp) = match colors {
        1 => (1u16, 1u16),
        4 => (5, 4),
        _ => (2, 3),
    };
    if mode == 2 && colors == 3 {
        for p in px.chunks_exact_mut(3) {
            let rgb = crate::lab8_to_srgb(p[0], p[1] ^ 0x80, p[2] ^ 0x80);
            p.copy_from_slice(&rgb);
        }
    }
    Some(("tif", tiff(w as u32, h as u32, photometric, spp, &px)))
}

/// A baseline uncompressed little-endian TIFF (one strip): gray (photometric 1), RGB (2) or CMYK (5).
fn tiff(w: u32, h: u32, photometric: u16, spp: u16, data: &[u8]) -> Vec<u8> {
    let mut entries: Vec<(u16, u16, u32, u32)> = vec![(256, 4, 1, w), (257, 4, 1, h), (258, 3, u32::from(spp), 0), (259, 3, 1, 1), (262, 3, 1, u32::from(photometric)), (273, 4, 1, 0), (277, 3, 1, u32::from(spp)), (278, 4, 1, h), (279, 4, 1, data.len() as u32), (284, 3, 1, 1)];
    if photometric == 5 {
        entries.push((332, 3, 1, 1));
    }
    let ifd_len = 2 + entries.len() * 12 + 4;
    let bits_at = 8 + ifd_len as u32;
    let data_at = bits_at + 2 * u32::from(spp);
    let mut out = b"II*\0".to_vec();
    out.extend(8u32.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        let value = match tag {
            258 if spp == 1 => 8,
            258 => bits_at,
            273 => data_at,
            _ => value,
        };
        if kind == 3 && count == 1 { out.extend((value as u16).to_le_bytes()); out.extend([0, 0]); } else { out.extend(value.to_le_bytes()); }
    }
    out.extend(0u32.to_le_bytes());
    for _ in 0..spp {
        out.extend(8u16.to_le_bytes());
    }
    out.extend_from_slice(data);
    out
}

/// The picture an EPS is shown and printed from: a Photoshop EPS's own picture (extension `tif` or `jpg`, `true`), else its
/// preview as PNG (`true`: it carried one; `false`: the placeholder of `size` points).
pub fn picture_file(bytes: &[u8], size: [f64; 2]) -> Result<(&'static str, Vec<u8>, bool), String> {
    let ps = if bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        let le = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        le(4).zip(le(8)).and_then(|(at, n)| bytes.get(at..at.checked_add(n)?)).unwrap_or(&[])
    } else {
        bytes
    };
    if let Some((ext, data)) = photoshop_body(ps) {
        return Ok((ext, data, true));
    }
    let (own, (w, h, rgba)) = match preview(bytes) {
        Some(p) => (true, p),
        None => (false, placeholder(size)),
    };
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("EPS preview size")?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(("png", png, own))
}

/// Writes the preview of the EPS `path` as a PNG (the placeholder of `size` points when the file has none); says whether the file
/// carried a preview of its own.
pub fn write_preview_png(path: &Path, size: [f64; 2], out: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (own, (w, h, rgba)) = match preview(&bytes) {
        Some(p) => (true, p),
        None => (false, placeholder(size)),
    };
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("EPS preview size")?;
    img.save_with_format(out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(own)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DOS binary EPS: the header, then the PostScript, then the TIFF preview.
    fn dos_eps(ps: &[u8], tiff: &[u8]) -> Vec<u8> {
        let mut out = vec![0xC5, 0xD0, 0xD3, 0xC6];
        let ps_at = 30u32;
        let tiff_at = ps_at + ps.len() as u32;
        for v in [ps_at, ps.len() as u32, 0, 0, tiff_at, tiff.len() as u32] {
            out.extend(v.to_le_bytes());
        }
        out.extend([0xFF, 0xFF]);
        out.extend_from_slice(ps);
        out.extend_from_slice(tiff);
        out
    }

    #[test]
    fn a_dos_eps_shows_its_tiff_and_an_epsi_its_hex_preview() {
        let tiff = crate::test_tiff(2, 2, 3, 8, &[255, 0, 0, 0, 0, 255]);
        let (w, h, px) = preview(&dos_eps(b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 2 1\n", &tiff)).unwrap();
        assert_eq!((w, h, &px[..8]), (2, 1, &[255, 0, 0, 255, 0, 0, 255, 255][..]));
        // 1 bit: 1 is black; rows padded to bytes
        let epsi = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 3 2\n%%BeginPreview: 3 2 1 2\n%A0\n%40\n%%EndPreview\nshowpage\n";
        let (w, h, px) = preview(epsi).unwrap();
        assert_eq!((w, h), (3, 2));
        let gray: Vec<u8> = px.chunks_exact(4).map(|p| p[0]).collect();
        assert_eq!(gray, [0, 255, 0, 255, 0, 255]);
        assert!(preview(b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 144 96\n0 0 1 setrgbcolor\n").is_none());
        let (w, h, px) = placeholder([144.0, 96.0]);
        assert_eq!((w, h, px.len()), (144, 96, 144 * 96 * 4));
    }

    #[test]
    fn a_photoshop_eps_is_read_from_its_own_picture() {
        // 2 x 1 CMYK, binary, interleaved: cyan, black
        let mut eps = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 2 1\n%ImageData: 2 1 8 4 0 4 1 \"beginimage\"\nbeginimage\n".to_vec();
        eps.extend([255, 0, 0, 0, 0, 0, 0, 255]);
        eps.extend(b"\n%%EOF\n");
        let (ext, tif, own) = picture_file(&eps, [2.0, 1.0]).unwrap();
        assert_eq!((ext, own), ("tif", true));
        assert_eq!(crate::tiff_photometric(&tif), Some(5), "CMYK stays CMYK");
        let s = crate::tiff_samples(&tif, true).unwrap();
        assert_eq!(&s.src[..8], &[255, 0, 0, 0, 0, 0, 0, 255]);
        // RGB, hexadecimal, a row per channel (block = columns)
        let eps = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 2 1\n%ImageData: 2 1 8 3 0 2 2 \"beginimage\"\nbeginimage\nFF00 00FF\n0000\n%%EOF\n";
        let (ext, tif, _) = picture_file(eps, [2.0, 1.0]).unwrap();
        assert_eq!(ext, "tif");
        let (w, h, rgba, _) = crate::decode_rgba(&tif, &folio_color::Colors::unmanaged(), true).unwrap();
        assert_eq!((w, h, &rgba[..8]), (2, 1, &[255, 0, 0, 255, 0, 255, 0, 255][..]));
        // without %ImageData: the preview (here none: the placeholder)
        let (ext, _, own) = picture_file(b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 10 10\n", [10.0, 10.0]).unwrap();
        assert_eq!((ext, own), ("png", false));
    }
}
