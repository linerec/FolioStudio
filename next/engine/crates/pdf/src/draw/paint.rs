//! Paints frame paths, fills, and strokes in their stacking order.

use super::{Drawer, grow};
use crate::colors::Resolved;
use crate::content::Stream;
use crate::flatten::R;
use crate::registry::{GsDef, PatternDef, Registry};
use folio_model::geom::{PartShape, StrokeArt, StrokeInk};
use folio_model::{BlendMode, ItemKind, PageItem, Paint};
use folio_render::effects::Part;
use folio_render::sep::part;

/// "Overprint [Black] at 100 %" covers fills as well as text and strokes.
const BLACK_OVERPRINT_FILLS: bool = true;


impl<'a, 'd> Drawer<'a, 'd> {
    fn fill_overprint(&self, item: &PageItem, r: &Resolved) -> bool {
        // Placed content (a generated QR code) keeps its own knockout.
        item.print.overprint_fill
            || (BLACK_OVERPRINT_FILLS && self.overprint_black && r.black100 && !item.print.placed_art)
    }

    pub(super) fn draw_body(&mut self, s: &mut Stream, item: &'d PageItem) {
        if let ItemKind::Group { children } = &item.kind {
            s.push();
            s.concat(&item.transform);
            self.unit_begin(s, part::CONTENT);
            self.draw_children(s, children);
            self.unit_end(s);
            s.pop();
            return;
        }
        // each part with the effects of its level (fill, then content, then stroke)
        let l = &item.print.level_fx;
        for (fx, which) in [(&l.fill, Part::Fill), (&l.content, Part::Content), (&l.stroke, Part::Stroke)] {
            if fx.any() {
                self.draw_effects_and_body(s, item, fx, which);
            } else {
                self.draw_body_part(s, item, which);
            }
        }
    }

