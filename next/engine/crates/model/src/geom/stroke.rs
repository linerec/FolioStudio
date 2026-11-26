//! Lowering of stroke styles and arrowheads to painted path parts.

use super::{
    Arrowhead, CornerAdjust, Dash, HashKind, PartShape, Seg, StrokeArt, StrokeInk,
    StrokePart, StrokePattern, StrokeSpec,
};
use super::curve::{KAPPA, Measured, Track, move_end, move_start};
use super::offset::{offset, signed_area};
use super::path::{segments, to_subpaths};
use super::point::{add, dist, left, mul, sub, unit};
use crate::{LineCap, LineJoin, Point, StrokeAlignment, SubPath};

/// Rounds half down (InDesign's count of pattern repeats: 12.5 → 12, 12.51 → 13).
fn round_half_down(x: f64) -> f64 {
    (x - 0.5).ceil()
}

/// Pattern lengths stretched so that `n` whole repeats fill `length` (dashes centred on both
/// ends), or, with `full_ends`, so that the run starts and ends with a whole first dash.
fn fit(pattern: &[f64], length: f64, adjust: CornerAdjust, full_ends: bool, builtin: bool) -> Vec<f64> {
    let p: f64 = pattern.iter().sum();
    let m = pattern.len() as f64;
    if p <= 1e-9 || adjust == CornerAdjust::None {
        return pattern.to_vec();
    }
    let count = if builtin || pattern.iter().step_by(2).any(|v| *v > 0.0) { round_half_down } else { f64::floor };
    let d0 = pattern[0];
    let mut out = pattern.to_vec();
    let half = m / 2.0;
    let (delta, on_dashes, on_gaps) = if full_ends {
        let k = count((length - d0) / p).max(0.0);
        match adjust {
            CornerAdjust::DashesAndGaps => ((length - k * p - d0) / (k * m + 1.0), true, true),
            CornerAdjust::Dashes => ((length - k * p - d0) / (k * half + 1.0), true, false),
            CornerAdjust::Gaps if k > 0.0 => ((length - k * p - d0) / (k * half), false, true),
            _ => ((length - k * p - d0) / (k * half + 1.0), true, false),
        }
    } else {
        let n = count(length / p).max(1.0);
        let extra = length / n - p;
        match adjust {
            CornerAdjust::DashesAndGaps => (extra / m, true, true),
            CornerAdjust::Dashes => (extra / half, true, false),
            _ => (extra / half, false, true),
        }
    };
    for (i, v) in out.iter_mut().enumerate() {
        let dash = i % 2 == 0;
        if (dash && on_dashes) || (!dash && on_gaps) {
            *v += delta;
        }
    }
    // A pattern squeezed below zero: take the deficit from the other kind.
    let neg: f64 = out.iter().filter(|v| **v < 0.0).sum();
    if neg < 0.0 {
        let others = out.iter().filter(|v| **v > 0.0).count().max(1) as f64;
        for v in out.iter_mut() {
            if *v < 0.0 {
                *v = 0.0;
            } else {
                *v = (*v + neg / others).max(0.0);
            }
        }
    }
    out
}

/// Splits segments into runs at anchors where the path does not continue straight on
/// (InDesign fits dash patterns between such anchors; collinear straight segments merge).
fn runs(segs: &[Seg], closed: bool) -> (Vec<Vec<Seg>>, bool) {
    let n = segs.len();
    let continues = |a: &Seg, b: &Seg| {
        a.is_line() && b.is_line() && matches!((a.end_dir(), b.start_dir()), (Some(u), Some(v)) if (u.x * v.x + u.y * v.y) > 1.0 - 1e-9)
    };
    let mut breaks: Vec<usize> = (1..n).filter(|&i| !continues(&segs[i - 1], &segs[i])).collect();
    let wrap_break = closed && n > 0 && !continues(&segs[n - 1], &segs[0]);
    if closed {
        if !wrap_break && breaks.is_empty() {
            return (vec![segs.to_vec()], true);
        }
        // rotate so the closed path starts at a break
        let start = if wrap_break { 0 } else { breaks[0] };
        let rotated: Vec<Seg> = segs[start..].iter().chain(&segs[..start]).copied().collect();
        breaks = (1..n).filter(|&i| !continues(&rotated[i - 1], &rotated[i])).collect();
        let mut out = Vec::new();
        let mut from = 0;
        for &b in &breaks {
            out.push(rotated[from..b].to_vec());
            from = b;
        }
        out.push(rotated[from..].to_vec());
        return (out, false);
    }
    let mut out = Vec::new();
    let mut from = 0;
    for &b in &breaks {
        out.push(segs[from..b].to_vec());
        from = b;
    }
    out.push(segs[from..].to_vec());
    (out, false)
}

