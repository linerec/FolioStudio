//! Transparency effects rasterised the way InDesign writes them into PDF/X-4: the object's
//! silhouette (fill, stroke, text, content) as an 8-bit alpha, blurred / spread / shifted /
//! shaded into the mask an effect colour is painted through. Shared by the screen renderer
//! and the PDF writer (which stores the masks as luminosity soft masks), so both show the
//! same effect.
use crate::geometry::{build_path, item_fill_rule, outline_path, to_skia};
use folio_model::geom::{PartShape, StrokeArt};
use folio_model::layout::{FrameText, GlyphSource};
use folio_model::{Affine, BevelStyle, BevelTechnique, ItemKind, ObjectEffects, PageItem, Rect, ShadowEffect};
use std::collections::HashMap;

mod feathers;
mod inner;
mod matte;
mod noise;
mod outer;
mod reach;

pub use feathers::feather_mask;
pub use inner::inner_layers;
pub use outer::{shadow, shadow_ppu, shadow_rect};
pub use reach::{inner_ppu, reach};

/// Effect rasters per inch of page space in PDF output (InDesign: 300).
pub const PPI: f64 = 300.0;
/// Largest effect raster side (pixels); larger effects use a lower resolution.
pub const MAX_SIDE: f64 = 4096.0;
/// Satin blurs with σ = this × size (fitted to InDesign's masks, not analysed yet).
const INNER_SIGMA: f64 = 0.42;

/// An 8-bit alpha raster covering `rect` (user space, y down).
#[derive(Clone)]
pub struct Alpha {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
    pub rect: Rect,
}

impl Alpha {
    fn like(&self, data: Vec<u8>) -> Alpha {
        Alpha { w: self.w, h: self.h, data, rect: self.rect }
    }
    /// Whether anything is painted.
    pub fn any(&self) -> bool {
        self.data.iter().any(|v| *v > 0)
    }
}

/// What the silhouette needs besides the item: frame text and glyph outlines.
pub struct Shapes<'s, 'd> {
    pub text: &'s dyn Fn(&str) -> Option<&'d FrameText>,
    pub glyphs: &'s dyn GlyphSource,
    pub inline: &'s HashMap<&'d str, &'d PageItem>,
}

/// Pixels per user unit for an effect drawn under `ctm` (user → page points) at `ppi`.
pub fn pixels_per_unit(ctm: &Affine, rect: &Rect) -> f64 {
    resolution(ctm, rect, PPI / 72.0)
}

/// Pixels per user unit at `per_point` pixels per page point, capped to `MAX_SIDE`.
pub fn resolution(ctm: &Affine, rect: &Rect, per_point: f64) -> f64 {
    let unit = (ctm.a * ctm.d - ctm.b * ctm.c).abs().sqrt().max(1e-6);
    let mut ppu = per_point * unit;
    let side = rect.width().max(rect.height()) * ppu;
    if side > MAX_SIDE {
        ppu *= MAX_SIDE / side;
    }
    ppu
}

/// The part of an item an effect belongs to (the Effects panel's levels): the whole object, or
/// its fill, its stroke, or its content (a text frame's text, a placed graphic, pasted items).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    All,
    Fill,
    Stroke,
    Content,
}

/// The item's coverage (fill area, stroke, text, nested items; or that of one `part`) in
/// `rect` of its parent's space, moved by `offset`, at `ppu` pixels per unit.
pub fn silhouette(item: &PageItem, shapes: &Shapes<'_, '_>, rect: &Rect, ppu: f64, offset: (f64, f64), part: Part) -> Option<Alpha> {
    silhouette_through(item, shapes, rect, ppu, &Affine::translate(offset.0, offset.1), part)
}

