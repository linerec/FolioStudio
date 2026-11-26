//! Emphasis marks (kenten, IDML `KentenKind`): a mark over (or under) every character of a
//! run, drawn as filled shapes in the text colour or the `KentenFillColor`, so renderers,
//! the PDF writer and separations need nothing new.
//!
//! Placement measured on InDesign 2025 (20.5.5) output (content-drops specimen
//! `drops-text`, page 4; artifacts/next-20261018/content-drops/kenten):
//! - A mark is set at `KentenFontSize` points, half the text size when "Auto".
//! - Its em box (0.88 above, 0.12 below its baseline) is centred on the character's
//!   advance; above the text the mark's em box bottom sits `KentenPlacement` above the
//!   text's em box top, below the text its em box top sits that far below the text's em
//!   box bottom.
//! - Spaces, tabs and objects take no mark.
//! - Only paragraphs set with a Japanese composer (`HL Composer J`, `HL Single J`) show
//!   them; the Paragraph, Single-line and World-Ready composers draw none.
//! - The text's em box is the font's (`FaceMetrics::embox_top`): marks of 6, 9 and 12 pt
//!   on 18 pt Malgun Gothic, Microsoft YaHei, Gulim and Arial sit within 0.01 pt of these
//!   rules.
//!
//! The mark shapes are this module's own geometry sized to InDesign's marks, measured at
//! 2400 dpi on 9 pt marks (mark em units, v up from the mark baseline): circles 0.70 across
//! centred 0.38 up (the em box centre), outlines 0.063 thick; the fisheye a 0.05 ring round
//! a dot 0.413 across; the bullseye two 0.05 rings 0.70 and 0.413 across; small circles
//! 0.44 and 0.487 across; triangles 0.116–0.884 by 0.053–0.72 with 0.06 outlines; the
//! sesame dots fitted to the row profile of the rendered marks. InDesign draws them from a font of its own, which
//! is not used or copied. A custom mark is the character in the run's font.

use folio_model::layout::{PlacedGlyph, TextShape};
use folio_model::{Kenten, KentenKind, Paint, PathPoint, Point, SubPath};

/// Mark em box above the mark baseline (the rest is below).
const MARK_ASCENT: f64 = 0.88;
/// Circle centre above the mark baseline, in mark ems.
const CENTRE: f64 = 0.38;
const RADIUS: f64 = 0.35;
/// Outline of the white circles.
const LINE: f64 = 0.0633;
/// Rings of the fisheye and bullseye, and their inner circle's radius.
const RING: f64 = 0.05;
const INNER: f64 = 0.2067;

/// The character a mark goes on, in frame inner coordinates.
pub(super) struct Target {
    /// Left and right of the character's advance.
    pub x0: f64,
    pub x1: f64,
    pub baseline: f64,
    /// Text size and its em box top above the baseline (points).
    pub size: f64,
    pub embox_top: f64,
}

/// A custom mark's glyph: the font, glyph and its advance in ems.
pub(super) type CustomGlyph = (folio_model::layout::FontId, u16, f64);

/// The marks of one character: shapes, and the glyph of a custom mark.
pub(super) fn marks(k: &Kenten, t: &Target, text_fill: &Paint, overprint: bool, custom: Option<CustomGlyph>) -> (Option<TextShape>, Option<PlacedGlyph>) {
    let paint = k.paint.clone().unwrap_or_else(|| text_fill.clone());
    if paint == Paint::None {
        return (None, None);
    }
    let ms = k.size.unwrap_or(t.size / 2.0);
    if ms <= 0.0 || !ms.is_finite() {
        return (None, None);
    }
    let centre = (t.x0 + t.x1) / 2.0;
    let mark_baseline = if k.below {
        t.baseline + (t.size - t.embox_top) + k.placement + MARK_ASCENT * ms
    } else {
        t.baseline - t.embox_top - k.placement - (1.0 - MARK_ASCENT) * ms
    };
    if k.kind == KentenKind::Custom {
        let glyph = custom.map(|(font, glyph, advance)| PlacedGlyph {
            font,
            glyph,
            x: centre - advance * ms / 2.0,
            y: mark_baseline,
            size: ms,
            hscale: 1.0,
            vscale: 1.0,
            skew: 0.0,
            path_transform: None,
            fill: paint,
            stroke: Paint::None,
            stroke_weight: 0.0,
            overprint_fill: overprint,
            overprint_stroke: false,
            fill_gradient: None,
            stroke_gradient: None,
        });
        return (None, glyph);
    }
    // Mark em units (u right from the em box left, v up from the mark baseline) → frame.
    let left = centre - ms / 2.0;
    let map = |u: f64, v: f64| Point::new(left + u * ms, mark_baseline - v * ms);
    let paths: Vec<SubPath> = shape(k.kind)
        .into_iter()
        .map(|sp| SubPath {
            points: sp.points.iter().map(|p| PathPoint { anchor: map(p.anchor.x, p.anchor.y), left: map(p.left.x, p.left.y), right: map(p.right.x, p.right.y) }).collect(),
            open: false,
        })
        .collect();
    (Some(TextShape { paths, paint, overprint, ..Default::default() }), None)
}