struct Parts {
    solid: Vec<SubPath>,
    dashed: Vec<(SubPath, Dash)>,
}

/// Dashes or dots fitted between corners, with the pieces at corners and ends drawn solid.
fn lower_dashes(segs: &[Seg], closed: bool, pattern: &[f64], adjust: CornerAdjust, builtin: bool, two_points: bool, out: &mut Parts) {
    if adjust == CornerAdjust::None {
        let path = if closed { rectangle_from_bottom_left(segs) } else { None };
        out.dashed.push((to_subpaths(path.as_deref().unwrap_or(segs), closed), Dash { array: pattern.to_vec(), phase: 0.0 }));
        return;
    }
    let full_ends = !closed && two_points && builtin && pattern[0] > 0.0;
    let (runs, loop_run) = runs(segs, closed);
    let tracks: Vec<Track> = runs.iter().map(|r| Track::new(r)).collect();
    let fitted: Vec<Vec<f64>> = tracks.iter().map(|t| fit(pattern, t.total, adjust, full_ends, builtin)).collect();
    if full_ends {
        out.dashed.push((to_subpaths(segs, false), Dash { array: fitted[0].clone(), phase: 0.0 }));
        return;
    }
    for (t, p) in tracks.iter().zip(&fitted) {
        let period: f64 = p.iter().sum();
        let d0 = p[0];
        let g0 = p.get(1).copied().unwrap_or(0.0);
        let g_last = *p.last().unwrap();
        let s0 = d0 / 2.0 + g0;
        let s1 = t.total - d0 / 2.0 - g_last / 2.0;
        if loop_run {
            // a closed path without corners: one run around, dash centred on its start
            let phase = (period - d0 / 2.0).rem_euclid(period.max(1e-9));
            out.dashed.push((to_subpaths(&t.cut(0.0, t.total), true), Dash { array: p.clone(), phase }));
            return;
        }
        if s1 > s0 + 1e-6 && period > 1e-9 {
            out.dashed.push((to_subpaths(&t.cut(s0, s1), false), Dash { array: p.clone(), phase: (d0 + g0).rem_euclid(period) }));
        }
    }
    // corner pieces: the half dash on each side of a break, joined
    let k = tracks.len();
    let joins = if closed { k } else { k.saturating_sub(1) };
    for i in 0..joins {
        let (a, b) = (&tracks[i], &tracks[(i + 1) % k]);
        let (ha, hb) = (fitted[i][0] / 2.0, fitted[(i + 1) % k][0] / 2.0);
        let mut segs = a.cut(a.total - ha, a.total);
        segs.extend(b.cut(0.0, hb));
        out.solid.push(to_subpaths(&segs, false));
    }
    if !closed && k > 0 {
        let first = &tracks[0];
        out.solid.push(to_subpaths(&first.cut(0.0, fitted[0][0] / 2.0), false));
        let last = &tracks[k - 1];
        out.solid.push(to_subpaths(&last.cut(last.total - fitted[k - 1][0] / 2.0, last.total), false));
    }
}

