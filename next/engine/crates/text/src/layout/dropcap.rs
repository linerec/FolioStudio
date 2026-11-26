//! Drop caps: Paragraph panel "Drop Cap Number of Lines" / "One or More Characters"
//! (next/oracle/rules/dropcap.ko.md). The first characters are set at a size that puts their
//! cap height from the cap line of the first text line down to the baseline of the last drop
//! cap line; the lines beside them are indented by the drop cap's advance.
use super::*;

/// A drop cap of a paragraph, as `place_paragraph` sets it.
#[derive(Clone, Copy, Debug)]
pub(super) struct DropCap {
    /// Lines the drop cap spans (at least 1).
    pub lines: usize,
    /// Characters of the paragraph set as the drop cap.
    pub chars: usize,
    /// How far Scale for Descenders lifts the drop cap's baseline above the last drop line's (pt).
    pub raise: f64,
    /// Align Left Edge: the drop cap and the text beside it move left by the first glyph's left side bearing.
    pub align_left_edge: bool,
}

impl<'c, 'a> Ctx<'c, 'a> {
    /// The paragraph with its first `drop_cap_characters` characters at the drop cap's size,
    /// and the drop cap. `None` when the paragraph has none (or its first characters are
    /// objects, tables or variables, which InDesign does not enlarge as text).
    pub(super) fn with_drop_cap(&self, para: &Paragraph, missing: &mut BTreeSet<(String, String)>) -> Option<(Paragraph, DropCap)> {
        let a = &para.attrs;
        let (lines, want) = (a.drop_cap_lines as usize, a.drop_cap_characters as usize);
        // One line is no drop cap: InDesign sets such a paragraph as it is (dropcap-1-3).
        if lines < 2 || want == 0 {
            return None;
        }
        let total: usize = para.runs.iter().map(|r| r.text.chars().count()).sum();
        let chars = want.min(total);
        if chars == 0 {
            return None;
        }
        let mut head = slice_paragraph(para, 0, chars, false);
        let tail = slice_paragraph(para, chars, total, true);
        if head.runs.iter().any(|r| r.variable.is_some() || !r.tables.is_empty() || r.note.is_some() || !r.inline.is_empty()) {
            return None;
        }
        let head_fonts = self.run_fonts(&head, missing);
        let tail_fonts = self.run_fonts(&tail, missing);
        // The text beside the drop cap: its first run with text sets the size and leading the
        // drop cap is measured against.
        let (base_attrs, base_font) = match tail.runs.iter().position(|r| !r.text.is_empty()) {
            Some(k) => (&tail.runs[k].attrs, tail_fonts[k].font),
            None => (&head.runs.last()?.attrs, head_fonts.last()?.font),
        };
        // The height of the capital 'H' (Arial: 1466 units; its OS/2 capital height says 1467, which
        // sizes a 3-line drop cap 0.03 pt too small: measured dropcap-3-1, dropcap-4-1).
        let cap_of = |font| self.table.glyph_top(font, 'H').filter(|&h| h > 0.0).unwrap_or_else(|| self.table.metrics(font).cap_height);
        let head_font = head_fonts.first()?.font;
        let cap = cap_of(head_font);
        if cap <= 0.0 {
            return None;
        }
        // The drop cap's own size (its character style's, else the text's) is magnified so that its cap height
        // reaches from the first line's cap line to the last drop line's baseline: size = z * (1 + (N-1) * leading /
        // (cap * text size)), i.e. (N-1) * leading / cap + size for z = size (measured with Times and Verdana
        // drop cap styles and a 30 pt style size: rules/dropcap.ko.md).
        let z = head.runs.iter().find(|r| !r.text.is_empty()).map_or(base_attrs.point_size, |r| r.attrs.point_size);
        let height_below = (lines - 1) as f64 * leading_of(base_attrs);
        let mut size = z * (1.0 + height_below / (cap * base_attrs.point_size));
        let mut raise = 0.0;
        if a.drop_cap_detail & 2 != 0 {
            // Scale for Descenders: when a drop cap glyph reaches lower than the text's own descenders
            // (OS/2 typographic descender x text size) below the baseline, the drop cap shrinks so that
            // its lowest point sits there, and its baseline rises by what is left of the cap-height span.
            let depth = head.runs.iter().flat_map(|r| r.text.chars()).filter_map(|c| self.table.glyph_bottom(head_font, c)).fold(0.0f64, |m, b| m.max(-b));
            let span = cap * size;
            // The text's descent: OS/2 sTypoDescender for Roman fonts; for a font with Hangul the bottom of 'p' (Malgun Gothic:
            // 0.2363 em, not its typographic descender 0.2002: measured, dropcap-korean-desc; like "Ascent", which is the top of 'd').
            let descent = if self.table.covers(base_font, '\u{AC00}') && !self.table.is_synthetic(base_font) {
                self.table.glyph_bottom(base_font, 'p').unwrap_or(self.table.metrics(base_font).typo_descender)
            } else {
                self.table.metrics(base_font).typo_descender
            };
            let room = -descent.min(0.0) * base_attrs.point_size;
            if depth * size > room + 1e-9 {
                size = (span + room) / (cap + depth);
                raise = span - cap * size;
            }
        }
        for r in &mut head.runs {
            r.attrs.point_size = size;
        }
        let mut out = Paragraph { style: para.style.clone(), attrs: para.attrs.clone(), runs: head.runs };
        out.runs.extend(tail.runs);
        Some((out, DropCap { lines, chars, raise, align_left_edge: a.drop_cap_detail & 1 != 0 }))
    }
}
