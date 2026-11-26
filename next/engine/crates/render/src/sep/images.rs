//! Placed images as ink. CMYK samples (TIFF, Adobe JPEG) go to the plates channel by
//! channel; gray (and 1-bit) images print on the black plate alone; RGB is converted with the
//! document's profiles (`folio_color::Colors::rgb_to_cmyk8`, document RGB → document CMYK, the
//! conversion of the CMYK blend space). Decoded once per file and kept with halved copies,
//! like the screen's images, and resampled the same way: exact area averaging when axis
//! aligned (the screen's taps, all five channels in one pass), bilinear otherwise.

use super::canvas::{Cov, Pixels};
use crate::{axis_taps, cmyk_jpeg_inks, tiff_samples, to_skia};
use folio_color::Colors;
use folio_model::Affine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tiny_skia::{Pixmap, PixmapPaint};

/// Premultiplied C, M, Y, K, alpha per pixel.
struct Level {
    w: u32,
    h: u32,
    px: Vec<[u8; 5]>,
}

/// A decoded image as ink with halved levels for strong reductions (level k = 1/2^k size).
pub struct InkImage {
    levels: Mutex<Vec<Arc<Level>>>,
}

type Decoded = HashMap<(String, bool, u64), Option<Arc<InkImage>>>;

/// Images decoded as ink, shared by every separations render of a document.
#[derive(Default)]
pub struct InkImages {
    images: Mutex<Decoded>,
    source_colors: Mutex<HashMap<String, Colors>>,
}