/// An axis-aligned rectangle restarted at its bottom-left corner, heading right: where
/// InDesign starts an unadjusted dash pattern on a rectangle (it writes rectangles that way).
fn rectangle_from_bottom_left(segs: &[Seg]) -> Option<Vec<Seg>> {
    if segs.len() != 4 || !segs.iter().all(|s| s.is_line() && (s.a.x == s.d.x || s.a.y == s.d.y)) {
        return None;
    }
    let corner = |s: &Seg| (s.a.x, -s.a.y);
    let start = (0..4).min_by(|&i, &j| corner(&segs[i]).partial_cmp(&corner(&segs[j])).unwrap_or(std::cmp::Ordering::Equal))?;
    let mut out: Vec<Seg> = (0..4).map(|k| segs[(start + k) % 4]).collect();
    if out[0].d.x <= out[0].a.x {
        // heading up: walk the other way round
        out = (0..4).map(|k| segs[(start + 4 - 1 - k) % 4]).map(|s| Seg::line(s.d, s.a)).collect();
    }
    Some(out)
}

/// A subpath of one point repeated: a dot with round caps.
fn dot(p: Point) -> SubPath {
    let pp = crate::PathPoint { anchor: p, left: p, right: p };
    SubPath { points: vec![pp, pp], open: true }
}

fn polygon(pts: &[Point]) -> SubPath {
    SubPath { points: pts.iter().map(|&p| crate::PathPoint { anchor: p, left: p, right: p }).collect(), open: false }
}

/// Arrowhead outline in local coordinates (x into the line from its end, y across) and how
/// far the line is pulled back from the end.
struct Head {
    /// Filled outlines.
    fill: Vec<Vec<Seg>>,
    /// Stroked outlines (width `ws`), and whether each is closed.
    stroke: Vec<(Vec<Seg>, bool)>,
    inset: f64,
}

const TAN_NARROW: f64 = 0.363_970_234; // tan 20°
const SIN_NARROW: f64 = 0.342_020_143; // sin 20°
const TAN_WIDE: f64 = 0.929_76;
const SIN_WIDE: f64 = 0.680_92;

fn circle(c: Point, r: f64) -> Vec<Seg> {
    let k = r * KAPPA;
    let p = |x: f64, y: f64| Point::new(c.x + x, c.y + y);
    vec![
        Seg { a: p(0.0, r), b: p(k, r), c: p(r, k), d: p(r, 0.0) },
        Seg { a: p(r, 0.0), b: p(r, -k), c: p(k, -r), d: p(0.0, -r) },
        Seg { a: p(0.0, -r), b: p(-k, -r), c: p(-r, -k), d: p(-r, 0.0) },
        Seg { a: p(-r, 0.0), b: p(-r, k), c: p(-k, r), d: p(0.0, r) },
    ]
}

fn poly_segs(pts: &[(f64, f64)], closed: bool) -> Vec<Seg> {
    let p: Vec<Point> = pts.iter().map(|&(x, y)| Point::new(x, y)).collect();
    let mut out: Vec<Seg> = p.windows(2).map(|w| Seg::line(w[0], w[1])).collect();
    if closed && p.len() > 2 {
        out.push(Seg::line(*p.last().unwrap(), p[0]));
    }
    out
}

