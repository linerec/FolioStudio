use crate::geom::{self, Seg, add, cross, dist, left, len, mul, sub};
use crate::{LineJoin, Point, SubPath, TextFrame};
use serde::{Deserialize, Serialize};

mod offset;
use offset::Correspondence;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HorizontalTextPath {
    pub frame: TextFrame,
    pub start: f64,
    pub end: f64,
    pub baseline: f64,
    pub path_alignment: PathAlignment,
    #[serde(default)]
    pub curve: Option<SubPath>,
    #[serde(default)]
    pub flipped: bool,
    #[serde(default)]
    pub effect: PathEffect,
    #[serde(default)]
    pub text_alignment: PathTextAlignment,
    #[serde(default)]
    pub spacing: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathAlignment {
    Top,
    #[default]
    Center,
    Bottom,
}

impl PathAlignment {
    pub fn height(self, weight: f64) -> f64 {
        match self {
            Self::Top => weight / 2.0,
            Self::Center => 0.0,
            Self::Bottom => -weight / 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathEffect {
    #[default]
    Rainbow,
    Skew,
    Ribbon,
    Stair,
    Gravity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathTextAlignment {
    #[default]
    Baseline,
    Ascender,
    Descender,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OffsetJoin {
    Round,
    Bevel,
    Miter(f64),
}

impl OffsetJoin {
    pub fn of_stroke(join: LineJoin, miter_limit: f64) -> Self {
        match join {
            LineJoin::Miter => Self::Miter(miter_limit),
            _ => Self::Bevel,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PathMeasure {
    samples: Vec<Sample>,
    segments: Vec<Seg>,
    closed: bool,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    point: Point,
    at: f64,
}

#[derive(Clone, Debug)]
pub struct Offset {
    height: f64,
    pub path: PathMeasure,
    pieces: Vec<Correspondence>,
    length: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Nearest {
    pub at: f64,
    pub distance: f64,
    pub side: f64,
}

fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}

// R1
fn divisions(s: &Seg) -> Option<usize> {
    if s.is_line() {
        return Some(1);
    }
    let second = |a, b, c| len(sub(sub(c, b), sub(b, a)));
    let bound = 6.0 * second(s.a, s.b, s.c).max(second(s.b, s.c, s.d));
    let count = (bound / 0.08).sqrt().ceil();
    (count.max(1.0) as usize).checked_next_power_of_two()
}

#[cfg(test)]
#[test]
fn large_curve_meets_the_same_chord_error_bound() {
    let origin = Point::new(0.0, 0.0);
    let far = Point::new(0.0, 100_000_000.0);
    let curve = Seg {
        a: origin,
        b: far,
        c: far,
        d: origin,
    };
    assert_eq!(divisions(&curve), Some(131_072));
}

impl PathMeasure {
    pub fn new(path: &SubPath) -> Option<Self> {
        if path.points.len() < 2
            || path.points.iter().any(|p| {
                [p.anchor, p.left, p.right]
                    .iter()
                    .any(|q| !q.x.is_finite() || !q.y.is_finite())
            })
        {
            return None;
        }
        Self::from_segments(geom::segments(path), !path.open)
    }

    fn from_segments(mut segments: Vec<Seg>, closed: bool) -> Option<Self> {
        segments.retain(|s| !(s.a == s.b && s.a == s.c && s.a == s.d));
        let mut last = Sample {
            point: segments.first()?.a,
            at: 0.0,
        };
        let mut samples = vec![last];
        for s in &segments {
            let n = divisions(s)?;
            samples.try_reserve(n).ok()?;
            for i in 1..=n {
                let point = s.eval(i as f64 / n as f64);
                let at = last.at + dist(last.point, point);
                if !at.is_finite() {
                    return None;
                }
                if at > last.at {
                    last = Sample { at, point };
                    samples.push(last);
                }
            }
        }
        (last.at > 0.0).then_some(Self {
            segments,
            samples,
            closed,
        })
    }

    pub fn length(&self) -> f64 {
        self.samples.last().unwrap().at
    }
    pub fn center(&self) -> Point {
        let mut lo = self.samples[0].point;
        let mut hi = lo;
        for s in &self.samples {
            lo.x = lo.x.min(s.point.x);
            lo.y = lo.y.min(s.point.y);
            hi.x = hi.x.max(s.point.x);
            hi.y = hi.y.max(s.point.y);
        }
        mul(add(lo, hi), 0.5)
    }

    pub fn is_clockwise(&self) -> bool {
        let origin = self.samples[0].point;
        self.samples
            .windows(2)
            .map(|w| cross(sub(w[0].point, origin), sub(w[1].point, origin)))
            .sum::<f64>()
            > 0.0
    }

    pub fn text_runs_backwards(&self, flipped: bool) -> bool {
        if self.closed {
            self.is_clockwise() == flipped
        } else {
            flipped
        }
    }

    pub fn at(&self, distance: f64) -> (Point, Point) {
        let d = if self.closed {
            distance.rem_euclid(self.length())
        } else {
            distance
        };
        let i = self
            .samples
            .partition_point(|s| s.at < d)
            .clamp(1, self.samples.len() - 1);
        let (a, b) = (self.samples[i - 1], self.samples[i]);
        let direction = mul(sub(b.point, a.point), 1.0 / dist(b.point, a.point));
        (add(a.point, mul(direction, d - a.at)), direction)
    }

    pub fn nearest(&self, q: Point) -> Nearest {
        let mut best = Nearest {
            at: 0.0,
            distance: f64::INFINITY,
            side: 0.0,
        };
        for w in self.samples.windows(2) {
            let (a, b) = (w[0], w[1]);
            let length = dist(a.point, b.point);
            let tangent = mul(sub(b.point, a.point), 1.0 / length);
            let delta = sub(q, a.point);
            let along = dot(delta, tangent).clamp(0.0, length);
            let error = sub(delta, mul(tangent, along));
            let distance = len(error);
            if distance < best.distance {
                best = Nearest {
                    at: a.at + along,
                    distance,
                    side: dot(error, left(tangent)),
                };
            }
        }
        best
    }
}