/// The coverage drawn through `pre` (parent → parent, e.g. a turn), over `rect` of where it lands.
fn silhouette_through(item: &PageItem, shapes: &Shapes<'_, '_>, rect: &Rect, ppu: f64, pre: &Affine, part: Part) -> Option<Alpha> {
    let w = (rect.width() * ppu).ceil().max(1.0) as u32;
    let h = (rect.height() * ppu).ceil().max(1.0) as u32;
    let mut mask = tiny_skia::Mask::new(w, h)?;
    let base = Affine::scale(ppu, ppu).then_inner(&Affine::translate(-rect.left, -rect.top)).then_inner(pre);
    match part {
        Part::All => paint_item(&mut mask, item, &base, shapes, 0),
        part => paint_part(&mut mask, item, &base, shapes, part),
    }
    Some(Alpha { w: w as usize, h: h as usize, data: mask.data().to_vec(), rect: *rect })
}

/// One part of an item (not a group) into `mask`.
fn paint_part(mask: &mut tiny_skia::Mask, item: &PageItem, parent: &Affine, shapes: &Shapes<'_, '_>, part: Part) {
    if !item.visible || matches!(item.kind, ItemKind::Group { .. }) {
        return;
    }
    let t = parent.then_inner(&item.transform);
    let rule = item_fill_rule(&item.paths);
    let path = build_path(&item.paths);
    match part {
        Part::Fill => {
            if let Some(p) = &path
                && item.fill != folio_model::Paint::None
            {
                mask.fill_path(p, rule, true, to_skia(&t));
            }
        }
        Part::Stroke => paint_stroke(mask, item, path.as_ref(), &t),
        Part::Content => {
            // the graphic's box and the pasted items, clipped to the frame; the text as it is
            let graphic = match &item.kind {
                ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => Some(g),
                _ => None,
            };
            if graphic.is_some() || !item.clipped.is_empty() {
                let (w, h) = (mask.width(), mask.height());
                if let (Some(mut inner), Some(mut frame)) = (tiny_skia::Mask::new(w, h), tiny_skia::Mask::new(w, h)) {
                    if let Some(g) = graphic
                        && let Some(r) = tiny_skia::Rect::from_ltrb(g.bounds.left as f32, g.bounds.top as f32, g.bounds.right as f32, g.bounds.bottom as f32)
                    {
                        inner.fill_path(&tiny_skia::PathBuilder::from_rect(r), tiny_skia::FillRule::Winding, true, to_skia(&t.then_inner(&g.transform)));
                    }
                    for c in &item.clipped {
                        paint_item(&mut inner, c, &t, shapes, 1);
                    }
                    if let Some(p) = &path {
                        frame.fill_path(p, rule, true, to_skia(&t));
                    }
                    for ((m, a), b) in mask.data_mut().iter_mut().zip(inner.data()).zip(frame.data()) {
                        *m = (*m).max(mul8(*a, *b));
                    }
                }
            }
            paint_text(mask, item, &t, shapes, 0);
        }
        Part::All => paint_item(mask, item, parent, shapes, 0),
    }
}

fn paint_art(mask: &mut tiny_skia::Mask, art: &StrokeArt, t: &Affine) {
    for part in &art.parts {
        let Some(p) = build_path(&part.paths) else { continue };
        match &part.shape {
            PartShape::Fill => mask.fill_path(&p, tiny_skia::FillRule::Winding, true, to_skia(t)),
            PartShape::Line { width, cap, join, miter_limit, dash } => {
                let st = tiny_skia::Stroke {
                    width: *width as f32,
                    line_cap: match cap {
                        folio_model::LineCap::Butt => tiny_skia::LineCap::Butt,
                        folio_model::LineCap::Round => tiny_skia::LineCap::Round,
                        folio_model::LineCap::Projecting => tiny_skia::LineCap::Square,
                    },
                    line_join: match join {
                        folio_model::LineJoin::Miter => tiny_skia::LineJoin::Miter,
                        folio_model::LineJoin::Round => tiny_skia::LineJoin::Round,
                        folio_model::LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                    },
                    miter_limit: *miter_limit as f32,
                    dash: dash.as_ref().and_then(|d| tiny_skia::StrokeDash::new(d.array.iter().map(|v| *v as f32).collect(), d.phase as f32)),
                };
                if let Some(outline) = p.stroke(&st, 1.0) {
                    mask.fill_path(&outline, tiny_skia::FillRule::Winding, true, to_skia(t));
                }
            }
        }
    }
}

