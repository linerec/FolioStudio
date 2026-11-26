//! Path geometry InDesign paints on top of a path's points, lowered to plain paths so that
//! every consumer (screen, PDF, preflight) draws the same shape: corner options, stroke styles
//! (dashes and dots fitted between corners, stripes, hashes, waves, diamonds, gap colour) and
//! arrowheads.
//!
//! The rules and constants were measured from InDesign 2025's own PDF/X-4 output of specimen
//! documents (artifacts/next-20261018/graphics/indesign): which anchors are corners, how
//! dash patterns stretch to fit, arrowhead sizes per stroke weight. Nothing here is copied
//! from Adobe data; numbers are fitted to the exported geometry.

mod corners;
mod curve;
pub mod handles;
mod offset;
mod parallel;
pub(crate) mod path;
mod point;
mod rect;
mod stroke;
mod transform;

use crate::{LineCap, LineJoin, Point, StrokeAlignment, SubPath};
use serde::{Deserialize, Serialize};

pub use corners::apply_corners;
pub use curve::KAPPA;
pub use offset::closed_contours;
pub use path::segments;
pub use stroke::stroke_art;

pub(crate) use offset::{offset_curve, tangent_intersection, trim_corner};
pub(crate) use point::{add, cross, dist, left, len, mul, sub};

use curve::Measured;
use point::unit;

// ---------------------------------------------------------------------------------------
// Model of what is lowered

/// IDML corner option of one corner (`TopLeftCornerOption` + `TopLeftCornerRadius`, …).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CornerKind {
    #[default]
    None,
    Rounded,
    InverseRounded,
    Inset,
    Bevel,
    Fancy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CornerOption {
    pub kind: CornerKind,
    /// Size along each edge (points).
    pub radius: f64,
}

impl CornerOption {
    pub fn active(&self) -> bool {
        self.kind != CornerKind::None && self.radius > 1e-9
    }
}

/// How dash and gap lengths stretch so a pattern ends on corners (`StrokeCornerAdjustment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CornerAdjust {
    None,
    Dashes,
    Gaps,
    #[default]
    DashesAndGaps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashKind {
    LeftSlant,
    RightSlant,
    Straight,
}

/// A stroke style resolved to what it paints (built-in InDesign styles or a document's
/// dashed / dotted / striped stroke style).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum StrokePattern {
    #[default]
    Solid,
    /// Dash, gap, dash, gap… lengths (points). `builtin`: InDesign's "Dashed" family, whose
    /// straight two-point lines start and end on a full dash.
    Dashed { pattern: Vec<f64>, adjust: CornerAdjust, cap: Option<LineCap>, builtin: bool },
    /// Round dots of the stroke weight; `gaps` between dot centres (points), cycled.
    Dotted { gaps: Vec<f64>, adjust: CornerAdjust, builtin: bool },
    /// Parallel lines `(from, to)` across the weight, 0 = left edge, 1 = right edge (left of
    /// the path direction).
    Striped(Vec<(f64, f64)>),
    Hash(HashKind),
    Wavy,
    WhiteDiamond,
}