/// Head geometry for stroke weight × scale `ws` (InDesign 2025, measured per weight).
fn head(kind: Arrowhead, ws: f64) -> Option<Head> {
    // Filled heads grow with √weight above 1 pt: half width and length fitted separately.
    let root = ws.max(1.0).sqrt();
    let narrow_h = ws / 2.0 + 2.0519 * root;
    let narrow_l = 1.37405 * ws + 5.63823 * root;
    let wide_h = ws / 2.0 + 3.40462 * root;
    let wide_l = 0.53779 * ws + 3.66184 * root;
    Some(match kind {
        Arrowhead::None => return None,
        Arrowhead::Simple | Arrowhead::SimpleWide => {
            let (tan, sin, extra) = if kind == Arrowhead::Simple { (TAN_NARROW, SIN_NARROW, 1.71) } else { (TAN_WIDE, SIN_WIDE, 2.724) };
            let v = ws / 2.0 / sin;
            let h = ws + extra;
            let x = v + h / tan;
            Head { fill: vec![], stroke: vec![(poly_segs(&[(x, h), (v, 0.0), (x, -h)], false), false)], inset: v }
        }
        Arrowhead::Triangle | Arrowhead::Barbed | Arrowhead::Curved => {
            let (h, l) = (narrow_h, narrow_l);
            let n = ws / 2.0 / SIN_NARROW + 4.0;
            let outline = match kind {
                Arrowhead::Triangle => poly_segs(&[(0.0, 0.0), (l, h), (l, -h)], true),
                Arrowhead::Barbed => poly_segs(&[(0.0, 0.0), (l, h), (n, 0.0), (l, -h)], true),
                _ => {
                    let (o, a, b) = (Point::new(0.0, 0.0), Point::new(l, h), Point::new(l, -h));
                    vec![Seg::line(o, a), Seg { a, b: a, c: Point::new(n, 0.0), d: b }, Seg::line(b, o)]
                }
            };
            Head { fill: vec![outline], stroke: vec![], inset: n }
        }
        Arrowhead::TriangleWide => {
            let (h, l) = (wide_h, wide_l);
            Head { fill: vec![poly_segs(&[(0.0, 0.0), (l, h), (l, -h)], true)], stroke: vec![], inset: ws / 2.0 / SIN_WIDE + 3.0 }
        }
        Arrowhead::Circle => Head { fill: vec![], stroke: vec![(circle(Point::new(2.5 * ws, 0.0), 2.0 * ws), true)], inset: 4.5 * ws },
        Arrowhead::CircleSolid => Head { fill: vec![circle(Point::new(2.0 * ws, 0.0), 2.0 * ws)], stroke: vec![], inset: 2.0 * ws },
        Arrowhead::Square => Head {
            fill: vec![],
            stroke: vec![(poly_segs(&[(0.5 * ws, -2.0 * ws), (4.5 * ws, -2.0 * ws), (4.5 * ws, 2.0 * ws), (0.5 * ws, 2.0 * ws)], true), true)],
            inset: 4.5 * ws,
        },
        Arrowhead::SquareSolid => Head {
            fill: vec![poly_segs(&[(0.0, -2.0 * ws), (4.0 * ws, -2.0 * ws), (4.0 * ws, 2.0 * ws), (0.0, 2.0 * ws)], true)],
            stroke: vec![],
            inset: 2.0 * ws,
        },
        Arrowhead::Bar => Head { fill: vec![], stroke: vec![(poly_segs(&[(0.5 * ws, -2.25 * ws), (0.5 * ws, 2.25 * ws)], false), false)], inset: 0.5 * ws },
    })
}

/// Places a head at `end` pointing away from the line (`into` = unit direction into the line).
fn place(segs: &[Seg], end: Point, into: Point, shift: f64) -> Vec<Seg> {
    let across = left(into);
    let map = |p: Point| add(end, add(mul(into, p.x - shift), mul(across, p.y)));
    segs.iter().map(|s| Seg { a: map(s.a), b: map(s.b), c: map(s.c), d: map(s.d) }).collect()
}