fn pt(x: f64, y: f64) -> PathPoint {
    let a = Point::new(x, y);
    PathPoint { anchor: a, left: a, right: a }
}

fn smooth(x: f64, y: f64, l: (f64, f64), r: (f64, f64)) -> PathPoint {
    PathPoint { anchor: Point::new(x, y), left: Point::new(l.0, l.1), right: Point::new(r.0, r.1) }
}

/// A circle wound counter-clockwise in mark units (v up).
fn circle(cx: f64, cy: f64, r: f64) -> SubPath {
    let k = folio_model::geom::KAPPA * r;
    SubPath {
        points: vec![
            smooth(cx + r, cy, (cx + r, cy - k), (cx + r, cy + k)),
            smooth(cx, cy + r, (cx + k, cy + r), (cx - k, cy + r)),
            smooth(cx - r, cy, (cx - r, cy + k), (cx - r, cy - k)),
            smooth(cx, cy - r, (cx - k, cy - r), (cx + k, cy - r)),
        ],
        open: false,
    }
}

/// The same outline run the other way (a hole under nonzero winding).
fn reversed(sp: SubPath) -> SubPath {
    let points = sp.points.into_iter().rev().map(|p| PathPoint { anchor: p.anchor, left: p.right, right: p.left }).collect();
    SubPath { points, open: sp.open }
}

/// A ring: outer radius `r`, `w` thick.
fn ring(r: f64, w: f64) -> [SubPath; 2] {
    [circle(0.5, CENTRE, r), reversed(circle(0.5, CENTRE, r - w))]
}

/// The sesame dot: a stroke from a cut at the upper left, thickening to a round end at the
/// lower right (fitted to the row profile of InDesign's rendered mark within 0.003 em).
fn sesame() -> SubPath {
    SubPath {
        points: vec![
            smooth(0.1948, 0.7021, (0.2198, 0.6983), (0.4379, 0.6333)),
            smooth(0.8231, 0.2052, (0.8175, 0.4825), (0.8494, 0.13)),
            smooth(0.7029, 0.0286, (0.7894, 0.0254), (0.6513, 0.0329)),
            smooth(0.5988, 0.0992, (0.6445, 0.0514), (0.54, 0.2939)),
            smooth(0.18, 0.6425, (0.3298, 0.5411), (0.145, 0.66)),
        ],
        open: false,
    }
}

/// The white sesame dot: a slightly smaller outline of the sesame dot, about 0.06 thick.
fn white_sesame() -> [SubPath; 2] {
    let outer = SubPath {
        points: vec![
            smooth(0.2492, 0.6914, (0.2086, 0.6914), (0.4548, 0.6345)),
            smooth(0.8294, 0.1683, (0.8388, 0.4725), (0.825, 0.1069)),
            smooth(0.7042, 0.0411, (0.7794, 0.0248), (0.6256, 0.0273)),
            smooth(0.6006, 0.0811, (0.6233, 0.0908), (0.485, 0.3052)),
            smooth(0.185, 0.6375, (0.3411, 0.523), (0.2256, 0.6944)),
        ],
        open: false,
    };
    let hole = SubPath {
        points: vec![
            smooth(0.2308, 0.655, (0.4133, 0.5008), (0.485, 0.5767)),
            smooth(0.7733, 0.2158, (0.7533, 0.4133), (0.7858, 0.0819)),
            smooth(0.7077, 0.0892, (0.7161, 0.088), (0.6444, 0.0811)),
            smooth(0.612, 0.1608, (0.6167, 0.1794), (0.535, 0.3236)),
        ],
        open: false,
    };
    [outer, reversed(hole)]
}

fn triangle() -> [(f64, f64); 3] {
    [(0.116, 0.0535), (0.884, 0.0535), (0.5, 0.72)]
}

