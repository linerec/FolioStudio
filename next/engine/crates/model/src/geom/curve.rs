//! Cubic segments, endpoint adjustments, and arc-length measurement.

use super::Seg;
use super::point::{add, dist, lerp, mul, sub, unit};
use crate::Point;

/// Bezier circle constant: handle length / radius for a quarter circle.
pub const KAPPA: f64 = 0.552_284_75;

impl Seg {
    pub fn line(a: Point, d: Point) -> Seg {
        Seg { a, b: a, c: d, d }
    }
    pub fn is_line(&self) -> bool {
        self.b == self.a && self.c == self.d
    }
    pub fn eval(&self, t: f64) -> Point {
        if self.is_line() {
            return lerp(self.a, self.d, t);
        }
        let ab = lerp(self.a, self.b, t);
        let bc = lerp(self.b, self.c, t);
        let cd = lerp(self.c, self.d, t);
        lerp(lerp(ab, bc, t), lerp(bc, cd, t), t)
    }
    pub(crate) fn deriv(&self, t: f64) -> Point {
        if self.is_line() {
            return sub(self.d, self.a);
        }
        let u = 1.0 - t;
        add(add(mul(sub(self.b, self.a), 3.0 * u * u), mul(sub(self.c, self.b), 6.0 * u * t)), mul(sub(self.d, self.c), 3.0 * t * t))
    }
    /// Unit tangent at `t`, falling back to the chord where the derivative vanishes.
    pub fn tangent(&self, t: f64) -> Point {
        unit(self.deriv(t))
            .or_else(|| unit(self.deriv((t + 1e-4).min(1.0))))
            .or_else(|| unit(self.deriv((t - 1e-4).max(0.0))))
            .or_else(|| unit(sub(self.d, self.a)))
            .unwrap_or(Point::new(1.0, 0.0))
    }
    /// Direction leaving the start point.
    pub fn start_dir(&self) -> Option<Point> {
        [self.b, self.c, self.d].into_iter().find(|p| dist(*p, self.a) > 1e-9).and_then(|p| unit(sub(p, self.a)))
    }
    /// Direction arriving at the end point.
    pub fn end_dir(&self) -> Option<Point> {
        [self.c, self.b, self.a].into_iter().find(|p| dist(*p, self.d) > 1e-9).and_then(|p| unit(sub(self.d, p)))
    }
    pub fn split(&self, t: f64) -> (Seg, Seg) {
        if self.is_line() {
            let m = lerp(self.a, self.d, t);
            return (Seg::line(self.a, m), Seg::line(m, self.d));
        }
        let ab = lerp(self.a, self.b, t);
        let bc = lerp(self.b, self.c, t);
        let cd = lerp(self.c, self.d, t);
        let abc = lerp(ab, bc, t);
        let bcd = lerp(bc, cd, t);
        let m = lerp(abc, bcd, t);
        (Seg { a: self.a, b: ab, c: abc, d: m }, Seg { a: m, b: bcd, c: cd, d: self.d })
    }
    /// The part between `t0` and `t1`.
    pub fn sub(&self, t0: f64, t1: f64) -> Seg {
        if self.is_line() {
            return Seg::line(lerp(self.a, self.d, t0), lerp(self.a, self.d, t1));
        }
        let (_, right) = self.split(t0);
        if t0 >= 1.0 - 1e-12 {
            return Seg::line(self.d, self.d);
        }
        let (part, _) = right.split(((t1 - t0) / (1.0 - t0)).clamp(0.0, 1.0));
        part
    }
}

/// Arc-length table of a segment (curves sampled finely enough for sub-point accuracy).
#[derive(Clone, Debug)]
pub(super) struct Measured {
    pub(super) seg: Seg,
    /// Cumulative length at t = i / (n - 1).
    cum: Vec<f64>,
}

impl Measured {
    pub(super) fn new(seg: Seg) -> Measured {
        if seg.is_line() {
            return Measured { seg, cum: vec![0.0, dist(seg.a, seg.d)] };
        }
        let poly = dist(seg.a, seg.b) + dist(seg.b, seg.c) + dist(seg.c, seg.d);
        let n = (poly / 0.25).ceil().clamp(16.0, 512.0) as usize;
        let mut cum = Vec::with_capacity(n + 1);
        cum.push(0.0);
        let mut prev = seg.a;
        for i in 1..=n {
            let p = seg.eval(i as f64 / n as f64);
            cum.push(cum[i - 1] + dist(prev, p));
            prev = p;
        }
        Measured { seg, cum }
    }
    pub(super) fn length(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }
    /// Parameter at arc length `s` from the start.
    pub(super) fn t_at(&self, s: f64) -> f64 {
        let total = self.length();
        if total <= 1e-12 {
            return 0.0;
        }
        let s = s.clamp(0.0, total);
        let n = self.cum.len() - 1;
        let i = self.cum.partition_point(|&c| c < s).clamp(1, n);
        let (c0, c1) = (self.cum[i - 1], self.cum[i]);
        let f = if c1 > c0 { (s - c0) / (c1 - c0) } else { 0.0 };
        ((i - 1) as f64 + f) / n as f64
    }
}

/// A run of segments measured by arc length.
pub(super) struct Track {
    segs: Vec<Measured>,
    /// Arc length at the start of each segment.
    starts: Vec<f64>,
    pub(super) total: f64,
}

impl Track {
    pub(super) fn new(segs: &[Seg]) -> Track {
        let segs: Vec<Measured> = segs.iter().map(|s| Measured::new(*s)).collect();
        let mut starts = Vec::with_capacity(segs.len());
        let mut total = 0.0;
        for m in &segs {
            starts.push(total);
            total += m.length();
        }
        Track { segs, starts, total }
    }
    fn locate(&self, s: f64) -> (usize, f64) {
        if self.segs.is_empty() {
            return (0, 0.0);
        }
        let s = s.clamp(0.0, self.total);
        let i = self.starts.partition_point(|&st| st <= s).saturating_sub(1).min(self.segs.len() - 1);
        (i, self.segs[i].t_at(s - self.starts[i]))
    }
    pub(super) fn point(&self, s: f64) -> Point {
        let (i, t) = self.locate(s);
        self.segs.get(i).map_or(Point::default(), |m| m.seg.eval(t))
    }
    pub(super) fn tangent(&self, s: f64) -> Point {
        let (i, t) = self.locate(s);
        self.segs.get(i).map_or(Point::new(1.0, 0.0), |m| m.seg.tangent(t))
    }
    /// The segments between arc lengths `s0` and `s1`.
    pub(super) fn cut(&self, s0: f64, s1: f64) -> Vec<Seg> {
        let mut out = Vec::new();
        if self.segs.is_empty() || s1 < s0 - 1e-9 {
            return out;
        }
        let (i0, t0) = self.locate(s0);
        let (i1, t1) = self.locate(s1);
        for i in i0..=i1 {
            let a = if i == i0 { t0 } else { 0.0 };
            let b = if i == i1 { t1 } else { 1.0 };
            if b < a {
                continue;
            }
            out.push(self.segs[i].seg.sub(a, b));
        }
        out
    }
}

pub(crate) fn move_start(s: Seg, v: Point) -> Seg {
    if s.is_line() { Seg::line(add(s.a, v), s.d) } else { Seg { a: add(s.a, v), b: add(s.b, v), c: s.c, d: s.d } }
}

pub(crate) fn move_end(s: Seg, v: Point) -> Seg {
    if s.is_line() { Seg::line(s.a, add(s.d, v)) } else { Seg { a: s.a, b: s.b, c: add(s.c, v), d: add(s.d, v) } }
}
