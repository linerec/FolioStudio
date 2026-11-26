//! Transparency flattening for PDF/X-1a and PDF/X-3 (PDF 1.3 has no transparency), after
//! InDesign's "[High Resolution]" flattener: everything stays vector except the areas where
//! transparency acts, and those become ink rasters drawn by the separations renderer
//! (`folio_render::sep`), which composites in the document's CMYK blend space with the spot
//! inks as their own plates, so the rasters are CMYK + spot (DeviceN) with no RGB round trip.
//!
//! How a page is flattened:
//! 1. The drawer writes the page as usual but leaves out every transparent construct
//!    (shadows, glows, feathers, opacity and blend modes, inner effects, partial
//!    transparency, images with alpha, placed PDFs with transparency) and records where it
//!    was ([`Site`]: its page rectangle and its place in the paint order, a
//!    [`UnitKey`]). Each top-level item's parts (behind / fill / content / stroke / inner
//!    effects) are units of paint order whose byte ranges in the content stream are noted
//!    ([`UnitRec`]); the stream's graphics state is forgotten at their boundaries, so any
//!    unit can later be wrapped in `q … Q`.
//! 2. The sites are spread on a grid of tiles aligned to the page (64 pixels at the smooth
//!    resolution, 15.36 pt at 300 ppi) and joined into connected regions ([`Cluster`]).
//!    A region's raster shows all paint up to its last transparent unit (`z_max`).
//! 3. Units up to `z_max` that reach a region are clipped to leave it out (`q`, a path of
//!    the region's tile rows with the even-odd rule, `W* n`, … `Q`); right after unit
//!    `z_max` the region's raster form is drawn; later units draw over it as vector, so text
//!    and line art above transparency stay live.
//! 4. The rasters are rendered on worker threads row by row of tiles: first at the smooth
//!    resolution (gradients and mesh, 300 ppi) with the hard edges of vector shapes
//!    recorded; tiles where an edge of line art or text crosses are rendered again at the
//!    line-art resolution (1200 ppi). Images overlap their neighbours and the region's edge
//!    by one of their own pixels (all on the page grid, same renderer), so no seam can open
//!    between rasters or between raster and vector. Effect masks are computed once per item
//!    at the smooth resolution and shared by all tiles.
//!
//! Overprint is simulated inside the rasters the way separations print it (an overprinting
//! colour leaves the other plates of what is below), and kept as OP / op / OPM 1 in the
//! vector parts. Text is never converted to outlines: text that transparency acts on is part
//! of the 1200 ppi raster, all other text stays text.

mod samples;
pub(crate) use samples::{SampleBuffer, TileEncoding};

use crate::content::Stream;
use folio_model::{Affine, Rect};
use folio_render::sep::{AreaOptions, DocInks, FxCache, InkImageSource, InkImages, UnitKey};
use std::collections::{HashMap, HashSet};

/// Tile side at the smooth resolution, pixels.
pub const TILE_PX: u32 = 64;
/// Sites are grown by this much (points) before they are spread on tiles.
const SITE_MARGIN: f64 = 0.5;

/// A rectangle in PDF page space (points, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct R {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl R {
    pub fn of(r: &Rect) -> R {
        R { x0: r.left.min(r.right), y0: r.top.min(r.bottom), x1: r.left.max(r.right), y1: r.top.max(r.bottom) }
    }
    fn intersects(&self, o: &R) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
    fn finite(&self) -> bool {
        self.x0.is_finite() && self.y0.is_finite() && self.x1.is_finite() && self.y1.is_finite()
    }
}

/// A transparent construct left out of the vector stream.
#[derive(Clone, Debug)]
pub struct Site {
    pub key: UnitKey,
    pub r: R,
}

/// One unit of the page's paint order in the content stream.
#[derive(Clone, Debug)]
pub struct UnitRec {
    pub key: UnitKey,
    /// Byte range in the page content.
    pub start: usize,
    pub end: usize,
    /// CTM at the unit (user space → page space).
    pub ctm: Affine,
    /// Page-space extent of the item the unit belongs to (`None`: unknown, assume all).
    pub extent: Option<R>,
}

