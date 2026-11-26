//! Ink coverage of placed CMYK images, sampled from the linked original (JPEG, TIFF, PSD
//! composite). RGB / gray images get their inks from the output conversion and are not
//! sampled; vector files are not rasterised. Results are cached per file identity
//! (path, size, modification time), in memory and optionally in a file.
//!
//! Coverage is measured on 3 × 3 pixel blocks (single-pixel peaks from sharpening are not
//! what a printer's TAC check means); `max` is the highest block mean, `over` the share of
//! blocks above the limit.

use crate::Severity;
use crate::messages::msg;
use crate::checks::Ctx;
use crate::rules::IMAGE_INK_COVERAGE;
use folio_links::{ColorSpace, ImageFormat, LinkStatus};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Images larger than this are not sampled (pixels).
const MAX_PIXELS: u64 = 80_000_000;
const BLOCK: usize = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkSample {
    /// Highest 3 × 3 block mean coverage, percent.
    pub max: f64,
    /// Coverage histogram: share of blocks above 280 / 300 / 320 / 340 %.
    pub over: [f64; 4],
    pub width: u32,
    pub height: u32,
}

type Key = (PathBuf, u64, Option<u128>);

fn cache() -> &'static Mutex<HashMap<Key, Option<InkSample>>> {
    static C: OnceLock<Mutex<HashMap<Key, Option<InkSample>>>> = OnceLock::new();
    C.get_or_init(Mutex::default)
}

fn key(p: &Path) -> Option<Key> {
    let m = std::fs::metadata(p).ok()?;
    let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos());
    Some((p.to_path_buf(), m.len(), t))
}

/// Samples a CMYK image file; `None` when the format / layout is not read here.
pub fn sample(path: &Path, format: ImageFormat) -> Option<InkSample> {
    let k = key(path)?;
    if let Some(r) = cache().lock().ok().and_then(|c| c.get(&k).cloned()) {
        return r;
    }
    let r = sample_uncached(path, format);
    if let Ok(mut c) = cache().lock() {
        c.insert(k, r.clone());
    }
    r
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: Vec<(Key, Option<InkSample>)>,
}

pub fn load_cache(file: &Path) -> usize {
    let Ok(b) = std::fs::read(file) else { return 0 };
    let Ok(c) = serde_json::from_slice::<CacheFile>(&b) else { return 0 };
    if c.version != 1 {
        return 0;
    }
    let n = c.entries.len();
    if let Ok(mut m) = cache().lock() {
        for (k, v) in c.entries {
            m.entry(k).or_insert(v);
        }
    }
    n
}

pub fn save_cache(file: &Path) -> std::io::Result<()> {
    let entries: Vec<(Key, Option<InkSample>)> = cache().lock().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    let bytes = serde_json::to_vec(&CacheFile { version: 1, entries })?;
    if let Some(p) = file.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file)
}

/// Coverage statistics of interleaved 8-bit CMYK ink amounts (0 = no ink).
fn stats(px: &[u8], w: usize, h: usize, stride: usize) -> InkSample {
    let bw = w / BLOCK;
    let bh = h / BLOCK;
    let mut max = 0.0f64;
    let mut over = [0u64; 4];
    let mut blocks = 0u64;
    for by in 0..bh.max(1) {
        for bx in 0..bw.max(1) {
            let mut sum = 0u32;
            let mut n = 0u32;
            for y in by * BLOCK..((by + 1) * BLOCK).min(h) {
                for x in bx * BLOCK..((bx + 1) * BLOCK).min(w) {
                    let o = (y * w + x) * stride;
                    if let Some(p) = px.get(o..o + 4) {
                        sum += p[0] as u32 + p[1] as u32 + p[2] as u32 + p[3] as u32;
                        n += 1;
                    }
                }
            }
            if n == 0 {
                continue;
            }
            let tac = sum as f64 / n as f64 * 100.0 / 255.0;
            max = max.max(tac);
            for (i, lim) in [280.0, 300.0, 320.0, 340.0].iter().enumerate() {
                if tac > *lim {
                    over[i] += 1;
                }
            }
            blocks += 1;
        }
    }
    let b = blocks.max(1) as f64;
    InkSample { max: (max * 10.0).round() / 10.0, over: over.map(|o| (o as f64 / b * 10000.0).round() / 100.0), width: w as u32, height: h as u32 }
}