fn paint_item(mask: &mut tiny_skia::Mask, item: &PageItem, parent: &Affine, shapes: &Shapes<'_, '_>, depth: usize) {
    if !item.visible || depth > 32 {
        return;
    }
    let t = parent.then_inner(&item.transform);
    if let ItemKind::Group { children } = &item.kind {
        for c in children {
            paint_item(mask, c, &t, shapes, depth + 1);
        }
        return;
    }
    let rule = item_fill_rule(&item.paths);
    let has_content = matches!(&item.kind, ItemKind::Rectangle { content: Some(_) } | ItemKind::Oval { content: Some(_) } | ItemKind::Polygon { content: Some(_) })
        || !item.clipped.is_empty();
    let path = build_path(&item.paths);
    if let Some(p) = &path
        && (item.fill != folio_model::Paint::None || has_content)
    {
        mask.fill_path(p, rule, true, to_skia(&t));
    }
    paint_stroke(mask, item, path.as_ref(), &t);
    paint_text(mask, item, &t, shapes, depth);
}

/// An item's stroke (`t`: item inner → mask pixels).
fn paint_stroke(mask: &mut tiny_skia::Mask, item: &PageItem, path: Option<&tiny_skia::Path>, t: &Affine) {
    let Some(p) = path else { return };
    if item.stroke.weight > 0.0 && (item.stroke.paint != folio_model::Paint::None || item.stroke.gap != folio_model::Paint::None) {
        match &item.stroke.art {
            Some(art) => paint_art(mask, art, t),
            None => {
                let st = tiny_skia::Stroke { width: item.stroke.weight as f32, ..Default::default() };
                if let Some(outline) = p.stroke(&st, 1.0) {
                    mask.fill_path(&outline, tiny_skia::FillRule::Winding, true, to_skia(t));
                }
            }
        }
    }
}

