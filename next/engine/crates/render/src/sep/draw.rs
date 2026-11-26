//! A page drawn on ink planes in the PDF writer's order and with its rules (`folio_pdf`
//! `draw.rs`): sources layer by layer (each page of the spread's master, then the spread),
//! nonprinting items left out, master items on the master page their centre lies on, a frame
//! as fill, placed graphic and pasted-in items clipped to it, text and inline objects, then
//! its stroke; overprint as the PDF sets it. Geometry, effect masks and image resampling are
//! the screen renderer's.

mod item_effects;
mod text;

use super::canvas::{Canvas, Cov, Pixels, Src, fill_cov, pixel_box, stroke_cov};
use super::{AreaOptions, DocInks, InkImages, SepStats, UnitKey, part};
use crate::geometry::{gradient_shader, item_fill_rule};
use crate::inks::{Col, CsDef, Resolved, Resolver};
use crate::{Scene, build_path, effects, item_extent, locate_page, skia_stroke, to_skia};
use folio_color::Colors;
use folio_model::geom::{PartShape, StrokeInk};
use folio_model::layout::{DocumentLayout, FrameText, GlyphSource};
use folio_model::{
    Affine, BlendMode, Document, GradientGeometry, Graphic, ItemKind, Page, PageItem, Paint, Rect,
    Spread,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use tiny_skia::{FillRule, PathBuilder, Pixmap};

/// A colour as plate values (0–255 per plate) and the plates it paints when it overprints:
/// a DeviceCMYK colour its non-zero inks (overprint mode 1), a spot colour its plate, a
/// mixed ink its inks, other colours (RGB, Lab, gray) all four process plates.
#[derive(Clone, Debug)]
pub(crate) struct PlaneCol {
    pub v: Vec<u8>,
    pub own: Vec<bool>,
    pub black100: bool,
}

/// Plate values of a resolved colour.
pub(crate) fn plane_col(r: &Resolved, res: &Resolver, inks: &DocInks, colors: &Colors, n: usize) -> PlaneCol {
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut v = vec![0u8; n];
    let mut own = vec![false; n];
    let process = |v: &mut Vec<u8>, own: &mut Vec<bool>, c: [u8; 4], nonzero_only: bool| {
        for k in 0..4 {
            v[k] = c[k];
            own[k] = !nonzero_only || c[k] > 0;
        }
    };
    // an ink the plate list does not have: its alternate folded into process
    let fold = |v: &mut Vec<u8>, own: &mut Vec<bool>, cmyk: [f32; 4], t: f32| {
        for k in 0..4 {
            let have = v[k] as f32 / 255.0;
            v[k] = q(1.0 - (1.0 - have) * (1.0 - cmyk[k] * t));
            own[k] = true;
        }
    };
    match &r.col {
        Col::Cmyk(c) => process(&mut v, &mut own, c.map(q), true),
        Col::Gray(g) => process(&mut v, &mut own, [0, 0, 0, q(1.0 - g)], false),
        Col::Space(i, vals) => match res.spaces.defs.get(*i) {
            Some(CsDef::Separation { ink, .. }) if ink == "All" => {
                v.fill(q(vals.first().copied().unwrap_or(0.0)));
                own.fill(true);
            }
            Some(CsDef::Separation { ink, alt }) => {
                let t = vals.first().copied().unwrap_or(0.0);
                match inks.plane(ink) {
                    Some(p) if p < n => {
                        v[p] = q(t);
                        own[p] = true;
                    }
                    _ => fold(&mut v, &mut own, alt.approx_cmyk(), t),
                }
            }
            Some(CsDef::DeviceN { inks: names, cmyk }) => {
                for (k, name) in names.iter().enumerate() {
                    let t = vals.get(k).copied().unwrap_or(0.0);
                    match inks.plane(name) {
                        Some(p) if p < n => {
                            v[p] = q(t);
                            own[p] = true;
                        }
                        _ => fold(&mut v, &mut own, cmyk.get(k).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]), t),
                    }
                }
            }
            Some(CsDef::Lab) => {
                let lab = [0, 1, 2].map(|k| vals.get(k).copied().unwrap_or(0.0) as f64);
                process(&mut v, &mut own, colors.srgb_to_cmyk8(folio_color::lab_to_srgb(lab)), false);
            }
            Some(CsDef::IccRgb) | None => {
                let rgb = [0, 1, 2].map(|k| q(vals.get(k).copied().unwrap_or(1.0)));
                process(&mut v, &mut own, colors.rgb_to_cmyk8(rgb), false);
            }
        },
    }
    PlaneCol { v, own, black100: r.black100 }
}