/// What the drawer records on a page it flattens.
#[derive(Default)]
pub struct PageFlat {
    pub sites: Vec<Site>,
    pub units: Vec<UnitRec>,
    /// The top-level item being drawn: (layer rank, source, item).
    pub cur: (u32, u32, u32),
    /// Nesting of `draw_item` (1 = a top-level item).
    pub depth: usize,
    /// Page extent of the current top-level item.
    pub extent: Option<R>,
    /// Kinds of transparency seen (report).
    pub kinds: std::collections::BTreeMap<&'static str, usize>,
}

impl PageFlat {
    /// The key of the unit being drawn, if any.
    pub fn open_key(&self) -> Option<UnitKey> {
        self.units.last().filter(|u| u.end == usize::MAX).map(|u| u.key)
    }

    /// Starts unit `part` of the current top-level item (only at the top level).
    pub fn begin(&mut self, s: &mut Stream, part: u8) {
        if self.depth != 1 {
            return;
        }
        let (r, si, ii) = self.cur;
        s.forget_state();
        self.units.push(UnitRec { key: (r, si, ii, part), start: s.len(), end: usize::MAX, ctm: s.ctm(), extent: self.extent });
    }

    pub fn end(&mut self, s: &mut Stream) {
        if self.depth != 1 {
            return;
        }
        if let Some(u) = self.units.last_mut()
            && u.end == usize::MAX
        {
            u.end = s.len();
        }
        s.forget_state();
    }

    /// Records a transparent construct over `local` (current user space); the caller leaves
    /// it out.
    pub fn site(&mut self, s: &Stream, local: &Rect, kind: &'static str) {
        let r = R::of(&s.ctm().apply_rect(local));
        if !r.finite() {
            return;
        }
        let key = self.open_key().unwrap_or((self.cur.0, self.cur.1, self.cur.2, 0));
        self.sites.push(Site { key, r });
        *self.kinds.entry(kind).or_default() += 1;
    }
}

/// A connected area rasterised at once.
#[derive(Clone, Debug)]
pub struct Cluster {
    /// Its last transparent unit: the raster shows the paint up to it.
    pub z_max: UnitKey,
    /// Tile rows: (row j, first column, last column), runs of tiles, sorted. Rows count down
    /// from the top of the media box (rasterisers put their first pixel row there), columns
    /// from its left.
    pub runs: Vec<(i32, i32, i32)>,
    pub bbox: R,
    /// Height of the media box: page y of the grid's top.
    pub top: f64,
}

/// Page rectangle of tiles `i0..=i1` × rows `j0..=j1` under a grid whose top is `top`.
pub fn tile_rect(top: f64, tile: f64, i0: i32, j0: i32, i1: i32, j1: i32) -> R {
    R { x0: i0 as f64 * tile, y0: top - (j1 + 1) as f64 * tile, x1: (i1 + 1) as f64 * tile, y1: top - j0 as f64 * tile }
}

impl Cluster {
    fn rects(&self, tile: f64) -> impl Iterator<Item = R> + '_ {
        self.runs.iter().map(move |&(j, i0, i1)| tile_rect(self.top, tile, i0, j, i1, j))
    }
    fn touches(&self, r: &R, tile: f64) -> bool {
        self.bbox.intersects(r) && self.rects(tile).any(|t| t.intersects(r))
    }
}

/// Tile side in points.
pub fn tile_pt(smooth_ppi: f64) -> f64 {
    TILE_PX as f64 * 72.0 / smooth_ppi
}