    /// The fill, the content or the stroke of an item (not a group).
    pub(super) fn draw_body_part(&mut self, s: &mut Stream, item: &'d PageItem, which: Part) {
        // the item's page extent (sites of partial transparency, rasterised graphics)
        let page_ext = if self.flat.is_some() { self.item_extent(item).map(|e| R::of(&s.ctm().apply_rect(&grow(e, 1.0)))) } else { None };
        s.push();
        s.concat(&item.transform);
        let has_path = item.paths.iter().any(|p| !p.points.is_empty());
        let even_odd = item.paths.len() > 1;
        let part = |s: &mut Stream, reg: &mut Registry<'d>, b: Option<(f64, BlendMode)>| -> bool {
            let Some((a, m)) = b else { return false };
            s.push();
            let gs = reg.gs(GsDef::Blend { alpha: (a * 1000.0).round() as u32, mode: m });
            s.ext_gstate(gs);
            s.transparency = true;
            true
        };
        if which == Part::Fill {
            self.unit_begin(s, part::FILL);
            // flattening: a fill with its own opacity or blend mode is rasterised
            let fill_flat = has_path && item.fill != Paint::None && item.print.fill_blend.is_some() && self.flat_site_page(page_ext, "fill opacity or blend mode");
            // Fill (with its own opacity / blend mode when the item has one).
            let fill_part = !fill_flat && has_path && item.fill != Paint::None && part(s, self.reg, item.print.fill_blend);
            if has_path && !fill_flat {
                match &item.fill {
                    Paint::Swatch { .. } => {
                        if let Some(r) = self.reg.res.resolve(&item.fill) {
                            let op = self.fill_overprint(item, &r);
                            if op {
                                self.reg.counts.overprint_fills += 1;
                            }
                            if item.print.overprint_fill {
                                self.reg.counts.explicit_overprint_fills += 1;
                            }
                            s.set_overprint(self.reg, Some(op), None);
                            s.set_fill(&r.col);
                            s.path(&item.paths);
                            s.fill(even_odd);
                        }
                    }
                    Paint::Gradient { id, .. } => {
                        let geo = item.fill_gradient.unwrap_or_default();
                        if let Some(sh) = self.shading(id, &geo, &item.id) {
                            if item.print.overprint_fill {
                                self.reg.counts.overprint_fills += 1;
                                self.reg.counts.explicit_overprint_fills += 1;
                            }
                            s.set_overprint(self.reg, Some(item.print.overprint_fill), None);
                            s.push();
                            s.path(&item.paths);
                            s.clip(even_odd);
                            s.shading(sh);
                            s.pop();
                        }
                    }
                    Paint::None => {}
                }
            }
            if fill_part {
                s.pop();
            }
            self.unit_end(s);
        } else if which == Part::Content {
            self.unit_begin(s, part::CONTENT);
            // The content (graphic, pasted items, text) with the content opacity / blend mode.
            if item.print.content_blend.is_some() && self.flat_site_page(page_ext, "content opacity or blend mode") {
                self.unit_end(s);
                s.pop();
                return;
            }
            let content_part = part(s, self.reg, item.print.content_blend);

            // Placed graphic, clipped to the frame.
            let graphic = match &item.kind {
                ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => {
                    Some(g)
                }
                _ => None,
            };
            if let Some(g) = graphic {
                s.push();
                if has_path {
                    s.path(&item.paths);
                    s.clip(even_odd);
                }
                self.draw_graphic(s, item, g);
                s.pop();
            }

            // Items pasted inside the frame.
            if !item.clipped.is_empty() {
                s.push();
                if has_path {
                    s.path(&item.paths);
                    s.clip(even_odd);
                }
                self.draw_children(s, &item.clipped);
                s.pop();
            }

            if let Some(math) = &item.math {
                self.draw_math(s, item, math);
            }

            if let Some(tf) = item.text_frame() {
                self.draw_text(s, &item.id, &tf.story);
            }
            if content_part {
                s.pop();
            }
            self.unit_end(s);
        } else {
            // Stroke.
            self.unit_begin(s, part::STROKE);
            if has_path && item.stroke.weight > 0.0 && !(item.print.stroke_blend.is_some() && self.flat_site_page(page_ext.map(|r| grow_r(r, item.stroke.weight)), "stroke opacity or blend mode")) {
                self.draw_stroke(s, item);
            }
            self.unit_end(s);
        }
        s.pop();
    }

    /// A frame's equation: the outlines InDesign stored with it (`folio_render::mathsvg`), filled
    /// in the equation's RGB colour, in the frame's coordinates. Without readable stored SVG nothing is drawn.
    fn draw_math(&mut self, s: &mut Stream, item: &PageItem, math: &folio_model::mathml::MathObject) {
        let Some(frame) = folio_model::paths_bounds(&item.paths) else { return };
        let Some(shapes) = folio_render::mathsvg::shapes(math, &frame) else { return };
        let own = folio_render::mathsvg::equation_rgb(math);
        for shape in shapes {
            let r = self.reg.res.rgb(shape.fill.unwrap_or(own));
            s.set_overprint(self.reg, Some(false), None);
            s.set_fill(&r.col);
            s.path(&shape.paths);
            s.fill(false);
        }
    }

    fn draw_stroke(&mut self, s: &mut Stream, item: &'d PageItem) {
        if let Some(art) = &item.stroke.art {
            self.draw_stroke_art(s, item, art);
            return;
        }
        let st = &item.stroke;
        let mut pattern = None;
        let resolved = match &st.paint {
            Paint::None => return,
            Paint::Swatch { .. } => match self.reg.res.resolve(&st.paint) {
                Some(r) => Some(r),
                None => return,
            },
            Paint::Gradient { id, .. } => {
                let geo = item.stroke_gradient.unwrap_or_default();
                match self.shading(id, &geo, &item.id) {
                    Some(sh) => {
                        pattern = Some(self.reg.add_pattern(PatternDef { shading: sh, matrix: s.ctm() }));
                        None
                    }
                    None => return,
                }
            }
        };
        let op = item.print.overprint_stroke || resolved.as_ref().is_some_and(|r| self.overprint_black && r.black100);
        if item.print.overprint_stroke {
            self.reg.counts.explicit_overprint_strokes += 1;
        }
        if op {
            self.reg.counts.overprint_strokes += 1;
        }
        // inside and outside aligned strokes come lowered (`stroke.art`)
        s.push();
        if let Some((a, m)) = item.print.stroke_blend {
            let gs = self.reg.gs(GsDef::Blend { alpha: (a * 1000.0).round() as u32, mode: m });
            s.ext_gstate(gs);
            s.transparency = true;
        }
        s.set_overprint(self.reg, None, Some(op));
        match (&resolved, pattern) {
            (Some(r), _) => s.set_stroke(&r.col),
            (None, Some(p)) => s.set_stroke_pattern(p),
            _ => {}
        }
        s.line_style(st.weight, st.join, st.cap, st.miter_limit);
        s.set_dash(None);
        s.path(&item.paths);
        s.stroke();
        s.pop();
    }

    /// A styled stroke as the reader lowered it (dashes fitted between corners, stripes,
    /// hashes, dots, gap colour, arrowheads): its parts in order, stroked with real PDF dash
    /// patterns or filled, in the stroke paint or the gap colour.
    fn draw_stroke_art(&mut self, s: &mut Stream, item: &'d PageItem, art: &'d StrokeArt) {
        let st = &item.stroke;
        s.push();
        if let Some((a, m)) = item.print.stroke_blend {
            let gs = self.reg.gs(GsDef::Blend { alpha: (a * 1000.0).round() as u32, mode: m });
            s.ext_gstate(gs);
            s.transparency = true;
        }
        // stroke paint: a colour or a gradient pattern; the gap colour is flat
        let stroke_col = match &st.paint {
            Paint::Swatch { .. } => self.reg.res.resolve(&st.paint).map(Ok),
            Paint::Gradient { id, .. } => {
                let geo = item.stroke_gradient.unwrap_or_default();
                self.shading(id, &geo, &item.id).map(|sh| Err(self.reg.add_pattern(PatternDef { shading: sh, matrix: s.ctm() })))
            }
            Paint::None => None,
        };
        let gap_col = match &st.gap {
            Paint::None => None,
            p => self.reg.res.resolve(p),
        };
        for part in &art.parts {
            let (paint, op) = match part.ink {
                StrokeInk::Stroke => {
                    let Some(c) = &stroke_col else { continue };
                    let op = item.print.overprint_stroke || matches!(c, Ok(r) if self.overprint_black && r.black100);
                    (c.as_ref().map(|r| r.col.clone()).map_err(|p| *p), op)
                }
                StrokeInk::Gap => {
                    let Some(r) = &gap_col else { continue };
                    (Ok(r.col.clone()), item.print.overprint_gap || (self.overprint_black && r.black100))
                }
            };
            match &part.shape {
                PartShape::Line { width, cap, join, miter_limit, dash } => {
                    s.set_overprint(self.reg, None, Some(op));
                    match &paint {
                        Ok(col) => s.set_stroke(col),
                        Err(p) => s.set_stroke_pattern(*p),
                    }
                    s.line_style(*width, *join, *cap, Some(*miter_limit));
                    s.set_dash(dash.as_ref());
                    s.path(&part.paths);
                    s.stroke();
                }
                PartShape::Fill => {
                    s.set_overprint(self.reg, Some(op), None);
                    match &paint {
                        Ok(col) => s.set_fill(col),
                        Err(p) => s.set_fill_pattern(*p),
                    }
                    s.path(&part.paths);
                    s.fill(false);
                }
            }
            if op {
                self.reg.counts.overprint_strokes += 1;
            }
        }
        if item.print.overprint_stroke {
            self.reg.counts.explicit_overprint_strokes += 1;
        }
        s.pop();
    }
}

fn grow_r(r: R, m: f64) -> R {
    R { x0: r.x0 - m, y0: r.y0 - m, x1: r.x1 + m, y1: r.y1 + m }
}