/// Items of one spread (or of a master spread mapped onto a page).
struct Source<'d> {
    items: &'d [PageItem],
    /// Spread (or master spread) coordinates → canvas pixels.
    transform: Affine,
    /// Master items: the master page's x range (master spread coordinates) and the document
    /// page they are drawn for.
    master: Option<(Rect, usize)>,
    hidden: Vec<&'d str>,
}

fn master_sources<'d>(doc: &'d Document, spread: &'d Spread, page: &'d Page, base: &Affine) -> Vec<Source<'d>> {
    folio_model::parents::sources(doc, spread, page).into_iter().map(|s| Source {
        items: &s.spread.items, transform: base.then_inner(&s.transform), master: Some((s.range, page.index)), hidden: s.hidden,
    }).collect()
}

struct Ctx<'a> {
    doc: &'a Document,
    colors: &'a Colors,
    layout: &'a DocumentLayout,
    glyphs: &'a dyn GlyphSource,
    glyph_paths: &'a crate::GlyphCache,
    images: &'a InkImages,
    inks: &'a DocInks,
    res: Resolver<'a>,
    cols: HashMap<(String, u32), Option<Rc<PlaneCol>>>,
    /// Layer id → (stacking rank, printed).
    layers: HashMap<&'a str, (usize, bool)>,
    inline: HashMap<&'a str, &'a PageItem>,
    page_text: HashMap<&'a str, &'a FrameText>,
    hidden: HashSet<&'a str>,
    overprint_black: bool,
    all: Vec<bool>,
    n: usize,
    stats: SepStats,
    /// Paint-order limit (flattening): top-level parts after it are not drawn.
    limit: Option<UnitKey>,
    /// The top-level item being drawn: (layer rank, source, item).
    cur: (u32, u32, u32),
    /// Nesting of `draw_item` (1 = a top-level item).
    depth: usize,
    /// Effect masks at this many pixels per spread point instead of the canvas resolution.
    fx_per_point: Option<f64>,
    fx: Option<&'a super::FxCache>,
    source: Option<&'a dyn super::InkImageSource>,
    /// The document page the sources are drawn for (effect cache keys).
    page: usize,
    /// Flattener Preview: only these top-level items (key -> bit per [`part`]) are drawn, as coverage.
    sel: Option<&'a HashMap<(u32, u32, u32), u8>>,
    /// Flattener Preview survey: records the facts of every printed top-level item instead of drawing.
    survey: Option<Vec<super::flat::Facts>>,
}

/// Draws page `index` of `scene` into a `w × h` canvas showing `area` (page coordinates) at
/// `scale`.
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_page(scene: &Scene, images: &InkImages, inks: &DocInks, index: usize, scale: f64, area: &Rect, w: usize, h: usize) -> Option<(Canvas, SepStats)> {
    let (_, _, bbox) = locate_page(scene.doc, index)?;
    let base = Affine::scale(scale, scale).then_inner(&Affine::translate(-(bbox.left + area.left), -(bbox.top + area.top)));
    draw_area(scene, images, inks, index, &base, w, h, &AreaOptions::default())
}