impl StrokePattern {
    /// A built-in InDesign stroke type (`StrokeStyle/$ID/<name>`) for stroke weight `w`; a
    /// "Dashed" stroke takes its own dash and gap (`StrokeDashAndGap`) and adjustment.
    pub fn builtin(name: &str, w: f64, dash_and_gap: Option<&[f64]>, adjust: CornerAdjust) -> Option<StrokePattern> {
        let stripes = |v: &[(f64, f64)]| Some(StrokePattern::Striped(v.to_vec()));
        match name {
            "Solid" => Some(StrokePattern::Solid),
            "Dashed" => {
                let mut p: Vec<f64> = dash_and_gap.unwrap_or(&[12.0]).iter().copied().filter(|v| v.is_finite() && *v >= 0.0).collect();
                if p.is_empty() {
                    p.push(12.0);
                }
                if p.len() % 2 == 1 {
                    // one value: the gap equals the dash; an odd list repeats (PDF semantics)
                    let again = p.clone();
                    p.extend(again);
                }
                Some(StrokePattern::Dashed { pattern: p, adjust, cap: None, builtin: true })
            }
            "Canned Dashed 3x2" => Some(StrokePattern::Dashed { pattern: vec![3.0 * w, 2.0 * w], adjust: CornerAdjust::DashesAndGaps, cap: None, builtin: true }),
            "Canned Dashed 4x4" => Some(StrokePattern::Dashed { pattern: vec![4.0 * w, 4.0 * w], adjust: CornerAdjust::DashesAndGaps, cap: None, builtin: true }),
            "Canned Dotted" => Some(StrokePattern::Dotted { gaps: vec![3.0 * w], adjust: CornerAdjust::Gaps, builtin: true }),
            "Japanese Dots" => Some(StrokePattern::Dotted { gaps: vec![2.0 * w], adjust: CornerAdjust::Gaps, builtin: true }),
            "Triple_Stroke" => stripes(&[(0.0, 0.167), (0.4165, 0.5835), (0.833, 1.0)]),
            "ThickThinThick" => stripes(&[(0.0, 0.286), (0.4285, 0.5715), (0.714, 1.0)]),
            "ThinThickThin" => stripes(&[(0.0, 0.143), (0.357, 0.643), (0.857, 1.0)]),
            "ThickThick" => stripes(&[(0.0, 0.333), (0.667, 1.0)]),
            "ThickThin" => stripes(&[(0.0, 0.5), (0.833, 1.0)]),
            "ThinThick" => stripes(&[(0.0, 0.167), (0.5, 1.0)]),
            "ThinThin" => stripes(&[(0.0, 0.167), (0.833, 1.0)]),
            "Left Slant Hash" => Some(StrokePattern::Hash(HashKind::LeftSlant)),
            "Right Slant Hash" => Some(StrokePattern::Hash(HashKind::RightSlant)),
            "Straight Hash" => Some(StrokePattern::Hash(HashKind::Straight)),
            "Wavy" => Some(StrokePattern::Wavy),
            "White Diamond" => Some(StrokePattern::WhiteDiamond),
            _ => None,
        }
    }
}

/// IDML line end (`LeftLineEnd` / `RightLineEnd`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrowhead {
    #[default]
    None,
    Simple,
    SimpleWide,
    Triangle,
    TriangleWide,
    Barbed,
    Curved,
    Circle,
    CircleSolid,
    Square,
    SquareSolid,
    Bar,
}

impl Arrowhead {
    /// Every arrowhead with its IDML name, in the Stroke panel's order.
    pub const ALL: [(Arrowhead, &'static str); 12] = [
        (Arrowhead::None, "None"),
        (Arrowhead::Simple, "SimpleArrowHead"),
        (Arrowhead::SimpleWide, "SimpleWideArrowHead"),
        (Arrowhead::Triangle, "TriangleArrowHead"),
        (Arrowhead::TriangleWide, "TriangleWideArrowHead"),
        (Arrowhead::Barbed, "BarbedArrowHead"),
        (Arrowhead::Curved, "CurvedArrowHead"),
        (Arrowhead::Circle, "CircleArrowHead"),
        (Arrowhead::CircleSolid, "CircleSolidArrowHead"),
        (Arrowhead::Square, "SquareArrowHead"),
        (Arrowhead::SquareSolid, "SquareSolidArrowHead"),
        (Arrowhead::Bar, "BarArrowHead"),
    ];

    pub fn from_idml(v: &str) -> Option<Arrowhead> {
        Self::ALL.iter().find(|(_, n)| *n == v).map(|(a, _)| *a)
    }

    pub fn idml(self) -> &'static str {
        Self::ALL.iter().find(|(a, _)| *a == self).map_or("None", |(_, n)| n)
    }
}

/// Arrowheads of an open path: `start` at its first point (IDML left end), `end` at its last.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineEnds {
    pub start: Arrowhead,
    pub end: Arrowhead,
    /// Percent (100 = the size for the stroke weight).
    pub start_scale: f64,
    pub end_scale: f64,
    /// `ArrowHeadAlignment="OutsidePath"`: heads sit beyond the path ends.
    pub outside: bool,
}

impl Default for LineEnds {
    fn default() -> Self {
        Self { start: Arrowhead::None, end: Arrowhead::None, start_scale: 100.0, end_scale: 100.0, outside: false }
    }
}