fn sample_uncached(path: &Path, format: ImageFormat) -> Option<InkSample> {
    let info = folio_links::probe(path).ok()?;
    if info.color_space != ColorSpace::Cmyk || info.width as u64 * info.height as u64 > MAX_PIXELS {
        return None;
    }
    match format {
        ImageFormat::Jpeg => jpeg(path),
        ImageFormat::Tiff => tiff(path),
        ImageFormat::Psd => psd(path),
        _ => None,
    }
}

fn jpeg(path: &Path) -> Option<InkSample> {
    use zune_jpeg::JpegDecoder;
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace as Zc;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let bytes = std::fs::read(path).ok()?;
    let mut probe = JpegDecoder::new(ZCursor::new(&bytes));
    probe.decode_headers().ok()?;
    let input = probe.input_colorspace()?;
    if !matches!(input, Zc::CMYK | Zc::YCCK) {
        return None;
    }
    let mut d = JpegDecoder::new_with_options(ZCursor::new(&bytes), DecoderOptions::default().jpeg_set_out_colorspace(input).set_max_width(1 << 16).set_max_height(1 << 16));
    let mut px = d.decode().ok()?;
    let (w, h) = d.dimensions()?;
    // Adobe CMYK JPEGs store 255 − ink; YCCK decodes to the ink of C, M, Y directly.
    for s in px.chunks_exact_mut(4) {
        if input == Zc::YCCK {
            let (yy, cb, cr) = (s[0] as f32, s[1] as f32 - 128.0, s[2] as f32 - 128.0);
            let v = |x: f32| x.round().clamp(0.0, 255.0) as u8;
            s[0] = v(yy + 1.402 * cr);
            s[1] = v(yy - 0.344_136 * cb - 0.714_136 * cr);
            s[2] = v(yy + 1.772 * cb);
        } else {
            s[0] = 255 - s[0];
            s[1] = 255 - s[1];
            s[2] = 255 - s[2];
        }
        s[3] = 255 - s[3];
    }
    Some(stats(&px, w, h, 4))
}

fn tiff(path: &Path) -> Option<InkSample> {
    use tiff::decoder::{Decoder, DecodingResult, Limits};
    use tiff::tags::Tag;
    let f = std::fs::File::open(path).ok()?;
    let mut d = Decoder::new(std::io::BufReader::new(f)).ok()?.with_limits(Limits::unlimited());
    let (w, h) = d.dimensions().ok()?;
    if d.get_tag_u32(Tag::PhotometricInterpretation).ok()? != 5 {
        return None;
    }
    let spp = d.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(4) as usize;
    if spp < 4 {
        return None;
    }
    let px: Vec<u8> = match d.read_image().ok()? {
        DecodingResult::U8(v) => v,
        DecodingResult::U16(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
        _ => return None,
    };
    Some(stats(&px, w as usize, h as usize, spp))
}

/// PSD / PSB composite image (raw or PackBits), planar channels; CMYK stored as 255 − ink.
fn psd(path: &Path) -> Option<InkSample> {
    let b = std::fs::read(path).ok()?;
    let be16 = |o: usize| b.get(o..o + 2).map(|s| u16::from_be_bytes([s[0], s[1]]));
    let be32 = |o: usize| b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let version = be16(4)?;
    let channels = be16(12)? as usize;
    let h = be32(14)? as usize;
    let w = be32(18)? as usize;
    if be16(22)? != 8 || be16(24)? != 4 || channels < 4 {
        return None;
    }
    let mut o = 26usize;
    o += 4 + be32(o)? as usize; // colour mode data
    o += 4 + be32(o)? as usize; // image resources
    let lm = if version == 2 { b.get(o..o + 8).map(|s| u64::from_be_bytes(s.try_into().unwrap_or([0; 8])))? as usize } else { be32(o)? as usize };
    o += if version == 2 { 8 } else { 4 } + lm; // layer and mask info
    let comp = be16(o)?;
    o += 2;
    let plane = w * h;
    let mut planar = vec![0u8; plane * 4];
    match comp {
        0 => {
            for c in 0..4 {
                planar[c * plane..(c + 1) * plane].copy_from_slice(b.get(o + c * plane..o + (c + 1) * plane)?);
            }
        }
        1 => {
            let count_size = if version == 2 { 4 } else { 2 };
            let rows = h * channels;
            let mut counts = Vec::with_capacity(rows);
            for r in 0..rows {
                let at = o + r * count_size;
                counts.push(if count_size == 4 { be32(at)? as usize } else { be16(at)? as usize });
            }
            let mut p = o + rows * count_size;
            for (r, &n) in counts.iter().enumerate() {
                let c = r / h;
                let y = r % h;
                if c < 4 {
                    let dst = &mut planar[c * plane + y * w..c * plane + (y + 1) * w];
                    packbits(b.get(p..p + n)?, dst);
                }
                p += n;
            }
        }
        _ => return None,
    }
    let mut px = vec![0u8; plane * 4];
    for i in 0..plane {
        for c in 0..4 {
            px[i * 4 + c] = 255 - planar[c * plane + i];
        }
    }
    Some(stats(&px, w, h, 4))
}

fn packbits(src: &[u8], dst: &mut [u8]) {
    let (mut i, mut o) = (0, 0);
    while i < src.len() && o < dst.len() {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let len = n as usize + 1;
            let end = (o + len).min(dst.len());
            let take = end - o;
            if let Some(s) = src.get(i..i + take) {
                dst[o..end].copy_from_slice(s);
            }
            i += len;
            o = end;
        } else if n != -128 {
            let len = (1 - n as i32) as usize;
            let v = src.get(i).copied().unwrap_or(0);
            i += 1;
            let end = (o + len).min(dst.len());
            dst[o..end].fill(v);
            o = end;
        }
    }
}