/// Draws the spread of page `index` (every page's master, then the spread's items) into a
/// `w × h` canvas, `base` mapping spread coordinates to canvas pixels.
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_area<'a>(
    scene: &'a Scene<'a>,
    images: &'a InkImages,
    inks: &'a DocInks,
    index: usize,
    base: &Affine,
    w: usize,
    h: usize,
    opts: &AreaOptions<'a>,
) -> Option<(Canvas, SepStats)> {
    let doc = scene.doc;
    let (spread, _, _) = locate_page(doc, index)?;
    let base = *base;
    let n = inks.len().max(4);
    let mut ctx = Ctx {
        doc,
        colors: scene.colors,
        layout: scene.layout,
        glyphs: scene.glyphs,
        glyph_paths: scene.glyph_cache,
        images,
        inks,
        res: Resolver::new(doc),
        cols: HashMap::new(),
        layers: doc.layers.iter().map(|l| (l.id.as_str(), (l.order, l.visible && l.printable))).collect(),
        inline: doc.stories.values().flat_map(|s| s.all_runs()).flat_map(|r| &r.inline).map(|i| (i.id.as_str(), i)).collect(),
        page_text: HashMap::new(),
        hidden: HashSet::new(),
        overprint_black: doc.print.overprint_black,
        all: vec![true; n],
        n,
        stats: SepStats::default(),
        limit: opts.limit,
        cur: (0, 0, 0),
        depth: 0,
        fx_per_point: opts.effect_ppi.map(|ppi| {
            // canvas pixels per spread point
            let s = (base.a * base.d - base.b * base.c).abs().sqrt().max(1e-9);
            ppi / 72.0 / s
        }),
        fx: opts.fx,
        source: opts.source,
        page: index,
        sel: opts.select,
        survey: None,
    };
    // The page is a transparency group with a transparent backdrop, paper composited last
    // (PDF): a blend mode acts only over what is painted below it — Screen over empty paper
    // shows the object as it is, as in InDesign's PDF.
    let mut canvas = Canvas::layer(w, h, n);
    canvas.mask = opts.select.is_some();
    if opts.edges {
        canvas.edges = Some(vec![0; w * h]);
    }
    // every page's master: facing pages' masters show at the spine and in the bleed
    let mut sources: Vec<Source> = spread.pages.iter().flat_map(|pg| master_sources(doc, spread, pg, &base)).collect();
    sources.push(Source { items: &spread.items, transform: base, master: None, hidden: Vec::new() });
    ctx.draw_sources(&mut canvas, &sources);
    Some((canvas, ctx.stats))
}

/// The facts of every printed top-level item of the spread of page `index`, in paint order, in
/// spread coordinates (Flattener Preview).
pub(crate) fn survey_area<'a>(scene: &'a Scene<'a>, images: &'a InkImages, inks: &'a DocInks, index: usize) -> Option<Vec<super::flat::Facts>> {
    let doc = scene.doc;
    let (spread, _, _) = locate_page(doc, index)?;
    let base = Affine::IDENTITY;
    let n = inks.len().max(4);
    let mut ctx = Ctx {
        doc, colors: scene.colors, layout: scene.layout, glyphs: scene.glyphs, glyph_paths: scene.glyph_cache, images, inks, res: Resolver::new(doc),
        cols: HashMap::new(),
        layers: doc.layers.iter().map(|l| (l.id.as_str(), (l.order, l.visible && l.printable))).collect(),
        inline: HashMap::new(), page_text: HashMap::new(), hidden: HashSet::new(), overprint_black: false, all: vec![true; n], n,
        stats: SepStats::default(), limit: None, cur: (0, 0, 0), depth: 0, fx_per_point: None, fx: None, source: None, page: index,
        sel: None, survey: Some(Vec::new()),
    };
    let mut canvas = Canvas::layer(1, 1, n);
    let mut sources: Vec<Source> = spread.pages.iter().flat_map(|pg| master_sources(doc, spread, pg, &base)).collect();
    sources.push(Source { items: &spread.items, transform: base, master: None, hidden: Vec::new() });
    ctx.draw_sources(&mut canvas, &sources);
    ctx.survey
}

/// Draws the selected parts of the spread's top-level items as coverage: plane 0 holds 255
/// where a selected paint lands (soft edges kept); every other item is left out.
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
pub(crate) fn coverage_area<'a>(scene: &'a Scene<'a>, images: &'a InkImages, inks: &'a DocInks, index: usize, base: &Affine, w: usize, h: usize, sel: &'a HashMap<(u32, u32, u32), u8>) -> Option<Vec<u8>> {
    let opts = AreaOptions { select: Some(sel), ..AreaOptions::default() };
    let (canvas, _) = draw_area(scene, images, inks, index, base, w, h, &opts)?;
    Some(canvas.ink[..w * h].to_vec())
}

