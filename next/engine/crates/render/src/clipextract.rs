//! Clipping paths made from a placed image (Object ▸ Clipping Path ▸ Options): Detect Edges,
//! Alpha Channel and Photoshop Path. The result is the `ClippingPathGeometry` the graphic keeps,
//! in the graphic's own coordinates (its `GraphicBounds`).
//!
//! Measured against InDesign 2025 (`artifacts/build-b-20261023/clip/`): a pixel is kept when its
//! darkness (255 − lightness, on white) or its alpha is above the threshold; Tolerance 0 gives the
//! pixel outline as straight lines, a larger tolerance smooth curves that stay within it (in pixels);
//! Inset Frame shrinks (or with a negative value grows) the kept area by that many points; Include
//! Inside Edges adds the holes; Restrict to Frame keeps what is inside the frame; Invert keeps the
//! shape and puts the image bounds before it (the frame's `InvertPath` turns it inside out).
//! A Photoshop path is the file's own path (knots in fractions of the image; the point before an
//! anchor is its left direction).

mod trace;

/// One closed (or open) subpath: (left direction, anchor, right direction) per point.
#[derive(Debug, Clone, PartialEq)]
pub struct Subpath {
    pub points: Vec<[(f64, f64); 3]>,
    pub open: bool,
}

/// What to extract. `kind`: DetectEdges, AlphaChannel or PhotoshopPath.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub kind: String,
    pub threshold: f64,
    pub tolerance: f64,
    /// Inset Frame, in points of the graphic (negative grows).
    pub inset: f64,
    pub inside_edges: bool,
    pub invert: bool,
    /// The alpha channel's or the Photoshop path's name (the first one when empty or unknown).
    pub path_name: String,
    /// The alpha channel's position, when it has no name to find it by.
    pub index: i32,
    /// Restrict to Frame: the frame's bounds in graphic coordinates (left, top, right, bottom).
    pub frame: Option<[f64; 4]>,
}

/// The alpha channels and Photoshop paths an image offers, by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sources {
    pub alpha_channels: Vec<String>,
    pub paths: Vec<String>,
}

fn be16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn be32(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// The Photoshop image resources of a PSD, a JPEG (APP13) or a TIFF (tag 34377).
fn resources(b: &[u8]) -> Vec<u8> {
    if b.starts_with(b"8BPS") {
        return crate::psd::planes(b).map(|p| p.resources.to_vec()).unwrap_or_default();
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let (mut out, mut at) = (Vec::new(), 2);
        while at + 4 <= b.len() && b[at] == 0xFF {
            let marker = b[at + 1];
            if marker == 0xDA || marker == 0xD9 { break; }
            let Some(len) = be16(b, at + 2) else { break };
            let seg = b.get(at + 4..at + 2 + len).unwrap_or_default();
            if marker == 0xED && seg.starts_with(b"Photoshop 3.0\0") {
                out.extend_from_slice(&seg[14..]);
            }
            at += 2 + len;
        }
        return out;
    }
    let little = b.starts_with(b"II*\0");
    if !little && !b.starts_with(b"MM\0*") { return Vec::new(); }
    let r16 = |at: usize| b.get(at..at + 2).map(|s| if little { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) } as usize);
    let r32 = |at: usize| b.get(at..at + 4).map(|s| if little { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) } else { u32::from_be_bytes([s[0], s[1], s[2], s[3]]) } as usize);
    let Some(ifd) = r32(4) else { return Vec::new() };
    let n = r16(ifd).unwrap_or(0);
    for k in 0..n {
        let e = ifd + 2 + k * 12;
        if r16(e) == Some(34377)
            && let (Some(count), Some(off)) = (r32(e + 4), r32(e + 8))
        {
            return b.get(off..off + count).map(<[u8]>::to_vec).unwrap_or_default();
        }
    }
    Vec::new()
}

