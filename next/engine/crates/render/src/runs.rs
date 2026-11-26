//! Paint passes for composed character ranges, shared by screen, separations and PDF.
use folio_model::{Paint, layout::PlacedGlyph};
use std::ops::Range;

pub fn stroke_width(g: &PlacedGlyph) -> f64 {
    if g.stroke == Paint::None { 0.0 } else { g.stroke_weight * if g.fill == Paint::None { 1.0 } else { 2.0 } }
}

/// A stroked range paints all its strokes, then all its fills.
pub fn paint_passes(glyphs: &[PlacedGlyph]) -> Vec<(Range<usize>, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let a = &glyphs[start];
        let stroked = a.stroke != Paint::None;
        let mut end = start + 1;
        while end < glyphs.len() {
            let b = &glyphs[end];
            if (b.stroke != Paint::None) != stroked || (stroked &&
                (a.fill != b.fill || a.stroke != b.stroke || a.stroke_weight != b.stroke_weight ||
                 a.overprint_fill != b.overprint_fill || a.overprint_stroke != b.overprint_stroke)) { break; }
            end += 1;
        }
        if stroked { out.push((start..end, true)); }
        out.push((start..end, false));
        start = end;
    }
    out
}