/// The stroke of `paths` lowered to parts, or `None` when a plain stroke along the paths is
/// what InDesign paints (solid style, no arrowheads).
pub fn stroke_art(paths: &[SubPath], spec: &StrokeSpec) -> Option<StrokeArt> {
    let w = spec.weight;
    let has_heads = (spec.ends.start != Arrowhead::None || spec.ends.end != Arrowhead::None) && paths.iter().any(|p| p.open);
    // a plain stroke is painted along the item's paths, except an inside or outside aligned
    // one on a closed path: InDesign strokes that path moved by half the weight
    let aligned = spec.alignment != StrokeAlignment::Center && paths.iter().any(|p| !p.open);
    if w <= 0.0 || !w.is_finite() || (spec.pattern == StrokePattern::Solid && !has_heads && !aligned) {
        return None;
    }
    let miter = spec.miter_limit.max(1.0);
    let line = |width: f64, cap: LineCap, join: LineJoin, dash: Option<Dash>| PartShape::Line { width, cap, join, miter_limit: miter, dash };
    let mut gap_paths: Vec<SubPath> = Vec::new();
    let mut solid: Vec<SubPath> = Vec::new();
    let mut styled: Vec<StrokePart> = Vec::new();
    let mut head_fill: Vec<SubPath> = Vec::new();
    let mut head_stroke: Vec<(f64, SubPath)> = Vec::new();
    for sp in paths {
        let mut segs = segments(sp);
        if segs.is_empty() {
            continue;
        }
        let closed = !sp.open;
        let two_points = sp.points.len() == 2;
        let left_is_inside = closed && signed_area(&segs) < 0.0;
        // Inside / outside alignment: the style follows the path moved by half the weight.
        if closed && spec.alignment != StrokeAlignment::Center {
            let inside = spec.alignment == StrokeAlignment::Inside;
            segs = offset(&segs, true, if inside == left_is_inside { w / 2.0 } else { -w / 2.0 }, spec.join, miter);
            if segs.is_empty() {
                continue;
            }
        }
        // Arrowheads on open paths: heads placed at the ends, the line pulled back by their
        // inset along the end direction (a curve's end handle moves with its anchor). Heads
        // longer than the line together are pushed out: InDesign then keeps the line from
        // half to three quarters of its length, the heads' notches on those points.
        let mut line_left = true;
        if !closed && has_heads {
            let total: f64 = segs.iter().map(|s| Measured::new(*s).length()).sum();
            let ws = |scale: f64| w * scale.max(1.0) / 100.0;
            let hs = head(spec.ends.start, ws(spec.ends.start_scale)).map(|h| (h, ws(spec.ends.start_scale)));
            let he = head(spec.ends.end, ws(spec.ends.end_scale)).map(|h| (h, ws(spec.ends.end_scale)));
            let (ns, ne) = (hs.as_ref().map_or(0.0, |h| h.0.inset), he.as_ref().map_or(0.0, |h| h.0.inset));
            let squeeze = !spec.ends.outside && ns + ne > total;
            let (start_pt, start_dir) = (segs[0].a, segs[0].start_dir().unwrap_or(Point::new(1.0, 0.0)));
            let last = *segs.last().unwrap();
            let (end_pt, end_dir) = (last.d, mul(last.end_dir().unwrap_or(Point::new(1.0, 0.0)), -1.0));
            let (cut_from, cut_to) = if he.is_some() { (total / 2.0, total * 0.75) } else { (total / 2.0, total) };
            for (h, at_start) in [(&hs, true), (&he, false)] {
                let Some((h, ws)) = h else { continue };
                let (pt, into) = if at_start { (start_pt, start_dir) } else { (end_pt, end_dir) };
                let shift = if spec.ends.outside {
                    h.inset
                } else if squeeze {
                    if at_start { h.inset - cut_from } else { h.inset - (total - cut_to) }
                } else {
                    0.0
                };
                for f in &h.fill {
                    head_fill.push(to_subpaths(&place(f, pt, into, shift), true));
                }
                for (s, closed_head) in &h.stroke {
                    head_stroke.push((*ws, to_subpaths(&place(s, pt, into, shift), *closed_head)));
                }
                if !spec.ends.outside && !squeeze {
                    let v = mul(into, h.inset);
                    if at_start {
                        segs[0] = move_start(segs[0], v);
                    } else {
                        let i = segs.len() - 1;
                        segs[i] = move_end(segs[i], v);
                    }
                }
            }
            if squeeze {
                let hs_none = hs.is_none();
                let (a, b) = if hs_none { (0.0, total - ne) } else { (cut_from, cut_to) };
                segs = Track::new(&segs).cut(a, b);
                line_left = b - a > 1e-6 && !segs.is_empty();
            }
        }
        if !line_left {
            continue;
        }
        let path = || to_subpaths(&segs, closed);
        if spec.gap && spec.pattern != StrokePattern::Solid {
            gap_paths.push(path());
        }
        match &spec.pattern {
            StrokePattern::Solid => solid.push(path()),
            StrokePattern::Dashed { pattern, adjust, cap, builtin } => {
                if pattern.iter().sum::<f64>() <= 1e-9 {
                    solid.push(path());
                    continue;
                }
                let mut p = Parts { solid: Vec::new(), dashed: Vec::new() };
                lower_dashes(&segs, closed, pattern, *adjust, *builtin, two_points, &mut p);
                let cap = cap.unwrap_or(spec.cap);
                if !p.solid.is_empty() {
                    styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(w, cap, spec.join, None), paths: p.solid });
                }
                for (sp, d) in p.dashed {
                    styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(w, cap, spec.join, Some(d)), paths: vec![sp] });
                }
            }
            StrokePattern::Dotted { gaps, adjust, builtin } => {
                let mut pattern = Vec::with_capacity(gaps.len() * 2);
                for g in gaps {
                    pattern.push(0.0);
                    pattern.push(g.max(0.0));
                }
                if pattern.iter().sum::<f64>() <= 1e-9 {
                    continue;
                }
                // dotted styles adjust their gaps only
                let adjust = if *adjust == CornerAdjust::None { CornerAdjust::None } else { CornerAdjust::Gaps };
                let mut p = Parts { solid: Vec::new(), dashed: Vec::new() };
                lower_dashes(&segs, closed, &pattern, adjust, *builtin, two_points, &mut p);
                // the pieces at corners and ends have no length: dots on their points
                let dots: Vec<SubPath> = p.solid.iter().filter_map(|s| s.points.first().map(|f| dot(f.anchor))).collect();
                if !dots.is_empty() {
                    styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(w, LineCap::Round, LineJoin::Round, None), paths: dots });
                }
                for (sp, d) in p.dashed {
                    styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(w, LineCap::Round, LineJoin::Round, Some(d)), paths: vec![sp] });
                }
            }
            StrokePattern::Striped(stripes) => {
                for &(a, b) in stripes {
                    let width = (b - a).abs() * w;
                    if width <= 1e-9 {
                        continue;
                    }
                    // the first stripe lies left of an open path, outside a closed one
                    let o = w * (0.5 - (a + b) / 2.0);
                    let moved = offset(&segs, closed, if left_is_inside { -o } else { o }, spec.join, miter);
                    if !moved.is_empty() {
                        styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(width, spec.cap, spec.join, None), paths: vec![to_subpaths(&moved, closed)] });
                    }
                }
            }
            StrokePattern::Hash(kind) => {
                let hashes = hash_lines(&Track::new(&segs), *kind, w);
                if !hashes.is_empty() {
                    styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(0.187 * w, LineCap::Butt, LineJoin::Miter, None), paths: hashes });
                }
            }
            StrokePattern::Wavy => {
                let wave = wave_path(&Track::new(&segs), closed, spec.whole_waves, w);
                styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(0.265 * w, LineCap::Butt, LineJoin::Miter, None), paths: vec![wave] });
            }
            StrokePattern::WhiteDiamond => {
                let width = 0.09 * w;
                let diamonds = diamond_outlines(&Track::new(&segs), w, width);
                styled.push(StrokePart { ink: StrokeInk::Stroke, shape: line(width, LineCap::Butt, LineJoin::Miter, None), paths: diamonds });
            }
        }
    }
    let mut parts: Vec<StrokePart> = Vec::new();
    if !gap_paths.is_empty() {
        parts.push(StrokePart { ink: StrokeInk::Gap, shape: line(w, spec.cap, spec.join, None), paths: gap_paths });
    }
    if !solid.is_empty() {
        parts.push(StrokePart { ink: StrokeInk::Stroke, shape: line(w, spec.cap, spec.join, None), paths: solid });
    }
    parts.extend(styled);
    if !head_fill.is_empty() {
        parts.push(StrokePart { ink: StrokeInk::Stroke, shape: PartShape::Fill, paths: head_fill });
    }
    // hollow heads are stroked solid with the scaled weight
    for (ws, p) in head_stroke {
        parts.push(StrokePart { ink: StrokeInk::Stroke, shape: line(ws, spec.cap, spec.join, None), paths: vec![p] });
    }
    (!parts.is_empty()).then_some(StrokeArt { parts })
}

