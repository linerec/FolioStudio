//! Parallel contours for stroke alignment and filled paths.

use super::{Seg, curve::{move_end, move_start}, parallel, path::segments};
use super::point::{add, cross, dist, len, mul, sub};
use crate::{LineJoin, Point, SubPath};

/// Flattens curves into lines (tolerance in points).
fn flatten(segs: &[Seg], tol: f64) -> Vec<Seg> {
    let mut out = Vec::new();
    for s in segs {
        if s.is_line() {
            out.push(*s);
            continue;
        }
        let poly = dist(s.a, s.b) + dist(s.b, s.c) + dist(s.c, s.d);
        let n = ((poly / tol).sqrt() * 0.6).ceil().clamp(4.0, 400.0) as usize;
        let mut prev = s.a;
        for i in 1..=n {
            let p = s.eval(i as f64 / n as f64);
            out.push(Seg::line(prev, p));
            prev = p;
        }
    }
    out
}

/// The path offset by `d` to the left of its direction (negative: right), as InDesign moves a
/// path for inside and outside stroke alignment. Lines move along their normal; curves stay
/// curves (`offset_curve`). Where neighbouring pieces overlap (the inner side of a corner) they
/// are cut where their end tangents cross; where they part (the outer side) the corner takes
/// the stroke's join: that crossing while within `miter_limit` × |d| of the corner (else
/// bevelled), an arc of radius |d| about the corner, or a straight bevel.
pub(super) fn offset(segs: &[Seg], closed: bool, d: f64, join: LineJoin, miter_limit: f64) -> Vec<Seg> {
    if d.abs() < 1e-9 {
        return segs.to_vec();
    }
    let (segs, mut pieces): (Vec<&Seg>, Vec<Vec<Seg>>) = segs.iter().filter_map(|s| {
        let side = offset_curve(s, d);
        (!side.is_empty()).then_some((s, side))
    }).unzip();
    if segs.is_empty() {
        return Vec::new();
    }
    let n = segs.len();
    let mut joins: Vec<Vec<Seg>> = vec![Vec::new(); n];
    for i in (if closed { 0 } else { 1 })..n {
        let p = (i + n - 1) % n;
        let (Some(u0), Some(u1)) = (segs[p].end_dir(), segs[i].start_dir()) else { continue };
        let (a, b) = (pieces[p].last().unwrap().d, pieces[i][0].a);
        let Some((t,x)) = tangent_intersection(a,u0,b,u1) else {
            let dot = u0.x * u1.x + u0.y * u1.y;
            if dot <= 0.0 || dist(a, b) > 1e-9 {
                joins[i].push(Seg::line(a, b));
            }
            continue;
        };
        let corner = segs[i].a;
        if t < 0.0 {
            let last = pieces[p].len() - 1;
            let (incoming, outgoing) = trim_corner(pieces[p][last], pieces[i][0], x);
            pieces[p][last] = incoming;
            pieces[i][0] = outgoing;
        } else if t == 0.0 || (join == LineJoin::Miter && dist(x, corner) <= miter_limit.max(1.0) * d.abs()) {
            // the pieces cut (or extended) to the crossing, handles moving with their ends
            let last = pieces[p].len() - 1;
            pieces[p][last] = move_end(pieces[p][last], sub(x, a));
            pieces[i][0] = move_start(pieces[i][0], sub(x, b));
        } else if join == LineJoin::Round {
            joins[i] = arc(corner, a, b);
        } else {
            joins[i].push(Seg::line(a, b));
        }
    }
    let mut out = Vec::with_capacity(n * 2);
    for (j, p) in joins.into_iter().zip(pieces) {
        out.extend(j);
        out.extend(p);
    }
    out.retain(|s| dist(s.a, s.d) > 1e-9 || !s.is_line());
    out
}

