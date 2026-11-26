//! Stroke types of text decorations (paragraph rules and borders, table strokes, the
//! footnote separator): what InDesign paints for a dashed, dotted, striped, hashed, wavy or
//! diamond line, as [`TextShape`]s shared by screen and print.
//!
//! The patterns come from `folio_model::geom` (the page-item stroke lowering). Measured with
//! InDesign 2025 (artifacts/next-20261018/fonts/indesign/text-strokes-x4.pdf):
//! - A paragraph rule and the footnote separator are open lines: `geom::stroke_art` of the
//!   two-point line ("Dashed" starts and ends on a full dash: a 340 pt rule 4 pt "Dashed"
//!   has dash = gap = 11.724; a 72 pt separator 14.4).
//! - Paragraph border edges and table edges run between corners: the pattern is fitted to
//!   each edge and centred on its ends, so a dash or dot sits on every corner (border
//!   248 pt: 10 periods of 24.8; 28 pt: one period, half dashes at the corners; table edge
//!   100 pt: 12.5 / 12.5; dots every 248/28 = 8.857 pt).
//! - Dots on a table's edges differ from dashes (alternating strokes, oracle tables-alternating): a horizontal edge has a dot on every
//!   crossing and starts half a dot in from the table's outer edge; a vertical edge keeps its dots inside it (`Ends::Free`).
//! - Stripes ("Thick - Thin") are measured from the left of the line's direction; a
//!   border's box runs clockwise, so its first stripe is outside.
//!
//! Gap colours: a table stroke carries its gap colour (`CellStroke::gap`, [Paper] unless set: a white gap shows over a cell's fill) and
//! tables.rs paints it under the pattern; a paragraph rule or border carries its gap colour too (`gap_paint`: structures.rs paints a solid bar or stroke of the same
//! extent under the pattern; InDesign 2025 reads the same IDML attributes, `ParagraphBorderGapColor`/`GapTint`/`GapOverprint`, `RuleAboveGap…`).

use folio_model::geom::{self, CornerAdjust, StrokePattern, StrokeSpec};
use folio_model::layout::TextShape;
use folio_model::{LineCap, Paint, PathPoint, Point, Rect, SubPath};

/// How a straight decoration line ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ends {
    /// An open line (paragraph rule, footnote separator).
    Free,
    /// An open character line that fits complete waves.
    WholeWaves,
    /// An edge between corners (paragraph border, table cell edge).
    Corners,
}

fn pt(x: f64, y: f64) -> PathPoint {
    let a = Point::new(x, y);
    PathPoint { anchor: a, left: a, right: a }
}

/// The pattern of a decoration's stroke type for weight `w`; `None` when it paints a solid
/// line (solid, or a document's own style the model does not describe).
pub(super) fn pattern(kind: &str, w: f64) -> Option<StrokePattern> {
    StrokePattern::builtin(kind, w, None, CornerAdjust::DashesAndGaps).filter(|p| *p != StrokePattern::Solid)
}

/// The shapes of a straight stroke from `a` to `b`, `w` wide, of stroke type `kind`; `None`
/// when the type paints a solid line (the caller's rectangle).
pub(super) fn line_shapes(a: (f64, f64), b: (f64, f64), w: f64, kind: &str, ends: Ends, paint: &Paint, overprint: bool) -> Option<Vec<TextShape>> {
    pattern_line_shapes(a, b, w, pattern(kind, w)?, ends, paint, overprint)
}

