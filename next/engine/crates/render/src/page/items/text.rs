//! Draws composed text, its shapes and decorations, and its inline items.

use super::{Ctx, draw_item};
use crate::paint::paint_for;
use crate::{build_path, color, runs, skia_stroke, solid, to_skia};
use folio_model::geom::PartShape;
use folio_model::layout::DecorationKind;
use folio_model::{Affine, Paint};
use tiny_skia::{FillRule, Mask, Pixmap};

/// Table fills and strokes, paragraph rules, shading and borders and footnote rules laid out
/// with a frame's text (`LaidLine::shapes`): every line's shapes, before any glyph.
fn draw_text_shapes(ctx: &Ctx, pixmap: &mut Pixmap, text: &folio_model::layout::FrameText, frame_t: &Affine, clip: Option<&Mask>) {
    // the highlights of conditions (`DecorationKind::Highlight`): behind all the text of the frame
    for d in text.lines.iter().flat_map(|l| &l.decorations).filter(|d| d.kind == DecorationKind::Highlight) {
        let Some(rgb) = d.rgb else { continue };
        let t = frame_t.then_inner(&Affine::translate(d.x0, d.y));
        for shape in &d.shapes {
            if let Some(path) = build_path(&shape.paths) {
                pixmap.fill_path(&path, &solid(rgb), FillRule::Winding, to_skia(&t), clip);
            }
        }
    }
    for shape in text.lines.iter().flat_map(|l| &l.shapes) {
        let Some(rgb) = color(ctx.doc, ctx.colors, &shape.paint, ctx.blend) else { continue };
        let Some(path) = build_path(&shape.paths) else { continue };
        match &shape.shape {
            PartShape::Line { width, cap, join, miter_limit, dash } => {
                let stroke = skia_stroke(*width, *cap, *join, Some(*miter_limit), dash.as_ref());
                pixmap.stroke_path(&path, &solid(rgb), &stroke, to_skia(frame_t), clip);
            }
            PartShape::Fill => pixmap.fill_path(&path, &solid(rgb), FillRule::Winding, to_skia(frame_t), clip),
        }
    }
}

pub(super) fn draw_text(ctx: &mut Ctx, pixmap: &mut Pixmap, frame: &str, frame_t: &Affine, clip: Option<&Mask>) {
    let Some(text) = ctx.page_text.get(frame).copied().or_else(|| ctx.layout.frames.get(frame)) else { return };
    draw_text_shapes(ctx, pixmap, text, frame_t, clip);
    for line in &text.lines {
        for (range, stroke) in runs::paint_passes(&line.glyphs) {
        for g in &line.glyphs[range] {
            let Some(rgb) = color(ctx.doc, ctx.colors, if stroke { &g.stroke } else { &g.fill }, ctx.blend) else { continue };
            let Some(path) = ctx.glyph_paths.get(ctx.glyphs, g.font, g.glyph) else { continue };
            // em units, y up → frame coordinates at the pen position
            let glyph_t = g.transform(line.rotation);
            let t = frame_t.then_inner(&glyph_t);
            // a gradient fill or stroke of the text: placed in the frame's coordinates (the glyph's outline goes there first)
            let gradient = if stroke { text_gradient_paint(ctx, &g.stroke, g.stroke_gradient.as_ref()) } else { text_gradient_paint(ctx, &g.fill, g.fill_gradient.as_ref()) };
            if stroke {
                if let Some(outline) = path.as_ref().clone().transform(to_skia(&glyph_t)) {
                    let style = skia_stroke(runs::stroke_width(g), folio_model::LineCap::Butt, folio_model::LineJoin::Miter, Some(4.0), None);
                    let flat = solid(rgb);
                    pixmap.stroke_path(&outline, gradient.as_ref().unwrap_or(&flat), &style, to_skia(frame_t), clip);
                }
            } else if let Some(gradient) = &gradient {
                if let Some(outline) = path.as_ref().clone().transform(to_skia(&glyph_t)) {
                    pixmap.fill_path(&outline, gradient, FillRule::Winding, to_skia(frame_t), clip);
                }
            } else {
                pixmap.fill_path(&path, &solid(rgb), FillRule::Winding, to_skia(&t), clip);
            }
            if !stroke || g.fill == Paint::None { ctx.stats.glyphs += 1; }
        }
        }
        for d in line.decorations.iter().filter(|d| d.kind != DecorationKind::Highlight) {
            let t = frame_t.then_inner(&Affine::translate(d.x0, d.y));
            for shape in &d.shapes {
                let Some(rgb) = d.rgb.or_else(|| color(ctx.doc, ctx.colors, &shape.paint, ctx.blend)) else { continue };
                let Some(path) = build_path(&shape.paths) else { continue };
                match &shape.shape {
                    PartShape::Line { width, cap, join, miter_limit, dash } => {
                        let stroke = skia_stroke(*width, *cap, *join, Some(*miter_limit), dash.as_ref());
                        pixmap.stroke_path(&path, &solid(rgb), &stroke, to_skia(&t), clip);
                    }
                    PartShape::Fill => pixmap.fill_path(&path, &solid(rgb), FillRule::Winding, to_skia(&t), clip),
                }
            }
        }
        for o in &line.objects {
            if let Some(&item) = ctx.inline.get(o.item.as_str())
                && item.visible
            {
                draw_item(ctx, pixmap, item, &frame_t.then_inner(&Affine::translate(o.x, o.y)), clip);
            }
        }
    }
}

/// The paint of a glyph whose fill or stroke is a gradient the layout placed (against its column and frame); `None` for a
/// flat paint or a gradient without an area (nested text), which show as their flat stand-in.
fn text_gradient_paint(ctx: &Ctx, paint: &Paint, placed: Option<&folio_model::PlacedGradient>) -> Option<tiny_skia::Paint<'static>> {
    let Paint::Gradient { id, .. } = paint else { return None };
    let radial = ctx.doc.gradients.get(id)?.kind == folio_model::GradientKind::Radial;
    let geo = placed?.geometry(radial)?;
    let paint = paint_for(ctx.doc, ctx.colors, paint, Some(&geo), ctx.blend)?;
    matches!(paint.shader, tiny_skia::Shader::LinearGradient(_) | tiny_skia::Shader::RadialGradient(_)).then_some(paint)
}
