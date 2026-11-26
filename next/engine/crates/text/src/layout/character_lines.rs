use folio_model::geom::{CornerAdjust, StrokePattern};
use folio_model::layout::{Decoration, TextShape};
use folio_model::{Document, LineCap};
use super::strokes;

/// Paints each underline / strike-through span: a solid bar, or the pattern of its stroke type
/// over the gap colour. A built-in type is named (`Wavy`); a document's own dashed, dotted or
/// striped style is its `Self` id (`DashedStrokeStyle/House dash`) and is read from the document.
pub(super) fn finish(doc: &Document, lines: &mut [Decoration]) {
    for d in lines {
        let w = if d.thickness < 0. { 1. } else { d.thickness };
        let width = d.x1 - d.x0;
        if w <= 0. { continue; }
        let own = doc.stroke_styles.iter().find(|s| s.id == d.stroke_type).and_then(|s| s.pattern());
        let pattern = match own {
            Some(p) => own_style_shapes(p, width, w, d),
            None => strokes::line_shapes((0., 0.), (width, 0.), w, &d.stroke_type, strokes::Ends::WholeWaves, &d.paint, d.overprint),
        };
        if let Some(pattern) = pattern {
            d.shapes.extend(TextShape::rect(0., -w / 2., width, w / 2., d.gap.clone(), d.gap_overprint));
            d.shapes.extend(pattern);
        } else {
            d.shapes.extend(TextShape::rect(0., -w / 2., width, w / 2., d.paint.clone(), d.overprint));
        }
    }
}