/// The triangle shrunk about its incentre so its edges move in by `w`.
fn triangle_hole(w: f64) -> SubPath {
    let t = triangle();
    let len = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let (la, lb, lc) = (len(t[1], t[2]), len(t[2], t[0]), len(t[0], t[1]));
    let p = la + lb + lc;
    let ic = ((la * t[0].0 + lb * t[1].0 + lc * t[2].0) / p, (la * t[0].1 + lb * t[1].1 + lc * t[2].1) / p);
    let area = ((t[1].0 - t[0].0) * (t[2].1 - t[0].1) - (t[2].0 - t[0].0) * (t[1].1 - t[0].1)).abs() / 2.0;
    let r_in = 2.0 * area / p;
    let s = (1.0 - w / r_in).max(0.0);
    let points = t.iter().rev().map(|&(x, y)| pt(ic.0 + (x - ic.0) * s, ic.1 + (y - ic.1) * s)).collect();
    SubPath { points, open: false }
}

fn shape(kind: KentenKind) -> Vec<SubPath> {
    let tri = || SubPath { points: triangle().iter().map(|&(x, y)| pt(x, y)).collect(), open: false };
    match kind {
        KentenKind::SesameDot => vec![sesame()],
        KentenKind::WhiteSesameDot => white_sesame().into(),
        KentenKind::BlackCircle => vec![circle(0.5, CENTRE, RADIUS)],
        KentenKind::WhiteCircle => ring(RADIUS, LINE).into(),
        KentenKind::SmallBlackCircle => vec![circle(0.5, CENTRE, 0.22)],
        KentenKind::SmallWhiteCircle => ring(0.2433, LINE).into(),
        KentenKind::Fisheye => {
            let mut v: Vec<SubPath> = ring(RADIUS, RING).into();
            v.push(circle(0.5, CENTRE, INNER));
            v
        }
        KentenKind::Bullseye => {
            let mut v: Vec<SubPath> = ring(RADIUS, RING).into();
            v.extend(ring(INNER, RING));
            v
        }
        KentenKind::BlackTriangle => vec![tri()],
        KentenKind::WhiteTriangle => vec![tri(), triangle_hole(0.06)],
        KentenKind::Custom => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black() -> Paint {
        Paint::Swatch { id: "Color/Black".into(), tint: 100.0 }
    }

    fn target() -> Target {
        Target { x0: 100.0, x1: 118.0, baseline: 200.0, size: 18.0, embox_top: 0.88 * 18.0 }
    }

    #[test]
    fn auto_size_mark_sits_on_the_em_box_top() {
        let k = Kenten { kind: KentenKind::BlackCircle, ..Kenten::default() };
        let (s, g) = marks(&k, &target(), &black(), false, None);
        assert!(g.is_none());
        let b = s.unwrap().bounds().unwrap();
        // 9 pt mark: 0.70 em across, centred on the advance; em box bottom on the text's
        // em box top (200 - 15.84), so its baseline is 1.08 above that.
        assert!(((b.left + b.right) / 2.0 - 109.0).abs() < 1e-9);
        assert!((b.right - b.left - 0.70 * 9.0).abs() < 1e-9);
        let mark_baseline = 200.0 - 15.84 - 0.12 * 9.0;
        assert!(((b.top + b.bottom) / 2.0 - (mark_baseline - CENTRE * 9.0)).abs() < 1e-9);
    }

    #[test]
    fn below_and_placement() {
        let k = Kenten { kind: KentenKind::BlackCircle, size: Some(6.0), placement: 2.0, below: true, ..Kenten::default() };
        let b = marks(&k, &target(), &black(), false, None).0.unwrap().bounds().unwrap();
        // Em box bottom of the text 200 + 2.16; mark em box top 2 pt lower.
        let mark_baseline = 200.0 + 2.16 + 2.0 + 0.88 * 6.0;
        assert!(((b.top + b.bottom) / 2.0 - (mark_baseline - CENTRE * 6.0)).abs() < 1e-9);
    }

    #[test]
    fn white_marks_have_holes_and_no_paint_draws_nothing() {
        for kind in [KentenKind::WhiteCircle, KentenKind::WhiteTriangle, KentenKind::WhiteSesameDot, KentenKind::SmallWhiteCircle] {
            let k = Kenten { kind, ..Kenten::default() };
            assert_eq!(marks(&k, &target(), &black(), false, None).0.unwrap().paths.len(), 2, "{kind:?}");
        }
        let k = Kenten::default();
        assert!(marks(&k, &target(), &Paint::None, false, None).0.is_none());
    }
}
