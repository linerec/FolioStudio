//! Locates pages and renders their master and spread items to a bitmap.

mod items;

use crate::{DisplayOptions, GlyphCache, ImageCache, page_region};
use folio_color::{BlendSpace, Colors};
use folio_model::layout::{DocumentLayout, GlyphSource};
use folio_model::{Affine, BlendMode, Document, ItemKind, Page, PageItem, Rect, Spread};
use items::{draw_items, has_effects};
use std::collections::HashMap;
use std::time::Instant;
use tiny_skia::Pixmap;

pub(super) use items::with_clip_layer;

/// RGBA bitmap, straight (non-premultiplied) alpha; pages are opaque.
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub stats: RenderStats,
}

#[derive(Clone, Debug, Default)]
pub struct RenderStats {
    pub items: usize,
    pub glyphs: usize,
    pub images: usize,
    pub missing_images: usize,
    /// Clip layers built (each sized to its frame's device bounds).
    pub layers: usize,
    /// Items skipped because they lie outside the bitmap.
    pub culled: usize,
    /// Graphics drawn as a pattern fill of their frame (no mask).
    pub pattern_images: usize,
    pub elapsed_ms: f64,
}

/// Largest bitmap one render may produce (pixels); zoomed views render regions instead.
pub const MAX_PIXELS: u64 = 48_000_000;

/// Where a page is: its spread and its rectangle in spread coordinates.
pub fn locate_page(doc: &Document, index: usize) -> Option<(&Spread, &Page, Rect)> {
    for spread in doc.spreads.iter().chain(&doc.master_spreads) {
        for page in &spread.pages {
            if page.index == index {
                return Some((spread, page, page.transform.apply_rect(&page.bounds)));
            }
        }
    }
    None
}

pub fn page_count(doc: &Document) -> usize {
    doc.spreads.iter().map(|s| s.pages.len()).sum()
}

struct Ctx<'a> {
    doc: &'a Document,
    colors: &'a Colors,
    layout: &'a DocumentLayout,
    glyphs: &'a dyn GlyphSource,
    images: &'a ImageCache,
    glyph_paths: &'a GlyphCache,
    layers: HashMap<&'a str, (usize, bool)>,
    /// Per-page text of master frames (page numbers) for the page being drawn.
    page_text: HashMap<&'a str, &'a folio_model::layout::FrameText>,
    /// The page's transparency blend space (see `page_blend_space`).
    blend: BlendSpace,
    /// Items anchored in text, by id (drawn where the layout placed them).
    inline: HashMap<&'a str, &'a PageItem>,
    /// Master items this page does not show (overridden by a page copy, or removed).
    hidden: std::collections::HashSet<&'a str>,
    stats: RenderStats,
    /// Display Performance (screen only; see `display.rs`).
    display: DisplayOptions,
}

/// What to draw: everything that `render_page` needs besides the page and scale.
pub struct Scene<'a> {
    pub doc: &'a Document,
    /// The document's display conversions (`folio_color::document_colors`).
    pub colors: &'a Colors,
    pub layout: &'a DocumentLayout,
    pub glyphs: &'a dyn GlyphSource,
    pub glyph_cache: &'a GlyphCache,
    pub images: &'a ImageCache,
}

/// Renders page `index` at `scale` pixels per point. `region` (page coordinates, points,
/// origin at the page's top left) limits the bitmap to that part of the page, so a zoomed
/// view costs only what is on screen.
pub fn render_page(scene: &Scene, index: usize, scale: f64, region: Option<Rect>) -> Option<Raster> {
    render_page_display(scene, index, scale, region, DisplayOptions::default())
}

