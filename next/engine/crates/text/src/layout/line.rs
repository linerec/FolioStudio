//! Measure individual lines and position their glyphs, inline objects and decorations.

use super::*;

impl<'c, 'a> Ctx<'c, 'a> {
    /// The glyph of a custom emphasis mark (`KentenCustomCharacter`) in the run's font.
    /// How far the first and last glyph of line `s..e` hang past the left and right margin, in points
    /// (optical margin alignment; zero when it is off, ignored by the paragraph, or the line is bidirectional).
    pub(super) fn optical_hang(&self, st: &TextState, a: &ParagraphAttrs, prepared: &Prepared, shaped: &ShapedParagraph, s: usize, e: usize) -> (f64, f64) {
        let Some(optical) = st.optical else { return (0.0, 0.0) };
        if a.ignore_edge_alignment || a.right_to_left || !shaped.levels.is_empty() && shaped.levels.iter().any(|&l| l != 0) || s >= e {
            return (0.0, 0.0);
        }
        let blank = |i: usize| folio_compose::is_trailing_blank(prepared.items[i].ch) || prepared.items[i].ch == '\u{9}';
        let glyph_of = |i: usize| -> Option<(FontId, char, f64)> {
            match prepared.origin[i] {
                Some(ci) => {
                    let c = &shaped.clusters[ci];
                    (c.kind == ClusterKind::Glyphs).then_some((c.font, c.ch, c.size))
                }
                // A hyphen drawn at a line end takes the face of the letter before it.
                None if prepared.items[i].ch == folio_compose::SOFT_HYPHEN => (s..i).rev().find_map(|j| prepared.origin[j]).map(|ci| {
                    let c = &shaped.clusters[ci];
                    (c.font, '-', c.size)
                }),
                None => None,
            }
        };
        let hang = |g: Option<(FontId, char, f64)>, left: bool| -> f64 {
            let Some((font, ch, size)) = g else { return 0.0 };
            // The optical size is limited to 0.75 .. 4 times the glyph size; the left edge takes it, the right one the glyph size.
            let q = optical.clamp(0.75 * size, 4.0 * size);
            let Some(edges) = self.fonts.with_face(font, |face| crate::optical_margin::edges(face, ch, size / q)).flatten() else { return 0.0 };
            if left { edges.left * q } else { edges.right * size }
        };
        let first = (s..e).find(|&i| !blank(i)).and_then(glyph_of);
        let last = (s..e).rev().find(|&i| !blank(i) && (prepared.origin[i].is_some() || prepared.items[i].ch == folio_compose::SOFT_HYPHEN)).and_then(glyph_of);
        (hang(first, true), hang(last, false))
    }

