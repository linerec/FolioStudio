//! Draws composed text and inline items onto ink planes.

use super::{Ctx, NORMAL, PlaneCol};
use super::super::canvas::{Canvas, Cov, fill_cov, stroke_cov};
use crate::{build_path, runs, skia_stroke, to_skia};
use folio_model::geom::PartShape;
use folio_model::{Affine, LineCap, LineJoin, Paint};
use std::rc::Rc;
use tiny_skia::FillRule;

impl<'a> Ctx<'a> {
    /// Text paints flat; a gradient on text prints as its first stop (as in the PDF).
    fn text_col(&mut self, paint: &Paint) -> Option<Rc<PlaneCol>> {
        match paint {
            Paint::Gradient { id, .. } => {
                let first = self.doc.gradients.get(id)?.stops.first()?.color.clone();
                self.col(&Paint::Swatch { id: first, tint: 100.0 })
            }
            p => self.col(p),
        }
    }

    /// Text paint passes, followed by decorations and inline objects.
    pub(super) fn text(&mut self, canvas: &mut Canvas, frame: &str, _story: &str, frame_t: &Affine, clip: Option<&Cov>) {
        let Some(text) = self.page_text.get(frame).copied().or_else(|| self.layout.frames.get(frame)) else { return };
        let (w, h) = (canvas.w, canvas.h);
        for shape in text.lines.iter().flat_map(|l| &l.shapes) {
            let Some(col) = self.text_col(&shape.paint) else { continue };
            let Some(path) = build_path(&shape.paths) else { continue };
            let op = shape.overprint || (self.overprint_black && col.black100);
            let cov = match &shape.shape {
                PartShape::Line { width, cap, join, miter_limit, dash } => {
                    stroke_cov(w, h, &path, &skia_stroke(*width, *cap, *join, Some(*miter_limit), dash.as_ref()), frame_t, clip)
                }
                PartShape::Fill => fill_cov(w, h, &path, FillRule::Winding, frame_t, clip),
            };
            if let Some(cov) = cov {
                self.paint_col(canvas, &cov, &col, op, NORMAL);
            }
        }
        for line in &text.lines {
            for (range, stroke_pass) in runs::paint_passes(&line.glyphs) {
            for g in &line.glyphs[range] {
                let fill = (!stroke_pass).then(|| self.text_col(&g.fill)).flatten();
                let stroke = stroke_pass.then(|| self.text_col(&g.stroke).map(|c| (c, runs::stroke_width(g)))).flatten();
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                let Some(path) = self.glyph_paths.get(self.glyphs, g.font, g.glyph) else { continue };
                let glyph_t = g.transform(line.rotation);
                let t = frame_t.then_inner(&glyph_t);
                if let Some(col) = &fill
                    && let Some(cov) = fill_cov(w, h, &path, FillRule::Winding, &t, clip)
                {
                    let op = (self.overprint_black && col.black100) || g.overprint_fill;
                    self.paint_col(canvas, &cov, col, op, NORMAL);
                }
                if let Some((col, sw)) = &stroke
                    && let Some(outline) = path.as_ref().clone().transform(to_skia(&glyph_t))
                    && let Some(cov) = stroke_cov(w, h, &outline, &skia_stroke(*sw, LineCap::Butt, LineJoin::Miter, Some(4.0), None), frame_t, clip)
                {
                    let op = (self.overprint_black && col.black100) || g.overprint_stroke;
                    self.paint_col(canvas, &cov, col, op, NORMAL);
                }
                if !stroke_pass || g.fill == Paint::None { self.stats.glyphs += 1; }
            }
            }
        }
        // (condition indicators are fixed RGB marks outside the plates)
        for d in text.lines.iter().flat_map(|l| &l.decorations).filter(|d| d.rgb.is_none()) {
            let t = frame_t.then_inner(&Affine::translate(d.x0, d.y));
            for shape in &d.shapes {
                let Some(col) = self.text_col(&shape.paint) else { continue };
                let Some(path) = build_path(&shape.paths) else { continue };
                let cov = match &shape.shape {
                    PartShape::Line { width, cap, join, miter_limit, dash } => {
                        stroke_cov(w, h, &path, &skia_stroke(*width, *cap, *join, Some(*miter_limit), dash.as_ref()), &t, clip)
                    }
                    PartShape::Fill => fill_cov(w, h, &path, FillRule::Winding, &t, clip),
                };
                if let Some(cov) = cov {
                    self.paint_col(canvas, &cov, &col, shape.overprint || self.overprint_black && col.black100, NORMAL);
                }
            }
        }
        for o in text.lines.iter().flat_map(|l| &l.objects) {
            if let Some(&item) = self.inline.get(o.item.as_str())
                && Self::printed(item)
            {
                self.draw_item(canvas, item, &frame_t.then_inner(&Affine::translate(o.x, o.y)), clip);
            }
        }
    }
}