/// Everything the stroke lowering needs about a stroke.
#[derive(Clone, Debug, Default)]
pub struct StrokeSpec {
    pub weight: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f64,
    pub alignment: StrokeAlignment,
    pub pattern: StrokePattern,
    pub ends: LineEnds,
    /// The style has a gap colour to paint under it.
    pub gap: bool,
    /// Fit complete waves between the ends of an open text decoration.
    pub whole_waves: bool,
}

/// Which paint a part takes: the stroke's, or the gap colour of a styled stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeInk {
    #[default]
    Stroke,
    Gap,
}

/// A dash pattern with PDF semantics: lengths (dash, gap, …) and the phase at the path start.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dash {
    pub array: Vec<f64>,
    pub phase: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PartShape {
    /// The paths stroked (a subpath whose points coincide is a dot with round caps).
    Line { width: f64, cap: LineCap, join: LineJoin, miter_limit: f64, dash: Option<Dash> },
    /// The paths filled, nonzero winding.
    #[default]
    Fill,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokePart {
    pub ink: StrokeInk,
    pub shape: PartShape,
    pub paths: Vec<SubPath>,
}

/// A stroke lowered to what InDesign paints, in the item's inner coordinates, in painting
/// order. Consumers paint the parts instead of stroking the item's paths.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StrokeArt {
    pub parts: Vec<StrokePart>,
}