const NORMAL: (f64, BlendMode) = (1.0, BlendMode::Normal);

impl<'a> Ctx<'a> {
    fn rank(&self, item: &PageItem) -> Option<usize> {
        match item.layer.as_deref().and_then(|l| self.layers.get(l)) {
            Some(&(rank, printed)) => printed.then_some(rank),
            None => Some(0),
        }
    }

    fn printed(item: &PageItem) -> bool {
        item.visible && !item.print.nonprinting
    }

    /// A paint as plate values; `None` for no paint.
    fn col(&mut self, paint: &Paint) -> Option<Rc<PlaneCol>> {
        let Paint::Swatch { id, tint } = paint else { return None };
        let key = (id.clone(), (tint * 1000.0).round() as u32);
        if let Some(hit) = self.cols.get(&key) {
            return hit.clone();
        }
        let col = self.res.resolve(paint).map(|r| Rc::new(plane_col(&r, &self.res, self.inks, self.colors, self.n)));
        self.cols.insert(key, col.clone());
        col
    }

    fn paint_col(&mut self, canvas: &mut Canvas, cov: &Cov, col: &PlaneCol, overprint: bool, blend: (f64, BlendMode)) {
        if overprint {
            self.stats.overprints += 1;
            canvas.paint(cov, &Src::Solid(&col.v), &col.own, blend.1, blend.0);
        } else {
            canvas.paint(cov, &Src::Solid(&col.v), &self.all, blend.1, blend.0);
        }
    }

