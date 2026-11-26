//! Draws page items, their transparency effects, and their clipped content.

mod graphics;
mod text;

use super::Ctx;
use crate::geometry::item_fill_rule;
use crate::paint::{color, paint_for, skia_blend};
use crate::{ViewDisplay, build_path, display, effects, item_extent, skia_stroke, to_skia};
use folio_model::geom::{PartShape, StrokeArt, StrokeInk};
use folio_model::{Affine, BlendMode, ItemKind, PageItem, Rect};
use graphics::{draw_graphic, draw_math};
use text::draw_text;
use tiny_skia::{FillRule, Mask, Pixmap, PixmapPaint, Transform};

pub(super) fn draw_items(ctx: &mut Ctx, pixmap: &mut Pixmap, items: &[PageItem], parent: &Affine, clip: Option<&Mask>) {
    // stable sort by layer order; hidden layers are skipped
    let mut order: Vec<(usize, &PageItem)> = Vec::with_capacity(items.len());
    for item in items {
        let (rank, visible) = item.layer.as_deref().and_then(|l| ctx.layers.get(l).copied()).unwrap_or((0, true));
        if visible && item.visible && !ctx.hidden.contains(item.id.as_str()) {
            order.push((rank, item));
        }
    }
    order.sort_by_key(|(rank, _)| *rank);
    for (_, item) in order {
        draw_item(ctx, pixmap, item, parent, clip);
    }
}

/// Draws `draw` into a transparent layer that covers only `path`'s device bounds, clipped to
/// the path (and to `clip`), then composites it. The cost follows the frame's area instead of
/// the page's. `draw` gets the layer, the offset to prepend to device transforms, and the
/// layer-sized clip mask.
pub(crate) fn with_clip_layer(
    pixmap: &mut Pixmap,
    clip: Option<&Mask>,
    path: &tiny_skia::Path,
    rule: FillRule,
    t: &Affine,
    draw: impl FnOnce(&mut Pixmap, &Affine, &Mask),
) -> bool {
    let b = path.bounds();
    let r = t.apply_rect(&Rect { left: b.left() as f64, top: b.top() as f64, right: b.right() as f64, bottom: b.bottom() as f64 });
    let x0 = r.left.floor().max(0.0) as i64;
    let y0 = r.top.floor().max(0.0) as i64;
    let x1 = (r.right.ceil() as i64).min(pixmap.width() as i64);
    let y1 = (r.bottom.ceil() as i64).min(pixmap.height() as i64);
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let (Some(mut layer), Some(mut mask)) = (Pixmap::new(w, h), Mask::new(w, h)) else { return false };
    let offset = Affine::translate(-x0 as f64, -y0 as f64);
    mask.fill_path(path, rule, true, to_skia(&offset.then_inner(t)));
    if let Some(parent) = clip {
        let pw = pixmap.width() as usize;
        let pd = parent.data();
        let md = mask.data_mut();
        for y in 0..h as usize {
            let row = (y0 as usize + y) * pw + x0 as usize;
            for x in 0..w as usize {
                let v = &mut md[y * w as usize + x];
                *v = ((*v as u16 * pd[row + x] as u16 + 127) / 255) as u8;
            }
        }
    }
    draw(&mut layer, &offset, &mask);
    pixmap.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    true
}

/// A styled stroke (dashes, stripes, hashes, gap colour, arrowheads) as the document reader
/// lowered it: its parts painted in order with the stroke or the gap paint.
fn draw_stroke_art(ctx: &mut Ctx, pixmap: &mut Pixmap, item: &PageItem, art: &StrokeArt, t: &Affine, clip: Option<&Mask>) {
    for part in &art.parts {
        let paint = match part.ink {
            StrokeInk::Stroke => paint_for(ctx.doc, ctx.colors, &item.stroke.paint, item.stroke_gradient.as_ref(), ctx.blend),
            StrokeInk::Gap => paint_for(ctx.doc, ctx.colors, &item.stroke.gap, None, ctx.blend),
        }
        .map(|p| partial(p, item.print.stroke_blend));
        let (Some(paint), Some(path)) = (paint, build_path(&part.paths)) else { continue };
        match &part.shape {
            PartShape::Line { width, cap, join, miter_limit, dash } => {
                let stroke = skia_stroke(*width, *cap, *join, Some(*miter_limit), dash.as_ref());
                pixmap.stroke_path(&path, &paint, &stroke, to_skia(t), clip);
            }
            PartShape::Fill => pixmap.fill_path(&path, &paint, FillRule::Winding, to_skia(t), clip),
        }
    }
}