/// Hash marks across the stroke every half weight (spacing stretched to fill the path):
/// slanted ones at 45°, straight ones across; each 0.187 × the weight wide.
fn hash_lines(t: &Track, kind: HashKind, w: f64) -> Vec<SubPath> {
    let hw = 0.187 * w;
    let (span, first) = match kind {
        HashKind::Straight => (t.total - hw, hw / 2.0),
        _ => (t.total - w, hw / (2.0 * std::f64::consts::SQRT_2)),
    };
    if span < 0.0 {
        return Vec::new();
    }
    let count = (span / (w / 2.0)).floor().max(0.0) as usize;
    let step = if count > 0 { span / count as f64 } else { 0.0 };
    let c = hw / (2.0 * std::f64::consts::SQRT_2);
    let h = w / 2.0 - c;
    (0..=count)
        .map(|i| {
            let s0 = first + i as f64 * step;
            let seg = match kind {
                HashKind::Straight => {
                    let (p, n) = (t.point(s0), left(t.tangent(s0)));
                    Seg::line(add(p, mul(n, w / 2.0)), sub(p, mul(n, w / 2.0)))
                }
                _ => {
                    let s1 = s0 + 2.0 * h;
                    let (p0, n0) = (t.point(s0), left(t.tangent(s0)));
                    let (p1, n1) = (t.point(s1), left(t.tangent(s1)));
                    if kind == HashKind::LeftSlant {
                        Seg::line(add(p0, mul(n0, h)), sub(p1, mul(n1, h)))
                    } else {
                        Seg::line(add(p1, mul(n1, h)), sub(p0, mul(n0, h)))
                    }
                }
            };
            to_subpaths(&[seg], false)
        })
        .collect()
}

