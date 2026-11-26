//! Build glyphs, inline objects, decorations and hit-test positions for a fitted line.

use super::*;
use crate::shape::Cluster;

pub(super) struct LineContent<'l, 'p, 'f> {
    pub(super) paragraph: &'l ParaCtx<'p>,
    pub(super) prepared: &'l Prepared,
    pub(super) placed: Option<&'l folio_compose::PlacedLine>,
    pub(super) flow: &'l Flow<'f>,
    pub(super) metrics: &'l LineMetrics,
    pub(super) baseline: f64,
    pub(super) left: f64,
    pub(super) right: f64,
    pub(super) slot_left: f64,
    pub(super) alignment: Alignment,
    pub(super) verso: bool,
    pub(super) kenten_drawn: bool,
}

impl LineContent<'_, '_, '_> {
    pub(super) fn build(
        &self, ctx: &Ctx<'_, '_>, items: std::ops::Range<usize>, last_line: bool,
        indent_here: &mut f64,
    ) -> (LaidLine, (f64, f64)) {
        let (s, e) = (items.start, items.end);
        let p = self.paragraph;
        let (para, shaped) = (p.para, p.shaped);
        let a = &para.attrs;
        let (prepared, placed, m) = (self.prepared, self.placed, self.metrics);
        let (baseline, line_left, line_right) = (self.baseline, self.left, self.right);
        let mut glyphs = Vec::new();
        let mut text_clusters = Vec::new();
        let mut decorations: Vec<Decoration> = Vec::new();
        // Where text of conditions sits on this line: conditions, left, right, baseline (their indicators).
        // (one per run of equal character attributes: InDesign draws 'pha ', 'b' and 'o' of three ranges as three marks)
        let mut indicator_runs: Vec<(&[String], f64, f64, f64, usize)> = Vec::new();
        let mut objects: Vec<PlacedObject> = Vec::new();
        let mut shapes: Vec<TextShape> = Vec::new();
        // Where above-line objects go: their room's top, set at the first.
        let mut above_cursor: Option<f64> = None;
        let (mut first_char, mut end_char) = (None, None);
        // Conditions' indicators stop at the last visible item. Underlines and strike-throughs do not: InDesign draws them
        // under the trailing spaces of a line and of a paragraph too (text-format/charlines2-trailing-*).
        let visible_end = (s..e)
            .rev()
            .find(|&i| !folio_compose::is_trailing_blank(prepared.items[i].ch))
            .map_or(s, |i| i + 1);
        // ... but a line that wraps keeps its last space in them (condx-hl-wrap: the space at the end of the first line
        // is highlighted; measured with highlights, the underlines of it are not measured).
        let indicator_end = if last_line { visible_end } else { e };
        // Item positions from the line start: the composer's, reordered by bidi level
        // when the line holds right-to-left text.
        let xs: Vec<f64> = match placed {
            Some(pl) if !shaped.levels.is_empty() => visual_positions(prepared, shaped, s, pl),
            Some(pl) => pl.x.clone(),
            None => Vec::new(),
        };
        for (k, idx) in (s..e).enumerate() {
            let x = line_left + xs.get(k).copied().unwrap_or(0.0);
            let gscale = placed.map_or(1.0, |p| p.glyph_scale);
            match prepared.origin[idx] {
                Some(ci) => {
                    let cl = &shaped.clusters[ci];
                    first_char.get_or_insert(cl.chars.start);
                    end_char = Some(cl.chars.end);
                    let attrs = &para.runs[cl.run].attrs;
                    let w = placed.map_or(0.0, |p| p.width[k]);
                    let rtl = shaped.levels.get(cl.chars.start).is_some_and(|level| level % 2 == 1);
                    text_clusters.push(folio_model::layout::TextCluster {
                        start: p.index[cl.chars.start], end: p.index[cl.chars.end],
                        x0: if rtl { x + w } else { x }, x1: if rtl { x } else { x + w },
                        top: baseline - m.ascent.max(1.0), bottom: baseline + m.descent.max(1.0), path_map: None,
                    });
                    if attrs.underline || attrs.strike_through {
                        let w = placed.map_or(0.0, |p| p.width[k]);
                        let metrics = ctx.table.metrics(cl.font);
                        let size = cl.size * attrs.vertical_scale / 100.0;
                        decorate(&mut decorations, attrs, &metrics, size, x, x + w, baseline - cl.rise, !shaped.vertical.is_empty());
                    }
                    if !attrs.conditions.is_empty() && idx < indicator_end && ctx.doc.condition_indicators != folio_model::conditions::Indicators::Hide {
                        let w = placed.map_or(0.0, |p| p.width[k]);
                        let y = baseline - cl.rise;
                        match indicator_runs.last_mut() {
                            Some(last) if last.4 == cl.run && last.0 == attrs.conditions.as_slice() && (last.3 - y).abs() < 1e-6 && (last.2 - x).abs() < 0.01 => last.2 = x + w,
                            _ => indicator_runs.push((attrs.conditions.as_slice(), x, x + w, y, cl.run)),
                        }
                    }
                    if cl.kind == ClusterKind::Object
                        && let Some(item) = inline_item(para, cl.run, cl.chars.start)
                    {
                        objects.extend(self.anchored_object(ctx, cl, item, x, &mut above_cursor));
                    }
                    if cl.kind == ClusterKind::IndentHere {
                        *indent_here = x - (self.slot_left + a.left_indent);
                    }
                    if cl.kind == ClusterKind::SoftHyphen {
                        if idx + 1 == e {
                            glyphs.push(hyphen_glyph(shaped, cl.run, attrs, x, baseline - cl.rise));
                        }
                        continue;
                    }
                    if cl.kind == ClusterKind::Tab {
                        glyphs.extend(tab_leaders::place(ctx.table, a, attrs, cl.font,
                            cl.size, x, baseline - cl.rise, w, self.slot_left));
                    }
                    self.place_cluster_glyphs(ctx, ci, x, w, gscale, &mut glyphs, &mut shapes);
                }
                None if idx + 1 == e => {
                    let r = prepared.run[idx];
                    let attrs = &para.runs[r].attrs;
                    let rise = attrs.baseline_shift;
                    glyphs.push(hyphen_glyph(shaped, r, attrs, x, baseline - rise));
                }
                None => {}
            }
        }
        let (start, end) = self.finish_clusters(&mut text_clusters, first_char, end_char, last_line);
        character_lines::finish(ctx.doc, &mut decorations);
        decorations.extend(indicators::decorations(ctx.doc, &indicator_runs, (m.bbox_above, m.bbox_below)));
        // The Tc InDesign's PDF writes for the line: the desired letter spacing plus what justification added
        // (the part it added is not scaled by the line's glyph scale, the desired part is).
        let letter_spacing = {
            let ls = a.letter_spacing.desired / 100.0;
            let gs = a.glyph_scaling.desired / 100.0 * placed.map_or(1.0, |p| p.glyph_scale);
            ls + placed.map_or(0.0, |p| p.letter_fraction) / gs.max(1e-9)
        };
        let line = LaidLine {
            text_clusters,
            baseline,
            left: line_left,
            right: line_right,
            glyphs,
            letter_spacing,
            start,
            end,
            decorations,
            objects,
            shapes,
            rotation: 0.0,
            table: None,
            cell: None,
        };
        (line, self.visible_bounds(s, visible_end, &xs))
    }

    fn finish_clusters(
        &self, text_clusters: &mut Vec<folio_model::layout::TextCluster>,
        first_char: Option<usize>, end_char: Option<usize>, last_line: bool,
    ) -> (usize, usize) {
        let p = self.paragraph;
        let shaped = p.shaped;
        let (baseline, m, line_left) = (self.baseline, self.metrics, self.left);
        let at = |i: usize| p.index[i.min(shaped.char_count)];
        let start = at(first_char.unwrap_or(shaped.char_count));
        let mut end = at(end_char.unwrap_or(shaped.char_count));
        if last_line && !p.last_para {
            end = at(shaped.char_count) + 1;
        }
        // Empty lines and explicit paragraph returns have insertion points too.
        let last = text_clusters.last();
        let from = last.map_or(start, |c| c.end);
        let x = last.map_or(line_left, |c| c.x1);
        if text_clusters.is_empty() || end > from {
            text_clusters.push(folio_model::layout::TextCluster {
                start: from, end, x0: x, x1: x,
                top: baseline - m.ascent.max(1.0), bottom: baseline + m.descent.max(1.0), path_map: None,
            });
        }
        (start, end)
    }

    fn visible_bounds(&self, s: usize, visible_end: usize, xs: &[f64]) -> (f64, f64) {
        let (prepared, placed) = (self.prepared, self.placed);
        let (shaped, line_left) = (self.paragraph.shaped, self.left);
        // Left and right of the visible text (text-width rules, shading, borders).
        let first_visible = (s..visible_end).find(|&i| !folio_compose::is_trailing_blank(prepared.items[i].ch));
        match (first_visible, placed) {
            (Some(f), Some(pl)) if visible_end > f && shaped.levels.is_empty() => {
                let (k0, k1) = (f - s, visible_end - 1 - s);
                (line_left + pl.x[k0], line_left + pl.x[k1] + pl.width[k1])
            }
            (Some(f), Some(pl)) if visible_end > f => {
                let ks = (f - s)..(visible_end - s);
                let l = ks.clone().map(|k| xs[k]).fold(f64::MAX, f64::min);
                let r = ks.map(|k| xs[k] + pl.width[k]).fold(f64::MIN, f64::max);
                (line_left + l, line_left + r)
            }
            _ => (line_left, line_left),
        }
    }

    fn anchored_object(
        &self, ctx: &Ctx<'_, '_>, cl: &Cluster, item: &PageItem, x: f64,
        above_cursor: &mut Option<f64>,
    ) -> Option<PlacedObject> {
        let (baseline, m) = (self.baseline, self.metrics);
        let fm = ctx.table.metrics(cl.font);
        let anchor = item.anchor.unwrap_or_default();
        match (anchor.position, self.flow.column(), self.flow.frame()) {
            (folio_model::AnchorPosition::AboveLine, Some(col), _) => {
                let cursor = above_cursor.get_or_insert(baseline - m.above_line - fm.embox_top * cl.size);
                anchors::place_above_line(item, cursor, &col, self.alignment, self.verso)
            }
            (folio_model::AnchorPosition::Custom, Some(col), Some(ff)) => {
                let font = anchors::AnchorFont {
                    size: cl.size,
                    cap: fm.cap_height * cl.size,
                    x_height: fm.x_height * cl.size,
                    embox_top: fm.embox_top * cl.size,
                };
                anchors::place_custom(item, x, baseline, &font, m, &col, ff, self.alignment)
            }
            // Inline: its bottom left at the pen on the baseline, the Y
            // offset up (InDesign keeps an inline item's geometry from
            // where it was made; only its size counts).
            _ => anchors::bounds(item).map(|b| PlacedObject {
                item: item.id.clone(),
                x: x - b.left,
                y: baseline - cl.rise - anchor.y_offset - b.bottom,
                ..PlacedObject::default()
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_cluster_glyphs(
        &self, ctx: &Ctx<'_, '_>, ci: usize, x: f64, w: f64, gscale: f64,
        glyphs: &mut Vec<PlacedGlyph>, shapes: &mut Vec<TextShape>,
    ) {
        let p = self.paragraph;
        let shaped = p.shaped;
        let cl = &shaped.clusters[ci];
        let a = &p.para.attrs;
        let attrs = &p.para.runs[cl.run].attrs;
        let baseline = self.baseline;
        let drop_chars = p.drop_cap.map_or(0, |d| d.chars);
        let drop_lines = p.drop_cap.map_or(0, |d| d.lines);
        let drop_raise = p.drop_cap.map_or(0.0, |d| d.raise);
        let scale = if cl.kind == ClusterKind::Glyphs {
            a.glyph_scaling.desired / 100.0 * gscale
        } else {
            1.0
        };
        let gradients = attrs.placed_gradients();
        let vertical = shaped.vertical.get(ci).copied();
        for g in &shaped.glyphs[cl.glyphs.clone()] {
            // Vertical text: where the quarter turn puts the glyph upright (or turned with the line).
            let vplace = vertical.map(|o| {
                let fm = ctx.table.metrics(g.font);
                let v = &attrs.vertical;
                crate::vertical::place(o, x, baseline - cl.rise, g.dx * scale, g.dy, cl.size, fm.embox_top, v.rotation,
                    (v.tatechuyoko_x, v.tatechuyoko_y), w)
            });
            glyphs.push(PlacedGlyph {
                font: g.font,
                glyph: g.id,
                x: vplace.map_or(x + g.dx * scale, |p| p.x),
                // A drop cap's baseline is that of the last line it spans.
                y: vplace.map_or(0.0, |p| p.y - (baseline - cl.rise - g.dy)) + baseline - cl.rise - g.dy + if cl.chars.end <= drop_chars { drop_lines.saturating_sub(1) as f64 * self.metrics.leading - drop_raise } else { 0.0 },
                size: cl.size,
                hscale: attrs.horizontal_scale / 100.0 * scale,
                vscale: attrs.vertical_scale / 100.0,
                skew: attrs.skew,
                path_transform: vplace.and_then(|p| p.linear),
                fill: attrs.fill.clone(),
                stroke: attrs.stroke.clone(),
                stroke_weight: attrs.stroke_weight,
                overprint_fill: attrs.overprint_fill,
                overprint_stroke: attrs.overprint_stroke,
                fill_gradient: gradients.0,
                stroke_gradient: gradients.1,
            });
        }
        if let Some(k) = attrs.kenten.as_ref().filter(|_| self.kenten_drawn)
            && cl.kind == ClusterKind::Glyphs
            && !cl.ch.is_whitespace()
        {
            let fm = ctx.table.metrics(cl.font);
            let target = kenten::Target {
                x0: x,
                x1: x + cl.advance * scale,
                baseline: baseline - cl.rise,
                size: cl.size,
                embox_top: fm.embox_top * cl.size,
            };
            let custom = (k.kind == folio_model::KentenKind::Custom)
                .then(|| ctx.custom_mark(cl.font, &k.custom))
                .flatten();
            let (shape, glyph) = kenten::marks(k, &target, &attrs.fill, attrs.overprint_fill, custom);
            shapes.extend(shape);
            glyphs.extend(glyph);
        }
    }
}