    pub(super) fn custom_mark(&self, font: FontId, mark: &str) -> Option<kenten::CustomGlyph> {
        let (glyph, advance) = self.table.glyph(font, mark.chars().next()?)?;
        Some((font, glyph, advance))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn line_metrics(
        &self,
        para: &Paragraph,
        shaped: &ShapedParagraph,
        prepared: &Prepared,
        s: usize,
        e: usize,
        last_line: bool,
        ret_attrs: Option<&CharacterAttrs>,
        ret_font: Option<FontId>,
        drop_chars: usize,
    ) -> LineMetrics {
        let mut m = LineMetrics::default();
        let mut any = false;
        for idx in s..e {
            let Some(ci) = prepared.origin[idx] else { continue };
            let cl = &shaped.clusters[ci];
            // A drop cap does not make its line taller.
            if cl.chars.end <= drop_chars {
                continue;
            }
            let attrs = &para.runs[cl.run].attrs;
            m.leading = m.leading.max(leading_of(attrs));
            // Superscript and Subscript set smaller glyphs higher or lower but the line keeps its
            // height: first baseline and leading are those of the unreduced size (measured:
            // text-format-char char-superscript, char-subscript: every baseline as without)
            let size = if matches!(attrs.position, folio_model::Position::Superscript | folio_model::Position::Subscript) { attrs.point_size.max(0.1) } else { cl.size };
            m.add_font(&self.table.metrics(cl.font), size);
            any = true;
            if cl.kind == ClusterKind::Object
                && let Some(item) = inline_item(para, cl.run, cl.chars.start)
            {
                let auto = matches!(attrs.leading, Leading::Auto).then_some((attrs.auto_leading, attrs.point_size));
                anchors::line_room(item, cl.rise, auto, &mut m);
            }
        }
        // The paragraph return belongs to the last line: its leading and its font's
        // heights count there (#347 p. 78: a 16 pt line ending in a 30 pt return sits
        // 10.6 pt lower, at the return's ascent).
        if last_line || !any {
            let fallback = CharacterAttrs::default();
            let attrs = ret_attrs.unwrap_or(&fallback);
            m.leading = m.leading.max(leading_of(attrs));
            let font = ret_font.unwrap_or(u32::MAX);
            m.add_font(&self.table.metrics(font), attrs.point_size);
        }
        m
    }
}

/// Positions (from the line start) of the items `s..` of a placed line in visual order: every
/// item keeps the room the composer gave it (its offset to the next item; the last its
/// width), and runs of items are reversed by bidi level (Unicode rule L2: from the highest
/// level down to the lowest odd one, every maximal run at or above the level). Trailing
/// blanks and tabs take the paragraph's level (rule L1); an inserted hyphen takes the
/// level of the item before it.
pub(super) fn visual_positions(prepared: &Prepared, shaped: &ShapedParagraph, s: usize, pl: &folio_compose::PlacedLine) -> Vec<f64> {
    let n = pl.x.len();
    let mut levels: Vec<u8> = Vec::with_capacity(n);
    for k in 0..n {
        let lv = match prepared.origin[s + k] {
            _ if prepared.items[s + k].ch == '\t' => 0,
            Some(ci) => shaped.level(shaped.clusters[ci].chars.start),
            None => levels.last().copied().unwrap_or(0),
        };
        levels.push(lv);
    }
    // Trailing blanks take the paragraph level (UAX #9 L1): after the text in a left-to-right
    // line, before it (hanging left of the line start) in a right-to-left one.
    let base = u8::from(shaped.rtl);
    let mut trailing = 0.0;
    for k in (0..n).rev() {
        if !folio_compose::is_trailing_blank(prepared.items[s + k].ch) {
            break;
        }
        levels[k] = base;
        trailing += if k + 1 < n { pl.x[k + 1] - pl.x[k] } else { pl.width[k] };
    }
    if levels.iter().all(|&l| l == 0) {
        return pl.x.clone();
    }
    let room = |k: usize| if k + 1 < n { pl.x[k + 1] - pl.x[k] } else { pl.width[k] };
    let mut order: Vec<usize> = (0..n).collect();
    // UAX #9 L2: from the highest level down to the lowest odd level at or above the line's
    // lowest (a line all at level 2, Latin in a right-to-left paragraph, stays as it is).
    let top = levels.iter().copied().max().unwrap_or(0);
    let min = levels.iter().copied().min().unwrap_or(0);
    let low = min | 1;
    for lv in (low..=top).rev() {
        let mut i = 0;
        while i < n {
            if levels[order[i]] < lv {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < n && levels[order[j]] >= lv {
                j += 1;
            }
            order[i..j].reverse();
            i = j;
        }
    }
    let mut xs = vec![0.0; n];
    let mut at = pl.x.first().copied().unwrap_or(0.0) - if shaped.rtl { trailing } else { 0.0 };
    for k in order {
        xs[k] = at;
        at += room(k);
    }
    xs
}

/// Moves a laid line's contents right by `dx` (its line box stays).
pub(super) fn shift_line(line: &mut LaidLine, dx: f64) {
    for c in &mut line.text_clusters { c.x0 += dx; c.x1 += dx; }
    for g in &mut line.glyphs {
        g.x += dx;
    }
    for d in &mut line.decorations {
        d.x0 += dx;
        d.x1 += dx;
    }
    for o in &mut line.objects {
        if !o.fixed_x {
            o.x += dx;
        }
    }
}

/// The anchored item of the U+FFFC at paragraph character `at` (in run `run`).
pub(super) fn inline_item(para: &Paragraph, run: usize, at: usize) -> Option<&PageItem> {
    let start: usize = para.runs[..run].iter().map(|r| r.text.chars().count()).sum();
    let k = para.runs[run].text.chars().take(at.checked_sub(start)?).filter(|&c| c == '\u{FFFC}').count();
    para.runs[run].inline.get(k)
}

pub(super) fn first_baseline(f: &TextFrame, m: &LineMetrics) -> f64 {
    let v = match f.first_baseline {
        FirstBaseline::Ascent => m.ascent.max(m.objects_above),
        FirstBaseline::CapHeight => m.cap,
        FirstBaseline::XHeight => m.x_height,
        FirstBaseline::Leading => m.leading,
        FirstBaseline::EmBoxHeight => m.embox,
        FirstBaseline::Fixed => f.min_first_baseline_offset,
    };
    v.max(f.min_first_baseline_offset)
}

/// Adds the underline and strike-through of one item, joining the previous span when it
/// continues it with the same rule.
pub(super) fn decorate(
    out: &mut Vec<Decoration>,
    a: &CharacterAttrs,
    m: &crate::fonts::FaceMetrics,
    size: f64,
    x0: f64,
    x1: f64,
    baseline: f64,
    vertical: bool,
) {
    let mut add = |kind: DecorationKind, y: f64, thickness: f64, paint: folio_model::Paint, overprint: bool, stroke_type: &str, gap: &folio_model::Paint, gap_overprint: bool| {
        if let Some(last) = out.iter_mut().rev().find(|d| d.kind == kind)
            && (last.x1 - x0).abs() < 0.01
            && (last.y - y).abs() < 1e-6
            && (last.thickness - thickness).abs() < 1e-6
            && last.paint == paint
            && last.overprint == overprint
            && last.stroke_type == stroke_type && &last.gap == gap && last.gap_overprint == gap_overprint
        {
            last.x1 = x1;
            return;
        }
        out.push(Decoration { kind, x0, x1, y, thickness, paint, overprint, stroke_type: stroke_type.into(), gap: gap.clone(), gap_overprint, shapes: Vec::new(), ..Default::default() });
    };
    if a.underline {
        let thickness = a.underline_weight.unwrap_or(m.underline_thickness * size);
        let y = match a.underline_offset {
            Some(off) => baseline + off,
            None => baseline - m.underline_position * size,
        };
        add(DecorationKind::Underline, y, thickness, a.underline_color.clone().unwrap_or_else(|| a.fill.clone()), a.underline_overprint, &a.underline_type, &a.underline_gap_color, a.underline_gap_overprint);
    }
    if a.strike_through {
        let thickness = a.strike_through_weight.unwrap_or(m.underline_thickness * size);
        // vertical text: through the em box centre (vtext-c2, InDesign 2025 PDF: 245.38 for an em box centred on 245.42,
        // where a third of the size above the Roman baseline would be 244.89; oracle vtext2/vt2-under)
        let y = baseline - a.strike_through_offset.unwrap_or(if vertical { (m.embox_top - 0.5) * size } else { size / 3.0 });
        add(DecorationKind::StrikeThrough, y, thickness, a.strike_through_color.clone().unwrap_or_else(|| a.fill.clone()), a.strike_through_overprint, &a.strike_through_type, &a.strike_through_gap_color, a.strike_through_gap_overprint);
    }
}

pub(super) fn hyphen_glyph(shaped: &ShapedParagraph, run: usize, attrs: &CharacterAttrs, x: f64, y: f64) -> PlacedGlyph {
    let h = shaped.hyphens[run];
    PlacedGlyph {
        font: h.font,
        glyph: h.glyph,
        x,
        y,
        size: h.size,
        hscale: attrs.horizontal_scale / 100.0,
        vscale: attrs.vertical_scale / 100.0,
        skew: attrs.skew,
            path_transform: None,
        fill: attrs.fill.clone(),
        stroke: attrs.stroke.clone(),
        stroke_weight: attrs.stroke_weight,
        overprint_fill: attrs.overprint_fill,
        overprint_stroke: attrs.overprint_stroke,
        fill_gradient: attrs.placed_gradients().0,
        stroke_gradient: attrs.placed_gradients().1,
    }
}