/// [`render_page`] as the window shows it: View ▸ Display Performance draws graphics as grey boxes in Fast Display.
pub fn render_page_display(scene: &Scene, index: usize, scale: f64, region: Option<Rect>, display: DisplayOptions) -> Option<Raster> {
    let started = Instant::now();
    let doc = scene.doc;
    let (spread, page, bbox) = locate_page(doc, index)?;
    let area = page_region(doc, index, region)?;
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return None;
    }
    let width = (area.width() * scale).round().max(1.0) as u32;
    let height = (area.height() * scale).round().max(1.0) as u32;
    if width as u64 * height as u64 > MAX_PIXELS {
        return None;
    }
    let mut pixmap = Pixmap::new(width, height)?;
    let [r, g, b] = scene.colors.paper_display();
    pixmap.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
    // spread coordinates → pixels of the requested area
    let base = Affine::scale(scale, scale).then_inner(&Affine::translate(-(bbox.left + area.left), -(bbox.top + area.top)));
    let (layout, glyphs, images) = (scene.layout, scene.glyphs, scene.images);

    let mut ctx = Ctx {
        doc,
        colors: scene.colors,
        layout,
        glyphs,
        images,
        glyph_paths: scene.glyph_cache,
        layers: doc.layers.iter().map(|l| (l.id.as_str(), (l.order, l.visible))).collect(),
        page_text: layout.page_frames.iter().filter(|p| p.page == index).map(|p| (p.frame.frame.as_str(), &p.frame)).collect(),
        blend: BlendSpace::Rgb,
        inline: doc
            .stories
            .values()
            .flat_map(|s| s.all_runs())
            .flat_map(|r| &r.inline)
            .map(|i| (i.id.as_str(), i))
            .collect(),
        hidden: page.master_overrides.iter().map(String::as_str).collect(),
        stats: RenderStats::default(),
        display,
    };

    let parents = folio_model::parents::sources(doc, spread, page);
    let page_px = base.apply_rect(&bbox);
    let mut layers: Vec<(&[PageItem], Affine)> = parents.iter().map(|s| (s.spread.items.as_slice(), base.then_inner(&s.transform))).collect();
    layers.push((&spread.items, base));
    ctx.blend = page_blend_space(&ctx, &layers, &page_px);
    for source in &parents {
        ctx.hidden = source.hidden.iter().copied().collect();
        draw_items(&mut ctx, &mut pixmap, &source.spread.items, &base.then_inner(&source.transform), None);
    }
    ctx.hidden.clear();
    draw_items(&mut ctx, &mut pixmap, &spread.items, &base, None);

    let mut stats = ctx.stats;
    stats.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    // Pages are opaque, so premultiplied equals straight alpha.
    Some(Raster { width, height, rgba: pixmap.take(), stats })
}

/// InDesign composites a page with transparency in its transparency blend space, by default
/// "Document CMYK" (RGB content then shows as converted to CMYK). A page has transparency
/// when an item drawn on it has opacity below 100 %, a blend mode or effects, places a PDF or EPS
/// (taken to carry transparency), or places an image with an alpha channel.
fn page_blend_space(ctx: &Ctx, layers: &[(&[PageItem], Affine)], page_px: &Rect) -> BlendSpace {
    fn visit(ctx: &Ctx, items: &[PageItem], parent: &Affine, page: &Rect) -> bool {
        items.iter().any(|item| {
            let visible = item.layer.as_deref().and_then(|l| ctx.layers.get(l)).is_none_or(|&(_, v)| v);
            if !visible || !item.visible || ctx.hidden.contains(item.id.as_str()) {
                return false;
            }
            let t = parent.then_inner(&item.transform);
            if let ItemKind::Group { children } = &item.kind {
                return ((item.opacity < 1.0 || item.blend_mode != BlendMode::Normal) && group_touches(children, &t, page))
                    || visit(ctx, children, &t, page);
            }
            let Some(b) = folio_model::paths_bounds(&item.paths) else { return false };
            let frame = t.apply_rect(&b);
            if !touches(&frame, page) {
                return false;
            }
            if item.opacity < 1.0 || item.blend_mode != BlendMode::Normal || has_effects(item) {
                return true;
            }
            let graphic = match &item.kind {
                ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => Some(g),
                _ => None,
            };
            // A placed graphic counts where it shows: its bounds inside the frame.
            if let Some(g) = graphic
                && touches(&intersection(&t.then_inner(&g.transform).apply_rect(&g.bounds), &frame), page)
                && (matches!(g.element.as_str(), "PDF" | "EPS" | "ImportedPage")
                    || g.preview.as_deref().is_some_and(|f| ctx.images.has_alpha(f)))
            {
                return true;
            }
            visit(ctx, &item.clipped, &t, page)
        })
    }
    fn intersection(a: &Rect, b: &Rect) -> Rect {
        Rect { left: a.left.max(b.left), top: a.top.max(b.top), right: a.right.min(b.right), bottom: a.bottom.min(b.bottom) }
    }
    fn touches(r: &Rect, page: &Rect) -> bool {
        r.left < page.right - 0.01 && page.left + 0.01 < r.right && r.top < page.bottom - 0.01 && page.top + 0.01 < r.bottom
    }
    fn group_touches(children: &[PageItem], t: &Affine, page: &Rect) -> bool {
        children.iter().any(|c| {
            let ct = t.then_inner(&c.transform);
            match &c.kind {
                ItemKind::Group { children } => group_touches(children, &ct, page),
                _ => folio_model::paths_bounds(&c.paths).is_some_and(|b| touches(&ct.apply_rect(&b), page)),
            }
        })
    }
    if layers.iter().any(|(items, t)| visit(ctx, items, t, page_px)) { BlendSpace::Cmyk } else { BlendSpace::Rgb }
}