/// The regions of a page with media size `media` (points).
pub fn clusters(flat: &PageFlat, media: [f64; 2], tile: f64) -> Vec<Cluster> {
    let (nx, ny) = ((media[0] / tile).ceil() as i32, (media[1] / tile).ceil() as i32);
    let top = media[1];
    // the tiles every site touches, with the latest unit there
    let mut tiles: HashMap<(i32, i32), UnitKey> = HashMap::new();
    for s in &flat.sites {
        let i0 = ((s.r.x0 - SITE_MARGIN) / tile).floor().max(0.0) as i32;
        let j0 = ((top - s.r.y1 - SITE_MARGIN) / tile).floor().max(0.0) as i32;
        let i1 = (((s.r.x1 + SITE_MARGIN) / tile).ceil() as i32 - 1).min(nx - 1);
        let j1 = (((top - s.r.y0 + SITE_MARGIN) / tile).ceil() as i32 - 1).min(ny - 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let e = tiles.entry((i, j)).or_insert(s.key);
                if s.key > *e {
                    *e = s.key;
                }
            }
        }
    }
    // connected components (tiles touching at an edge or a corner)
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    let mut keys: Vec<(i32, i32)> = tiles.keys().copied().collect();
    keys.sort_unstable_by_key(|&(i, j)| (j, i));
    let mut out = Vec::new();
    for start in keys {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut comp = Vec::new();
        let mut z = tiles[&start];
        while let Some((i, j)) = stack.pop() {
            comp.push((i, j));
            z = z.max(tiles[&(i, j)]);
            for dj in -1..=1 {
                for di in -1..=1 {
                    let n = (i + di, j + dj);
                    if tiles.contains_key(&n) && seen.insert(n) {
                        stack.push(n);
                    }
                }
            }
        }
        comp.sort_unstable_by_key(|&(i, j)| (j, i));
        let mut runs: Vec<(i32, i32, i32)> = Vec::new();
        for (i, j) in comp {
            match runs.last_mut() {
                Some(r) if r.0 == j && r.2 + 1 == i => r.2 = i,
                _ => runs.push((j, i, i)),
            }
        }
        let bbox = runs.iter().fold(R { x0: f64::MAX, y0: f64::MAX, x1: f64::MIN, y1: f64::MIN }, |b, &(j, i0, i1)| {
            let r = tile_rect(top, tile, i0, j, i1, j);
            R { x0: b.x0.min(r.x0), y0: b.y0.min(r.y0), x1: b.x1.max(r.x1), y1: b.y1.max(r.y1) }
        });
        out.push(Cluster { z_max: z, runs, bbox, top });
    }
    out
}

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" || s == "-" { "0".into() } else { s.to_owned() }
}

fn matrix(m: &Affine) -> String {
    format!("{} {} {} {} {} {} cm\n", num(m.a), num(m.b), num(m.c), num(m.d), num(m.tx), num(m.ty))
}

/// The page content with the regions clipped out of the units before their raster and the
/// raster forms drawn after their last transparent unit. `forms[k]` is the form index of
/// `clusters[k]`, `gs_off` an overprint-off ExtGState. Returns the content and the number
/// of units clipped.
pub fn assemble(content: &[u8], flat: &PageFlat, clusters: &[Cluster], forms: &[usize], gs_off: usize, media: [f64; 2], tile: f64) -> (Vec<u8>, usize) {
    // (offset, order, bytes): at one offset a unit's end, then rasters, then a unit's start
    let mut ins: Vec<(usize, u8, usize, Vec<u8>)> = Vec::new();
    let outer = R { x0: -1000.0, y0: -1000.0, x1: media[0] + 1000.0, y1: media[1] + 1000.0 };
    let mut clipped = 0;
    for (ui, u) in flat.units.iter().enumerate() {
        if u.end == usize::MAX || u.end <= u.start {
            continue;
        }
        let hit: Vec<&Cluster> = clusters.iter().filter(|c| u.key <= c.z_max && u.extent.is_none_or(|e| c.touches(&e, tile))).collect();
        if hit.is_empty() {
            continue;
        }
        let Some(inv) = u.ctm.invert() else { continue };
        // the stream may not end in white space where a unit starts
        let mut pre = String::from("\nq\n");
        pre.push_str(&matrix(&inv));
        let rect = |r: &R| format!("{} {} {} {} re\n", num(r.x0), num(r.y0), num(r.x1 - r.x0), num(r.y1 - r.y0));
        pre.push_str(&rect(&outer));
        for c in hit {
            for r in c.rects(tile) {
                pre.push_str(&rect(&r));
            }
        }
        pre.push_str("W* n\n");
        pre.push_str(&matrix(&u.ctm));
        ins.push((u.start, 2, ui, pre.into_bytes()));
        ins.push((u.end, 0, ui, b"\nQ\n".to_vec()));
        clipped += 1;
    }
    for (k, c) in clusters.iter().enumerate() {
        // after the region's last transparent unit (or, if it drew nothing, the last unit
        // before it)
        let at = flat.units.iter().enumerate().filter(|(_, u)| u.end != usize::MAX && u.key <= c.z_max).max_by_key(|(_, u)| u.key);
        let (offset, ctm, ui) = match at {
            Some((ui, u)) => (u.end, u.ctm, ui),
            None => (content.len(), Affine::IDENTITY, usize::MAX),
        };
        let Some(inv) = ctm.invert() else { continue };
        let text = format!("\nq\n/{} gs\n{}/{} Do\nQ\n", crate::content::gs_name(gs_off), matrix(&inv), crate::content::form_name(forms[k]));
        ins.push((offset, 1, ui, text.into_bytes()));
    }
    ins.sort_by_key(|(o, order, ui, _)| (*o, *order, *ui));
    let mut out = Vec::with_capacity(content.len() + ins.iter().map(|x| x.3.len()).sum::<usize>());
    let mut at = 0;
    for (o, _, _, bytes) in ins {
        let o = o.min(content.len());
        if o > at {
            out.extend_from_slice(&content[at..o]);
            at = o;
        }
        out.extend_from_slice(&bytes);
    }
    out.extend_from_slice(&content[at..]);
    (out, clipped)
}