/// Each resource block: (id, name, data).
fn blocks(mut b: &[u8]) -> Vec<(usize, Vec<u8>, &[u8])> {
    let mut out = Vec::new();
    while b.len() >= 12 && &b[..4] == b"8BIM" {
        let Some(id) = be16(b, 4) else { break };
        let nl = b[6] as usize;
        let name = b.get(7..7 + nl).unwrap_or_default().to_vec();
        let o = 6 + ((nl + 2) & !1);
        let Some(size) = be32(b, o) else { break };
        let Some(data) = b.get(o + 4..o + 4 + size) else { break };
        out.push((id, name, data));
        let next = o + 4 + size + (size & 1);
        if next > b.len() { break; }
        b = &b[next..];
    }
    out
}

fn text(name: &[u8]) -> String {
    // Photoshop writes Pascal names in the system code page; ASCII and UTF-8 read as they are
    String::from_utf8_lossy(name).into_owned()
}

/// The names of the extra (alpha) channels: the Unicode names (1045), else the Pascal ones (1006).
fn alpha_names(res: &[u8]) -> Vec<String> {
    let blocks = blocks(res);
    if let Some((_, _, d)) = blocks.iter().find(|(id, _, _)| *id == 1045) {
        let (mut out, mut at) = (Vec::new(), 0);
        while let Some(n) = be32(d, at) {
            let units: Option<Vec<u16>> = (0..n).map(|k| be16(d, at + 4 + k * 2).map(|u| u as u16)).collect();
            let Some(units) = units else { break };
            out.push(String::from_utf16_lossy(&units).trim_end_matches('\0').to_owned());
            at += 4 + n * 2;
        }
        if !out.is_empty() { return out; }
    }
    let mut out = Vec::new();
    if let Some((_, _, d)) = blocks.iter().find(|(id, _, _)| *id == 1006) {
        let mut at = 0;
        while at < d.len() {
            let n = d[at] as usize;
            let Some(name) = d.get(at + 1..at + 1 + n) else { break };
            out.push(text(name));
            at += 1 + n;
        }
    }
    out
}

/// The image's alpha channels and Photoshop paths.
pub fn sources(b: &[u8]) -> Sources {
    let res = resources(b);
    let paths = blocks(&res).into_iter().filter(|(id, _, _)| (2000..=2997).contains(id)).map(|(_, n, _)| text(&n)).collect();
    let count = if let Some(p) = crate::psd::planes(b) {
        p.planes.len() - p.colors - usize::from(p.merged_alpha)
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        image::load_from_memory(b).ok().map(|i| usize::from(i.color().has_alpha())).unwrap_or(0)
    } else {
        0
    };
    let names = alpha_names(&res);
    let alpha_channels = (0..count).map(|k| names.get(k).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| format!("Alpha {}", k + 1))).collect();
    Sources { alpha_channels, paths }
}

/// A Photoshop path resource as subpaths in fractions of the image (x, y).
fn photoshop_path(data: &[u8]) -> Vec<Subpath> {
    let fixed = |at: usize| be32(data, at).map(|v| v as u32 as i32 as f64 / f64::from(1 << 24));
    let mut out: Vec<Subpath> = Vec::new();
    let mut left = 0usize;
    for rec in 0..data.len() / 26 {
        let at = rec * 26;
        let Some(sel) = be16(data, at) else { break };
        match sel {
            0 | 3 => {
                left = be16(data, at + 2).unwrap_or(0);
                out.push(Subpath { points: Vec::new(), open: sel == 3 });
            }
            1 | 2 | 4 | 5 if left > 0 => {
                let p = |k: usize| -> (f64, f64) { (fixed(at + 2 + k * 8 + 4).unwrap_or(0.0), fixed(at + 2 + k * 8).unwrap_or(0.0)) };
                if let Some(s) = out.last_mut() { s.points.push([p(0), p(1), p(2)]); }
                left -= 1;
            }
            _ => {}
        }
    }
    out.retain(|s| !s.points.is_empty());
    out
}

/// The kept pixels: `on[y * w + x]`.
pub(crate) struct Mask {
    pub w: usize,
    pub h: usize,
    pub on: Vec<bool>,
}