/// Whether an item has transparency effects or partial (fill / stroke / content) opacity.
pub(crate) fn has_effects(item: &PageItem) -> bool {
    let fx = &item.print.fx;
    fx.drop_shadow.is_some()
        || fx.outer_glow.is_some()
        || fx.feather.is_some()
        || fx.directional_feather.is_some()
        || fx.gradient_feather.is_some()
        || fx.inner_shadow.is_some()
        || fx.inner_glow.is_some()
        || fx.bevel.is_some()
        || fx.satin.is_some()
        || item.print.fill_blend.is_some()
        || item.print.stroke_blend.is_some()
        || item.print.content_blend.is_some()
        || item.print.level_fx.fill.any()
        || item.print.level_fx.stroke.any()
        || item.print.level_fx.content.any()
}

/// A transparent layer over the device rectangle `r` (cut to the bitmap) with the parent
/// clip cut to it: the layer, its device origin and its clip.
fn layer_over(pixmap: &Pixmap, r: &Rect, clip: Option<&Mask>) -> Option<(Pixmap, i64, i64, Option<Mask>)> {
    let x0 = (r.left.floor() - 1.0).max(0.0) as i64;
    let y0 = (r.top.floor() - 1.0).max(0.0) as i64;
    let x1 = (r.right.ceil() as i64 + 1).min(pixmap.width() as i64);
    let y1 = (r.bottom.ceil() as i64 + 1).min(pixmap.height() as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let layer = Pixmap::new(w, h)?;
    let sub_clip = clip.and_then(|parent_clip| {
        let mut m = Mask::new(w, h)?;
        let pw = pixmap.width() as usize;
        let (pd, md) = (parent_clip.data(), m.data_mut());
        for y in 0..h as usize {
            let row = (y0 as usize + y) * pw + x0 as usize;
            md[y * w as usize..(y + 1) * w as usize].copy_from_slice(&pd[row..row + w as usize]);
        }
        Some(m)
    });
    Some((layer, x0, y0, sub_clip))
}

/// InDesign applies an item's opacity and blend mode to the item as a whole (fill, content,
/// text, stroke and effects together; a group's members together): such an item is drawn
/// opaque into a layer covering its device bounds, then composited once.
fn draw_item(ctx: &mut Ctx, pixmap: &mut Pixmap, item: &PageItem, parent: &Affine, clip: Option<&Mask>) {
    let fx = has_effects(item);
    if item.opacity >= 1.0 && item.blend_mode == BlendMode::Normal {
        if fx {
            draw_item_fx(ctx, pixmap, item, parent, clip);
        } else {
            draw_item_body(ctx, pixmap, item, parent, clip);
        }
        return;
    }
    if item.opacity <= 0.0 {
        return;
    }
    let reach = effects::item_reach(item);
    let Some(r) = item_extent(item).map(|r| parent.apply_rect(&effects::grow(r, reach))) else { return };
    let Some((mut layer, x0, y0, sub_clip)) = layer_over(pixmap, &r, clip) else { return };
    let offset = Affine::translate(-x0 as f64, -y0 as f64);
    ctx.stats.layers += 1;
    if fx {
        draw_item_fx(ctx, &mut layer, item, &offset.then_inner(parent), sub_clip.as_ref());
    } else {
        draw_item_body(ctx, &mut layer, item, &offset.then_inner(parent), sub_clip.as_ref());
    }
    let paint = PixmapPaint { opacity: item.opacity as f32, blend_mode: skia_blend(item.blend_mode), ..Default::default() };
    pixmap.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &paint, Transform::identity(), None);
}