/// A region to render: its tiles, the spread-to-page transform and the paint limit.
pub struct FlatJob {
    /// Document page whose spread is drawn (any page of the output page).
    pub page: usize,
    /// Spread coordinates → PDF page space.
    pub to_page: Affine,
    pub cluster: Cluster,
    /// Output page (report).
    pub out_page: usize,
    /// Resolution of its smooth tiles, decided after pass 1 ([`smooth_ppi_for`]).
    pub smooth_ppi: Option<f64>,
}

/// A raster written for a region.
pub struct TileImage {
    /// Placement in page space.
    pub r: R,
    /// What of it is shown: its tiles grown by one pixel of the smooth resolution (the ring a
    /// coarser image has is wider than that).
    pub clip: R,
    pub w: u32,
    pub h: u32,
    pub layers: Vec<TileLayer>,
    pub line_art: bool,
    /// Area covered (square points, without the overlap).
    pub area: f64,
}

pub struct TileLayer {
    /// Plates (indices into the document inks) the samples hold, in order; `[0, 1, 2, 3]`
    /// alone is DeviceCMYK.
    pub plates: Vec<usize>,
    /// Compressed interleaved samples.
    pub data: Vec<u8>,
    pub(crate) encoding: TileEncoding,
}

pub struct RenderCtx<'a> {
    pub scene: &'a folio_render::Scene<'a>,
    pub inks: &'a DocInks,
    pub ink_images: &'a InkImages,
    pub source: Option<&'a dyn InkImageSource>,
    pub smooth_ppi: f64,
    pub line_art_ppi: f64,
    pub level: u32,
}

/// Ink, alpha, an intermediate compositing layer, and coverage/image sampling buffers.
pub(crate) fn work_bytes(cols: i32, rows: i32, scale: f64, inks: usize) -> u64 {
    let pixels = |tiles| ((tiles as f64 * TILE_PX as f64 * scale).round() as u64).saturating_add(2);
    pixels(cols).saturating_mul(pixels(rows)).saturating_mul(5 * inks.max(4) as u64 + 12)
}

fn to_canvas(job: &FlatJob, x0: f64, y1: f64, scale: f64) -> Affine {
    let p = 1.0 / scale;
    Affine::new(scale, 0.0, 0.0, -scale, -(x0 - p) * scale, (y1 + p) * scale).then_inner(&job.to_page)
}

const DEVICE_N_MAX: usize = 8;