/// IMAGE_INK_COVERAGE findings for placed CMYK images on printing pages.
pub(crate) fn check(ctx: &mut Ctx<'_>) {
    let max_ink = ctx.o.max_ink;
    let mut done: HashMap<PathBuf, Option<InkSample>> = HashMap::new();
    let mut found = Vec::new();
    let g = ctx.g;
    for p in &g.items {
        if !p.prints() || p.page.is_none() {
            continue;
        }
        let gr = match &p.item.kind {
            folio_model::ItemKind::Rectangle { content: Some(gr) } | folio_model::ItemKind::Oval { content: Some(gr) } | folio_model::ItemKind::Polygon { content: Some(gr) } => gr,
            _ => continue,
        };
        let Some(e) = ctx.links.entry(&gr.id) else { continue };
        let Some(LinkStatus::Found { path, .. }) = &e.status else { continue };
        let Some(Ok(info)) = &e.info else { continue };
        if info.vector || info.color_space != ColorSpace::Cmyk {
            continue;
        }
        let s = done.entry(path.clone()).or_insert_with(|| sample(path, info.format)).clone();
        found.push((g.doc_page(p), p.item.id.clone(), gr.id.clone(), path.clone(), s, g.master_name(p).map(str::to_owned)));
    }
    let mut unread = 0;
    for (page, frame, graphic, path, s, master) in found {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match s {
            Some(s) if s.max > max_ink + 0.5 => {
                let share = if max_ink <= 300.0 { s.over[1] } else { s.over[3] };
                ctx.push(
                    IMAGE_INK_COVERAGE,
                    Severity::Warning,
                    page,
                    Some(&frame),
                    msg("preflight.image.ink", format!("이미지 잉크 총량 최대 {:.0}% (기준 {max_ink:.0}%): {name}", s.max), json!({"max":format!("{:.0}",s.max),"threshold":format!("{max_ink:.0}"),"name":name})),
                    json!({"graphic": graphic, "path": path, "max": s.max, "shareOver": {"280": s.over[0], "300": s.over[1], "320": s.over[2], "340": s.over[3]}, "shareOverLimit": share, "pixels": [s.width, s.height], "source": "original", "master": master}),
                );
            }
            Some(_) => {}
            None => unread += 1,
        }
    }
    if unread > 0 {
        ctx.limitations.push(format!("CMYK 이미지 {unread}개는 형식(압축 방식 등) 때문에 잉크 총량을 재지 못함"));
    }
    ctx.limitations.push("이미지 잉크 총량은 연결된 원본의 CMYK 값(3×3 화소 평균)으로 쟀고, RGB·회색 이미지와 PDF/EPS 안의 내용은 재지 않음".into());
}