fn image_mask(b: &[u8], req: &Request) -> Result<Mask, String> {
    let t = req.threshold;
    if req.kind == "AlphaChannel" {
        if let Some(p) = crate::psd::planes(b) {
            let extra = p.colors + usize::from(p.merged_alpha);
            let names = alpha_names(p.resources);
            let count = p.planes.len() - extra;
            let k = (0..count).find(|k| names.get(*k).is_some_and(|n| *n == req.path_name)).or_else(|| usize::try_from(req.index).ok().filter(|k| *k < count))
                .or((count > 0).then_some(0)).ok_or("이 그림에는 알파 채널이 없습니다")?;
            return Ok(Mask { w: p.w, h: p.h, on: p.planes[extra + k].iter().map(|&a| f64::from(a) > t).collect() });
        }
        let img = image::load_from_memory(b).map_err(|_| "그림을 읽지 못했습니다".to_owned())?;
        if !img.color().has_alpha() { return Err("이 그림에는 알파 채널이 없습니다".into()); }
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        return Ok(Mask { w, h, on: rgba.pixels().map(|p| f64::from(p[3]) > t).collect() });
    }
    let (w, h, rgba, _) = crate::decode_rgba(b, &folio_color::Colors::default(), true).ok_or("그림을 읽지 못했습니다")?;
    let on = rgba.chunks_exact(4).map(|p| {
        let a = f64::from(p[3]) / 255.0;
        let l = 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]);
        255.0 - (l * a + 255.0 * (1.0 - a)) > t
    }).collect();
    Ok(Mask { w: w as usize, h: h as usize, on })
}

/// The clipping path of `kind` for an image file's bytes, in the coordinates of a graphic whose
/// `GraphicBounds` are `bounds` (left, top, right, bottom).
pub fn extract(b: &[u8], req: &Request, bounds: [f64; 4]) -> Result<Vec<Subpath>, String> {
    let [l, top, r, bottom] = bounds;
    let (bw, bh) = (r - l, bottom - top);
    if !(bw > 0.0 && bh > 0.0) { return Err("그림 크기가 없습니다".into()); }
    let mut subs = if req.kind == "PhotoshopPath" {
        let res = resources(b);
        let paths: Vec<_> = blocks(&res).into_iter().filter(|(id, _, _)| (2000..=2997).contains(id)).collect();
        let (_, _, data) = paths.iter().find(|(_, n, _)| text(n) == req.path_name).or(paths.first()).ok_or("이 그림에는 Photoshop 패스가 없습니다")?;
        photoshop_path(data).into_iter()
            .map(|s| Subpath { open: s.open, points: s.points.iter().map(|p| p.map(|(x, y)| (l + x * bw, top + y * bh))).collect() })
            .collect()
    } else {
        let mask = image_mask(b, req)?;
        let (sx, sy) = (bw / mask.w as f64, bh / mask.h as f64);
        let frame = req.frame.map(|[fl, ft, fr, fb]| [(fl - l) / sx, (ft - top) / sy, (fr - l) / sx, (fb - top) / sy]);
        trace::outline(&mask, req.tolerance, req.inset / sx.max(sy).max(f64::MIN_POSITIVE), req.inside_edges, frame)
            .into_iter()
            .map(|s| Subpath { open: false, points: s.into_iter().map(|p| p.map(|(x, y)| (l + x * sx, top + y * sy))).collect() })
            .collect::<Vec<_>>()
    };
    if subs.is_empty() { return Err("클리핑 패스로 남길 부분이 없습니다".into()); }
    if req.invert {
        // InDesign keeps the image bounds first (closed, the first corner again at the end)
        let corners = [(l, top), (r, top), (r, bottom), (l, bottom), (l, top)];
        subs.insert(0, Subpath { open: false, points: corners.iter().map(|&c| [c, c, c]).collect() });
    }
    Ok(subs)
}

#[cfg(test)]
mod tests;