/// A wave along the path, first to the left.
fn wave_path(t: &Track, closed: bool, whole: bool, w: f64) -> SubPath {
    // half waves at least two weights long, one more round a closed path to make them even
    // (oracle: basic-stroke stroke-type-wavy-*)
    let mut n = (t.total / (2.0 * w)).floor().max(1.0) as usize;
    if closed && n % 2 == 1 {
        n += 1;
    } else if whole {
        n = (n / 2).max(1) * 2;
    }
    let lambda = t.total / n as f64;
    let amp = 0.4889 * w;
    let mut wave = Vec::with_capacity(n);
    for k in 0..n {
        let s = k as f64 * lambda;
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        let off = |q: f64| add(t.point(q), mul(left(t.tangent(q)), sign * amp));
        wave.push(Seg { a: t.point(s), b: off(s + lambda / 4.0), c: off(s + 3.0 * lambda / 4.0), d: t.point(s + lambda) });
    }
    to_subpaths(&wave, false)
}

/// Place each diamond on the chord of its arc-length interval, then inset its sides so
/// the outside of the stroke reaches the chord endpoints and the full pattern width.
fn diamond_outlines(t: &Track, w: f64, stroke_width: f64) -> Vec<SubPath> {
    let overlap = 0.05 * w;
    let span = (t.total - overlap).max(0.0);
    let count = (span / w).floor().max(1.0);
    let pitch = span / count;
    (0..count as usize)
        .filter_map(|i| {
            let s = i as f64 * pitch;
            let start = t.point(s);
            let end = t.point(s + pitch + overlap);
            let axis = unit(sub(end, start))?;
            let chord = dist(start, end);
            let scale = 1.0 - (stroke_width / chord).hypot(stroke_width / w);
            if scale <= 0.0 {
                return None;
            }
            let center = mul(add(start, end), 0.5);
            let along = mul(axis, 0.5 * chord * scale);
            let across = mul(left(axis), 0.5 * w * scale);
            Some(polygon(&[sub(center, along), add(center, across), add(center, along), sub(center, across)]))
        })
        .collect()
}
