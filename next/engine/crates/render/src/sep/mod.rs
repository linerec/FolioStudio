//! Separations and overprint preview: a page rendered as ink planes, one per plate — C, M,
//! Y, K and the spot inks the document prints on after the Ink Manager (conversion to process,
//! aliases) — then shown as InDesign's Separations Preview and Overprint Preview show them.
//!
//! What lands on a plate follows print, not the screen: colours resolve through
//! [`crate::inks`] (the resolver the PDF writer uses), nonprinting items and layers are left
//! out, masters are drawn layer by layer with the page items and every page of the spread's
//! master shows at the spine, as in the PDF. Fills, strokes and text knock out every plate
//! unless they overprint (Overprint Fill / Stroke, text overprint, and "Overprint [Black]
//! at 100 %"); an overprinting DeviceCMYK colour paints only its non-zero inks (OPM 1), a spot
//! colour only its plate. Transparency blends per ink in the document's CMYK blend space
//! ([`canvas`]); placed images print their own CMYK channels, gray on black, RGB through the
//! document profiles ([`images`]); effects use the masks the PDF writer exports
//! ([`crate::effects`]).
//!
//! Known limits: a placed PDF or EPS is its RGB proxy converted to CMYK (its own spot
//! colours do not separate), knockout groups composite as ordinary groups, and a group with
//! opacity is composited as an isolated layer (its members' blend modes do not reach the
//! page below it).

mod canvas;
mod draw;
pub mod flat;
pub mod images;
mod list;
pub mod view;

pub use images::{InkImage, InkImages};
pub use list::document_inks;
pub use view::{Look, View, composite, composite_with};

use crate::Scene;
use folio_model::{Affine, Graphic, Rect};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A position in a spread's paint order, for flattening: (layer rank, source, item in the
/// source, part). Sources are each page's master in page order, then the spread; parts of a
/// top-level item are listed in [`part`]. The PDF writer numbers its paint the same way, so
/// a raster can show exactly what precedes a point of the vector stream.
pub type UnitKey = (u32, u32, u32, u8);

/// Parts of a top-level item in paint order.
pub mod part {
    /// Shadows and glows behind it, or the whole item when it is composited as a group
    /// (opacity, blend mode, isolation, knockout).
    pub const BEHIND: u8 = 0;
    /// Its fill (or, feathered, the whole feathered body).
    pub const FILL: u8 = 1;
    /// Placed graphic, pasted-in items, text and inline objects (a group's members).
    pub const CONTENT: u8 = 2;
    pub const STROKE: u8 = 3;
    /// Inner shadow, inner glow, satin, bevel and emboss.
    pub const INNER: u8 = 4;
}

/// The image a graphic prints, as ink: lets a print renderer use found originals (with the
/// conversion the PDF uses) instead of the reader's previews.
pub trait InkImageSource: Sync {
    /// The graphic's image, or `None` to draw its preview.
    fn image(&self, g: &Graphic) -> Option<Arc<InkImage>>;
}

/// Effect raster cache key: (item address, effect kind, pixels per unit × 10⁶, page).
pub type FxKey = (usize, u8, i64, usize);

/// One inner effect layer as cached: mask, colour, blend mode, opacity.
pub type InnerFx = (crate::effects::Alpha, folio_model::Paint, folio_model::BlendMode, f64);

#[derive(Clone)]
pub enum FxEntry {
    Alpha(Option<Arc<crate::effects::Alpha>>),
    Inner(Arc<Vec<InnerFx>>),
}

/// Effect rasters shared by the renders of one page's regions (shadows, feathers and inner
/// effects are computed once per item, not once per tile).
#[derive(Default)]
pub struct FxCache {
    map: Mutex<HashMap<FxKey, FxEntry>>,
}

impl FxCache {
    pub fn new() -> Self {
        FxCache { map: Mutex::new(HashMap::new()) }
    }
    pub(crate) fn get(&self, k: &FxKey) -> Option<FxEntry> {
        self.map.lock().expect("render cache lock is not poisoned").get(k).cloned()
    }
    pub(crate) fn put(&self, k: FxKey, v: FxEntry) {
        self.map.lock().expect("render cache lock is not poisoned").insert(k, v);
    }
    pub fn len(&self) -> usize {
        self.map.lock().expect("render cache lock is not poisoned").len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How [`render_area`] draws.
#[derive(Default)]
pub struct AreaOptions<'a> {
    /// Draw only the paint up to and including this unit.
    pub limit: Option<UnitKey>,
    /// Record hard edges ([`AreaPlanes::edges`]).
    pub edges: bool,
    /// Effect rasters at this resolution (pixels per inch) instead of the canvas's.
    pub effect_ppi: Option<f64>,
    pub fx: Option<&'a FxCache>,
    pub source: Option<&'a dyn InkImageSource>,
    /// Flattener Preview: draw only these top-level items (key (layer rank, source, item) -> bit
    /// per [`part`]) as coverage ([`flat`]).
    pub select: Option<&'a HashMap<(u32, u32, u32), u8>>,
}

/// An area of a spread as ink planes.
pub struct AreaPlanes {
    pub width: u32,
    pub height: u32,
    /// Plates in [`DocInks::inks`] order, plane after plane (0 = no ink, 255 = 100 %).
    pub data: Vec<u8>,
    pub count: usize,
    /// One byte per pixel: 1 where a vector shape's edge crosses the pixel.
    pub edges: Option<Vec<u8>>,
    pub stats: SepStats,
}

impl AreaPlanes {
    pub fn plane(&self, p: usize) -> &[u8] {
        let wh = self.width as usize * self.height as usize;
        &self.data[p * wh..(p + 1) * wh]
    }
}

/// Renders a `w × h` area of the spread that holds page `index` as ink planes: `to_canvas`
/// maps spread coordinates to canvas pixels (any area, bleed and beyond included).
// Preserve the public rendering API shared by preview and PDF flattening.
#[allow(clippy::too_many_arguments)]
pub fn render_area<'a>(
    scene: &'a Scene<'a>,
    ink_images: &'a InkImages,
    inks: &'a DocInks,
    index: usize,
    to_canvas: &Affine,
    w: u32,
    h: u32,
    opts: &AreaOptions<'a>,
) -> Option<AreaPlanes> {
    let started = Instant::now();
    if w == 0 || h == 0 || w as u64 * h as u64 * (inks.len().max(4) as u64) > crate::MAX_PIXELS * 4 {
        return None;
    }
    let (canvas, mut stats) = draw::draw_area(scene, ink_images, inks, index, to_canvas, w as usize, h as usize, opts)?;
    stats.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    Some(AreaPlanes { width: w, height: h, count: canvas.n, data: canvas.ink, edges: canvas.edges, stats })
}