    fn draw_sources(&mut self, canvas: &mut Canvas, sources: &[Source<'a>]) {
        let mut ranks: Vec<usize> = Vec::new();
        for src in sources {
            for it in src.items {
                if let Some(r) = self.rank(it)
                    && !ranks.contains(&r)
                {
                    ranks.push(r);
                }
            }
        }
        ranks.sort_unstable();
        let layout = self.layout;
        for rank in ranks {
            for (si, src) in sources.iter().enumerate() {
                self.page_text.clear();
                self.hidden = src.hidden.iter().copied().collect();
                if let Some((_, page)) = src.master {
                    for pf in layout.page_frames.iter().filter(|p| p.page == page) {
                        self.page_text.insert(pf.frame.frame.as_str(), &pf.frame);
                    }
                }
                for (ii, item) in src.items.iter().enumerate() {
                    if self.rank(item) != Some(rank) || !Self::printed(item) {
                        continue;
                    }
                    self.cur = (rank as u32, si as u32, ii as u32);
                    // nothing after the limit is drawn
                    if let Some(l) = self.limit
                        && (self.cur.0, self.cur.1, self.cur.2, 0) > l
                    {
                        continue;
                    }
                    if let Some((mp, _)) = &src.master {
                        // master items belong to the master page their centre lies on
                        let on_page = item_extent(item).is_none_or(|e| {
                            let cx = (e.left + e.right) / 2.0;
                            cx >= mp.left - 0.01 && cx <= mp.right + 0.01
                        });
                        if !on_page {
                            continue;
                        }
                    }
                    if let Some(survey) = self.survey.as_mut() {
                        if !self.hidden.contains(item.id.as_str()) && item.opacity > 0.0 {
                            let layout = self.layout;
                            let page_text = &self.page_text;
                            let texted = |it: &PageItem| {
                                layout.frames.get(&it.id).is_some_and(|f| !f.lines.is_empty()) || page_text.get(it.id.as_str()).is_some_and(|f| !f.lines.is_empty())
                            };
                            let facts = super::flat::facts_of(item, self.cur, &src.transform, &texted);
                            survey.push(facts);
                        }
                        continue;
                    }
                    if let Some(sel) = self.sel {
                        // coverage: what is not selected takes away the coverage of what lies below it
                        let selected = sel.contains_key(&self.cur);
                        canvas.erase = !selected;
                        self.draw_item(canvas, item, &src.transform, None);
                        canvas.erase = false;
                        continue;
                    }
                    self.draw_item(canvas, item, &src.transform, None);
                }
            }
        }
    }

    fn draw_children(&mut self, canvas: &mut Canvas, items: &'a [PageItem], t: &Affine, clip: Option<&Cov>) {
        let mut order: Vec<(usize, &'a PageItem)> = items.iter().filter(|i| Self::printed(i)).filter_map(|i| self.rank(i).map(|r| (r, i))).collect();
        order.sort_by_key(|(r, _)| *r);
        for (_, item) in order {
            self.draw_item(canvas, item, t, clip);
        }
    }

    /// An item with its effects; with opacity, a blend mode or group isolation it is drawn
    /// into a layer and composited once (InDesign applies them to the object as a whole).
    fn draw_item(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>) {
        if self.hidden.contains(item.id.as_str()) {
            return;
        }
        self.depth += 1;
        self.draw_item_inner(canvas, item, parent, clip);
        self.depth -= 1;
    }

    /// Whether `part` of the top-level item being drawn is within the paint-order limit
    /// (always inside nested items: they belong to their top-level item's part).
    fn allow(&self, part: u8) -> bool {
        if self.depth == 1
            && let Some(sel) = self.sel
            && let Some(m) = sel.get(&self.cur)
            && m & (1 << part) == 0
        {
            return false;
        }
        match self.limit {
            Some(l) if self.depth == 1 => (self.cur.0, self.cur.1, self.cur.2, part) <= l,
            _ => true,
        }
    }

    fn draw_item_inner(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>) {
        self.stats.items += 1;
        let ext = item_extent(item);
        let device = ext.map(|e| {
            let l = &item.print.level_fx;
            let mut r = effects::grow(e, effects::item_reach(item));
            for fx in [&item.print.fx, &l.fill, &l.content, &l.stroke] {
                for (kind, s) in effects::outer(fx) {
                    r = r.union(&effects::shadow_rect(&e, s, kind));
                }
            }
            parent.apply_rect(&r)
        });
        if let Some(d) = &device
            && pixel_box(d, canvas.w, canvas.h, 2.0, clip).is_none()
        {
            self.stats.culled += 1;
            return;
        }
        if item.opacity <= 0.0 {
            return;
        }
        let group = item.opacity < 1.0
            || item.blend_mode != BlendMode::Normal
            || (matches!(item.kind, ItemKind::Group { .. }) && (item.print.isolate_blending || item.print.knockout_group));
        if !group {
            self.effects_and_body(canvas, item, parent, clip, &item.print.fx, effects::Part::All);
            return;
        }
        if !self.allow(part::BEHIND) {
            return;
        }
        let Some(d) = device else { return };
        let Some((x0, y0, x1, y1)) = pixel_box(&d, canvas.w, canvas.h, 1.0, clip) else { return };
        let (lw, lh) = (x1 - x0, y1 - y0);
        let mut layer = canvas.sublayer(lw, lh);
        let sub = clip.map(|c| c.rebase(x0, y0, lw, lh));
        let lt = Affine::translate(-(x0 as f64), -(y0 as f64)).then_inner(parent);
        self.stats.layers += 1;
        // one unit of paint order as a whole: its parts are not limited separately
        self.depth += 1;
        self.effects_and_body(&mut layer, item, &lt, sub.as_ref(), &item.print.fx, effects::Part::All);
        self.depth -= 1;
        canvas.composite(&layer, x0, y0, item.blend_mode, item.opacity, None);
    }

    fn body(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>) {
        let t = parent.then_inner(&item.transform);
        if let ItemKind::Group { children } = &item.kind {
            if self.allow(part::CONTENT) {
                self.draw_children(canvas, children, &t, clip);
            }
            return;
        }
        let levels = &item.print.level_fx;
        for (fx, which) in [(&levels.fill, effects::Part::Fill), (&levels.content, effects::Part::Content), (&levels.stroke, effects::Part::Stroke)] {
            if fx.any() {
                self.effects_and_body(canvas, item, parent, clip, fx, which);
            } else {
                self.body_part(canvas, item, parent, clip, which);
            }
        }
    }

    /// The fill, the content or the stroke of an item (not a group).
    fn body_part(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>, which: effects::Part) {
        let t = parent.then_inner(&item.transform);
        let path = build_path(&item.paths);
        let rule = item_fill_rule(&item.paths);
        if which == effects::Part::Fill {
            if let Some(p) = &path
                && self.allow(part::FILL)
            {
                self.fill(canvas, item, p, rule, &t, clip);
            }
            return;
        }
        if which == effects::Part::Stroke {
            if item.stroke.weight > 0.0 && self.allow(part::STROKE) {
                self.stroke(canvas, item, path.as_ref(), &t, clip);
            }
            return;
        }
        if !self.allow(part::CONTENT) {
            // the content comes after the limit
            return;
        }
        match item.print.content_blend {
            Some((a, m)) => {
                if let Some(r) = item_extent(item).map(|r| parent.apply_rect(&r))
                    && let Some((x0, y0, x1, y1)) = pixel_box(&r, canvas.w, canvas.h, 1.0, clip)
                {
                    let (lw, lh) = (x1 - x0, y1 - y0);
                    let mut layer = canvas.sublayer(lw, lh);
                    let sub = clip.map(|c| c.rebase(x0, y0, lw, lh));
                    let lt = Affine::translate(-(x0 as f64), -(y0 as f64)).then_inner(&t);
                    self.stats.layers += 1;
                    self.content(&mut layer, item, path.as_ref(), rule, &lt, sub.as_ref());
                    canvas.composite(&layer, x0, y0, m, a, None);
                }
            }
            None => self.content(canvas, item, path.as_ref(), rule, &t, clip),
        }
    }

    fn fill(&mut self, canvas: &mut Canvas, item: &'a PageItem, path: &tiny_skia::Path, rule: FillRule, t: &Affine, clip: Option<&Cov>) {
        let blend = item.print.fill_blend.unwrap_or(NORMAL);
        match &item.fill {
            Paint::None => {}
            Paint::Swatch { .. } => {
                let Some(col) = self.col(&item.fill) else { return };
                // "Overprint [Black] at 100 %" covers fills too, not placed content
                let op = item.print.overprint_fill || (self.overprint_black && col.black100 && !item.print.placed_art);
                let Some(cov) = fill_cov(canvas.w, canvas.h, path, rule, t, clip) else { return };
                self.paint_col(canvas, &cov, &col, op, blend);
            }
            Paint::Gradient { id, .. } => {
                let geo = item.fill_gradient.unwrap_or_default();
                let Some(cov) = fill_cov(canvas.w, canvas.h, path, rule, t, clip) else { return };
                self.paint_gradient(canvas, &cov, id, &geo, t, item.print.overprint_fill, blend);
            }
        }
    }

    // Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.

    #[allow(clippy::too_many_arguments)]
    fn paint_gradient(&mut self, canvas: &mut Canvas, cov: &Cov, id: &str, geo: &GradientGeometry, t: &Affine, overprint: bool, blend: (f64, BlendMode)) {
        let Some((px, used)) = self.gradient(id, geo, t, cov) else { return };
        self.stats.soft_paints += 1;
        if overprint {
            self.stats.overprints += 1;
            canvas.paint(cov, &Src::Pixels(&px), &used, blend.1, blend.0);
        } else {
            canvas.paint(cov, &Src::Pixels(&px), &self.all, blend.1, blend.0);
        }
    }

    /// A gradient's plate values over `cov`'s rectangle, blended between stops in ink with
    /// each stop's midpoint (as InDesign), and the plates any stop uses.
    fn gradient(&mut self, id: &str, geo: &GradientGeometry, t: &Affine, cov: &Cov) -> Option<(Pixels, Vec<bool>)> {
        let doc = self.doc;
        let g = doc.gradients.get(id)?;
        let n = self.n;
        let stops: Vec<(f64, f64, Vec<u8>)> = g
            .stops
            .iter()
            .map(|s| {
                let v = self.col(&Paint::Swatch { id: s.color.clone(), tint: 100.0 }).map(|c| c.v.clone()).unwrap_or_else(|| vec![0; n]);
                (s.location, s.midpoint, v)
            })
            .collect();
        if stops.is_empty() {
            return None;
        }
        let mut lut = vec![0u8; 256 * n];
        for u in 0..256usize {
            let pos = u as f64 / 255.0 * 100.0;
            let (a, b, mix) = ramp_at(&stops, pos);
            for p in 0..n {
                let (x, y) = (a[p] as f64, b[p] as f64);
                lut[u * n + p] = (x + (y - x) * mix).round().clamp(0.0, 255.0) as u8;
            }
        }
        let used: Vec<bool> = (0..n).map(|p| (0..256).any(|u| lut[u * n + p] > 0)).collect();
        // the ramp position (0–255) of every pixel, from tiny-skia's gradient geometry
        let mut pm = Pixmap::new(cov.w as u32, cov.h as u32)?;
        let shader_t = to_skia(&Affine::translate(-(cov.x0 as f64), -(cov.y0 as f64)).then_inner(t));
        let ends = vec![
            tiny_skia::GradientStop::new(0.0, tiny_skia::Color::BLACK),
            tiny_skia::GradientStop::new(1.0, tiny_skia::Color::WHITE),
        ];
        let shader = gradient_shader(g.kind, geo, ends, shader_t)?;
        let paint = tiny_skia::Paint { shader, anti_alias: false, ..Default::default() };
        pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, cov.w as f32, cov.h as f32)?, &paint, tiny_skia::Transform::identity(), None);
        let pos: Vec<u8> = pm.data().chunks_exact(4).map(|px| px[0]).collect();
        let planes = (0..n).map(|p| used[p].then(|| pos.iter().map(|&u| lut[u as usize * n + p]).collect())).collect();
        Some((Pixels { w: cov.w, h: cov.h, planes, alpha: None }, used))
    }