/// [`line_shapes`] of an already resolved pattern (a document's own stroke style).
pub(super) fn pattern_line_shapes(a: (f64, f64), b: (f64, f64), w: f64, pattern: StrokePattern, ends: Ends, paint: &Paint, overprint: bool) -> Option<Vec<TextShape>> {
    if w <= 0.0 || *paint == Paint::None {
        return Some(Vec::new());
    }
    let paths = match (&pattern, ends) {
        (StrokePattern::Dashed { pattern, .. }, Ends::Corners) => centred_dashes(a, b, w, pattern),
        (StrokePattern::Dotted { gaps, .. }, Ends::Corners) => centred_dots(a, b, w, gaps),
        _ => {
            // A dotted rule keeps its end dots inside the rule (4 pt dots from 60 to 400 pt
            // are centred from 62 to 398).
            let (a, b) = match &pattern {
                StrokePattern::Dotted { .. } if dist(a, b) > w => (lerp(a, b, w / 2.0 / dist(a, b)), lerp(b, a, w / 2.0 / dist(a, b))),
                _ => (a, b),
            };
            let path = SubPath { points: vec![pt(a.0, a.1), pt(b.0, b.1)], open: true };
            return Some(art_shapes(&[path], w, pattern, paint, overprint, ends == Ends::WholeWaves));
        }
    };
    Some(vec![TextShape { paths, paint: paint.clone(), overprint, ..Default::default() }])
}

/// The shapes of a rectangle's stroke (a paragraph border with four equal edges), its
/// corners turned like a frame's: `geom::stroke_art` of the closed path.
pub(super) fn box_shapes(r: Rect, w: f64, kind: &str, paint: &Paint, overprint: bool) -> Option<Vec<TextShape>> {
    let pattern = pattern(kind, w)?;
    if w <= 0.0 || *paint == Paint::None {
        return Some(Vec::new());
    }
    let path = SubPath { points: vec![pt(r.left, r.top), pt(r.right, r.top), pt(r.right, r.bottom), pt(r.left, r.bottom)], open: false };
    Some(art_shapes(&[path], w, pattern, paint, overprint, false))
}

/// A solid border of four equal edges: one closed path stroked with mitered corners, the way
/// InDesign paints it (a stroke, not four filled bars: the two rasterise differently).
pub(super) fn solid_box(r: Rect, w: f64, paint: &Paint, overprint: bool) -> Option<Vec<TextShape>> {
    if w <= 0.0 || *paint == Paint::None {
        return None;
    }
    let path = SubPath { points: vec![pt(r.left, r.top), pt(r.right, r.top), pt(r.right, r.bottom), pt(r.left, r.bottom)], open: false };
    let shape = geom::PartShape::Line { width: w, cap: LineCap::Butt, join: folio_model::LineJoin::Miter, miter_limit: 4.0, dash: None };
    Some(vec![TextShape { paths: vec![path], shape, paint: paint.clone(), overprint }])
}

/// A solid border of four equal edges where the paragraph runs on (`top` or `bottom` edge
/// not drawn): the drawn edges as stroked open paths, mitered at the corners, as InDesign paints them.
pub(super) fn solid_edges(r: Rect, w: f64, top: bool, bottom: bool, kind: &str, paint: &Paint, overprint: bool) -> Option<Vec<TextShape>> {
    if w <= 0.0 || *paint == Paint::None || pattern(kind, w).is_some() {
        return None;
    }
    let open = |pts: &[(f64, f64)]| SubPath { points: pts.iter().map(|p| pt(p.0, p.1)).collect(), open: true };
    let paths = match (top, bottom) {
        (true, false) => vec![open(&[(r.left, r.bottom), (r.left, r.top), (r.right, r.top), (r.right, r.bottom)])],
        (false, true) => vec![open(&[(r.left, r.top), (r.left, r.bottom), (r.right, r.bottom), (r.right, r.top)])],
        (false, false) => vec![open(&[(r.left, r.top), (r.left, r.bottom)]), open(&[(r.right, r.top), (r.right, r.bottom)])],
        (true, true) => return None,
    };
    let shape = geom::PartShape::Line { width: w, cap: LineCap::Butt, join: folio_model::LineJoin::Miter, miter_limit: 4.0, dash: None };
    Some(vec![TextShape { paths, shape, paint: paint.clone(), overprint }])
}