/// Runs `f` with the shapes the effect rasteriser needs (frame text, glyphs, inline items).
fn with_shapes<'a, R>(ctx: &Ctx<'a>, f: impl FnOnce(&effects::Shapes<'_, 'a>) -> R) -> R {
    let (layout, page_text) = (ctx.layout, &ctx.page_text);
    let text_of = |id: &str| -> Option<&'a folio_model::layout::FrameText> { page_text.get(id).copied().or_else(|| layout.frames.get(id)) };
    let shapes = effects::Shapes { text: &text_of, glyphs: ctx.glyphs, inline: &ctx.inline };
    f(&shapes)
}

/// Paints `rgb` through an effect mask (parent coordinates, `ppu` pixels per unit) with the
/// effect's blend mode and opacity.
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
fn paint_mask(pixmap: &mut Pixmap, a: &effects::Alpha, rgb: [u8; 3], blend: BlendMode, opacity: f64, parent: &Affine, ppu: f64, clip: Option<&Mask>) {
    let Some(mut pm) = Pixmap::new(a.w as u32, a.h as u32) else { return };
    for (px, al) in pm.data_mut().chunks_exact_mut(4).zip(&a.data) {
        let al16 = *al as u16;
        px[0] = ((rgb[0] as u16 * al16 + 127) / 255) as u8;
        px[1] = ((rgb[1] as u16 * al16 + 127) / 255) as u8;
        px[2] = ((rgb[2] as u16 * al16 + 127) / 255) as u8;
        px[3] = *al;
    }
    let t = parent.then_inner(&Affine::translate(a.rect.left, a.rect.top)).then_inner(&Affine::scale(1.0 / ppu, 1.0 / ppu));
    let paint = PixmapPaint {
        opacity: opacity.clamp(0.0, 1.0) as f32,
        blend_mode: skia_blend(blend),
        quality: tiny_skia::FilterQuality::Bilinear,
    };
    pixmap.draw_pixmap(0, 0, pm.as_ref(), &paint, to_skia(&t), clip);
}

/// An item with effects (see `draw_fx_part`).
fn draw_item_fx(ctx: &mut Ctx, pixmap: &mut Pixmap, item: &PageItem, parent: &Affine, clip: Option<&Mask>) {
    draw_fx_part(ctx, pixmap, item, parent, clip, &item.print.fx, effects::Part::All, &mut |ctx, pm, p, c| draw_item_body(ctx, pm, item, p, c));
}