/// A text frame's glyphs and inline items (`t`: frame inner → mask pixels).
fn paint_text(mask: &mut tiny_skia::Mask, item: &PageItem, t: &Affine, shapes: &Shapes<'_, '_>, depth: usize) {
    let t = *t;
    if item.text_frame().is_some()
        && let Some(text) = (shapes.text)(&item.id)
    {
        for line in &text.lines {
            for g in &line.glyphs {
                if g.fill == folio_model::Paint::None {
                    continue;
                }
                let Some(cmds) = shapes.glyphs.outline(g.font, g.glyph) else { continue };
                let Some(p) = outline_path(&cmds) else { continue };
                let shear = -(g.skew.to_radians()).tan();
                let gt = Affine::translate(g.x, g.y)
                    .then_inner(&Affine::new(1.0, 0.0, shear, 1.0, 0.0, 0.0))
                    .then_inner(&Affine::scale(g.size * g.hscale, -g.size * g.vscale));
                mask.fill_path(&p, tiny_skia::FillRule::Winding, true, to_skia(&t.then_inner(&gt)));
            }
            for o in &line.objects {
                if let Some(it) = shapes.inline.get(o.item.as_str()) {
                    paint_item(mask, it, &t.then_inner(&Affine::translate(o.x, o.y)), shapes, depth + 1);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Raster operations

/// Three box blurs ≈ a Gaussian of standard deviation `sigma` pixels; outside the raster is
/// empty.
pub fn blur(a: &mut Alpha, sigma: f64) {
    // three passes of radius r add 3·r(r+1)/3 = r(r+1) to the variance
    let r = ((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0;
    let r = r.round() as usize;
    if r == 0 {
        return;
    }
    for _ in 0..3 {
        box_pass(&mut a.data, a.w, a.h, r, true);
        box_pass(&mut a.data, a.w, a.h, r, false);
    }
}

fn box_pass(d: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    if len == 0 {
        return;
    }
    let idx = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let mut buf = vec![0u32; len];
    let win = (2 * r + 1) as u32;
    for line in 0..lines {
        for i in 0..len {
            buf[i] = d[idx(line, i)] as u32;
        }
        let mut sum: u32 = 0;
        // window [i - r, i + r], zero outside
        for v in buf.iter().take(r.min(len - 1) + 1) {
            sum += v;
        }
        for i in 0..len {
            d[idx(line, i)] = ((sum + win / 2) / win) as u8;
            if i + r + 1 < len {
                sum += buf[i + r + 1];
            }
            if i >= r {
                sum -= buf[i - r];
            }
        }
    }
}

fn morph(a: &mut Alpha, radius: usize, grow: bool) {
    if radius == 0 {
        return;
    }
    for horizontal in [true, false] {
        let (len, lines) = if horizontal { (a.w, a.h) } else { (a.h, a.w) };
        let w = a.w;
        let idx = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
        let mut buf = vec![0u8; len];
        for line in 0..lines {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = a.data[idx(line, i)];
            }
            for i in 0..len {
                let (lo, hi) = (i.saturating_sub(radius), (i + radius).min(len - 1));
                let edge = !grow && (i < radius || i + radius >= len);
                let v = if grow { *buf[lo..=hi].iter().max().unwrap_or(&0) } else { *buf[lo..=hi].iter().min().unwrap_or(&0) };
                a.data[idx(line, i)] = if edge { 0 } else { v };
            }
        }
    }
}

/// Grows the coverage by `radius` pixels (square structuring element).
pub fn dilate(a: &mut Alpha, radius: usize) {
    morph(a, radius, true);
}

/// Shrinks the coverage by `radius` pixels (outside the raster counts as empty).
pub fn erode(a: &mut Alpha, radius: usize) {
    morph(a, radius, false);
}

/// The raster moved by whole pixels (empty where nothing moves in).
fn shifted(a: &Alpha, dx: f64, dy: f64) -> Vec<u8> {
    let (sx, sy) = (dx.round() as i64, dy.round() as i64);
    let mut out = vec![0u8; a.data.len()];
    for y in 0..a.h as i64 {
        let ys = y - sy;
        if ys < 0 || ys >= a.h as i64 {
            continue;
        }
        for x in 0..a.w as i64 {
            let xs = x - sx;
            if xs >= 0 && xs < a.w as i64 {
                out[(y * a.w as i64 + x) as usize] = a.data[(ys * a.w as i64 + xs) as usize];
            }
        }
    }
    out
}

fn mul8(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16 + 127) / 255) as u8
}


/// Euclidean distance (pixels) from each covered pixel (≥ 50 %) to the nearest uncovered
/// one or the raster edge; 0 outside (Felzenszwalb–Huttenlocher, exact).
fn distance_inside(a: &Alpha) -> Vec<f32> {
    distance_field(a, true)
}

/// As `distance_inside`; `edge_outside`: whether beyond the raster counts as uncovered.
fn distance_field(a: &Alpha, edge_outside: bool) -> Vec<f32> {
    squared_distance_field(a, edge_outside, |v| v < 128)
        .into_iter().map(|v| v.sqrt() as f32).collect()
}

fn squared_distance_field(a: &Alpha, edge_outside: bool, is_seed: impl Fn(u8) -> bool) -> Vec<f64> {
    let (w, h) = (a.w, a.h);
    let inf = 1e20f64;
    let border = if edge_outside { 0.0 } else { inf };
    // Squared distance to the nearest seed, including outside pixels when requested.
    let mut f: Vec<f64> = a.data.iter().map(|v| if is_seed(*v) { 0.0 } else { inf }).collect();
    let mut tmp = vec![0f64; w.max(h) + 2];
    let mut line = vec![0f64; w.max(h) + 2];
    for x in 0..w {
        let n = h + 2;
        line[0] = border;
        for y in 0..h {
            line[y + 1] = f[y * w + x];
        }
        line[n - 1] = border;
        dt1(&line[..n], &mut tmp[..n]);
        for y in 0..h {
            f[y * w + x] = tmp[y + 1];
        }
    }
    for y in 0..h {
        let n = w + 2;
        line[0] = border;
        line[1..w + 1].copy_from_slice(&f[y * w..(y + 1) * w]);
        line[n - 1] = border;
        dt1(&line[..n], &mut tmp[..n]);
        f[y * w..(y + 1) * w].copy_from_slice(&tmp[1..w + 1]);
    }
    f
}

/// One-dimensional squared distance transform of sampled function `f` (lower envelope of
/// parabolas).
fn dt1(f: &[f64], d: &mut [f64]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: replace the first parabola
                v[0] = q;
                z[0] = f64::NEG_INFINITY;
                z[1] = f64::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        *out = dq * dq + f[p];
    }
}


/// Gradient feather: opacity 100 % → 0 % along `angle` across `ext` (or over `length`).
pub fn gradient_feather(a: &mut Alpha, ppu: f64, ext: &Rect, g: &folio_model::GradientFeather, transform: &Affine) {
    if g.kind == folio_model::GradientKind::Radial {
        // 100 % at the centre to 0 % at the radius (half the bounds' diagonal unless given)
        // a stored length places it; length 0 is worked out from the bounds
        let given = g.length.filter(|l| *l > 1e-8);
        let (cx, cy) = match (given, g.start) {
            (Some(_), Some(p)) => (p.x, p.y),
            _ => ((ext.left + ext.right) / 2.0, (ext.top + ext.bottom) / 2.0),
        };
        let radius = given.unwrap_or(0.5 * ext.width().hypot(ext.height())).max(1e-3);
        // Stored starts and lengths use the item's inner coordinates; the mask uses its parent's.
        let inner = if given.is_some() && g.start.is_some() { transform.invert() } else { None };
        let rect = a.rect;
        for y in 0..a.h {
            for x in 0..a.w {
                let (ux, uy) = (rect.left + (x as f64 + 0.5) / ppu, rect.top + (y as f64 + 0.5) / ppu);
                let p = folio_model::Point::new(ux, uy);
                let p = inner.map_or(p, |t| t.apply(p));
                let t = ((p.x - cx).hypot(p.y - cy) / radius).clamp(0.0, 1.0);
                let i = y * a.w + x;
                a.data[i] = (a.data[i] as f64 * (1.0 - t)).round() as u8;
            }
        }
        return;
    }
    let ang = g.angle.to_radians();
    let (dx, dy) = (ang.cos(), -ang.sin());
    let corners = [(ext.left, ext.top), (ext.right, ext.top), (ext.left, ext.bottom), (ext.right, ext.bottom)];
    let proj: Vec<f64> = corners.iter().map(|(x, y)| x * dx + y * dy).collect();
    let (p0, p1) = (proj.iter().cloned().fold(f64::MAX, f64::min), proj.iter().cloned().fold(f64::MIN, f64::max));
    let len = g.length.filter(|l| *l > 0.0).unwrap_or((p1 - p0).max(1e-6));
    let rect = a.rect;
    for y in 0..a.h {
        for x in 0..a.w {
            let (ux, uy) = (rect.left + (x as f64 + 0.5) / ppu, rect.top + (y as f64 + 0.5) / ppu);
            let t = ((ux * dx + uy * dy - p0) / len).clamp(0.0, 1.0);
            let i = y * a.w + x;
            a.data[i] = (a.data[i] as f64 * (1.0 - t)).round() as u8;
        }
    }
}

pub fn offset(r: &Rect, dx: f64, dy: f64) -> Rect {
    Rect { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy }
}

pub fn grow(r: Rect, m: f64) -> Rect {
    Rect { left: r.left - m, top: r.top - m, right: r.right + m, bottom: r.bottom + m }
}

// ---------------------------------------------------------------------------------------
// Effects

/// A drop shadow or an outer glow (they differ in how far the blur reaches).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outer {
    DropShadow,
    OuterGlow,
}

/// An item's drop shadow and outer glow, in drawing order.
pub fn outer(fx: &ObjectEffects) -> impl Iterator<Item = (Outer, &ShadowEffect)> {
    [(Outer::DropShadow, fx.drop_shadow.as_ref()), (Outer::OuterGlow, fx.outer_glow.as_ref())].into_iter().filter_map(|(k, e)| e.map(|e| (k, e)))
}

/// How far an item's effects of every level reach beyond its extent (parent units).
pub fn item_reach(item: &PageItem) -> f64 {
    let l = &item.print.level_fx;
    [&item.print.fx, &l.fill, &l.stroke, &l.content].into_iter().map(reach).fold(0.0, f64::max)
}

/// One inner effect layer: the colour painted through `mask` with its blend and opacity.
pub struct InnerLayer<'d> {
    pub mask: Alpha,
    pub color: &'d folio_model::Paint,
    pub blend: folio_model::BlendMode,
    pub opacity: f64,
}

/// Bevel and emboss highlight and shadow masks: the surface rises with slope
/// depth × (1 − d / size) along the distance d from the edge (constant for chisel), and is
/// lit from the light's angle and altitude; brighter than flat is highlight, darker shadow.
fn bevel(m: &Alpha, ppu: f64, b: &folio_model::BevelEmboss) -> (Alpha, Alpha) {
    let n = m.data.len();
    let size = (b.size * ppu).max(1.0);
    // distances from pixel centres to the edge, which lies half a pixel out
    let edge = |d: Vec<f32>| -> Vec<f32> { d.into_iter().map(|v| (v - 0.5).max(0.0)).collect() };
    let inside_d = edge(distance_inside(m));
    // outside the object the raster's border is no edge
    let outside_d = edge(distance_field(&m.like(m.data.iter().map(|v| 255 - v).collect()), false));
    let a = b.angle.to_radians();
    let (lx, ly) = (a.cos(), -a.sin());
    let alt = b.altitude.to_radians();
    let (lz, lxy) = (alt.sin().max(1e-3), alt.cos());
    // smooth: steepest at the edge; chisel: the same height as an even slope
    let slope = |d: f32, span: f64| -> f64 {
        let d = d as f64;
        if d >= span {
            return 0.0;
        }
        match b.technique {
            BevelTechnique::Smooth => b.depth * (1.0 - d / span),
            _ => b.depth / 2.0,
        }
    };
    let (w, h) = (m.w, m.h);
    // gradient of a distance field: the unit direction away from the nearest edge
    let grad = |f: &[f32], x: usize, y: usize| -> (f64, f64) {
        let at = |xx: usize, yy: usize| f[yy * w + xx] as f64;
        let gx = (at((x + 1).min(w - 1), y) - at(x.saturating_sub(1), y)) / 2.0;
        let gy = (at(x, (y + 1).min(h - 1)) - at(x, y.saturating_sub(1))) / 2.0;
        let l = gx.hypot(gy);
        if l < 1e-9 { (0.0, 0.0) } else { (gx / l, gy / l) }
    };
    let (inner_span, outer_span) = match b.style {
        BevelStyle::Inner => (size, 0.0),
        BevelStyle::Outer => (0.0, size),
        BevelStyle::Emboss | BevelStyle::Pillow => (size / 2.0, size / 2.0),
    };
    let mut hi = vec![0u8; n];
    let mut sh = vec![0u8; n];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let (s, ux, uy) = if m.data[i] >= 128 {
                if inner_span <= 0.0 {
                    continue;
                }
                // inside: the surface rises from the edge, facing outward
                let (gx, gy) = grad(&inside_d, x, y);
                (slope(inside_d[i], inner_span), -gx, -gy)
            } else {
                if outer_span <= 0.0 {
                    continue;
                }
                // outside: falling away from the object (a pillow's rises, facing it)
                let (gx, gy) = grad(&outside_d, x, y);
                let flip = if b.style == BevelStyle::Pillow { -1.0 } else { 1.0 };
                (slope(outside_d[i], outer_span), gx * flip, gy * flip)
            };
            if s <= 0.0 {
                continue;
            }
            let dir = if b.up { 1.0 } else { -1.0 };
            let ul = (ux * lx + uy * ly) * dir;
            let th = s.atan();
            let nl = th.sin() * lxy * ul + th.cos() * lz;
            let hv = ((nl - lz) / (1.0 - lz).max(1e-3)).clamp(0.0, 1.0);
            let sv = ((lz - nl) / lz).clamp(0.0, 1.0);
            hi[i] = (hv * 255.0).round() as u8;
            sh[i] = (sv * 255.0).round() as u8;
        }
    }
    let mut hi = m.like(hi);
    let mut sh = m.like(sh);
    // InDesign's Soften barely changes its masks (measured: no blur fits best); a soft
    // chisel is the hard one with its edges eased
    let soft = match b.technique {
        BevelTechnique::ChiselSoft => b.size / 8.0,
        _ => 0.0,
    };
    if soft > 0.0 {
        blur(&mut hi, soft * ppu / 2.0);
        blur(&mut sh, soft * ppu / 2.0);
    }
    if b.style == BevelStyle::Inner {
        for ((h1, s1), c) in hi.data.iter_mut().zip(sh.data.iter_mut()).zip(&m.data) {
            *h1 = mul8(*h1, *c);
            *s1 = mul8(*s1, *c);
        }
    }
    (hi, sh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(w: usize, h: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Alpha {
        let mut data = vec![0u8; w * h];
        for y in y0..y1 {
            for x in x0..x1 {
                data[y * w + x] = 255;
            }
        }
        Alpha { w, h, data, rect: Rect { left: 0.0, top: 0.0, right: w as f64, bottom: h as f64 } }
    }

    #[test]
    fn radial_feather_start_moves_with_the_item() {
        let ext = Rect { left: 55.0, top: 475.0, right: 215.0, bottom: 555.0 };
        let g = folio_model::GradientFeather {
            kind: folio_model::GradientKind::Radial,
            length: Some(80.0),
            start: Some(folio_model::Point::new(135.0, 515.0)),
            ..Default::default()
        };
        let mut original = square(160, 80, 0, 0, 160, 80);
        original.rect = ext;
        gradient_feather(&mut original, 1.0, &ext, &g, &Affine::IDENTITY);
        assert!(original.data[40 * 160 + 80] > 250);
        let transform = Affine::translate(5.0, -415.945);
        let moved_ext = transform.apply_rect(&ext);
        let mut moved = square(160, 80, 0, 0, 160, 80);
        moved.rect = moved_ext;
        gradient_feather(&mut moved, 1.0, &moved_ext, &g, &transform);
        assert_eq!(original.data, moved.data);
    }

    #[test]
    fn exact_distance_inside() {
        let a = square(40, 30, 10, 10, 30, 20);
        let d = distance_inside(&a);
        // the centre row of a 20 × 10 block: 1 px in from its edges, 5 in the middle
        assert_eq!(d[15 * 40 + 10], 1.0);
        assert_eq!(d[15 * 40 + 20], 5.0);
        assert_eq!(d[14 * 40 + 20], 5.0);
        assert_eq!(d[10 * 40 + 10], 1.0);
        assert_eq!(d[5 * 40 + 5], 0.0);
    }

    #[test]
    fn bevel_lights_the_top_and_shades_the_bottom() {
        // a 100 × 80 px object, 1 px per point, size 7, light from 120° at 30° altitude:
        // InDesign's highlight on the top edge 0.76 and the left edge 0.32, shadow on the right 0.89
        let m = square(140, 120, 20, 20, 120, 100);
        let b = folio_model::BevelEmboss { size: 7.0, depth: 1.0, up: true, angle: 120.0, altitude: 30.0, ..Default::default() };
        let (hi, sh) = bevel(&m, 1.0, &b);
        let at = |a: &Alpha, x: usize, y: usize| a.data[y * a.w + x] as f64 / 255.0;
        assert!((at(&hi, 70, 20) - 0.76).abs() < 0.05, "top {}", at(&hi, 70, 20));
        assert!((at(&hi, 20, 60) - 0.32).abs() < 0.05, "left {}", at(&hi, 20, 60));
        assert!((at(&sh, 119, 60) - 0.88).abs() < 0.06, "right {}", at(&sh, 119, 60));
        assert!(at(&sh, 70, 99) > 0.95, "bottom {}", at(&sh, 70, 99));
        assert_eq!(at(&hi, 70, 60), 0.0);
    }
}
