//! The extent of a stroked path, mitre tips included (outlines-c3, measured on InDesign 2025).
//!
//! An inline polygon from Create Outlines on a range with a text stroke is put and sized by the outer edge of its stroke:
//! the bare path grown half the weight, and at a corner as far as the mitre tip while the mitre length stays within the
//! limit (default 4; beyond it the corner is bevelled). 33 cases (`artifacts/outlines-c3-20261023/measure/idml-s`) agree to
//! 0.002 pt; the text after such an object starts at the right edge of this box.

use crate::{Point, Rect, SubPath};

type P = [f64; 2];

/// Tangent of a cubic segment at its start (`start`) or end, skipping coincident handles; the direction of travel.
fn tangent(s: &[P; 4], start: bool) -> P {
    let pts = if start { [s[0], s[1], s[2], s[3]] } else { [s[3], s[2], s[1], s[0]] };
    for p in &pts[1..] {
        let d = [p[0] - pts[0][0], p[1] - pts[0][1]];
        if d[0].hypot(d[1]) > 1e-9 {
            return if start { d } else { [-d[0], -d[1]] };
        }
    }
    [0.0, 0.0]
}

/// Bounds of the closed subpaths stroked `weight` wide (centred on the path) with mitre joins of `miter_limit`;
/// open subpaths contribute their bare extent grown by half the weight. `None` without any point.
pub fn stroke_bounds(paths: &[SubPath], weight: f64, miter_limit: f64) -> Option<Rect> {
    let hw = weight.max(0.0) / 2.0;
    let mut b: Option<(f64, f64, f64, f64)> = None;
    let mut add = |x: f64, y: f64| {
        b = Some(match b {
            None => (x, y, x, y),
            Some((l, t, r, bo)) => (l.min(x), t.min(y), r.max(x), bo.max(y)),
        });
    };
    for sp in paths {
        let n = sp.points.len();
        if n == 0 {
            continue;
        }
        if sp.open {
            // the extent of the path (curve extrema are covered by its control polygon here) grown by half the weight
            for q in &sp.points {
                for p in [q.anchor, q.left, q.right] {
                    add(p.x - hw, p.y - hw);
                    add(p.x + hw, p.y + hw);
                }
            }
            continue;
        }
        let pt = |p: Point| -> P { [p.x, p.y] };
        // a zero length segment (the explicit closing point equal to the start) has no direction: leave it out
        let segs: Vec<[P; 4]> = (0..n)
            .map(|k| {
                let (a, c) = (&sp.points[k], &sp.points[(k + 1) % n]);
                [pt(a.anchor), pt(a.right), pt(c.left), pt(c.anchor)]
            })
            .filter(|s| s.iter().any(|q| (q[0] - s[0][0]).hypot(q[1] - s[0][1]) > 1e-9))
            .collect();
        for s in &segs {
            add(s[0][0], s[0][1]);
            for axis in 0..2 {
                let (p0, p1, p2, p3) = (s[0][axis], s[1][axis], s[2][axis], s[3][axis]);
                let (a, bb, c) = (p3 - 3.0 * p2 + 3.0 * p1 - p0, p2 - 2.0 * p1 + p0, p1 - p0);
                let mut roots = Vec::new();
                if a.abs() < 1e-12 {
                    if bb.abs() > 1e-12 {
                        roots.push(-c / (2.0 * bb));
                    }
                } else {
                    let d = bb * bb - a * c;
                    if d >= 0.0 {
                        roots.push((-bb + d.sqrt()) / a);
                        roots.push((-bb - d.sqrt()) / a);
                    }
                }
                for t in roots.into_iter().filter(|t| *t > 0.0 && *t < 1.0) {
                    let u = 1.0 - t;
                    let at = |i: usize| u * u * u * s[0][i] + 3.0 * u * u * t * s[1][i] + 3.0 * u * t * t * s[2][i] + t * t * t * s[3][i];
                    let q = [at(0), at(1)];
                    let d = [
                        3.0 * u * u * (s[1][0] - s[0][0]) + 6.0 * u * t * (s[2][0] - s[1][0]) + 3.0 * t * t * (s[3][0] - s[2][0]),
                        3.0 * u * u * (s[1][1] - s[0][1]) + 6.0 * u * t * (s[2][1] - s[1][1]) + 3.0 * t * t * (s[3][1] - s[2][1]),
                    ];
                    let len = d[0].hypot(d[1]).max(1e-12);
                    // the edge of the stroke at a curve's extremum is half the weight further along the normal
                    let (nx, ny) = (-d[1] / len, d[0] / len);
                    add(q[0], q[1]);
                    add(q[0] + nx * hw, q[1] + ny * hw);
                    add(q[0] - nx * hw, q[1] - ny * hw);
                }
            }
        }
        if hw <= 0.0 {
            continue;
        }
        for k in 0..segs.len() {
            let (prev, cur) = (&segs[(k + segs.len() - 1) % segs.len()], &segs[k]);
            let (tin, tout) = (tangent(prev, false), tangent(cur, true));
            let (li, lo) = (tin[0].hypot(tin[1]), tout[0].hypot(tout[1]));
            if li < 1e-9 || lo < 1e-9 {
                continue;
            }
            let (din, dout) = ([tin[0] / li, tin[1] / li], [tout[0] / lo, tout[1] / lo]);
            let a = cur[0];
            for sgn in [1.0, -1.0] {
                let (nin, nout) = ([-din[1] * sgn, din[0] * sgn], [-dout[1] * sgn, dout[0] * sgn]);
                add(a[0] + nin[0] * hw, a[1] + nin[1] * hw);
                add(a[0] + nout[0] * hw, a[1] + nout[1] * hw);
                let (cross, dot) = (din[0] * dout[1] - din[1] * dout[0], din[0] * dout[0] + din[1] * dout[1]);
                if cross.abs() > 1e-9 || dot < 0.0 {
                    // the angle between the two edges
                    let theta = std::f64::consts::PI - cross.abs().atan2(dot);
                    if theta > 1e-9 && 1.0 / (theta / 2.0).sin() <= miter_limit {
                        let bis = [nin[0] + nout[0], nin[1] + nout[1]];
                        let bl = bis[0].hypot(bis[1]);
                        if bl > 1e-9 {
                            let m = hw / (theta / 2.0).sin();
                            add(a[0] + bis[0] / bl * m, a[1] + bis[1] / bl * m);
                        }
                    }
                }
            }
        }
    }
    b.map(|(left, top, right, bottom)| Rect { left, top, right, bottom })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathPoint;

    fn poly(pts: &[(f64, f64)]) -> SubPath {
        SubPath { points: pts.iter().map(|&(x, y)| PathPoint { anchor: Point::new(x, y), left: Point::new(x, y), right: Point::new(x, y) }).collect(), open: false }
    }

    #[test]
    fn a_square_grows_half_the_weight_on_every_side() {
        let r = stroke_bounds(&[poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])], 2.0, 4.0).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (-1.0, -1.0, 11.0, 11.0));
    }

    #[test]
    fn a_sharp_corner_reaches_its_mitre_tip_until_the_limit() {
        // apex angle 2*atan(1/5) = 22.6 degrees: mitre ratio 1/sin(11.3 deg) = 5.1 > 4, so it is bevelled, not mitred
        let sharp = stroke_bounds(&[poly(&[(0.0, 0.0), (10.0, -2.0), (10.0, 2.0)])], 2.0, 4.0).unwrap();
        assert!(sharp.left > -1.5, "bevelled: {}", sharp.left);
        // a right angle: mitre ratio 1.41, the tip is hw*sqrt(2) from the corner along the bisector (x extent hw)
        let right = stroke_bounds(&[poly(&[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)])], 2.0, 4.0).unwrap();
        assert!((right.left + 1.0).abs() < 1e-9 && (right.top + 1.0).abs() < 1e-9);
        // the 45 degree corners reach further than half the weight: 1/tan(22.5 deg) = 2.41
        assert!(right.right > 10.0 + 2.0, "{}", right.right);
    }
}
