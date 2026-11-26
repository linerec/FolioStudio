//! Writes frame text, glyph positioning, and text decorations.

use super::{Drawer, union};
use crate::colors::{Col, Resolved};
use crate::content::Stream;
use crate::registry::PatternDef;
use folio_model::geom::PartShape;
use folio_model::layout::{DecorationKind, FrameText, LaidLine};
use folio_model::{Affine, GradientKind, Paint, Point, Rect};
use folio_render::runs;

/// Where a character run ends and the glyphs written so far are off the layout by at least this much (thousandths of the
/// glyph size), InDesign sets the position again with a `Td`.
const RESYNC_UNITS: f32 = 0.5;

type CharacterRun<'d> = (usize, usize, &'d folio_model::CharacterAttrs);

struct GlyphSpacing {
    rounded: Vec<f64>,
    exact: Vec<f64>,
    tracked: bool,
    letter_spacing: bool,
    glyph_run: Vec<Option<usize>>,
}

impl<'a, 'd> Drawer<'a, 'd> {
    pub(super) fn frame_text(&self, id: &str) -> Option<&'d FrameText> {
        self.page_text.get(id).copied().or_else(|| self.layout.frames.get(id))
    }

    pub(super) fn text_extent(&self, frame: &str) -> Option<Rect> {
        let t = self.frame_text(frame)?;
        let mut r: Option<Rect> = None;
        for line in &t.lines {
            let size = line.glyphs.iter().map(|g| g.size * g.vscale.max(1.0)).fold(0.0, f64::max).max(1.0);
            let mut lr = Rect { left: line.left - size, top: line.baseline - size * 1.5, right: line.right + size, bottom: line.baseline + size };
            for g in &line.glyphs {
                lr.left = lr.left.min(g.x - size);
                lr.right = lr.right.max(g.x + size * 2.0);
            }
            r = union(r, Some(lr));
            for shape in &line.shapes {
                r = union(r, shape.bounds());
            }
            for o in &line.objects {
                if let Some(item) = self.inline.get(o.item.as_str())
                    && let Some(e) = self.item_extent(item)
                {
                    r = union(r, Some(Affine::translate(o.x, o.y).apply_rect(&e)));
                }
            }
        }
        r
    }

    /// Glyph indices of `line` before which a word space goes: as many as the story has
    /// spaces inside the line, at the widest gaps the layout left.
    fn space_slots(&mut self, story: &str, line: &folio_model::layout::LaidLine) -> Vec<usize> {
        let doc = self.doc;
        let Some((key, st)) = doc.stories.get_key_value(story) else { return Vec::new() };
        let chars = self.story_chars.entry(key.as_str()).or_insert_with(|| {
            let mut v = Vec::new();
            for (i, p) in st.paragraphs.iter().enumerate() {
                if i > 0 {
                    v.push('\r');
                }
                for r in &p.runs {
                    v.extend(r.text.chars());
                }
            }
            v
        });
        let is_space = |c: char| c == ' ' || c == '\t' || c == '\u{a0}' || c == '\u{3000}' || ('\u{2000}'..='\u{200a}').contains(&c);
        let (a, b) = (line.start.min(chars.len()), line.end.min(chars.len()));
        let range = &chars[a..b];
        let trimmed = range.iter().rposition(|c| !is_space(*c) && *c != '\r' && *c != '\n').map_or(0, |i| i + 1);
        let n = range[..trimmed].iter().filter(|c| is_space(**c)).count();
        if n == 0 || line.glyphs.len() < 2 {
            return Vec::new();
        }
        let mut gaps: Vec<(f64, usize)> = Vec::new();
        for j in 1..line.glyphs.len() {
            let (p, g) = (&line.glyphs[j - 1], &line.glyphs[j]);
            let w = self.reg.fonts.glyph(p.font, p.glyph).map_or(0.0, |x| x.1 as f64);
            let gap = (g.x - (p.x + w / 1000.0 * p.size * p.hscale)) / p.size.max(0.01);
            if gap > 0.05 {
                gaps.push((gap, j));
            }
        }
        gaps.sort_by(|x, y| y.0.total_cmp(&x.0));
        let mut out: Vec<usize> = gaps.into_iter().take(n).map(|(_, j)| j).collect();
        out.sort_unstable();
        out
    }

    /// Records which character each glyph of `line` shows when glyphs and visible characters
    /// pair one to one (no ligature, no added hyphen).
    fn observe_text(&mut self, story: &str, line: &folio_model::layout::LaidLine) {
        let Some(chars) = self.story_chars.get(story) else { return };
        let (a, b) = (line.start.min(chars.len()), line.end.min(chars.len()));
        let visible: Vec<char> = chars[a..b]
            .iter()
            .copied()
            .filter(|c| !c.is_whitespace() && !c.is_control() && *c != '\u{FFFC}' && *c != '\u{AD}' && *c != '\u{FEFF}')
            .collect();
        // '#' stands for computed text (page numbers) the glyphs show differently
        if visible.len() != line.glyphs.len() || visible.is_empty() || visible.contains(&'#') {
            return;
        }
        for (g, c) in line.glyphs.iter().zip(visible) {
            self.reg.fonts.observe(g.font, g.glyph, c);
        }
    }

    /// Table fills and strokes, paragraph rules, shading and borders and footnote rules laid
    /// out with the frame's text (`LaidLine::shapes`): every line's shapes, before any glyph,
    /// as vector fills in the document's inks.
    fn draw_text_shapes(&mut self, s: &mut Stream, text: &'d FrameText) {
        // The highlights of conditions, all lines of the frame, before the text (condx-hl-wrap: the second line's
        // rectangle comes before the first line's text in InDesign's file).
        for line in &text.lines {
            self.draw_highlights(s, line);
        }
        for shape in text.lines.iter().flat_map(|l| &l.shapes) {
            let Some(r) = self.text_paint(&shape.paint, false) else { continue };
            let op = shape.overprint || self.overprint_black && r.black100;
            match &shape.shape {
                PartShape::Line { width, cap, join, miter_limit, dash } => {
                    s.set_overprint(self.reg, None, Some(op));
                    s.set_stroke(&r.col);
                    s.line_style(*width, *join, *cap, Some(*miter_limit));
                    s.set_dash(dash.as_ref());
                    s.path(&shape.paths);
                    s.stroke();
                }
                PartShape::Fill => {
                    s.set_overprint(self.reg, Some(op), None);
                    s.set_fill(&r.col);
                    s.path(&shape.paths);
                    s.fill(false);
                }
            }
        }
    }

    /// The highlight indicators of a line (`DecorationKind::Highlight`): opaque rectangles in the condition's colour.
    fn draw_highlights(&mut self, s: &mut Stream, line: &folio_model::layout::LaidLine) {
        for d in line.decorations.iter().filter(|d| d.kind == DecorationKind::Highlight && !d.screen_only) {
            let Some(rgb) = d.rgb else { continue };
            let r = self.reg.res.rgb(rgb);
            s.push();
            s.concat(&Affine::translate(d.x0, d.y));
            for shape in &d.shapes {
                s.set_overprint(self.reg, Some(false), None);
                s.set_fill(&r.col);
                s.path(&shape.paths);
                s.fill(false);
            }
            s.pop();
        }
    }

    pub(super) fn draw_text(&mut self, s: &mut Stream, frame: &str, story: &str) {
        let Some(text) = self.frame_text(frame) else { return };
        self.draw_text_shapes(s, text);
        self.reg.counts.text_frames_drawn += 1;
        self.reg.drawn_frames.insert(frame.to_owned());
        #[derive(PartialEq, Clone)]
        struct Key {
            font: u32,
            size: f64,
            h: f64,
            v: f64,
            skew: f64,
            y: f64,
            /// Glyphs of a turned line (rotated table cell) are each placed on their own.
            x: Option<f64>,
        }
        /// What the glyph is painted with: fill (+ overprint), stroke (+ overprint, width).
        #[derive(PartialEq, Clone)]
        struct Look {
            fill: Option<(Col, bool)>,
            stroke: Option<(Col, bool, f64)>,
            /// A gradient the layout placed: the pattern that paints the fill / stroke instead of the colour.
            fill_pat: Option<usize>,
            stroke_pat: Option<usize>,
        }
        let mut gradient_patterns: std::collections::HashMap<(String, [u64; 4], [u64; 6]), usize> = std::collections::HashMap::new();
        let mut in_text = false;
        let mut mode: Option<u8> = None;
        for line in &text.lines {
            // A line with shapes (a table cell's fill, a footnote's separator rule, paragraph shading) has them painted
            // before its text in InDesign's file: the text object ends there and the next one starts with an absolute
            // `Tm` (text-special-index; footnotes/fn-body-push).
            if !line.shapes.is_empty() {
                s.end_text_chain();
            }
            // InDesign PDF/X text operands: measured by style-ligatures-22 and
            // wrap-box. Geometry remains full precision; PDF relative leading is
            // rounded to .001 em and metric kerning to .1/1000 em.
            let (line_runs, tracking) = self.line_runs(story, line);
            let mut last_width = 0.0f32;
            let mut cur_tc = 0.0f64;
            let mut cursor_offset = 0.0f64;
            let mut last_rid: Option<usize> = None;
            type TextRun = (Key, Vec<(Vec<u8>, f32)>, f64, f64);
            let mut run: Option<TextRun> = None; // key, items, pen x (where the viewer draws), pen x (where the layout is)
            let mut cur: Option<Look> = None;
            // World-Ready PDF writes bidi glyphs in visual order. Keeping the
            // logical order requires a large TJ back-jump for every glyph and
            // bypasses the font's rounded PDF advances (text-multilingual ar/he).
            // Source mapping still observes the original logical glyph list.
            let mut visual;
            let paint_line=if line.rotation==0.0 && line.glyphs.iter().all(|g|g.hscale>0.0)
                && line.text_clusters.iter().any(|c|c.x0>c.x1) {
                visual=line.clone();
                visual.glyphs.sort_by(|a,b|a.x.total_cmp(&b.x));
                &visual
            }else{line};
            let spaces = self.space_slots(story, paint_line);
            self.observe_text(story, line);
            let GlyphSpacing {
                rounded: tcs,
                exact: tc_exact,
                tracked: line_tracked,
                letter_spacing: letter_tc,
                glyph_run,
            } =
                self.glyph_spacing(line, paint_line, &line_runs, tracking);
            for (range, stroke_pass) in runs::paint_passes(&paint_line.glyphs) {
            for j in range {
                let g = &paint_line.glyphs[j];
                // A word space before this glyph ends the current run (it draws nothing).
                if spaces.binary_search(&j).is_ok()
                    && let Some((key, items, pen, lay)) = run.as_mut()
                    && let Some((code, w)) = self.reg.fonts.space(key.font)
                {
                    match items.last_mut() {
                        Some(last) if last.1 == 0.0 => last.0.extend_from_slice(&code.to_be_bytes()),
                        _ => items.push((code.to_be_bytes().to_vec(), 0.0)),
                    }
                    *pen += (f64::from(w.round()) + cur_tc) / 1000.0 * key.size * key.h;
                    if j > 0 {
                        *lay += (tc_exact[j - 1] - tcs[j - 1]) / 1000.0 * key.size * key.h;
                    }
                    *lay += (f64::from(w) + cur_tc) / 1000.0 * key.size * key.h;
                    last_width = w;
                }
                let fill_placed = g.fill_gradient.is_some_and(|p| p.area.is_some());
                let stroke_placed = g.stroke_gradient.is_some_and(|p| p.area.is_some());
                let fill = (!stroke_pass).then(|| self.text_paint(&g.fill, fill_placed)).flatten();
                let stroke = stroke_pass.then(|| self.text_paint(&g.stroke, stroke_placed).map(|r| (r, runs::stroke_width(g)))).flatten();
                let fill_pat = if fill.is_some() { self.glyph_pattern(s, &mut gradient_patterns, frame, &g.fill, g.fill_gradient.as_ref()) } else { None };
                let stroke_pat = if stroke.is_some() { self.glyph_pattern(s, &mut gradient_patterns, frame, &g.stroke, g.stroke_gradient.as_ref()) } else { None };
                let count_glyph = !stroke_pass || g.fill == Paint::None;
                if fill.is_none() && stroke.is_none() {
                    if !stroke_pass && g.stroke == Paint::None { self.reg.counts.glyphs_without_paint += 1; }
                    continue;
                }
                let Some((code, w)) = self.reg.fonts.glyph(g.font, g.glyph) else {
                    if count_glyph { self.reg.counts.glyphs_without_font += 1; }
                    let why = self.reg.fonts.entries.get(&g.font).and_then(|e| e.why_not.clone()).unwrap_or_else(|| "font missing".into());
                    self.reg.warn.add("font", format!("glyphs not drawn: {why}"), Some(self.page), Some(frame));
                    continue;
                };
                if count_glyph { self.reg.counts.glyphs += 1; }
                let op_fill = fill.as_ref().is_some_and(|r| (self.overprint_black && r.black100) || g.overprint_fill);
                let op_stroke = stroke.as_ref().is_some_and(|(r, _)| (self.overprint_black && r.black100) || g.overprint_stroke);
                if let Some(r) = &fill {
                    if r.k_only() {
                        self.reg.counts.black_only_glyphs += 1;
                    }
                    if op_fill {
                        self.reg.counts.black_overprint_glyphs += 1;
                    }
                }
                if count_glyph { self.reg.fonts.mark_drawn(g.font, usize::from(g.glyph == 0)); }
                let look = Look { fill: fill.map(|r| (r.col, op_fill)), stroke: stroke.map(|(r, w)| (r.col, op_stroke, w)), fill_pat, stroke_pat };
                let turned = line.rotation != 0.0 || g.path_transform.is_some();
                let key = Key { font: g.font, size: g.size, h: g.hscale, v: g.vscale, skew: g.skew, y: g.y, x: turned.then_some(g.x) };
                let same_look = cur.as_ref() == Some(&look);
                let mut continues = g.path_transform.is_none() && same_look && run.as_ref().is_some_and(|(k, _, _, _)| *k == key);
                let tc = tcs[j];
                let rid = glyph_run[j];
                // Where the capitalization or the position (superscript, numerator, ...) changes, InDesign starts a new show
                // operator: with the new `Tc` when only that changed, and with a `Td` that sets the position again (the width
                // rounding of the glyphs before it dropped) when the pen is off the layout's position. The other attributes (OpenType
                // features, tracking) do not start one (measured: ot-lower_case, ot-ot_numerator, ot-fraction-cambria, probe-*).
                let mut split = false;
                if continues && rid != last_rid
                    && let (Some(a), Some(b)) = (last_rid, rid)
                    && (line_runs[a].2.capitalization != line_runs[b].2.capitalization || line_runs[a].2.position != line_runs[b].2.position)
                {
                    let (_, _, pen, lay) = run.as_ref().expect("continuation requires an existing text run");
                    let scale = g.size * g.hscale;
                    if scale.abs() > 1e-9 {
                        let gap = ((*pen - g.x) * 1000.0 / scale) as f32;
                        let kern = ((*lay - g.x) * 1000.0 / scale) as f32;
                        if (cur_tc != 0.0 && tc == 0.0) || (kern.abs() <= 0.01 && gap.abs() >= RESYNC_UNITS) {
                            continues = false;
                        } else {
                            // the new operator counts the width rounding from here on
                            if let Some((_, _, pen, lay)) = run.as_mut() {
                                *pen = *lay;
                            }
                            split = tc != cur_tc;
                        }
                    }
                }
                last_rid = rid;
                if !continues {
                    let font_switch = run.as_ref().filter(|(k, _, _, lay)| {
                        same_look && !turned && tracking == Some(0.0) && k.font != key.font && k.size == key.size
                            && k.h == key.h && k.v == key.v && k.skew == key.skew
                            && k.y == key.y && (*lay - g.x).abs() < 1e-6 && cur_tc == tc
                    }).map(|(_, _, pen, _)| *pen + cursor_offset);
                    if let Some((_, items, _, _)) = run.take() {
                        s.show_positioned(&items);
                    }
                    if !in_text {
                        s.begin_text();
                        in_text = true;
                        mode = None;
                    }
                    if !same_look {
                        let m = match (&look.fill, &look.stroke) {
                            (Some(_), Some(_)) => 2u8,
                            (None, Some(_)) => 1,
                            _ => 0,
                        };
                        s.set_overprint(self.reg, look.fill.as_ref().map(|f| f.1), look.stroke.as_ref().map(|x| x.1));
                        if let Some((c, _)) = &look.fill {
                            match look.fill_pat {
                                Some(p) => s.set_fill_pattern(p),
                                None => s.set_fill(c),
                            }
                        }
                        if let Some((c, _, width)) = &look.stroke {
                            match look.stroke_pat {
                                Some(p) => s.set_stroke_pattern(p),
                                None => s.set_stroke(c),
                            }
                            s.line_style(*width, folio_model::LineJoin::Miter, folio_model::LineCap::Butt, Some(4.0));
                            s.set_dash(None);
                        }
                        if mode != Some(m) {
                            s.text_render_mode(m);
                            mode = Some(m);
                        }
                        cur = Some(look);
                    }
                    // Tc preserves uniform tracking without repeated TJ rounding.
                    // InDesign uses Tf=1 and puts point size into Tm.
                    s.font(g.font, 1.0);
                    s.char_spacing(tc / 1000.0);
                    cur_tc = tc;
                    let shear = g.skew.to_radians().tan();
                    let (a, b, c, d) = (g.hscale*g.size, 0.0, shear*g.vscale*g.size, -g.vscale*g.size);
                    // InDesign moves with `Td` (in units of the text matrix, rounded to .001) while the matrix stays the
                    // same, whatever the font and the frame: Tf and Tm are separate operators and one text object runs
                    // on across frames. A new matrix is an absolute `Tm`. The rounding errors add up, so the positions
                    // written here are the ones the moves reach (the chain lives in the stream, in its root space).
                    let ctm = s.ctm();
                    let (ux, uy) = (g.size * g.hscale, g.size * g.vscale);
                    let flat = !turned && ctm.b == 0.0 && ctm.c == 0.0 && ctm.a == 1.0 && ctm.d.abs() == 1.0 && ux.abs() > 1e-9 && uy.abs() > 1e-9;
                    let (x, y) = if let Some(x) = font_switch.filter(|_| flat) {
                        let y = s.text_chain().and_then(|ch| ctm.invert().map(|inv| inv.apply(Point { x: ch.printed.0, y: ch.printed.1 }).y)).unwrap_or(g.y);
                        (x, y)
                    } else if flat {
                        let root = ctm.apply(Point { x: g.x, y: g.y });
                        let matrix = [g.size, g.hscale, g.vscale, g.skew];
                        let printed = match s.text_chain() {
                            Some(ch) if ch.matrix == matrix => (
                                ch.printed.0 + ((root.x - ch.exact.0) / ux * 1000.0).round() / 1000.0 * ux,
                                ch.printed.1 + ((root.y - ch.exact.1) / uy * 1000.0).round() / 1000.0 * uy,
                            ),
                            _ => (root.x, root.y),
                        };
                        s.set_text_chain(matrix, (root.x, root.y), printed);
                        ctm.invert().map_or((g.x, g.y), |inv| {
                            let p = inv.apply(Point { x: printed.0, y: printed.1 });
                            (p.x, p.y)
                        })
                    } else {
                        s.end_text_chain();
                        (g.x, g.y)
                    };
                    if turned {
                        // Turned clockwise in the frame's y-down space.
                        let t = g.transform(line.rotation);
                        s.text_matrix([t.a,t.b,t.c,t.d,t.tx,t.ty]);
                    } else {
                        s.text_matrix([a, b, c, d, x, y]);
                    }
                    // A font-only change keeps the painted cursor but restarts width accumulation.
                    cursor_offset = if font_switch.is_some() && flat { x - g.x } else { 0.0 };
                    run = Some((key, Vec::new(), if font_switch.is_some() && flat { g.x } else { x }, g.x));
                }
                let (_, items, pen, lay) = run.as_mut().expect("text run was created or continued");
                let scale = g.size * g.hscale;
                if !items.is_empty() && scale.abs() > 1e-9 {
                    // `pen` is where the viewer draws the next glyph: the font's PDF widths (whole thousandths) and the
                    // adjustments written so far put it there; `lay` is where the layout's exact widths put it. InDesign
                    // writes an adjustment where the layout kerns (lay is off g.x) and then closes the whole gap to the
                    // layout's position, the rounding of the widths since the last adjustment included (measured:
                    // ot-ss06, text-case-words-lower: 12.8 where the kern alone is 11.1). Rounding alone never writes one.
                    let gap = ((*pen - g.x) * 1000.0 / scale) as f32;
                    let kern = ((*lay - g.x) * 1000.0 / scale) as f32;
                    if kern.abs() > 0.01 {
                        let adj = if kern.abs() >= 200.0 {
                            // a space-sized gap, not a kern (a line whose spaces are not glyphs): written as it is
                            kern
                        } else if line_tracked {
                            // text with tracking (Tc): the kern, rounded, and only the rounding of the glyph before it
                            (((kern * 10.0).round() / 10.0 + last_width.round() - last_width) * 10.0).round() / 10.0
                        } else {
                            (gap * 10.0).round() / 10.0
                        };
                        // With letter spacing the Tc is rounded to a whole thousandth, so every glyph is off by what the rounding
                        // leaves: InDesign writes an adjustment only from half a thousandth on (measured: 1.08 % writes none for
                        // Hangul, 1.00 % writes 0.5 for each).
                        let adj = if letter_tc && kern.abs() < 200.0 && adj.abs() < 0.5 { 0.0 } else { adj };
                        items.last_mut().expect("text adjustment follows an item").1 += adj;
                        *pen -= f64::from(adj) * scale / 1000.0;
                    }
                }
                if split {
                    // same position, the next operator with the new Tc
                    s.show_positioned(items);
                    items.clear();
                    s.char_spacing(tc / 1000.0);
                    cur_tc = tc;
                }
                match items.last_mut() {
                    Some(last) if last.1 == 0.0 => last.0.extend_from_slice(&code.to_be_bytes()),
                    _ => items.push((code.to_be_bytes().to_vec(), 0.0)),
                }
                *pen += (f64::from(w.round()) + cur_tc) / 1000.0 * scale;
                *lay = g.x + (f64::from(w) + cur_tc) / 1000.0 * scale;
                last_width = w;
            }
            if let Some((_, items, _, _)) = run.take() {
                s.show_positioned(&items);
            }
            }
        }
        if in_text {
            s.char_spacing(0.0);
            if mode.is_some_and(|m| m != 0) {
                s.text_render_mode(0);
            }
            s.end_text();
        }
        self.draw_text_decorations(s, text);
        self.draw_inline_objects(s, text);
    }

    fn line_runs(&self, story: &str, line: &LaidLine) -> (Vec<CharacterRun<'d>>, Option<f64>) {
        let mut at = 0usize;
        let mut tracking: Option<f64> = None;
        let mut uniform = true;
        // The character runs the line shows: (first char, end char, attributes).
        let mut line_runs: Vec<CharacterRun<'d>> = Vec::new();
        let doc = self.doc;
        if let Some(st) = doc.stories.get(story) {
            for p in &st.paragraphs {
                for r in &p.runs {
                    let end = at + r.text.chars().count();
                    if at < line.end && end > line.start {
                        if tracking.is_some_and(|t| t != r.attrs.tracking) { uniform = false; }
                        tracking = Some(r.attrs.tracking);
                        line_runs.push((at, end, &r.attrs));
                    }
                    at = end;
                }
                at += 1;
            }
        }
        (line_runs, if uniform { tracking } else { None })
    }

    fn glyph_spacing(
        &self,
        line: &LaidLine,
        paint_line: &LaidLine,
        line_runs: &[CharacterRun<'d>],
        tracking: Option<f64>,
    ) -> GlyphSpacing {
        // The character run each glyph belongs to (through the cluster that starts where the glyph does).
        let glyph_run: Vec<Option<usize>> = if std::ptr::eq(paint_line, line) {
            paint_line.glyphs.iter().map(|g| {
                let c = line.text_clusters.iter().filter(|c| (c.x0 - g.x).abs() < 0.05).min_by(|a, b| (a.x0 - g.x).abs().total_cmp(&(b.x0 - g.x).abs()))?;
                line_runs.iter().position(|(a, b, _)| *a <= c.start && c.start < *b)
            }).collect()
        } else {
            vec![None; paint_line.glyphs.len()]
        };
        // Character spacing (Tc) of each glyph: the line's tracking; a numerator or denominator run (OpenType `numr` and
        // `dnom`) carries the whole number of thousandths its first pair is off by, the rest of the pair adjustments being
        // written on top (measured: ot-ot_numerator, ot-ot_denominator; InDesign writes -0.008 Tc for Calibri's).
        // Letter spacing (the paragraph's, widened or narrowed by justification) is written as the line's Tc like tracking,
        // whole thousandths of the em (measured: parjust-spacing-*, 1.08 % of Malgun's 351.56 space is 3.80, written 0.004).
        let letter_tc = line.letter_spacing != 0.0;
        let line_tracked = tracking.is_some_and(|t| t != 0.0) || letter_tc;
        let mut tcs = vec![tracking.unwrap_or(0.0); paint_line.glyphs.len()];
        // The Tc before rounding: where the layout puts a word space, the glyph before it has advanced by this, not by the Tc.
        let mut tc_exact = tcs.clone();
        if letter_tc {
            for (j, ((t, x), g)) in tcs.iter_mut().zip(tc_exact.iter_mut()).zip(&paint_line.glyphs).enumerate() {
                if let Some((_, w)) = self.reg.fonts.space(g.font) {
                    // tracking of the glyph's own run (a line of runs with different tracking has no uniform one)
                    let own = glyph_run[j].map_or(*t, |r| line_runs[r].2.tracking);
                    *x = own + line.letter_spacing * f64::from(w);
                    *t = x.round();
                }
            }
        }
        if !line_tracked {
            let mut j = 0;
            while j < glyph_run.len() {
                let Some(r) = glyph_run[j] else { j += 1; continue };
                let mut e = j + 1;
                while e < glyph_run.len() && glyph_run[e] == Some(r) { e += 1; }
                if matches!(line_runs[r].2.position, folio_model::Position::OtNumerator | folio_model::Position::OtDenominator) && e - j >= 2 {
                    let (a, b) = (&paint_line.glyphs[j], &paint_line.glyphs[j + 1]);
                    let scale = a.size * a.hscale;
                    if let Some((_, w)) = self.reg.fonts.glyph(a.font, a.glyph) && scale.abs() > 1e-9 {
                        let off = ((b.x - a.x) * 1000.0 / scale - f64::from(w)).trunc();
                        if off != 0.0 {
                            for t in &mut tcs[j..e] { *t = off; }
                        }
                    }
                }
                j = e;
            }
        }
        GlyphSpacing { rounded: tcs, exact: tc_exact, tracked: line_tracked, letter_spacing: letter_tc, glyph_run }
    }

    fn draw_text_decorations(&mut self, s: &mut Stream, text: &'d FrameText) {
        for line in &text.lines {
            for d in &line.decorations {
                // a condition indicator that is for the screen only (`ShowIndicators`) does not print;
                // highlights were painted before their line's text
                if d.screen_only || d.kind == DecorationKind::Highlight { continue; }
                s.push();
                s.concat(&Affine::translate(d.x0, d.y));
                for shape in &d.shapes {
                    let Some(r) = (match d.rgb { Some(rgb) => Some(self.reg.res.rgb(rgb)), None => self.text_paint(&shape.paint, false) }) else { continue };
                    let op = shape.overprint || self.overprint_black && r.black100;
                    if let PartShape::Line { width, cap, join, miter_limit, dash } = &shape.shape {
                        s.set_overprint(self.reg, None, Some(op));
                        s.set_stroke(&r.col);
                        s.line_style(*width, *join, *cap, Some(*miter_limit));
                        s.set_dash(dash.as_ref());
                        s.path(&shape.paths);
                        s.stroke();
                    } else if let Some((width, path)) = crate::text_lines::horizontal_stroke(&shape.paths) {
                        s.set_overprint(self.reg, None, Some(op));
                        s.set_stroke(&r.col);
                        s.line_style(width, folio_model::LineJoin::Miter, folio_model::LineCap::Butt, Some(4.));
                        s.set_dash(None);
                        s.path(&[path]);
                        s.stroke();
                    } else {
                        s.set_overprint(self.reg, Some(op), None);
                        s.set_fill(&r.col);
                        s.path(&shape.paths);
                        s.fill(false);
                    }
                }
                s.pop();
            }
        }
    }

    fn draw_inline_objects(&mut self, s: &mut Stream, text: &'d FrameText) {
        for line in &text.lines {
            for o in &line.objects {
                if let Some(&item) = self.inline.get(o.item.as_str())
                    && self.printed(item)
                {
                    s.push();
                    s.concat(&Affine::translate(o.x, o.y));
                    self.draw_item(s, item);
                    s.pop();
                }
            }
        }
    }

    /// The pattern that paints a glyph whose fill or stroke is a gradient the layout placed (against its column and frame): its
    /// shading in the frame's coordinates, one pattern for every glyph with the same placement. `None` for a flat paint, or a
    /// gradient with no area (table cells, footnotes), which is written as its first stop (`text_paint`).
    fn glyph_pattern(&mut self, s: &Stream, patterns: &mut std::collections::HashMap<(String, [u64; 4], [u64; 6]), usize>, frame: &str, paint: &Paint, placed: Option<&folio_model::PlacedGradient>) -> Option<usize> {
        let Paint::Gradient { id, .. } = paint else { return None };
        let radial = self.doc.gradients.get(id)?.kind == GradientKind::Radial;
        let geo = placed?.geometry(radial)?;
        let m = s.ctm();
        let key = (id.clone(), [geo.start.x, geo.start.y, geo.length, geo.angle].map(f64::to_bits), [m.a, m.b, m.c, m.d, m.tx, m.ty].map(f64::to_bits));
        if let Some(&p) = patterns.get(&key) {
            return Some(p);
        }
        let shading = self.shading(id, &geo, frame)?;
        let p = self.reg.add_pattern(PatternDef { shading, matrix: m });
        patterns.insert(key, p);
        Some(p)
    }

    /// Text paints flat unless the layout placed its gradient (`glyph_pattern`); a gradient without a place is written as its
    /// first stop (reported).
    fn text_paint(&mut self, p: &Paint, placed: bool) -> Option<Resolved> {
        match p {
            Paint::Gradient { id, .. } => {
                if !placed {
                    self.reg.warn.add("text", "gradient on text written as its first colour", Some(self.page), None);
                }
                let first = self.doc.gradients.get(id)?.stops.first()?.color.clone();
                self.reg.res.resolve(&Paint::Swatch { id: first, tint: 100.0 })
            }
            _ => self.reg.res.resolve(p),
        }
    }
}