fn tile_layers(planes: &folio_render::sep::AreaPlanes, level: u32, smooth: bool, samples: &mut SampleBuffer) -> Vec<TileLayer> {
    let used: Vec<usize> = (0..planes.count).filter(|&p| planes.plane(p).iter().any(|&v| v != 0)).collect();
    if used.is_empty() {
        return Vec::new();
    }
    let plates = if used.iter().all(|&p| p < 4) { vec![0, 1, 2, 3] } else { used };
    plates.chunks(DEVICE_N_MAX).map(|group| {
        let (encoding, data) = samples.encode(planes, group, level, smooth && group == [0, 1, 2, 3]);
        TileLayer { plates: group.to_vec(), data, encoding }
    }).collect()
}

/// Pass 1 for one row of tiles of a region: rendered at the smooth resolution with the hard
/// edges of vector shapes recorded; returns, per tile, whether an edge crosses it or the
/// one-pixel ring its image overlaps its neighbours with (a smooth image never shows an
/// edge).
pub fn hard_tiles(ctx: &RenderCtx<'_>, job: &FlatJob, run: (i32, i32, i32), fx: &FxCache) -> RowEdges {
    let (j, i0, i1) = run;
    let tile = tile_pt(ctx.smooth_ppi);
    let row = tile_rect(job.cluster.top, tile, i0, j, i1, j);
    let (x0, y1) = (row.x0, row.y1);
    let s = ctx.smooth_ppi / 72.0;
    let n_tiles = (i1 - i0 + 1) as usize;
    let tp = TILE_PX as usize;
    let (w, h) = (n_tiles * tp + 2, tp + 2);
    let opts = AreaOptions { limit: Some(job.cluster.z_max), edges: true, effect_ppi: Some(ctx.smooth_ppi), fx: Some(fx), source: ctx.source, select: None };
    let Some(lo) = folio_render::sep::render_area(ctx.scene, ctx.ink_images, ctx.inks, job.page, &to_canvas(job, x0, y1, s), w as u32, h as u32, &opts) else {
        return RowEdges { hard: vec![true; n_tiles], image_detail: 1.0, soft: true };
    };
    let hard = (0..n_tiles).map(|k| lo.edges.as_ref().is_some_and(|e| (0..tp + 2).any(|y| e[y * w + k * tp..y * w + (k + 1) * tp + 2].iter().any(|v| *v != 0)))).collect();
    RowEdges { hard, image_detail: lo.stats.image_detail, soft: lo.stats.soft_paints > 0 }
}

/// What pass 1 found in a row of tiles.
pub struct RowEdges {
    /// Per tile: a hard edge crosses it (or its ring).
    pub hard: Vec<bool>,
    /// Image pixels per pixel at the smooth resolution (0 without images).
    pub image_detail: f64,
    /// Effects or gradients were painted.
    pub soft: bool,
}

/// The resolution a region's smooth tiles need: the full smooth resolution where effects or
/// gradients are painted or images are finer than half of it; a half or a quarter where
/// the smooth content is only coarser images (e.g. a 72 ppi preview) and flat colour, which a
/// finer raster would only repeat.
pub fn smooth_ppi_for(base: f64, image_detail: f64, soft: bool) -> f64 {
    if soft {
        return base;
    }
    let image_ppi = image_detail * base;
    if image_ppi <= base / 4.0 * 1.1 {
        base / 4.0
    } else if image_ppi <= base / 2.0 * 1.1 {
        base / 2.0
    } else {
        base
    }
}

/// A rectangle of tiles rendered as one image: columns `i0..=i1`, rows `j0..=j1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    pub i0: i32,
    pub j0: i32,
    pub i1: i32,
    pub j1: i32,
    pub line_art: bool,
}

/// Largest images, tiles: line art 8 × 4 (2048 × 1024 pixels at 1200 ppi), smooth 16 × 16
/// (1024 × 1024 at 300 ppi).
const LINE_ART_MAX: (i32, i32) = (8, 4);
const SMOOTH_MAX: (i32, i32) = (16, 16);

