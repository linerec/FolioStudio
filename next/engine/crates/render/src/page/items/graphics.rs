//! Draws placed graphics and equations, with the frame and image clipping paths.

use super::{Ctx, with_clip_layer};
use crate::sampling::resample_box;
use crate::{ImageMode, build_path, graphic_is_proxy, mathsvg, solid, to_skia};
use folio_model::{Affine, Graphic, PageItem, Rect, SubPath};
use tiny_skia::{FillRule, Mask, PathBuilder, Pixmap, PixmapPaint, Transform};

/// A frame's equation: the outlines InDesign stored with it, filled in the equation's colour
/// (mathsvg.rs). An equation without readable stored SVG draws nothing.
pub(super) fn draw_math(pixmap: &mut Pixmap, item: &PageItem, math: &folio_model::mathml::MathObject, t: &Affine, clip: Option<&Mask>) {
    let Some(frame) = folio_model::paths_bounds(&item.paths) else { return };
    let Some(shapes) = mathsvg::shapes(math, &frame) else { return };
    let own = mathsvg::equation_rgb(math);
    for shape in shapes {
        if let Some(path) = build_path(&shape.paths) {
            pixmap.fill_path(&path, &solid(shape.fill.unwrap_or(own)), FillRule::Winding, to_skia(t), clip);
        }
    }
}

// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_graphic(
    ctx: &mut Ctx,
    pixmap: &mut Pixmap,
    g: &Graphic,
    frame: Option<&tiny_skia::Path>,
    frame_paths: &[SubPath],
    rule: FillRule,
    frame_t: &Affine,
    clip: Option<&Mask>,
) {
    if let Some(c) = &g.clip
        && let Some(clip_path) = build_path(&c.paths)
    {
        // The image clipped to its frame and to its clipping path: a layer over the frame
        // whose mask is the frame ∩ the path (an inverted path's geometry already holds the
        // image bounds, `GraphicClip::invert`).
        let frame_path = match frame {
            Some(p) => Some(p.clone()),
            None => tiny_skia::Rect::from_ltrb(g.bounds.left as f32, g.bounds.top as f32, g.bounds.right as f32, g.bounds.bottom as f32)
                .map(PathBuilder::from_rect)
                .and_then(|p| p.transform(to_skia(&g.transform))),
        };
        let Some(frame_path) = frame_path else { return };
        let gt = frame_t.then_inner(&g.transform);
        ctx.stats.layers += 1;
        let mut unclipped = g.clone();
        unclipped.clip = None;
        with_clip_layer(pixmap, clip, &frame_path, rule, frame_t, |layer, offset, mask| {
            let mut m = mask.clone();
            let to_layer = offset.then_inner(&gt);
            m.intersect_path(&clip_path, FillRule::Winding, true, to_skia(&to_layer));
            draw_graphic(ctx, layer, &unclipped, None, frame_paths, rule, &offset.then_inner(frame_t), Some(&m));
        });
        return;
    }
    let Some(file) = g.preview.as_deref() else {
        ctx.stats.missing_images += 1;
        return;
    };
    if matches!(g.element.as_str(), "SVG" | "WMF") && g.preview_from_source {
        let Some(scene) = ctx.images.svg(file) else { ctx.stats.missing_images += 1; return };
        let t = frame_t.then_inner(&g.transform).then_inner(&scene.placement(&g.bounds));
        if let Some(path) = frame {
            with_clip_layer(pixmap, clip, path, rule, frame_t, |layer, offset, mask| {
                scene.draw(layer, &offset.then_inner(&t), Some(mask), ctx.colors, ctx.blend);
            });
        } else { scene.draw(pixmap, &t, clip, ctx.colors, ctx.blend); }
        ctx.stats.images += 1;
        return;
    }
    let mode = ImageMode { blend: ctx.blend, proxy: graphic_is_proxy(g) };
    let Some(image) = ctx.images.get(file, mode, ctx.colors) else {
        ctx.stats.missing_images += 1;
        return;
    };
    ctx.stats.images += 1;
    let b = &g.bounds;
    let (iw, ih) = (image.width() as f64, image.height() as f64);
    if iw <= 0.0 || ih <= 0.0 || b.width() <= 0.0 || b.height() <= 0.0 {
        return;
    }
    // original image pixels → frame inner coordinates
    let to_graphic = Affine::translate(b.left, b.top).then_inner(&Affine::scale(b.width() / iw, b.height() / ih));
    let img_to_frame = g.transform.then_inner(&to_graphic);
    let to_device = frame_t.then_inner(&img_to_frame);
    let device_scale = (to_device.a * to_device.d - to_device.b * to_device.c).abs().sqrt();
    let (level, factor) = image.level_for(device_scale);
    let level_to_frame = img_to_frame.then_inner(&Affine::scale(factor, factor));

    // Axis-aligned (the usual case): resample exactly, image pixels as squares averaged over
    // each device pixel (a PDF image without interpolation), then fill the frame with the
    // result 1:1. Rotated or skewed images use the filtered drawing below.
    let level_to_device = frame_t.then_inner(&level_to_frame);
    if level_to_device.b.abs() < 1e-9 && level_to_device.c.abs() < 1e-9 {
        let visible = match frame {
            Some(p) => {
                let b = p.bounds();
                frame_t.apply_rect(&Rect { left: b.left() as f64, top: b.top() as f64, right: b.right() as f64, bottom: b.bottom() as f64 })
            }
            None => Rect { left: 0.0, top: 0.0, right: pixmap.width() as f64, bottom: pixmap.height() as f64 },
        };
        let area = (
            (visible.left.floor() as i64).max(0),
            (visible.top.floor() as i64).max(0),
            (visible.right.ceil() as i64).min(pixmap.width() as i64),
            (visible.bottom.ceil() as i64).min(pixmap.height() as i64),
        );
        let Some((sampled, ox, oy)) = resample_box(level.as_ref(), &level_to_device, area) else { return };
        ctx.stats.pattern_images += 1;
        match frame {
            Some(path) => {
                let Some(inv) = frame_t.invert() else { return };
                let shader_t = inv.then_inner(&Affine::translate(ox as f64, oy as f64));
                let paint = tiny_skia::Paint {
                    shader: tiny_skia::Pattern::new(
                        sampled.as_ref(),
                        tiny_skia::SpreadMode::Pad,
                        tiny_skia::FilterQuality::Nearest,
                        1.0,
                        to_skia(&shader_t),
                    ),
                    anti_alias: true,
                    ..Default::default()
                };
                pixmap.fill_path(path, &paint, rule, to_skia(frame_t), clip);
            }
            None => {
                pixmap.draw_pixmap(ox as i32, oy as i32, sampled.as_ref(), &PixmapPaint::default(), Transform::identity(), clip);
            }
        }
        return;
    }

    // Does the image cover the whole frame? Then the frame path, filled with the image as a
    // pattern, is the clipped image: no mask needed.
    let covers = img_to_frame.invert().zip(folio_model::paths_bounds(frame_paths)).is_some_and(|(inv, fb)| {
        let r = inv.apply_rect(&fb);
        r.left >= -0.5 && r.top >= -0.5 && r.right <= iw + 0.5 && r.bottom <= ih + 0.5
    });
    let quality = tiny_skia::FilterQuality::Bilinear;
    if let (Some(path), true) = (frame, covers) {
        let pattern = tiny_skia::Pattern::new(level.as_ref().as_ref(), tiny_skia::SpreadMode::Pad, quality, 1.0, to_skia(&level_to_frame));
        let paint = tiny_skia::Paint { shader: pattern, anti_alias: true, ..Default::default() };
        pixmap.fill_path(path, &paint, rule, to_skia(frame_t), clip);
        ctx.stats.pattern_images += 1;
        return;
    }
    let paint = PixmapPaint { quality, ..Default::default() };
    match frame {
        Some(p) => {
            ctx.stats.layers += 1;
            with_clip_layer(pixmap, clip, p, rule, frame_t, |layer, offset, mask| {
                let t = offset.then_inner(frame_t).then_inner(&level_to_frame);
                layer.draw_pixmap(0, 0, level.as_ref().as_ref(), &paint, to_skia(&t), Some(mask));
            });
        }
        None => {
            let t = frame_t.then_inner(&level_to_frame);
            pixmap.draw_pixmap(0, 0, level.as_ref().as_ref(), &paint, to_skia(&t), clip);
        }
    }
}