impl StrokeArt {
    /// Bounds of everything painted (with half the line widths), inner coordinates.
    pub fn bounds(&self) -> Option<crate::Rect> {
        let mut out: Option<crate::Rect> = None;
        for p in &self.parts {
            let Some(b) = crate::paths_bounds(&p.paths) else { continue };
            let m = match &p.shape {
                PartShape::Line { width, miter_limit, .. } => width / 2.0 * miter_limit.max(1.5),
                PartShape::Fill => 0.0,
            };
            let r = crate::Rect { left: b.left - m, top: b.top - m, right: b.right + m, bottom: b.bottom + m };
            out = Some(out.map_or(r, |o| o.union(&r)));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------
// Segments

/// One cubic segment; a straight segment has its handles on its ends (IDML convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub a: Point,
    pub b: Point,
    pub c: Point,
    pub d: Point,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathPoint;

    fn poly(pts: &[(f64, f64)], open: bool) -> SubPath {
        SubPath {
            points: pts.iter().map(|&(x, y)| PathPoint { anchor: Point::new(x, y), left: Point::new(x, y), right: Point::new(x, y) }).collect(),
            open,
        }
    }
    /// IDML rectangle order: top left, bottom left, bottom right, top right.
    fn rect(l: f64, t: f64, r: f64, b: f64) -> SubPath {
        poly(&[(l, t), (l, b), (r, b), (r, t)], false)
    }
    fn all(kind: CornerKind, radius: f64) -> [CornerOption; 4] {
        [CornerOption { kind, radius }; 4]
    }

    #[test]
    fn compound_offset_preserves_holes_and_disjoint_rings() {
        let outer = rect(0.0, 0.0, 100.0, 100.0);
        let hole = rect(20.0, 20.0, 80.0, 80.0);
        let mut island = rect(120.0, 20.0, 150.0, 50.0);
        island.points.reverse();
        let rings = closed_contours(&[outer, hole, island], 2.0);
        let lefts: Vec<_> = rings.iter()
            .map(|ring| ring.iter().map(|s| s.a.x).fold(f64::INFINITY, f64::min))
            .collect();
        assert_eq!(lefts, [-2.0, 22.0, 118.0]);
    }
    fn anchors(sp: &SubPath) -> Vec<(f64, f64)> {
        sp.points.iter().map(|p| ((p.anchor.x * 1000.0).round() / 1000.0, (p.anchor.y * 1000.0).round() / 1000.0)).collect()
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 2e-3
    }

    #[test]
    fn rounded_corner_matches_indesign() {
        // InDesign PDF: m 42,30 c 35.373,30 30,35.373 30,42 for a 12 pt corner at (30,30).
        let out = apply_corners(&rect(30.0, 30.0, 150.0, 110.0), &all(CornerKind::Rounded, 12.0));
        let tl = out.points.iter().position(|p| close(p.anchor.x, 42.0) && close(p.anchor.y, 30.0)).expect("tangent point");
        let p = &out.points[tl];
        assert!(close(p.right.x, 35.373) && close(p.right.y, 30.0), "{:?}", p.right);
        let q = &out.points[(tl + 1) % out.points.len()];
        assert!(close(q.anchor.x, 30.0) && close(q.anchor.y, 42.0));
        assert!(close(q.left.x, 30.0) && close(q.left.y, 35.373));
        assert_eq!(out.points.len(), 8);
        assert!(!out.open);
    }

    #[test]
    fn corner_kinds_and_clamping() {
        // inset 12: 42,125 → 42,137 → 30,137 (InDesign)
        let inset = apply_corners(&rect(30.0, 125.0, 150.0, 205.0), &all(CornerKind::Inset, 12.0));
        // the anchors in path order, read cyclically from `from`
        let cyc = |a: &[(f64, f64)], from: (f64, f64), n: usize| -> Vec<(f64, f64)> {
            let i = a.iter().position(|p| *p == from).expect("start point");
            (0..n).map(|k| a[(i + k) % a.len()]).collect()
        };
        assert_eq!(cyc(&anchors(&inset), (42.0, 125.0), 3), vec![(42.0, 125.0), (42.0, 137.0), (30.0, 137.0)]);
        // fancy 12 at (30,410)
        let fancy = apply_corners(&rect(30.0, 410.0, 150.0, 490.0), &all(CornerKind::Fancy, 12.0));
        assert_eq!(
            cyc(&anchors(&fancy), (42.0, 410.0), 7),
            vec![(42.0, 410.0), (42.0, 418.0), (34.0, 418.0), (34.0, 414.0), (38.0, 414.0), (38.0, 422.0), (30.0, 422.0)]
        );
        // rounded 60 on 120 × 80: radius 40 (half the short side)
        let big = apply_corners(&rect(310.0, 125.0, 430.0, 205.0), &all(CornerKind::Rounded, 60.0));
        assert!(anchors(&big).contains(&(350.0, 125.0)) && anchors(&big).contains(&(310.0, 165.0)));
        // a 140 × 20 rectangle: inverse rounded 30 reaches 10 up the short side, 30 along the long one
        let opts = [
            CornerOption { kind: CornerKind::Rounded, radius: 30.0 },
            CornerOption { kind: CornerKind::InverseRounded, radius: 30.0 },
            CornerOption { kind: CornerKind::Bevel, radius: 30.0 },
            CornerOption { kind: CornerKind::Fancy, radius: 30.0 },
        ];
        let thin = apply_corners(&rect(400.0, 340.0, 540.0, 360.0), &opts);
        let a = anchors(&thin);
        for p in [(410.0, 340.0), (400.0, 350.0), (430.0, 360.0), (510.0, 360.0), (540.0, 350.0), (520.0, 343.333), (510.0, 346.667), (510.0, 340.0)] {
            assert!(a.contains(&p), "{p:?} missing in {a:?}");
        }
    }

    #[test]
    fn corner_options_by_point_index_and_smooth_points() {
        // A four-point polygon takes TL, BL, BR, TR in point order; other paths take TL everywhere.
        let opts = [
            CornerOption { kind: CornerKind::Rounded, radius: 6.0 },
            CornerOption { kind: CornerKind::InverseRounded, radius: 24.0 },
            CornerOption { kind: CornerKind::Inset, radius: 18.0 },
            CornerOption { kind: CornerKind::Bevel, radius: 12.0 },
        ];
        let p = apply_corners(&poly(&[(180.0, 200.0), (40.0, 200.0), (40.0, 300.0), (180.0, 300.0)], false), &opts);
        let a = anchors(&p);
        for q in [(174.0, 200.0), (180.0, 206.0), (64.0, 200.0), (40.0, 224.0), (40.0, 282.0), (58.0, 282.0), (58.0, 300.0), (168.0, 300.0), (180.0, 288.0)] {
            assert!(a.contains(&q), "{q:?} missing in {a:?}");
        }
        let tri = apply_corners(&poly(&[(400.0, 480.0), (540.0, 480.0), (470.0, 580.0)], false), &opts);
        // all three corners rounded with 6 pt reach
        assert_eq!(tri.points.len(), 6);
        assert!(tri.points.iter().any(|q| close(q.anchor.x, 406.0) && close(q.anchor.y, 480.0)));
        // an open polyline: only the inner corner
        let open = apply_corners(&poly(&[(450.0, 400.0), (510.0, 300.0), (570.0, 400.0)], true), &all(CornerKind::Rounded, 20.0));
        let a = anchors(&open);
        assert_eq!(a.first(), Some(&(450.0, 400.0)));
        assert_eq!(a.last(), Some(&(570.0, 400.0)));
        assert!(a.contains(&(499.71, 317.15)) && a.contains(&(520.29, 317.15)), "{a:?}");
        // smooth points (an ellipse) are not corners
        let k = 0.552_284_75 * 70.0;
        let oval = SubPath {
            points: vec![
                PathPoint { anchor: Point::new(105.0, 435.0), left: Point::new(105.0 - k, 435.0), right: Point::new(105.0 + k, 435.0) },
                PathPoint { anchor: Point::new(175.0, 385.0), left: Point::new(175.0, 412.6), right: Point::new(175.0, 357.4) },
            ],
            open: false,
        };
        assert_eq!(apply_corners(&oval, &all(CornerKind::Bevel, 10.0)), oval);
    }

    fn spec(pattern: StrokePattern, w: f64) -> StrokeSpec {
        StrokeSpec { weight: w, miter_limit: 4.0, pattern, ..StrokeSpec::default() }
    }

    fn dashes(art: &StrokeArt) -> Vec<(Vec<f64>, f64, Vec<(f64, f64)>)> {
        art.parts
            .iter()
            .filter_map(|p| match &p.shape {
                PartShape::Line { dash: Some(d), .. } => Some((
                    d.array.iter().map(|v| (v * 1000.0).round() / 1000.0).collect(),
                    (d.phase * 1000.0).round() / 1000.0,
                    p.paths.iter().flat_map(anchors).collect(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn dashes_fit_between_corners_like_indesign() {
        let dashed = |adjust| StrokePattern::Dashed { pattern: vec![12.0, 6.0], adjust, cap: None, builtin: true };
        // rect 150 × 60, DashesAndGaps: 60 → 3 × [13, 7]; 150 → 8 × [12.375, 6.375]
        let art = stroke_art(&[rect(40.0, 280.0, 190.0, 340.0)], &spec(dashed(CornerAdjust::DashesAndGaps), 4.0)).unwrap();
        let d = dashes(&art);
        assert!(d.iter().any(|(a, ph, pts)| *a == vec![13.0, 7.0] && *ph == 0.0 && pts[0] == (40.0, 293.5) && pts[1] == (40.0, 330.0)), "{d:?}");
        assert!(d.iter().any(|(a, _, _)| *a == vec![12.375, 6.375]));
        // Dashes: [14, 6] and [12.75, 6]; Gaps: [12, 8] and [12, 6.75]
        let d = dashes(&stroke_art(&[rect(40.0, 120.0, 190.0, 180.0)], &spec(dashed(CornerAdjust::Dashes), 4.0)).unwrap());
        assert!(d.iter().any(|(a, _, _)| *a == vec![14.0, 6.0]) && d.iter().any(|(a, _, _)| *a == vec![12.75, 6.0]));
        let d = dashes(&stroke_art(&[rect(40.0, 200.0, 190.0, 260.0)], &spec(dashed(CornerAdjust::Gaps), 4.0)).unwrap());
        assert!(d.iter().any(|(a, _, _)| *a == vec![12.0, 8.0]) && d.iter().any(|(a, _, _)| *a == vec![12.0, 6.75]));
        // 4 values, DashesAndGaps on a 60 side: [11.5, 3.5, 1.5, 3.5] phase 15
        let four = StrokePattern::Dashed { pattern: vec![12.0, 4.0, 2.0, 4.0], adjust: CornerAdjust::DashesAndGaps, cap: None, builtin: true };
        let d = dashes(&stroke_art(&[rect(40.0, 360.0, 190.0, 420.0)], &spec(four, 4.0)).unwrap());
        assert!(d.iter().any(|(a, ph, _)| *a == vec![11.5, 3.5, 1.5, 3.5] && *ph == 15.0), "{d:?}");
        // a straight two-point line starts and ends on a whole dash: 150 → [11.647, 5.647]
        let d = dashes(&stroke_art(&[poly(&[(400.0, 310.0), (550.0, 310.0)], true)], &spec(dashed(CornerAdjust::DashesAndGaps), 4.0)).unwrap());
        assert_eq!(d, vec![(vec![11.647, 5.647], 0.0, vec![(400.0, 310.0), (550.0, 310.0)])]);
        // three collinear points: one run with half dashes at the ends, [11.611, 5.611]
        let art = stroke_art(&[poly(&[(30.0, 300.0), (100.0, 300.0), (185.0, 300.0)], true)], &spec(dashed(CornerAdjust::DashesAndGaps), 4.0)).unwrap();
        let d = dashes(&art);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, vec![11.611, 5.611]);
        assert!(close(d[0].2[0].0, 41.417));
        // None on a rectangle starts at its bottom-left corner heading right (as InDesign)
        let d = dashes(&stroke_art(&[rect(40.0, 40.0, 190.0, 100.0)], &spec(dashed(CornerAdjust::None), 4.0)).unwrap());
        assert_eq!(d[0].2[..3], [(40.0, 100.0), (190.0, 100.0), (190.0, 40.0)]);
        // None: one plain dash pattern along the whole path
        let d = dashes(&stroke_art(&[poly(&[(30.0, 600.0), (110.0, 600.0), (110.0, 640.0), (185.0, 640.0)], true)], &spec(dashed(CornerAdjust::None), 4.0)).unwrap());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, vec![12.0, 6.0]);
    }

    #[test]
    fn dots_and_builtin_styles() {
        // Canned dotted, w 4, rect 150 × 60: [0, 12] on the short sides, [0, 12.5] on the long
        let p = StrokePattern::builtin("Canned Dotted", 4.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let art = stroke_art(&[rect(220.0, 40.0, 370.0, 100.0)], &spec(p, 4.0)).unwrap();
        let d = dashes(&art);
        assert!(d.iter().any(|(a, _, pts)| *a == vec![0.0, 12.0] && pts[0] == (220.0, 52.0)), "{d:?}");
        assert!(d.iter().any(|(a, _, _)| *a == vec![0.0, 12.5]));
        // four corner dots
        let dots = art.parts.iter().find(|p| matches!(p.shape, PartShape::Line { dash: None, .. })).unwrap();
        assert_eq!(dots.paths.len(), 4);
        assert!(dots.paths.iter().all(|s| s.points.len() == 2 && s.points[0].anchor == s.points[1].anchor));
        // Japanese dots on a 155 pt line, w 6: [0, 11.923] from 261.923
        let p = StrokePattern::builtin("Japanese Dots", 6.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let d = dashes(&stroke_art(&[poly(&[(250.0, 380.0), (405.0, 380.0)], true)], &spec(p, 6.0)).unwrap());
        assert_eq!(d[0].0, vec![0.0, 11.923]);
        assert!(close(d[0].2[0].0, 261.923));
        // default Dashed (12/12) on a 150 line: [11.538, 11.538]
        let p = StrokePattern::builtin("Dashed", 6.0, Some(&[12.0]), CornerAdjust::DashesAndGaps).unwrap();
        let d = dashes(&stroke_art(&[poly(&[(220.0, 0.0), (370.0, 0.0)], true)], &spec(p, 6.0)).unwrap());
        assert_eq!(d[0].0, vec![11.538, 11.538]);
        // Canned dashed 3x2 on 150, w 6: [19.333, 13.333]
        let p = StrokePattern::builtin("Canned Dashed 3x2", 6.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let d = dashes(&stroke_art(&[poly(&[(220.0, 0.0), (370.0, 0.0)], true)], &spec(p, 6.0)).unwrap());
        assert_eq!(d[0].0, vec![19.333, 13.333]);
    }

    #[test]
    fn stripes_hashes_waves() {
        // ThickThin, w 6, line to +x: a 3 pt stripe 1.5 to the left (−y), a 1 pt one 2.5 right
        let p = StrokePattern::builtin("ThickThin", 6.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let art = stroke_art(&[poly(&[(0.0, 0.0), (150.0, 0.0)], true)], &StrokeSpec { gap: true, ..spec(p, 6.0) }).unwrap();
        assert_eq!(art.parts[0].ink, StrokeInk::Gap);
        let widths: Vec<(f64, f64)> = art.parts[1..]
            .iter()
            .map(|p| match &p.shape {
                PartShape::Line { width, .. } => ((width * 1000.0).round() / 1000.0, (p.paths[0].points[0].anchor.y * 1000.0).round() / 1000.0),
                _ => (0.0, 0.0),
            })
            .collect();
        assert_eq!(widths, vec![(3.0, -1.5), (1.002, 2.499)]);
        // Right slant hash, w 6, 155 pt: 50 hashes, the first from (5.603, −2.603) to (0.396, 2.604)
        let p = StrokePattern::builtin("Right Slant Hash", 6.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let art = stroke_art(&[poly(&[(0.0, 0.0), (155.0, 0.0)], true)], &spec(p, 6.0)).unwrap();
        let h = &art.parts[0];
        assert_eq!(h.paths.len(), 50);
        let a = anchors(&h.paths[0]);
        assert!(close(a[0].0, 5.603) && close(a[0].1, -2.603) && close(a[1].0, 0.397) && close(a[1].1, 2.603), "{a:?}");
        // Straight hash, w 6, 150: 50 hashes from x 0.561, 3.038 apart
        let p = StrokePattern::builtin("Straight Hash", 6.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let art = stroke_art(&[poly(&[(0.0, 0.0), (150.0, 0.0)], true)], &spec(p, 6.0)).unwrap();
        assert_eq!(art.parts[0].paths.len(), 50);
        assert!(close(art.parts[0].paths[1].points[0].anchor.x, 3.599));
        // Wavy, w 6, 155: 12 half waves of 12.917
        let p = StrokePattern::builtin("Wavy", 6.0, None, CornerAdjust::DashesAndGaps).unwrap();
        let art = stroke_art(&[poly(&[(250.0, 460.0), (405.0, 460.0)], true)], &spec(p, 6.0)).unwrap();
        let wave = &art.parts[0].paths[0];
        assert_eq!(wave.points.len(), 13);
        assert!(close(wave.points[1].anchor.x, 262.917) && close(wave.points[0].right.y, 457.067));
    }

    fn heads(art: &StrokeArt) -> (Vec<Vec<(f64, f64)>>, Vec<(f64, f64)>) {
        let fill = art.parts.iter().filter(|p| p.shape == PartShape::Fill).flat_map(|p| p.paths.iter().map(anchors)).collect();
        let line = art.parts.iter().find(|p| matches!(p.shape, PartShape::Line { .. })).map(|p| anchors(&p.paths[0])).unwrap_or_default();
        (fill, line)
    }

    #[test]
    fn arrowheads_match_indesign_sizes() {
        let ends = |a: Arrowhead, b: Arrowhead| LineEnds { start: a, end: b, ..LineEnds::default() };
        let line = poly(&[(230.0, 0.0), (380.0, 0.0)], true);
        // triangle, w 4: tip at the end, 16.773 long, ±6.104, line pulled back 9.849
        let art = stroke_art(std::slice::from_ref(&line), &StrokeSpec { ends: ends(Arrowhead::Triangle, Arrowhead::Triangle), ..spec(StrokePattern::Solid, 4.0) }).unwrap();
        let (fill, l) = heads(&art);
        assert!(close(l[0].0, 239.849) && close(l[1].0, 370.151), "{l:?}");
        let t = &fill[0];
        assert_eq!(t[0], (230.0, 0.0));
        assert!(close(t[1].0, 246.773) && close(t[1].1.abs(), 6.104), "{t:?}");
        // w 1 and w 10 follow the same rule
        for (w, len, half) in [(1.0, 7.012, 2.552), (10.0, 31.57, 11.488), (0.5, 6.325, 2.302)] {
            let art = stroke_art(std::slice::from_ref(&line), &StrokeSpec { ends: ends(Arrowhead::Triangle, Arrowhead::None), ..spec(StrokePattern::Solid, w) }).unwrap();
            let (fill, _) = heads(&art);
            assert!(close(fill[0][1].0 - 230.0, len) && close(fill[0][1].1.abs(), half), "w {w}: {:?}", fill[0]);
        }
        // circle w 4: radius 8 centred 10 in, line from 18
        let art = stroke_art(std::slice::from_ref(&line), &StrokeSpec { ends: ends(Arrowhead::Circle, Arrowhead::None), ..spec(StrokePattern::Solid, 4.0) }).unwrap();
        let first_line = art.parts.iter().find_map(|p| match &p.shape {
            PartShape::Line { width, .. } if (*width - 4.0).abs() < 1e-9 => Some(anchors(&p.paths[0])),
            _ => None,
        });
        assert!(close(first_line.unwrap()[0].0, 248.0));
        let all: Vec<(f64, f64)> = art.parts.iter().flat_map(|p| p.paths.iter().flat_map(anchors)).collect();
        assert!(all.contains(&(240.0, 8.0)) || all.contains(&(240.0, -8.0)), "{all:?}");
        // simple, scaled 50 %: stroked with half the weight, vertex 2.924 in
        let art = stroke_art(
            &[poly(&[(30.0, 40.0), (200.0, 40.0)], true)],
            &StrokeSpec { ends: LineEnds { start: Arrowhead::Simple, start_scale: 50.0, ..LineEnds::default() }, ..spec(StrokePattern::Solid, 4.0) },
        )
        .unwrap();
        let chevron = art.parts.iter().find(|p| matches!(p.shape, PartShape::Line { width, .. } if (width - 2.0).abs() < 1e-9)).unwrap();
        let a = anchors(&chevron.paths[0]);
        assert!(close(a[1].0, 32.924) && close((a[0].1 - 40.0).abs(), 3.71), "{a:?}");
        // outside alignment: line unshortened, head beyond the end
        let art = stroke_art(
            &[poly(&[(50.0, 120.0), (200.0, 120.0)], true)],
            &StrokeSpec { ends: LineEnds { start: Arrowhead::Triangle, outside: true, ..LineEnds::default() }, ..spec(StrokePattern::Solid, 4.0) },
        )
        .unwrap();
        let (fill, l) = heads(&art);
        assert_eq!(l[0], (50.0, 120.0));
        assert!(close(fill[0][0].0, 40.151));
    }

    #[test]
    fn heads_longer_than_the_line_are_pushed_out() {
        // InDesign: 12 pt line, w 4 triangles: line 456–459, tips at 446.151 and 468.849
        let art = stroke_art(
            &[poly(&[(450.0, 90.0), (462.0, 90.0)], true)],
            &StrokeSpec { ends: LineEnds { start: Arrowhead::Triangle, end: Arrowhead::Triangle, ..LineEnds::default() }, ..spec(StrokePattern::Solid, 4.0) },
        )
        .unwrap();
        let (fill, l) = heads(&art);
        assert!(close(l[0].0, 456.0) && close(l[1].0, 459.0), "{l:?}");
        assert!(close(fill[0][0].0, 446.151) && close(fill[1][0].0, 468.849), "{fill:?}");
    }

    #[test]
    fn inside_alignment_moves_the_style() {
        // a dashed inside-aligned 6 pt stroke on 150 × 60 runs 3 pt inside: sides 144 and 54
        let p = StrokePattern::Dashed { pattern: vec![12.0, 6.0], adjust: CornerAdjust::DashesAndGaps, cap: None, builtin: true };
        let art = stroke_art(&[rect(400.0, 650.0, 550.0, 710.0)], &StrokeSpec { alignment: StrokeAlignment::Inside, ..spec(p, 6.0) }).unwrap();
        let d = dashes(&art);
        assert!(d.iter().all(|(a, _, _)| *a == vec![12.0, 6.0]), "{d:?}");
        assert!(d.iter().any(|(_, _, pts)| pts.contains(&(403.0, 665.0))), "{d:?}");
    }
}