/// `geom::stroke_art` of `paths`; character waves retain their cubic strokes.
fn art_shapes(paths: &[SubPath], w: f64, pattern: StrokePattern, paint: &Paint, overprint: bool, whole_waves: bool) -> Vec<TextShape> {
    let keep_stroke = whole_waves && pattern == StrokePattern::Wavy;
    let spec = StrokeSpec { weight: w, miter_limit: 4.0, pattern, whole_waves, ..StrokeSpec::default() };
    let Some(art) = geom::stroke_art(paths, &spec) else {
        return Vec::new();
    };
    art.parts
        .iter()
        .filter(|p| p.ink == geom::StrokeInk::Stroke)
        .map(|part| {
            if keep_stroke {
                return TextShape { paths: part.paths.clone(), shape: part.shape.clone(), paint: paint.clone(), overprint };
            }
            let paths = match &part.shape {
                geom::PartShape::Fill => part.paths.clone(),
                geom::PartShape::Line { width, cap, dash, .. } => {
                    part.paths.iter().flat_map(|sp| stroke_outline(sp, *width, *cap, dash.as_ref())).collect()
                }
            };
            TextShape { paths, paint: paint.clone(), overprint, ..Default::default() }
        })
        .filter(|s| !s.paths.is_empty())
        .collect()
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// A dash pattern fitted to a line between corners: whole periods, each element scaled
/// alike, the line starting and ending in the middle of a dash.
fn centred_dashes(a: (f64, f64), b: (f64, f64), w: f64, pattern: &[f64]) -> Vec<SubPath> {
    let len = dist(a, b);
    let period: f64 = pattern.iter().sum();
    if len <= 0.0 || period <= 0.0 {
        return Vec::new();
    }
    let k = (len / period).round().max(1.0);
    let s = len / (k * period);
    let array: Vec<f64> = pattern.iter().map(|v| v * s).collect();
    let phase = array[0] / 2.0;
    let dash = geom::Dash { array, phase };
    stroke_outline(&SubPath { points: vec![pt(a.0, a.1), pt(b.0, b.1)], open: true }, w, LineCap::Butt, Some(&dash))
}

/// Round dots of the stroke weight on both corners and evenly between them.
fn centred_dots(a: (f64, f64), b: (f64, f64), w: f64, gaps: &[f64]) -> Vec<SubPath> {
    let len = dist(a, b);
    let gap = gaps.first().copied().unwrap_or(w).max(1e-6);
    let k = (len / gap).round().max(1.0) as usize;
    (0..=k).map(|i| circle(lerp(a, b, i as f64 / k as f64), w / 2.0)).collect()
}

/// A circle as four Bézier quarters, wound like `quad`'s outlines (so they add up).
fn circle(c: (f64, f64), r: f64) -> SubPath {
    let k = geom::KAPPA * r;
    let p = |x: f64, y: f64, lx: f64, ly: f64, rx: f64, ry: f64| PathPoint {
        anchor: Point::new(x, y),
        left: Point::new(x + lx, y + ly),
        right: Point::new(x + rx, y + ry),
    };
    let (x, y) = c;
    SubPath {
        points: vec![
            p(x + r, y, 0.0, k, 0.0, -k),
            p(x, y - r, k, 0.0, -k, 0.0),
            p(x - r, y, 0.0, -k, 0.0, k),
            p(x, y + r, -k, 0.0, k, 0.0),
        ],
        open: false,
    }
}

/// The outline of segment `a`–`b` stroked `w` wide with butt ends (wound like `circle`).
fn quad(a: (f64, f64), b: (f64, f64), w: f64) -> Option<SubPath> {
    let len = dist(a, b);
    if len <= 0.0 {
        return None;
    }
    let (nx, ny) = (-(b.1 - a.1) / len * w / 2.0, (b.0 - a.0) / len * w / 2.0);
    Some(SubPath {
        points: vec![pt(a.0 + nx, a.1 + ny), pt(b.0 + nx, b.1 + ny), pt(b.0 - nx, b.1 - ny), pt(a.0 - nx, a.1 - ny)],
        open: false,
    })
}

/// A subpath as a polyline (curves in 16 straight pieces each).
fn polyline(sp: &SubPath) -> Vec<(f64, f64)> {
    let segs = geom::segments(sp);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for s in &segs {
        if out.is_empty() {
            out.push((s.a.x, s.a.y));
        }
        if s.is_line() {
            out.push((s.d.x, s.d.y));
        } else {
            for i in 1..=16 {
                let p = s.eval(i as f64 / 16.0);
                out.push((p.x, p.y));
            }
        }
    }
    out
}

/// The outline of a stroked subpath: `w` wide, `cap` at the ends of every dash (PDF dash
/// semantics; a zero-length dash with round caps is a dot). Joins are left as the pieces
/// meet (rules and edges are straight; a wavy line is cut fine enough).
fn stroke_outline(sp: &SubPath, w: f64, cap: LineCap, dash: Option<&geom::Dash>) -> Vec<SubPath> {
    let line = polyline(sp);
    if line.len() < 2 {
        return Vec::new();
    }
    let pieces: Vec<Vec<(f64, f64)>> = match dash.filter(|d| d.array.iter().sum::<f64>() > 0.0) {
        None => vec![line],
        Some(d) => dash_pieces(&line, d),
    };
    let mut out = Vec::new();
    for piece in pieces {
        let len: f64 = piece.windows(2).map(|p| dist(p[0], p[1])).sum();
        let (first, last) = (piece[0], piece[piece.len() - 1]);
        if len <= 1e-9 {
            if cap != LineCap::Butt {
                out.push(if cap == LineCap::Round { circle(first, w / 2.0) } else { square(first, w) });
            }
            continue;
        }
        let mut pts = piece.clone();
        if cap == LineCap::Projecting {
            let n = pts.len();
            let d0 = dist(pts[0], pts[1]);
            pts[0] = lerp(pts[1], pts[0], (d0 + w / 2.0) / d0);
            let d1 = dist(pts[n - 2], pts[n - 1]);
            pts[n - 1] = lerp(pts[n - 2], pts[n - 1], (d1 + w / 2.0) / d1);
        }
        out.extend(pts.windows(2).filter_map(|p| quad(p[0], p[1], w)));
        if cap == LineCap::Round {
            out.push(circle(first, w / 2.0));
            out.push(circle(last, w / 2.0));
        }
    }
    out
}

fn square(c: (f64, f64), w: f64) -> SubPath {
    let h = w / 2.0;
    SubPath { points: vec![pt(c.0 - h, c.1 + h), pt(c.0 + h, c.1 + h), pt(c.0 + h, c.1 - h), pt(c.0 - h, c.1 - h)], open: false }
}

/// The "on" pieces of a polyline under a dash pattern (PDF semantics: lengths on, off, …
/// cycled, starting `phase` into the pattern).
fn dash_pieces(line: &[(f64, f64)], d: &geom::Dash) -> Vec<Vec<(f64, f64)>> {
    let arr: Vec<f64> = if d.array.len() % 2 == 1 { d.array.iter().chain(d.array.iter()).copied().collect() } else { d.array.clone() };
    let period: f64 = arr.iter().sum();
    let mut i = 0usize;
    let mut left = arr[0];
    let mut phase = d.phase.rem_euclid(period);
    while phase > 0.0 {
        if phase >= left {
            phase -= left;
            i = (i + 1) % arr.len();
            left = arr[i];
        } else {
            left -= phase;
            phase = 0.0;
        }
    }
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut cur: Option<Vec<(f64, f64)>> = i.is_multiple_of(2).then(|| vec![line[0]]);
    for seg in line.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let len = dist(a, b);
        let mut at = 0.0;
        while len - at >= left {
            at += left;
            let p = lerp(a, b, at / len);
            match cur.take() {
                Some(mut c) => {
                    c.push(p);
                    out.push(c);
                }
                None => cur = Some(vec![p]),
            }
            i = (i + 1) % arr.len();
            left = arr[i];
            if left <= 0.0 && i.is_multiple_of(2) {
                // A zero-length dash: a dot.
                out.push(vec![p]);
                i = (i + 1) % arr.len();
                left = arr[i];
                cur = None;
            }
        }
        left -= len - at;
        if let Some(c) = cur.as_mut() {
            c.push(b);
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(paths: &[SubPath]) -> Vec<(f64, f64)> {
        paths
            .iter()
            .map(|p| {
                let b = p.bounds().unwrap();
                ((b.left * 1000.0).round() / 1000.0, (b.right * 1000.0).round() / 1000.0)
            })
            .collect()
    }

    #[test]
    fn dashes_between_corners_are_centred_like_indesign() {
        // Table edge 100 pt, "Dashed" 2 pt: 4 periods of 25, half dashes at the corners.
        let p = centred_dashes((0.0, 0.0), (100.0, 0.0), 2.0, &[12.0, 12.0]);
        assert_eq!(xs(&p), vec![(0.0, 6.25), (18.75, 31.25), (43.75, 56.25), (68.75, 81.25), (93.75, 100.0)]);
        // Border edge 28 pt: one period, a gap of 14 in the middle.
        assert_eq!(xs(&centred_dashes((0.0, 0.0), (28.0, 0.0), 3.0, &[12.0, 12.0])), vec![(0.0, 7.0), (21.0, 28.0)]);
        // Dotted 3 pt over 248 pt: 29 dots 8.857 apart.
        let dots = centred_dots((0.0, 0.0), (248.0, 0.0), 3.0, &[9.0]);
        assert_eq!(dots.len(), 29);
        assert!((dots[1].bounds().unwrap().left - (248.0 / 28.0 - 1.5)).abs() < 1e-9);
    }

    #[test]
    fn solid_and_unknown_styles_stay_the_callers_rectangle() {
        let paint = Paint::Swatch { id: "Color/Black".into(), tint: 100.0 };
        for kind in ["Solid", "", "Atlas dots"] {
            assert!(line_shapes((10.0, 5.0), (50.0, 5.0), 2.0, kind, Ends::Free, &paint, false).is_none());
        }
        // Dashed rule: dashes, the first starting at the line's start.
        let s = line_shapes((0.0, 0.0), (340.0, 0.0), 4.0, "Dashed", Ends::Free, &paint, false).unwrap();
        let b = s[0].paths[0].bounds().unwrap();
        assert!(b.left.abs() < 1e-6 && (b.right - 11.724).abs() < 0.01, "{b:?}");
    }

    #[test]
    fn character_waves_keep_cubic_strokes() {
        let paint = Paint::Swatch { id: "Color/Black".into(), tint: 100.0 };
        let shapes = line_shapes((0.0, 0.0), (123.481, 0.0), 10.0, "Wavy", Ends::WholeWaves, &paint, true).unwrap();
        assert_eq!(shapes.len(), 1);
        let shape = &shapes[0];
        assert!(matches!(shape.shape, geom::PartShape::Line { width, cap: LineCap::Butt, dash: None, .. } if (width - 2.65).abs() < 1e-9));
        assert!(shape.overprint);
        assert_eq!(shape.paths.len(), 1);
        let segments = geom::segments(&shape.paths[0]);
        assert_eq!(segments.len(), 6);
        assert!(segments.iter().all(|segment| !segment.is_line()));
        assert!((segments[0].b.y + 4.888).abs() < 0.002);
        assert!((segments[5].d.x - 123.481).abs() < 1e-9);
    }

    #[test]
    fn dash_pieces_follow_pdf_semantics() {
        let line = [(0.0, 0.0), (10.0, 0.0)];
        let d = geom::Dash { array: vec![2.0, 3.0], phase: 1.0 };
        let p = dash_pieces(&line, &d);
        let spans: Vec<(f64, f64)> = p.iter().map(|v| (v[0].0, v[v.len() - 1].0)).collect();
        assert_eq!(spans, vec![(0.0, 1.0), (4.0, 6.0), (9.0, 10.0)]);
        // Zero-length dashes: dots.
        let d = geom::Dash { array: vec![0.0, 5.0], phase: 0.0 };
        assert_eq!(dash_pieces(&line, &d).len(), 3);
    }
}