    fn content(&mut self, canvas: &mut Canvas, item: &'a PageItem, path: Option<&tiny_skia::Path>, rule: FillRule, t: &Affine, clip: Option<&Cov>) {
        if let ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } = &item.kind {
            self.graphic(canvas, g, path, rule, t, clip);
        }
        if !item.clipped.is_empty() {
            match path {
                Some(p) => {
                    if let Some(c) = fill_cov(canvas.w, canvas.h, p, rule, t, clip) {
                        self.draw_children(canvas, &item.clipped, t, Some(&c));
                    }
                }
                None => self.draw_children(canvas, &item.clipped, t, clip),
            }
        }
        if let Some(tf) = item.text_frame() {
            self.text(canvas, &item.id, &tf.story, t, clip);
        }
    }

    /// A placed image through its frame and clipping path: its own ink, knocking out what is
    /// below where it is opaque.
    fn graphic(&mut self, canvas: &mut Canvas, g: &'a Graphic, frame: Option<&tiny_skia::Path>, rule: FillRule, frame_t: &Affine, clip: Option<&Cov>) {
        let gt = frame_t.then_inner(&g.transform);
        let b = g.bounds;
        let cov = match frame {
            Some(p) => fill_cov(canvas.w, canvas.h, p, rule, frame_t, clip),
            None => tiny_skia::Rect::from_ltrb(b.left as f32, b.top as f32, b.right as f32, b.bottom as f32)
                .map(PathBuilder::from_rect)
                .and_then(|p| fill_cov(canvas.w, canvas.h, &p, FillRule::Winding, &gt, clip)),
        };
        let Some(mut cov) = cov else { return };
        if let Some(c) = &g.clip
            && let Some(clip_path) = build_path(&c.paths)
        {
            // Nonzero as InDesign writes it (an inverted path holds the image bounds).
            let shape = fill_cov(canvas.w, canvas.h, &clip_path, FillRule::Winding, &gt, None);
            match shape {
                Some(s) => cov.multiply(&s),
                None => return,
            }
        }
        if canvas.mask {
            // coverage (Flattener Preview): the graphic's shape, whether its image can be read or not
            canvas.paint(&cov, &Src::Solid(&[]), &self.all, BlendMode::Normal, 1.0);
            return;
        }
        // the print source's image (a found original), else the reader's preview
        let image = match self.source.and_then(|s| s.image(g)) {
            Some(i) => i,
            None => {
                let Some(file) = g.preview.as_deref() else {
                    self.stats.missing_images += 1;
                    return;
                };
                let Some(image) = self.images.get(file, crate::graphic_is_proxy(g), self.colors) else {
                    self.stats.missing_images += 1;
                    return;
                };
                image
            }
        };
        let (iw, ih) = (image.width() as f64, image.height() as f64);
        if iw <= 0.0 || ih <= 0.0 || b.width() <= 0.0 || b.height() <= 0.0 {
            return;
        }
        let to_device = gt.then_inner(&Affine::translate(b.left, b.top)).then_inner(&Affine::scale(b.width() / iw, b.height() / ih));
        let Some(px) = image.sample(&to_device, &cov, self.n) else { return };
        self.stats.images += 1;
        let scale = (to_device.a * to_device.d - to_device.b * to_device.c).abs().sqrt();
        if scale > 0.0 {
            self.stats.image_detail = self.stats.image_detail.max(1.0 / scale);
        }
        canvas.paint(&cov, &Src::Pixels(&px), &self.all, BlendMode::Normal, 1.0);
    }

    fn stroke(&mut self, canvas: &mut Canvas, item: &'a PageItem, path: Option<&tiny_skia::Path>, t: &Affine, clip: Option<&Cov>) {
        let st = &item.stroke;
        let blend = item.print.stroke_blend.unwrap_or(NORMAL);
        if let Some(art) = &st.art {
            for part in &art.parts {
                let Some(p) = build_path(&part.paths) else { continue };
                let cov = match &part.shape {
                    PartShape::Line { width, cap, join, miter_limit, dash } => {
                        stroke_cov(canvas.w, canvas.h, &p, &skia_stroke(*width, *cap, *join, Some(*miter_limit), dash.as_ref()), t, clip)
                    }
                    PartShape::Fill => fill_cov(canvas.w, canvas.h, &p, FillRule::Winding, t, clip),
                };
                let Some(cov) = cov else { continue };
                match (part.ink, &st.paint) {
                    (StrokeInk::Stroke, Paint::Gradient { id, .. }) => {
                        let geo = item.stroke_gradient.unwrap_or_default();
                        self.paint_gradient(canvas, &cov, id, &geo, t, item.print.overprint_stroke, blend);
                    }
                    (StrokeInk::Stroke, paint) => {
                        let Some(col) = self.col(paint) else { continue };
                        let op = item.print.overprint_stroke || (self.overprint_black && col.black100);
                        self.paint_col(canvas, &cov, &col, op, blend);
                    }
                    (StrokeInk::Gap, _) => {
                        let Some(col) = self.col(&st.gap) else { continue };
                        let op = item.print.overprint_gap || (self.overprint_black && col.black100);
                        self.paint_col(canvas, &cov, &col, op, blend);
                    }
                }
            }
            return;
        }
        let Some(path) = path else { return };
        if st.paint == Paint::None {
            return;
        }
        // inside and outside aligned strokes come lowered (`stroke.art`)
        let Some(cov) = stroke_cov(canvas.w, canvas.h, path, &skia_stroke(st.weight, st.cap, st.join, st.miter_limit, None), t, clip) else { return };
        match &st.paint {
            Paint::Gradient { id, .. } => {
                let geo = item.stroke_gradient.unwrap_or_default();
                self.paint_gradient(canvas, &cov, id, &geo, t, item.print.overprint_stroke, blend);
            }
            paint => {
                let Some(col) = self.col(paint) else { return };
                let op = item.print.overprint_stroke || (self.overprint_black && col.black100);
                self.paint_col(canvas, &cov, &col, op, blend);
            }
        }
    }

}

/// The two stops around `pos` (percent along the ramp) and how far to mix them.
fn ramp_at(stops: &[(f64, f64, Vec<u8>)], pos: f64) -> (&[u8], &[u8], f64) {
    let first = &stops[0];
    if pos <= first.0 || stops.len() == 1 {
        return (&first.2, &first.2, 0.0);
    }
    for w in stops.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if pos <= b.0 {
            let f = if b.0 > a.0 { ((pos - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 1.0 };
            return (&a.2, &b.2, folio_color::GradientCurve::new(b.1).sample(f));
        }
    }
    let last = &stops[stops.len() - 1];
    (&last.2, &last.2, 0.0)
}