/// The region's tiles grouped into rectangles of one resolution (fewer, larger images: fewer
/// boundaries for a viewer to draw).
pub fn plan_rects(job: &FlatJob, hard: &HashMap<(i32, i32), bool>) -> Vec<TileRect> {
    let mut left: HashMap<(i32, i32), bool> = HashMap::new();
    for &(j, i0, i1) in &job.cluster.runs {
        for i in i0..=i1 {
            left.insert((i, j), hard.get(&(i, j)).copied().unwrap_or(true));
        }
    }
    let mut keys: Vec<(i32, i32)> = left.keys().copied().collect();
    keys.sort_unstable_by_key(|&(i, j)| (j, i));
    let mut out = Vec::new();
    for (i, j) in keys {
        let Some(&kind) = left.get(&(i, j)) else { continue };
        let (mw, mh) = if kind { LINE_ART_MAX } else { SMOOTH_MAX };
        let mut i1 = i;
        while i1 + 1 - i < mw && left.get(&(i1 + 1, j)) == Some(&kind) {
            i1 += 1;
        }
        let mut j1 = j;
        while j1 + 1 - j < mh && (i..=i1).all(|x| left.get(&(x, j1 + 1)) == Some(&kind)) {
            j1 += 1;
        }
        for y in j..=j1 {
            for x in i..=i1 {
                left.remove(&(x, y));
            }
        }
        out.push(TileRect { i0: i, j0: j, i1, j1, line_art: kind });
    }
    out
}

/// Pass 2: one rectangle of a region rendered at its resolution, with a one-pixel ring
/// around it; `None` when it holds no ink.
pub(crate) fn render_rect(ctx: &RenderCtx<'_>, job: &FlatJob, r: TileRect, fx: &FxCache, samples: &mut SampleBuffer) -> Option<TileImage> {
    let tile = tile_pt(ctx.smooth_ppi);
    let t = tile_rect(job.cluster.top, tile, r.i0, r.j0, r.i1, r.j1);
    let (xa, xb, ya, yb) = (t.x0, t.x1, t.y0, t.y1);
    let ppi = if r.line_art { ctx.line_art_ppi } else { job.smooth_ppi.unwrap_or(ctx.smooth_ppi) };
    let ring = 72.0 / ctx.smooth_ppi;
    let s = ppi / 72.0;
    let p = 1.0 / s;
    let w = ((xb - xa) * s).round() as usize + 2;
    let h = ((yb - ya) * s).round() as usize + 2;
    let opts = AreaOptions { limit: Some(job.cluster.z_max), edges: false, effect_ppi: Some(ctx.smooth_ppi), fx: Some(fx), source: ctx.source, select: None };
    let planes = folio_render::sep::render_area(ctx.scene, ctx.ink_images, ctx.inks, job.page, &to_canvas(job, xa, yb, s), w as u32, h as u32, &opts)?;
    let layers = tile_layers(&planes, ctx.level, !r.line_art, samples);
    if layers.is_empty() {
        return None;
    }
    Some(TileImage {
        r: R { x0: xa - p, y0: ya - p, x1: xb + p, y1: yb + p },
        clip: R { x0: xa - ring.min(p), y0: ya - ring.min(p), x1: xb + ring.min(p), y1: yb + ring.min(p) },
        w: w as u32,
        h: h as u32,
        layers,
        line_art: r.line_art,
        area: (xb - xa) * (yb - ya),
    })
}

/// The flattener's images: a graphic's found original (or embedded data) decoded to ink with
/// the conversion the vector output uses ([`crate::images::decode_ink`]), so a photo cut by a
/// region's edge prints the same numbers on both sides; previews otherwise (the separations
/// renderer's own decoding). Decoded images are kept while they fit in `budget` bytes.
type ImageKey = (crate::graphics::ImageSource, Option<folio_model::image_color::ImageColor>);
type CachedInkImage = std::sync::Arc<std::sync::OnceLock<Option<std::sync::Arc<folio_render::sep::InkImage>>>>;