impl InkImages {
    /// Originals used as previews retain their own RGB profile (basic/place-icc).
    pub fn register_source_colors(&self, path: String, colors: Colors) {
        self.source_colors.lock().expect("render cache lock is not poisoned").insert(path, colors);
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// The image at `path` as ink with its alpha; `proxy`: a placed PDF / EPS proxy, its
    /// alpha a 1-bit mask over its colour as stored (as on screen, [`crate::ImageMode`]).
    /// `None` when it cannot be decoded.
    pub fn get(&self, path: &str, proxy: bool, colors: &Colors) -> Option<Arc<InkImage>> {
        let source = self.source_colors.lock().expect("render cache lock is not poisoned").get(path).cloned();
        let colors = source.as_ref().unwrap_or(colors);
        let key = (path.to_string(), proxy, colors.key());
        if let Some(hit) = self.images.lock().expect("render cache lock is not poisoned").get(&key) {
            return hit.clone();
        }
        let decoded = decode(path, proxy, colors).map(Arc::new);
        self.images.lock().expect("render cache lock is not poisoned").insert(key, decoded.clone());
        decoded
    }

    pub fn len(&self) -> usize {
        self.images.lock().expect("render cache lock is not poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Straight C, M, Y, K, alpha per pixel.
type Samples = (u32, u32, Vec<[u8; 5]>);

fn decode(path: &str, proxy: bool, colors: &Colors) -> Option<InkImage> {
    let bytes = std::fs::read(path).ok()?;
    let (w, h, mut px) = ink_samples(&bytes, colors, !proxy)?;
    if proxy {
        for p in &mut px {
            p[4] = if p[4] > 0 { 255 } else { 0 };
        }
    }
    InkImage::from_samples(w, h, px)
}

/// Ink samples of an image file: CMYK as stored, gray as black ink, 1-bit as a black stencil
/// (white is transparent, as InDesign paints bitmaps), RGB through the document profiles.
/// `straight`: associated TIFF alpha is undone (else the colour as stored).
fn ink_samples(bytes: &[u8], colors: &Colors, straight: bool) -> Option<Samples> {
    let tiff = bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*");
    if tiff && let Some(t) = tiff_samples(bytes, straight) {
        let n = (t.w * t.h) as usize;
        let mut out = Vec::with_capacity(n);
        let mut rgb = RgbInk::new(colors);
        for s in t.src.chunks_exact(t.spp).take(n) {
            let a = |i: usize| if t.spp > i { s[i] } else { 255 };
            out.push(match t.photometric {
                5 if t.spp >= 4 => [s[0], s[1], s[2], s[3], a(4)],
                0 | 1 if t.bits == 1 => {
                    let black = if t.photometric == 0 { s[0] != 0 } else { s[0] == 0 };
                    [0, 0, 0, 255, if black { 255 } else { 0 }]
                }
                0 | 1 => {
                    let v = if t.photometric == 0 { 255 - s[0] } else { s[0] };
                    [0, 0, 0, 255 - v, a(1)]
                }
                3 => {
                    let m = t.map.as_ref()?;
                    let size = m.len() / 3;
                    let i = (s[0] as usize).min(size.saturating_sub(1));
                    let c = rgb.ink([(m[i] >> 8) as u8, (m[size + i] >> 8) as u8, (m[2 * size + i] >> 8) as u8]);
                    [c[0], c[1], c[2], c[3], a(1)]
                }
                _ if t.spp >= 3 => {
                    let c = rgb.ink([s[0], s[1], s[2]]);
                    [c[0], c[1], c[2], c[3], a(3)]
                }
                _ => return None,
            });
        }
        return Some((t.w, t.h, out));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut out = Vec::new();
        if let Some((w, h)) = cmyk_jpeg_inks(bytes, |c| out.push([c[0], c[1], c[2], c[3], 255])) {
            return Some((w, h, out));
        }
    }
    let img = image::load_from_memory(bytes).ok()?;
    use image::ColorType;
    let gray = matches!(img.color(), ColorType::L8 | ColorType::La8 | ColorType::L16 | ColorType::La16);
    // a bitmap (1-bit) TIFF the tiff crate hands over packed: a black stencil
    let stencil = tiff && tiff_bits(bytes) == Some(1);
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgb = RgbInk::new(colors);
    let out = img
        .pixels()
        .map(|p| {
            if stencil {
                [0, 0, 0, 255, if p[0] < 128 { p[3] } else { 0 }]
            } else if gray {
                [0, 0, 0, 255 - p[0], p[3]]
            } else {
                let c = rgb.ink([p[0], p[1], p[2]]);
                [c[0], c[1], c[2], c[3], p[3]]
            }
        })
        .collect();
    Some((w, h, out))
}

fn tiff_bits(bytes: &[u8]) -> Option<u32> {
    let mut d = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).ok()?;
    d.get_tag_u32(tiff::tags::Tag::BitsPerSample).ok()
}

/// RGB → ink with the last result kept (photos have runs of equal pixels).
struct RgbInk<'c> {
    colors: &'c Colors,
    last: Option<([u8; 3], [u8; 4])>,
}

impl<'c> RgbInk<'c> {
    fn new(colors: &'c Colors) -> Self {
        RgbInk { colors, last: None }
    }
    fn ink(&mut self, rgb: [u8; 3]) -> [u8; 4] {
        match self.last {
            Some((i, o)) if i == rgb => o,
            _ => {
                let o = self.colors.rgb_to_cmyk8(rgb);
                self.last = Some((rgb, o));
                o
            }
        }
    }
}

/// 2 × 2 box filter of premultiplied samples.
fn halve(src: &Level) -> Option<Level> {
    let (w, h) = (src.w as usize, src.h as usize);
    if w < 2 || h < 2 {
        return None;
    }
    let (nw, nh) = (w / 2, h / 2);
    let mut px = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let i = 2 * y * w + 2 * x;
            let q = [&src.px[i], &src.px[i + 1], &src.px[i + w], &src.px[i + w + 1]];
            px.push(std::array::from_fn(|c| ((q.iter().map(|p| p[c] as u32).sum::<u32>() + 2) / 4) as u8));
        }
    }
    Some(Level { w: nw as u32, h: nh as u32, px })
}