// R4
pub(crate) fn trim_corner(incoming: Seg, outgoing: Seg, tip: Point) -> (Seg, Seg) {
    let (mut t, mut u) = (1.0, 0.0);
    for _ in 0..16 {
        let (a, b) = (incoming.eval(t), outgoing.eval(u));
        let error = sub(a, b);
        if len(error) < 1e-9 {
            return (incoming.sub(0.0, t), outgoing.sub(u, 1.0));
        }
        let (da, db) = (incoming.deriv(t), outgoing.deriv(u));
        let det = cross(da, db);
        if det.abs() < 1e-14 { break; }
        t = (t - cross(error, db) / det).clamp(0.0, 1.0);
        u = (u - cross(error, da) / det).clamp(0.0, 1.0);
    }
    (move_end(incoming, sub(tip, incoming.d)), move_start(outgoing, sub(tip, outgoing.a)))
}

pub(crate) fn tangent_intersection(a: Point, u: Point, b: Point, v: Point) -> Option<(f64, Point)> {
    let det = cross(u, v);
    if det.abs() < 1e-9 { return None; }
    let delta = sub(b, a);
    let along = cross(delta, v) / det;
    Some((along, add(a, mul(u, along))))
}

// R3
pub(crate) fn offset_curve(s: &Seg, d: f64) -> Vec<Seg> {
    parallel::curve(s, d)
}

/// The shorter circular arc about `c` from `a` to `b` (same distance from `c`), as cubics of
/// at most a quarter turn each.
fn arc(c: Point, a: Point, b: Point) -> Vec<Seg> {
    let (va, vb) = (sub(a, c), sub(b, c));
    let r = len(va);
    let turn = (va.x * vb.y - va.y * vb.x).atan2(va.x * vb.x + va.y * vb.y);
    if r < 1e-9 || turn.abs() < 1e-9 {
        return vec![Seg::line(a, b)];
    }
    let pieces = (turn.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = turn / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let rot = |v: Point, t: f64| Point::new(v.x * t.cos() - v.y * t.sin(), v.x * t.sin() + v.y * t.cos());
    let mut out = Vec::with_capacity(pieces);
    let mut v0 = va;
    for i in 1..=pieces {
        let v1 = if i == pieces { vb } else { rot(va, step * i as f64) };
        // tangents: the radius turned a quarter in the direction of travel
        let (t0, t1) = (Point::new(-v0.y, v0.x), Point::new(-v1.y, v1.x));
        out.push(Seg { a: add(c, v0), b: add(add(c, v0), mul(t0, k)), c: sub(add(c, v1), mul(t1, k)), d: add(c, v1) });
        v0 = v1;
    }
    out
}

/// Signed area of a closed run of segments (positive when it turns clockwise on a y-down page).
pub(super) fn signed_area(segs: &[Seg]) -> f64 {
    polygon_area(&flatten(segs, 0.25))
}

fn polygon_area(edges: &[Seg]) -> f64 {
    edges.iter().map(|s| s.a.x * s.d.y - s.d.x * s.a.y).sum::<f64>() * 0.5
}

fn encloses(edges: &[Seg], p: Point) -> bool {
    edges.iter().filter(|s| {
        (s.a.y > p.y) != (s.d.y > p.y)
            && p.x < s.a.x + (p.y - s.a.y) * (s.d.x - s.a.x) / (s.d.y - s.a.y)
    }).count() % 2 == 1
}

/// Parallel contours of an even-odd filled path. Positive distances expand the filled area.
pub fn closed_contours(paths: &[SubPath], distance: f64) -> Vec<Vec<Seg>> {
    let rings: Vec<_> = paths.iter().filter(|p| !p.open && p.points.len() >= 2).map(segments).collect();
    if distance == 0.0 {
        return rings;
    }
    let polygons: Vec<_> = rings.iter().map(|r| flatten(r, 0.25)).collect();
    rings.iter().enumerate().map(|(i, ring)| {
        let p = ring[0].a;
        let depth = polygons.iter().enumerate().filter(|(j, edges)| *j != i && encloses(edges, p)).count();
        let outward = polygon_area(&polygons[i]).signum() * distance;
        let shift = if depth % 2 == 0 { outward } else { -outward };
        offset(ring, true, shift, LineJoin::Round, 1.0)
    }).collect()
}
