//! Shared glyph paths, decoded previews, and lazily downsampled image levels.

use crate::geometry::outline_path;
use crate::image_decode::{ImageMode, decode, header_has_alpha};
use crate::svg;
use folio_color::Colors;
use folio_model::layout::{FontId, GlyphSource};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tiny_skia::Pixmap;

/// Outline path per (font, glyph); `None` for empty glyphs.
type GlyphPaths = HashMap<(FontId, u16), Option<Arc<tiny_skia::Path>>>;

/// Glyph outlines converted to tiny-skia paths, shared by all renders and threads.
#[derive(Default)]
pub struct GlyphCache {
    paths: RwLock<GlyphPaths>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get(&self, glyphs: &dyn GlyphSource, font: FontId, glyph: u16) -> Option<Arc<tiny_skia::Path>> {
        if let Some(hit) = self.paths.read().expect("render cache lock is not poisoned").get(&(font, glyph)) {
            return hit.clone();
        }
        let path = glyphs.outline(font, glyph).and_then(|cmds| outline_path(&cmds)).map(Arc::new);
        self.paths.write().expect("render cache lock is not poisoned").insert((font, glyph), path.clone());
        path
    }

    pub fn len(&self) -> usize {
        self.paths.read().expect("render cache lock is not poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A decoded image and its halved copies (level k = 1/2^k size), built on demand.
/// Drawing a strongly reduced image from a smaller level is faster and avoids aliasing.
pub struct CachedImage {
    levels: Mutex<Vec<Arc<Pixmap>>>,
}

impl CachedImage {
    pub fn width(&self) -> u32 {
        self.levels.lock().expect("render cache lock is not poisoned")[0].width()
    }
    pub fn height(&self) -> u32 {
        self.levels.lock().expect("render cache lock is not poisoned")[0].height()
    }
    pub fn original(&self) -> Arc<Pixmap> {
        self.levels.lock().expect("render cache lock is not poisoned")[0].clone()
    }

    /// The level to draw at `device_scale` device pixels per original pixel, and that
    /// level's size factor (2^k).
    pub fn level_for(&self, device_scale: f64) -> (Arc<Pixmap>, f64) {
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
}

/// 2x2 box filter on premultiplied RGBA.
pub(crate) fn halve(src: &Pixmap) -> Option<Pixmap> {
    let (w, h) = (src.width(), src.height());
    if w < 2 || h < 2 {
        return None;
    }
    let (nw, nh) = (w / 2, h / 2);
    let mut out = Pixmap::new(nw, nh)?;
    let s = src.data();
    let d = out.data_mut();
    let stride = (w * 4) as usize;
    for y in 0..nh as usize {
        for x in 0..nw as usize {
            let i = (2 * y) * stride + 2 * x * 4;
            for c in 0..4 {
                let sum = s[i + c] as u32 + s[i + 4 + c] as u32 + s[i + stride + c] as u32 + s[i + stride + 4 + c] as u32;
                d[(y * nw as usize + x) * 4 + c] = ((sum + 2) / 4) as u8;
            }
        }
    }
    Some(out)
}

/// Decoded image per (file, drawing mode, colour conversion key); `None` when it could not
/// be decoded.
type DecodedImages = HashMap<(String, ImageMode, u64), Option<Arc<CachedImage>>>;

/// Decoded preview images, shared across renders, per drawing mode.
#[derive(Default)]
pub struct ImageCache {
    images: Mutex<DecodedImages>,
    svg: Mutex<HashMap<String, Option<Arc<svg::Scene>>>>,
    /// New placements may use their original raster as the preview. Its embedded RGB
    /// profile differs from the document profile already applied to InDesign previews.
    source_colors: Mutex<HashMap<String, Colors>>,
    /// Whether a preview file has an alpha channel (read from its header).
    alpha: Mutex<HashMap<String, bool>>,
}

impl ImageCache {
    pub fn svg(&self, path: &str) -> Option<Arc<svg::Scene>> {
        let mut cache = self.svg.lock().expect("render cache lock is not poisoned");
        cache.entry(path.to_owned()).or_insert_with(|| svg::read(std::path::Path::new(path)).ok().map(Arc::new)).clone()
    }

    pub fn register_source_colors(&self, path: String, colors: Colors) {
        self.source_colors.lock().expect("render cache lock is not poisoned").insert(path, colors);
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// The image at `path` decoded for `mode`, its colours converted with `colors`.
    pub fn get(&self, path: &str, mode: ImageMode, colors: &Colors) -> Option<Arc<CachedImage>> {
        let source = self.source_colors.lock().expect("render cache lock is not poisoned").get(path).cloned();
        let colors = source.as_ref().unwrap_or(colors);
        let key = (path.to_string(), mode, colors.key());
        if let Some(hit) = self.images.lock().expect("render cache lock is not poisoned").get(&key) {
            return hit.clone();
        }
        let decoded = decode(path, mode, colors).map(|p| Arc::new(CachedImage { levels: Mutex::new(vec![Arc::new(p)]) }));
        self.images.lock().expect("render cache lock is not poisoned").insert(key, decoded.clone());
        decoded
    }

    /// Whether the image file has an alpha channel, from its header only.
    pub fn has_alpha(&self, path: &str) -> bool {
        if let Some(&hit) = self.alpha.lock().expect("render cache lock is not poisoned").get(path) {
            return hit;
        }
        let a = header_has_alpha(path).unwrap_or(false);
        self.alpha.lock().expect("render cache lock is not poisoned").insert(path.to_string(), a);
        a
    }
}