impl InkImage {
    /// An image from straight C, M, Y, K (ink, 0 = none) and alpha samples, row by row.
    pub fn from_samples(w: u32, h: u32, mut px: Vec<[u8; 5]>) -> Option<InkImage> {
        if w == 0 || h == 0 || px.len() < (w as usize) * (h as usize) {
            return None;
        }
        px.truncate(w as usize * h as usize);
        let pre = |v: u8, a: u8| ((v as u16 * a as u16 + 127) / 255) as u8;
        for p in &mut px {
            if p[4] < 255 {
                let a = p[4];
                *p = [pre(p[0], a), pre(p[1], a), pre(p[2], a), pre(p[3], a), a];
            }
        }
        Some(InkImage { levels: Mutex::new(vec![Arc::new(Level { w, h, px })]) })
    }

    /// Bytes held by the decoded levels.
    pub fn bytes(&self) -> usize {
        self.levels.lock().expect("render cache lock is not poisoned").iter().map(|l| l.px.len() * 5).sum()
    }

    pub fn width(&self) -> u32 {
        self.levels.lock().expect("render cache lock is not poisoned")[0].w
    }
    pub fn height(&self) -> u32 {
        self.levels.lock().expect("render cache lock is not poisoned")[0].h
    }

    /// The level to draw at `device_scale` device pixels per original pixel and its size
    /// factor (as the screen's `CachedImage::level_for`).
    fn level_for(&self, device_scale: f64) -> (Arc<Level>, f64) {
        let mut k = 0usize;
        let mut s = device_scale;
        while s < 0.5 && k < 8 {
            s *= 2.0;
            k += 1;
        }
        let mut levels = self.levels.lock().expect("render cache lock is not poisoned");
        while levels.len() <= k {
            let last = levels.last().expect("image levels are seeded with the original image").clone();
            match halve(&last) {
                Some(h) => levels.push(Arc::new(h)),
                None => break,
            }
        }
        let k = k.min(levels.len() - 1);
        (levels[k].clone(), (1u32 << k) as f64)
    }

    /// The image's ink over `cov`'s rectangle, `to_device` mapping original image pixels to
    /// the canvas: C, M, Y, K planes (of `n`) and alpha, straight.
    pub(crate) fn sample(&self, to_device: &Affine, cov: &Cov, n: usize) -> Option<Pixels> {
        let scale = (to_device.a * to_device.d - to_device.b * to_device.c).abs().sqrt();
        let (level, factor) = self.level_for(scale);
        let t = to_device.then_inner(&Affine::scale(factor, factor));
        let (w, h) = (cov.w, cov.h);
        // premultiplied C, M, Y, K, alpha per pixel of the coverage rectangle
        let mut acc = vec![[0u8; 5]; w * h];
        if t.b.abs() < 1e-9 && t.c.abs() < 1e-9 {
            resample(&level, &t, cov, &mut acc)?;
        } else {
            rotated(&level, &t, cov, &mut acc)?;
        }
        let mut planes: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; w * h]).collect();
        let mut alpha = vec![0u8; w * h];
        // 255 / alpha in 16.16 fixed point: unpremultiplying without a division per sample
        let recip: Vec<u32> = (0..256u32).map(|al| (255 * 65536 + al / 2).checked_div(al).unwrap_or(0)).collect();
        for (i, p) in acc.iter().enumerate() {
            let al = p[4];
            if al == 0 {
                continue;
            }
            alpha[i] = al;
            if al == 255 {
                for c in 0..4 {
                    planes[c][i] = p[c];
                }
            } else {
                let r = recip[al as usize];
                for c in 0..4 {
                    planes[c][i] = ((p[c] as u32 * r + 32768) >> 16).min(255) as u8;
                }
            }
        }
        let mut process = planes.into_iter();
        let planes = (0..n).map(|p| if p < 4 { process.next() } else { None }).collect();
        Some(Pixels { w, h, planes, alpha: Some(alpha) })
    }
}