pub struct PrintImages<'a> {
    entries: &'a HashMap<String, folio_links::LinkEntry>,
    conv: &'a crate::images::Conv,
    cache: std::sync::Mutex<HashMap<ImageKey, CachedInkImage>>,
    held: std::sync::atomic::AtomicU64,
    budget: u64,
    decode_budget: crate::memory::Budget,
    pub failures: std::sync::Mutex<Vec<String>>,
    pub decoded: std::sync::atomic::AtomicUsize,
}

impl<'a> PrintImages<'a> {
    pub(crate) fn new(entries: &'a HashMap<String, folio_links::LinkEntry>, conv: &'a crate::images::Conv, budget: u64) -> Self {
        PrintImages {
            entries,
            conv,
            cache: Default::default(),
            held: Default::default(),
            budget,
            decode_budget: crate::memory::Budget::new(budget),
            failures: Default::default(),
            decoded: Default::default(),
        }
    }

    fn decode(&self, src: &crate::graphics::ImageSource, color: Option<&folio_model::image_color::ImageColor>) -> Option<std::sync::Arc<folio_render::sep::InkImage>> {
        match crate::images::decode_ink(src, self.conv, color) {
            Ok((w, h, px)) => {
                let img = folio_render::sep::InkImage::from_samples(w, h, px)?;
                self.decoded.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Some(std::sync::Arc::new(img))
            }
            Err(e) => {
                self.failures.lock().expect("PDF worker/cache lock is not poisoned").push(e);
                None
            }
        }
    }
}

impl InkImageSource for PrintImages<'_> {
    fn image(&self, g: &folio_model::Graphic) -> Option<std::sync::Arc<folio_render::sep::InkImage>> {
        let crate::graphics::RasterSource::Image(src) = crate::graphics::raster_source(self.entries, g) else { return None };
        let cell = {
            let mut c = self.cache.lock().expect("PDF worker/cache lock is not poisoned");
            use std::sync::atomic::Ordering::Relaxed;
            if self.held.load(Relaxed) > self.budget {
                c.retain(|_, cell| {
                    if self.held.load(Relaxed) <= self.budget || std::sync::Arc::strong_count(cell) != 1 {
                        return true;
                    }
                    let Some(Some(image)) = cell.get() else { return true };
                    if std::sync::Arc::strong_count(image) != 1 {
                        return true;
                    }
                    self.held.fetch_sub(reserved_bytes(image), Relaxed);
                    false
                });
            }
            c.entry((src.clone(), g.image_color.clone())).or_default().clone()
        };
        cell.get_or_init(|| {
            let info = self.entries.get(&g.id).and_then(|e| e.info.as_ref()?.as_ref().ok());
            // Decoded channels and working copies. Unknown dimensions run alone.
            let cost = info.map_or(u64::MAX, |i| u64::from(i.width).saturating_mul(u64::from(i.height)).saturating_mul(24));
            self.decode_budget.run(cost, || {
                let img = self.decode(&src, g.image_color.as_ref());
                if let Some(i) = &img {
                    self.held.fetch_add(reserved_bytes(i), std::sync::atomic::Ordering::Relaxed);
                }
                img
            })
        })
        .clone()
    }
}

fn reserved_bytes(image: &folio_render::sep::InkImage) -> u64 {
    // Reserve the original five channels and the smaller levels built during resampling.
    (u64::from(image.width()) * u64::from(image.height())).saturating_mul(20) / 3
}