/// Whether a plate is one of the four process inks or a spot ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InkKind {
    Process,
    Spot,
}

/// One plate of the document.
#[derive(Clone, Debug, PartialEq)]
pub struct PlateInk {
    /// The plate's name as it prints ("Cyan", "PANTONE 280 C").
    pub name: String,
    pub kind: InkKind,
    /// Ink Manager `InkType` (Normal, Transparent, Opaque, OpaqueIgnore); empty when the
    /// ink is not in the Ink Manager.
    pub ink_type: String,
    /// CMYK (0–1) of full coverage, for simulating the ink on screen: process inks are unit
    /// CMYK, a spot ink its alternate (Lab and RGB alternates through the document profiles).
    pub cmyk: [f32; 4],
    /// Display colour of full coverage.
    pub rgb: [u8; 3],
    /// Spot swatches that print on this plate through an Ink Manager alias.
    pub aliases: Vec<String>,
}

/// The plates a document prints on and where they are used.
#[derive(Clone, Debug, Default)]
pub struct DocInks {
    /// Cyan, Magenta, Yellow, Black, then the spot inks (Ink Manager order).
    pub inks: Vec<PlateInk>,
    /// Plates used by each page (document page index order); process plates count as used
    /// when anything on the page resolves to process ink.
    pub pages: Vec<Vec<usize>>,
    /// Spot inks the Ink Manager converts to process (they print on C, M, Y, K).
    pub converted: Vec<String>,
    index: HashMap<String, usize>,
}

impl DocInks {
    /// Plate index of an ink name (process inks also as "Process Cyan" …).
    pub fn plane(&self, name: &str) -> Option<usize> {
        match name {
            "Cyan" | "Process Cyan" => Some(0),
            "Magenta" | "Process Magenta" => Some(1),
            "Yellow" | "Process Yellow" => Some(2),
            "Black" | "Process Black" => Some(3),
            _ => self.index.get(name).copied(),
        }
    }

    pub fn len(&self) -> usize {
        self.inks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inks.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct SepStats {
    pub items: usize,
    pub glyphs: usize,
    pub images: usize,
    pub missing_images: usize,
    pub layers: usize,
    pub culled: usize,
    /// Fills, strokes and glyphs painted with overprint.
    pub overprints: usize,
    /// Largest image detail drawn: image pixels per canvas pixel (0 without images).
    pub image_detail: f64,
    /// Effect masks and gradients painted (content that needs the full smooth resolution).
    pub soft_paints: usize,
    pub elapsed_ms: f64,
}

/// A page (or a region of it) as ink coverage per plate.
pub struct InkPlanes {
    pub width: u32,
    pub height: u32,
    /// Plates in [`DocInks::inks`] order, `width × height` each, one after the other:
    /// 0 = no ink, 255 = 100 %.
    pub data: Vec<u8>,
    pub count: usize,
    pub stats: SepStats,
}

impl InkPlanes {
    /// Plate `p`, row by row.
    pub fn plane(&self, p: usize) -> &[u8] {
        let wh = self.width as usize * self.height as usize;
        &self.data[p * wh..(p + 1) * wh]
    }

    /// Ink of every plate at pixel `(x, y)`, percent.
    pub fn at(&self, x: u32, y: u32) -> Option<Vec<f64>> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y * self.width + x) as usize;
        Some((0..self.count).map(|p| self.plane(p)[i] as f64 * 100.0 / 255.0).collect())
    }

    /// Bytes held (for cache budgets).
    pub fn bytes(&self) -> usize {
        self.data.len()
    }
}

/// Renders page `index` at `scale` pixels per point as ink planes for the plates of `inks`
/// (normally [`document_inks`] of the same document); `region` as in [`crate::render_page`].
pub fn render_inks(scene: &Scene, ink_images: &InkImages, inks: &DocInks, index: usize, scale: f64, region: Option<Rect>) -> Option<InkPlanes> {
    let started = Instant::now();
    let area = crate::page_region(scene.doc, index, region)?;
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return None;
    }
    let width = (area.width() * scale).round().max(1.0) as u32;
    let height = (area.height() * scale).round().max(1.0) as u32;
    // planes are one byte per ink: the same budget as an RGBA render
    if width as u64 * height as u64 * (inks.len().max(4) as u64) > crate::MAX_PIXELS * 4 {
        return None;
    }
    let (canvas, mut stats) = draw::draw_page(scene, ink_images, inks, index, scale, &area, width as usize, height as usize)?;
    stats.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    Some(InkPlanes { width, height, count: canvas.n, data: canvas.ink, stats })
}