/// What draws one part of an item, under a parent transform and clip.
type PartBody<'b, 'a> = dyn FnMut(&mut Ctx<'a>, &mut Pixmap, &Affine, Option<&Mask>) + 'b;

/// A part of an item with its effects (the object as a whole, or its fill, content or stroke,
/// InDesign's Effects levels): shadows and glows of that part behind it, then the part with its
/// inner effects, feathered when it is (the same masks the PDF writer exports).
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
fn draw_fx_part<'a>(ctx: &mut Ctx<'a>, pixmap: &mut Pixmap, item: &PageItem, parent: &Affine, clip: Option<&Mask>, fx: &folio_model::ObjectEffects, part: effects::Part, body: &mut PartBody<'_, 'a>) {
    let Some(ext) = item_extent(item) else {
        body(ctx, pixmap, parent, clip);
        return;
    };
    for (kind, e) in effects::outer(fx) {
        let rect = effects::shadow_rect(&ext, e, kind);
        let ppu = effects::shadow_ppu(effects::resolution(parent, &rect, 1.0), e, kind);
        let Some(a) = with_shapes(ctx, |sh| effects::shadow(item, sh, &rect, ppu, e, kind, part)) else { continue };
        if let Some(rgb) = color(ctx.doc, ctx.colors, &e.color, ctx.blend) {
            paint_mask(pixmap, &a, rgb, e.blend, e.opacity, parent, ppu, clip);
        }
    }
    let ppu = effects::resolution(parent, &effects::grow(ext, 2.0), 1.0);
    let Some(feather) = with_shapes(ctx, |sh| effects::feather_mask(item, sh, &ext, ppu, fx, part)) else {
        body(ctx, pixmap, parent, clip);
        draw_inner_effects(ctx, pixmap, item, parent, &ext, clip, fx, part);
        return;
    };
    // feathered: the part and its inner effects in a layer, composited through the mask
    let r = parent.apply_rect(&feather.rect);
    let Some((mut layer, x0, y0, sub_clip)) = layer_over(pixmap, &r, clip) else { return };
    let offset = Affine::translate(-x0 as f64, -y0 as f64);
    let lt = offset.then_inner(parent);
    ctx.stats.layers += 1;
    body(ctx, &mut layer, &lt, sub_clip.as_ref());
    draw_inner_effects(ctx, &mut layer, item, &lt, &ext, sub_clip.as_ref(), fx, part);
    let Some(mut mask_pm) = Pixmap::new(layer.width(), layer.height()) else { return };
    paint_mask(&mut mask_pm, &feather, [0, 0, 0], BlendMode::Normal, 1.0, &lt, ppu, None);
    // the layer's own pixels take the feather (a draw mask must match the target's size)
    layer.apply_mask(&Mask::from_pixmap(mask_pm.as_ref(), tiny_skia::MaskType::Alpha));
    pixmap.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

/// Inner shadow, inner glow, satin, bevel and emboss over a part already drawn.
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
fn draw_inner_effects(ctx: &mut Ctx, pixmap: &mut Pixmap, item: &PageItem, parent: &Affine, ext: &Rect, clip: Option<&Mask>, fx: &folio_model::ObjectEffects, part: effects::Part) {
    if fx.inner_shadow.is_none() && fx.inner_glow.is_none() && fx.satin.is_none() && fx.bevel.is_none() {
        return;
    }
    let ppu = effects::inner_ppu(effects::resolution(parent, &effects::grow(*ext, effects::reach(fx) + 30.0), 1.0), fx);
    let layers = with_shapes(ctx, |sh| effects::inner_layers(item, sh, ext, ppu, fx, part));
    for l in layers {
        if !l.mask.any() || l.opacity <= 0.0 {
            continue;
        }
        if let Some(rgb) = color(ctx.doc, ctx.colors, l.color, ctx.blend) {
            paint_mask(pixmap, &l.mask, rgb, l.blend, l.opacity, parent, ppu, clip);
        }
    }
}

/// A paint with an item's partial transparency (opacity and blend mode of its fill or
/// stroke) applied.
fn partial(mut p: tiny_skia::Paint<'static>, b: Option<(f64, BlendMode)>) -> tiny_skia::Paint<'static> {
    if let Some((a, m)) = b {
        p.shader.apply_opacity(a.clamp(0.0, 1.0) as f32);
        p.blend_mode = skia_blend(m);
    }
    p
}

/// Draws an item at full opacity (see `draw_item`).
fn draw_item_body(ctx: &mut Ctx, pixmap: &mut Pixmap, item: &PageItem, parent: &Affine, clip: Option<&Mask>) {
    ctx.stats.items += 1;
    let t = parent.then_inner(&item.transform);
    if let ItemKind::Group { children } = &item.kind {
        draw_items(ctx, pixmap, children, &t, clip);
        return;
    }
    // Skip items entirely outside the bitmap (e.g. the facing page of a spread). Text may
    // run past its frame a little (italics, hanging punctuation), so frames get a margin.
    if let Some(bounds) = folio_model::paths_bounds(&item.paths) {
        let bounds = match item.stroke.art.as_ref().and_then(StrokeArt::bounds) {
            Some(a) => bounds.union(&a),
            None => bounds,
        };
        let r = t.apply_rect(&bounds);
        let unit = (t.a * t.d - t.b * t.c).abs().sqrt();
        let margin = 2.0 + unit * (item.stroke.weight + if item.text_frame().is_some() { 36.0 } else { 0.0 });
        if r.right + margin < 0.0 || r.bottom + margin < 0.0 || r.left - margin > pixmap.width() as f64 || r.top - margin > pixmap.height() as f64 {
            ctx.stats.culled += 1;
            return;
        }
    }
    let path = build_path(&item.paths);
    let fill_rule = item_fill_rule(&item.paths);
    let levels = &item.print.level_fx;

    let mut fill = |ctx: &mut Ctx, pixmap: &mut Pixmap, parent: &Affine, clip: Option<&Mask>| {
        let t = parent.then_inner(&item.transform);
        if let Some(path) = &path
            && let Some(paint) = paint_for(ctx.doc, ctx.colors, &item.fill, item.fill_gradient.as_ref(), ctx.blend)
        {
            pixmap.fill_path(path, &partial(paint, item.print.fill_blend), fill_rule, to_skia(&t), clip);
        }
    };
    // The content (graphic, pasted items, text) with its own opacity / blend mode: drawn
    // into a layer over the item, composited once.
    let mut content = |ctx: &mut Ctx, pixmap: &mut Pixmap, parent: &Affine, clip: Option<&Mask>| {
        let t = parent.then_inner(&item.transform);
        match item.print.content_blend {
            Some((a, m)) => {
                if let Some(r) = item_extent(item).map(|r| parent.apply_rect(&r))
                    && let Some((mut layer, x0, y0, sub_clip)) = layer_over(pixmap, &r, clip)
                {
                    ctx.stats.layers += 1;
                    let lt = Affine::translate(-x0 as f64, -y0 as f64).then_inner(&t);
                    draw_content(ctx, &mut layer, item, path.as_ref(), fill_rule, &lt, sub_clip.as_ref());
                    let paint = PixmapPaint { opacity: a.clamp(0.0, 1.0) as f32, blend_mode: skia_blend(m), ..Default::default() };
                    pixmap.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &paint, Transform::identity(), None);
                }
            }
            None => draw_content(ctx, pixmap, item, path.as_ref(), fill_rule, &t, clip),
        }
    };
    let mut stroke = |ctx: &mut Ctx, pixmap: &mut Pixmap, parent: &Affine, clip: Option<&Mask>| {
        let t = parent.then_inner(&item.transform);
        if item.stroke.weight > 0.0
            && let Some(art) = &item.stroke.art
        {
            draw_stroke_art(ctx, pixmap, item, art, &t, clip);
        } else if item.stroke.weight > 0.0
            && let Some(path) = &path
            && let Some(paint) = paint_for(ctx.doc, ctx.colors, &item.stroke.paint, item.stroke_gradient.as_ref(), ctx.blend)
        {
            // inside and outside aligned strokes come lowered (`stroke.art`)
            let paint = partial(paint, item.print.stroke_blend);
            let st = &item.stroke;
            let stroke = skia_stroke(st.weight, st.cap, st.join, st.miter_limit, None);
            pixmap.stroke_path(path, &paint, &stroke, to_skia(&t), clip);
        }
    };
    // each part with the effects of its level (fill, then content, then stroke)
    for (fx, part, body) in [(&levels.fill, effects::Part::Fill, &mut fill as &mut PartBody), (&levels.content, effects::Part::Content, &mut content), (&levels.stroke, effects::Part::Stroke, &mut stroke)] {
        if fx.any() {
            draw_fx_part(ctx, pixmap, item, parent, clip, fx, part, body);
        } else {
            body(ctx, pixmap, parent, clip);
        }
    }
}

/// What a frame holds, inside its fill and under its stroke: the placed graphic and pasted-in
/// items clipped to the frame, and its text (not clipped).
fn draw_content(ctx: &mut Ctx, pixmap: &mut Pixmap, item: &PageItem, path: Option<&tiny_skia::Path>, fill_rule: FillRule, t: &Affine, clip: Option<&Mask>) {
    let graphic = match &item.kind {
        ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => Some(g),
        _ => None,
    };
    if let Some(g) = graphic {
        if ctx.display.shown(item.local_display, g.local_display) == ViewDisplay::Optimized {
            display::grey_box(pixmap, g, path, fill_rule, t, clip);
        } else {
            draw_graphic(ctx, pixmap, g, path, &item.paths, fill_rule, t, clip);
        }
    }
    if let Some(math) = &item.math {
        draw_math(pixmap, item, math, t, clip);
    }
    if !item.clipped.is_empty() {
        if let Some(p) = path {
            ctx.stats.layers += 1;
            let children = &item.clipped;
            with_clip_layer(pixmap, clip, p, fill_rule, t, |layer, offset, mask| {
                draw_items(ctx, layer, children, &offset.then_inner(t), Some(mask));
            });
        } else {
            draw_items(ctx, pixmap, &item.clipped, t, clip);
        }
    }
    if item.text_frame().is_some() {
        draw_text(ctx, pixmap, &item.id, t, clip);
    }
}
