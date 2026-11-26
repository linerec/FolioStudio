use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Point(pub f64, pub f64);

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0, self.1 + rhs.1)
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0, self.1 - rhs.1)
    }
}
impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, scale: f64) -> Self {
        Self(self.0 * scale, self.1 * scale)
    }
}
impl Point {
    pub const ZERO: Point = Point(0., 0.);
    pub fn length(self) -> f64 {
        self.0.hypot(self.1)
    }
    pub fn dot(self, other: Self) -> f64 {
        self.0 * other.0 + self.1 * other.1
    }
    pub fn unit(self) -> Self {
        let length = self.length();
        if length == 0. {
            self
        } else {
            Self(self.0 / length, self.1 / length)
        }
    }
    pub fn between(self, other: Self, t: f64) -> Self {
        self * (1. - t) + other * t
    }
    pub fn finite(self) -> bool {
        self.0.is_finite() && self.1.is_finite()
    }
}

pub(super) type Cubic = [Point; 4];

pub(super) fn segments(points: &[Point]) -> impl Iterator<Item = Cubic> + '_ {
    points.windows(4).step_by(3).map(|s| [s[0], s[1], s[2], s[3]])
}

pub(super) fn split([a, b, c, d]: Cubic, t: f64) -> (Cubic, Cubic) {
    let ab = a.between(b, t);
    let bc = b.between(c, t);
    let cd = c.between(d, t);
    let left = ab.between(bc, t);
    let right = bc.between(cd, t);
    let center = left.between(right, t);
    ([a, ab, left, center], [center, right, cd, d])
}

pub(super) fn at(curve: Cubic, t: f64) -> Point {
    split(curve, t).0[3]
}

fn max_components(differences: impl Iterator<Item = Point>) -> f64 {
    let max = differences.fold(Point::ZERO, |m, p| Point(m.0.max(p.0.abs()), m.1.max(p.1.abs())));
    max.length()
}

pub(super) fn curvature_steps(curve: Cubic, epsilon: f64) -> usize {
    if epsilon == 0. {
        return 1;
    }
    let delta = max_components(curve.windows(3).map(|p| p[2] - p[1] * 2. + p[0]));
    (0.75 * delta / epsilon).sqrt().ceil().max(1.) as usize
}

pub(super) fn sample_steps(curve: Cubic, tol: f64) -> usize {
    let speed = max_components(curve.windows(2).map(|p| p[1] - p[0])) * 3.;
    curvature_steps(curve, tol / 2.).max((speed / tol).ceil() as usize)
}

pub(super) fn ratio(part: f64, whole: f64) -> f64 {
    if whole == 0. { part } else { part / whole }
}

pub(super) fn projection(p: Point, a: Point, b: Point) -> Option<(f64, Point)> {
    let edge = b - a;
    let t = (p - a).dot(edge) / edge.dot(edge);
    (0. ..=1.).contains(&t).then(|| (t, a + edge * t))
}

pub(super) fn hit_distance(controls: &[Point], query: Point) -> f64 {
    segments(controls)
        .map(|c| {
            let dist = |t| (at(c, t) - query).length();
            let best = (0..=64)
                .min_by(|&a, &b| dist(a as f64 / 64.).total_cmp(&dist(b as f64 / 64.)))
                .unwrap();
            let mut lo = (best - 1).max(0) as f64 / 64.;
            let mut hi = (best + 1).min(64) as f64 / 64.;
            for _ in 0..40 {
                let step = (hi - lo) / 3.;
                if dist(lo + step) <= dist(hi - step) {
                    hi -= step;
                } else {
                    lo += step;
                }
            }
            dist((lo + hi) / 2.).min(dist(best as f64 / 64.))
        })
        .fold(f64::INFINITY, f64::min)
}