/// Content of a region's form: its rasters, each `w 0 0 h x y cm /Rn Do`, smooth ones first
/// so the line-art rasters cover the ring smooth ones overlap them with.
pub fn form_content(tiles: &[TileImage]) -> Vec<u8> {
    let mut order: Vec<(usize, &TileImage)> = tiles.iter().enumerate().collect();
    order.sort_by_key(|(i, t)| (t.line_art, *i));
    let mut s = String::new();
    for (i, t) in order {
        let (r, c) = (t.r, t.clip);
        // a coarse image is cut to its tiles and a hair around them
        if c != r {
            s.push_str(&format!("q {} {} {} {} re W n ", num(c.x0), num(c.y0), num(c.x1 - c.x0), num(c.y1 - c.y0)));
        } else {
            s.push_str("q ");
        }
        s.push_str(&format!("{} 0 0 {} {} {} cm /R{} Do", num(r.x1 - r.x0), num(r.y1 - r.y0), num(r.x0), num(r.y0), i));
        if t.layers.len() > 1 {
            // The first group knocks out the backdrop. The other groups add their own
            // plates without clearing any of the ink the first group painted.
            s.push_str(" /Inks gs");
            for l in 1..t.layers.len() {
                s.push_str(&format!(" /R{i}_{l} Do"));
            }
        }
        s.push_str(" Q\n");
    }
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    type FlatSite = ((u32, u32, u32, u8), R);

    fn flat_with(sites: &[FlatSite]) -> PageFlat {
        PageFlat { sites: sites.iter().map(|(k, r)| Site { key: *k, r: *r }).collect(), ..Default::default() }
    }

    #[test]
    fn sites_join_into_regions_on_the_tile_grid() {
        let t = tile_pt(300.0);
        assert!((t - 15.36).abs() < 1e-9);
        // two overlapping sites and one far away
        let f = flat_with(&[
            ((0, 1, 2, 0), R { x0: 10.0, y0: 10.0, x1: 40.0, y1: 20.0 }),
            ((0, 1, 5, 1), R { x0: 35.0, y0: 15.0, x1: 50.0, y1: 25.0 }),
            ((0, 1, 3, 0), R { x0: 300.0, y0: 300.0, x1: 310.0, y1: 305.0 }),
        ]);
        // a page whose height is a whole number of tiles (rows count from the top)
        let hgt = 55.0 * t;
        let c = clusters(&f, [595.0, hgt], t);
        assert_eq!(c.len(), 2);
        let a = c.iter().find(|c| c.bbox.x0 < 100.0).unwrap();
        assert_eq!(a.z_max, (0, 1, 5, 1));
        // tiles 0..=3 in x (0–61.4 pt), the two bottom rows
        assert_eq!(a.runs, vec![(53, 0, 3), (54, 0, 3)]);
        assert!((a.bbox.y0).abs() < 1e-9 && (a.bbox.y1 - 2.0 * t).abs() < 1e-9);
        let b = c.iter().find(|c| c.bbox.x0 > 100.0).unwrap();
        assert_eq!(b.runs, vec![(35, 19, 20)]);
    }

    #[test]
    fn units_up_to_the_last_transparent_unit_are_clipped_and_the_raster_follows_it() {
        let t = tile_pt(300.0);
        let mut f = flat_with(&[((0, 0, 1, 0), R { x0: 5.0, y0: 5.0, x1: 10.0, y1: 10.0 })]);
        let content = b"AAAABBBBCCCC".to_vec();
        let near = Some(R { x0: 0.0, y0: 0.0, x1: 20.0, y1: 20.0 });
        f.units = vec![
            UnitRec { key: (0, 0, 0, 1), start: 0, end: 4, ctm: Affine::IDENTITY, extent: near },
            UnitRec { key: (0, 0, 1, 0), start: 4, end: 8, ctm: Affine::IDENTITY, extent: near },
            UnitRec { key: (0, 0, 2, 2), start: 8, end: 12, ctm: Affine::IDENTITY, extent: near },
        ];
        let c = clusters(&f, [100.0, 7.0 * t], t);
        let (out, clipped) = assemble(&content, &f, &c, &[7], 3, [100.0, 7.0 * t], t);
        let s = String::from_utf8(out).unwrap();
        // unit 0 is clipped (below the raster), unit 2 is not (above it)
        assert_eq!(clipped, 2);
        assert!(s.starts_with("\nq\n1 0 0 1 0 0 cm\n-1000 -1000 2100 2107.52 re\n0 0 15.36 15.36 re\nW* n\n1 0 0 1 0 0 cm\nAAAA\nQ\n"), "{s}");
        let raster = s.find("/X7 Do").unwrap();
        assert!(raster > s.find("BBBB").unwrap() && raster < s.find("CCCC").unwrap(), "{s}");
        assert!(s.ends_with("CCCC"), "{s}");
    }
}