/// Exact area resampling (image pixels as squares averaged over each device pixel, the
/// screen's `resample_box`) of an axis-aligned image into `out` over `cov`'s rectangle.
fn resample(src: &Level, t: &Affine, cov: &Cov, out: &mut [[u8; 5]]) -> Option<()> {
    let (sw, sh) = (src.w, src.h);
    if t.a.abs() < 1e-12 || t.d.abs() < 1e-12 {
        return None;
    }
    let (x0, y0) = (cov.x0 as i64, cov.y0 as i64);
    let (w, h) = (cov.w, cov.h);
    let cols = axis_taps(t.a, t.tx, sw, x0, x0 + w as i64);
    let rows = axis_taps(t.d, t.ty, sh, y0, y0 + h as i64);
    // the source rows any output row uses, then a horizontal pass over them
    let r_lo = rows.start.iter().copied().min().unwrap_or(0) as usize;
    let r_hi = rows.range.iter().zip(&rows.start).map(|(&(f, e), &s)| s as usize + (e - f) as usize).max().unwrap_or(0).min(sh as usize);
    if r_hi <= r_lo {
        return Some(());
    }
    let mut tmp = vec![[0f32; 5]; (r_hi - r_lo) * w];
    for sr in r_lo..r_hi {
        let row = &src.px[sr * sw as usize..(sr + 1) * sw as usize];
        let line = &mut tmp[(sr - r_lo) * w..(sr - r_lo + 1) * w];
        for (i, (&(f, e), &j0)) in cols.range.iter().zip(&cols.start).enumerate() {
            let mut a = [0f32; 5];
            for (k, &wt) in cols.weights[f as usize..e as usize].iter().enumerate() {
                let p = &row[j0 as usize + k];
                for c in 0..5 {
                    a[c] += wt * p[c] as f32;
                }
            }
            line[i] = a;
        }
    }
    for (y, (&(f, e), &r0)) in rows.range.iter().zip(&rows.start).enumerate() {
        if e == f {
            continue;
        }
        let o = &mut out[y * w..(y + 1) * w];
        for (i, px) in o.iter_mut().enumerate() {
            let mut a = [0f32; 5];
            for (k, &wt) in rows.weights[f as usize..e as usize].iter().enumerate() {
                let p = &tmp[(r0 as usize + k - r_lo) * w + i];
                for c in 0..5 {
                    a[c] += wt * p[c];
                }
            }
            let alpha = a[4].round().clamp(0.0, 255.0);
            *px = [
                a[0].round().clamp(0.0, alpha) as u8,
                a[1].round().clamp(0.0, alpha) as u8,
                a[2].round().clamp(0.0, alpha) as u8,
                a[3].round().clamp(0.0, alpha) as u8,
                alpha as u8,
            ];
        }
    }
    Some(())
}

/// A rotated or skewed image: drawn bilinear with tiny-skia, ink channels in two passes.
fn rotated(src: &Level, t: &Affine, cov: &Cov, out: &mut [[u8; 5]]) -> Option<()> {
    let size = tiny_skia::IntSize::from_wh(src.w, src.h)?;
    let cmy: Vec<u8> = src.px.iter().flat_map(|p| [p[0], p[1], p[2], p[4]]).collect();
    let k: Vec<u8> = src.px.iter().flat_map(|p| [p[3], p[3], p[3], p[4]]).collect();
    let tr = to_skia(&Affine::translate(-(cov.x0 as f64), -(cov.y0 as f64)).then_inner(t));
    let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    let draw = |data: Vec<u8>| -> Option<Pixmap> {
        let img = Pixmap::from_vec(data, size)?;
        let mut pm = Pixmap::new(cov.w as u32, cov.h as u32)?;
        pm.draw_pixmap(0, 0, img.as_ref(), &paint, tr, None);
        Some(pm)
    };
    let (a, b) = (draw(cmy)?, draw(k)?);
    for ((o, pa), pb) in out.iter_mut().zip(a.data().chunks_exact(4)).zip(b.data().chunks_exact(4)) {
        *o = [pa[0], pa[1], pa[2], pb[0], pa[3]];
    }
    Some(())
}