/// A document's own stroke style on a character line, as InDesign 2025 paints it (measured with
/// text-format/charlines2-custom-*; page items fit the same styles differently):
/// - a dashed line starts and ends on a whole dash, the pattern's lengths adjusted by the style's
///   corner setting (`Dashed` 8 4 on 123.481 pt: "dashes and gaps" shortens both by 0.2152 to
///   7.785 / 3.785, "gaps" keeps the dash and makes the gap 3.548); with round caps the line is
///   first drawn half the weight in from both ends (dashes 8.183 / 4.183 from 43 to 162.481 at 4 pt);
/// - a dotted line keeps its end dots inside the line (half the weight in) and fits whole periods:
///   "dashes and gaps" lengthens all four elements of `6 12` alike (6 periods, dots 0.478 long at
///   4 pt, since a dot cannot shrink), "gaps" only the gaps (7 periods: 5.534 / 11.534, dots of no length);
/// - a striped line is the stripes across the weight, as for page items.
///   (InDesign's scripting sets "None" and "Dashes" as "Gaps", so those two are not measured; they
///   keep the page-item lowering.)
fn own_style_shapes(pattern: StrokePattern, width: f64, w: f64, d: &Decoration) -> Option<Vec<TextShape>> {
    let (mut a, mut b) = ((0., 0.), (width, 0.));
    let mut inset = |round: bool| if round && width > w { (a, b) = ((w / 2., 0.), (width - w / 2., 0.)); };
    let pattern = match pattern {
        StrokePattern::Dashed { pattern, adjust, cap, .. } => {
            inset(cap == Some(LineCap::Round));
            StrokePattern::Dashed { pattern, adjust, cap, builtin: true }
        }
        StrokePattern::Dotted { gaps, adjust: adjust @ (CornerAdjust::DashesAndGaps | CornerAdjust::Gaps), .. } => {
            inset(true);
            let pattern = gaps.iter().flat_map(|g| [0., g.max(0.)]).collect();
            // the count of periods rounds (half down) unless a dot would have to shrink
            StrokePattern::Dashed { pattern, adjust, cap: Some(LineCap::Round), builtin: adjust == CornerAdjust::Gaps }
        }
        other => other,
    };
    strokes::pattern_line_shapes(a, b, w, pattern, strokes::Ends::Free, &d.paint, d.overprint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::Paint;
    use folio_model::stroke_styles::Definition;

    fn style(kind: &str, values: &[f64]) -> Definition {
        Definition { id: format!("{kind}/Own"), name: "Own".into(), kind: kind.into(), values: values.to_vec(), adjustment: "DashesAndGaps".into(), cap: "ButtEndCap".into() }
    }

    fn with(def: Definition, adjustment: &str, cap: &str) -> Definition {
        Definition { adjustment: adjustment.into(), cap: cap.into(), ..def }
    }

    /// The pieces (left, right) of the painted line, in order, for a style of the document on a
    /// 123.481 pt span (text-format/charlines2-custom-*: InDesign 2025).
    fn pieces(def: Definition, w: f64) -> Vec<(f64, f64)> {
        let doc = Document { stroke_styles: vec![def.clone()], ..Document::default() };
        let mut lines = vec![Decoration { x1: 123.481, thickness: w, paint: Paint::Swatch { id: "Color/Black".into(), tint: 100.0 }, stroke_type: def.id, ..Decoration::default() }];
        finish(&doc, &mut lines);
        let mut out: Vec<(f64, f64)> = lines[0].shapes.iter().flat_map(|s| s.paths.iter().filter_map(|p| p.bounds()).map(|b| (b.left, b.right))).collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    #[test]
    fn own_dashed_style_starts_and_ends_on_a_whole_dash() {
        // 8 4 over 123.481 pt: ten periods of 7.785 + 3.785 and a last dash, at 2 pt and 4 pt alike.
        for w in [2.0, 4.0] {
            let p = pieces(style("DashedStrokeStyle", &[8.0, 4.0]), w);
            assert_eq!(p.len(), 11, "{p:?}");
            assert!(p[0].0.abs() < 1e-3 && (p[0].1 - 7.785).abs() < 2e-3, "{:?}", p[0]);
            assert!((p[1].0 - 11.57).abs() < 2e-3, "{:?}", p[1]);
            assert!((p[10].0 - 115.696).abs() < 3e-3 && (p[10].1 - 123.481).abs() < 1e-3, "{:?}", p[10]);
        }
    }

    /// `pieces` with the overlapping ones merged (a round-capped dash is a bar and two circles).
    fn merged(p: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::new();
        for (l, r) in p {
            match out.last_mut() {
                Some(last) if l <= last.1 => last.1 = last.1.max(r),
                _ => out.push((l, r)),
            }
        }
        out
    }

    #[test]
    fn own_dotted_style_keeps_end_dots_inside_and_lengthens_every_dot() {
        // 6 12 at 4 pt: dots centred at 2.0, 8.957, 21.913, ... 121.481 (13 of them); the dots
        // between the ends are 0.478 long (InDesign adds the fitting delta to dashes and gaps alike).
        let p = merged(pieces(style("DottedStrokeStyle", &[6.0, 12.0]), 4.0));
        assert_eq!(p.len(), 13, "{p:?}");
        assert!(p[0].0.abs() < 1e-3 && p[0].1 < 4.3, "{:?}", p[0]);
        assert!((p[12].1 - 123.481).abs() < 1e-3 && p[12].0 > 119.0, "{:?}", p[12]);
        let centre = |i: usize| (p[i].0 + p[i].1) / 2.0;
        assert!((centre(1) - 8.957).abs() < 0.01 && (centre(2) - 21.913).abs() < 0.01 && (centre(11) - 108.524).abs() < 0.01);
        assert!((p[1].1 - p[1].0 - 4.478).abs() < 0.01, "{:?}", p[1]);
    }

    #[test]
    fn own_styles_adjusting_only_gaps_or_with_round_caps() {
        // "Gaps": the dash stays 8, the gap becomes 3.548 (ten periods and a last dash).
        let p = pieces(with(style("DashedStrokeStyle", &[8.0, 4.0]), "Gaps", "ButtEndCap"), 4.0);
        assert_eq!(p.len(), 11, "{p:?}");
        assert!((p[0].1 - 8.0).abs() < 1e-3 && (p[1].0 - 11.548).abs() < 2e-3 && (p[10].1 - 123.481).abs() < 1e-3, "{p:?}");
        // Round caps: the line is drawn 2 pt (half the weight) in from both ends, dashes 8.183 / 4.183 plus the caps.
        let p = merged(pieces(with(style("DashedStrokeStyle", &[8.0, 4.0]), "DashesAndGaps", "RoundEndCap"), 4.0));
        assert_eq!(p.len(), 10, "{p:?}");
        assert!(p[0].0.abs() < 1e-3 && (p[0].1 - (2.0 + 8.183 + 2.0)).abs() < 5e-3 && (p[9].1 - 123.481).abs() < 1e-3, "{p:?}");
        // Dotted "Gaps": 7 periods, gaps 5.534 / 11.534, dots of no length (15 dots).
        let p = merged(pieces(with(style("DottedStrokeStyle", &[6.0, 12.0]), "Gaps", "ButtEndCap"), 4.0));
        assert_eq!(p.len(), 15, "{p:?}");
        let centre = |i: usize| (p[i].0 + p[i].1) / 2.0;
        assert!((centre(1) - (2.0 + 5.534)).abs() < 0.01 && (centre(2) - (2.0 + 17.068)).abs() < 0.01, "{p:?}");
        assert!((p[1].1 - p[1].0 - 4.0).abs() < 0.01, "{:?}", p[1]);
    }

    #[test]
    fn own_striped_style_is_stripes_across_the_weight() {
        // 0 20 50 100 at 4 pt: 0.8 pt and 2 pt bars, the first 1.6 pt above the line's centre.
        let p = pieces(style("StripedStrokeStyle", &[0.0, 20.0, 50.0, 100.0]), 4.0);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p.iter().all(|(l, r)| l.abs() < 1e-3 && (r - 123.481).abs() < 1e-3));
    }
}
