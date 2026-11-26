use super::bezier::*;

#[derive(Clone, Copy)]
struct Sample {
    point: Point,
    length: f64,
    parameter: f64,
}

pub(super) struct Chain {
    pub controls: Vec<Point>,
    table: Vec<Sample>,
    pub tol: f64,
}

impl Chain {
    pub fn new(controls: Vec<Point>, tol: f64) -> Self {
        let mut last = Sample {
            point: controls[0],
            length: 0.,
            parameter: 0.,
        };
        let mut table = vec![last];
        for (i, c) in segments(&controls).enumerate() {
            let steps = sample_steps(c, tol);
            for j in 1..=steps {
                let t = j as f64 / steps as f64;
                let point = at(c, t);
                last = Sample {
                    point,
                    length: last.length + (point - last.point).length(),
                    parameter: i as f64 + t,
                };
                table.push(last);
            }
        }
        Self { controls, table, tol }
    }

    pub fn len(&self) -> f64 {
        self.table.last().unwrap().length
    }
    pub fn first(&self) -> Point {
        self.controls[0]
    }
    pub fn last(&self) -> Point {
        *self.controls.last().unwrap()
    }
    pub fn sample_points(&self) -> impl Iterator<Item = Point> + '_ {
        self.table.iter().map(|s| s.point)
    }
    pub fn tangent_start(&self) -> Point {
        self.controls.get(1).map_or(Point::ZERO, |p| (*p - self.first()) * 3.)
    }
    pub fn tangent_end(&self) -> Point {
        self.controls
            .iter()
            .rev()
            .nth(1)
            .map_or(Point::ZERO, |p| (self.last() - *p) * 3.)
    }
    fn parameter(&self, length: f64) -> f64 {
        let i = self.table.partition_point(|s| s.length < length);
        if i == self.table.len() {
            return self.table[i - 1].parameter;
        }
        let end = self.table[i];
        if i == 0 {
            return end.parameter;
        }
        let start = self.table[i - 1];
        if end.length == start.length {
            return end.parameter;
        }
        (length - start.length) / (end.length - start.length) * (end.parameter - start.parameter) + start.parameter
    }
    pub fn point(&self, length: f64) -> Point {
        let b = self.parameter(length);
        let i = b.floor() as usize * 3;
        if i + 3 >= self.controls.len() {
            return self.last();
        }
        at(self.controls[i..i + 4].try_into().unwrap(), b.fract())
    }
    pub fn nearest(&self, query: Point) -> f64 {
        let mut closest = f64::INFINITY;
        let mut length = 0.;
        let mut consider = |p: Point, l| {
            let distance = (query - p).length();
            if distance < closest {
                closest = distance;
                length = l;
            }
        };
        for pair in self.table.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            consider(a.point, a.length);
            if let Some((u, p)) = projection(query, a.point, b.point) {
                consider(p, (b.length - a.length) * u + a.length);
            }
        }
        consider(self.last(), self.len());
        length
    }
    pub fn direction(&self, length: f64) -> Point {
        let lo = (length - 2. * self.tol).max(0.);
        let hi = (length + 2. * self.tol).min(self.len());
        if hi == lo {
            Point::ZERO
        } else {
            (self.point(hi) - self.point(lo)) * (1. / (hi - lo))
        }
    }
    pub fn anchor_length(&self, length: f64, after: bool) -> f64 {
        let b = self.parameter(length);
        let anchor = if after { b.ceil() } else { b.floor() };
        let i = self.table.partition_point(|s| s.parameter < anchor);
        self.table[i].length
    }
    pub fn cut(&self, length: f64) -> (Self, Self) {
        let b = self.parameter(length);
        let i = b.floor() as usize * 3;
        let mut front = self.controls[..=i].to_vec();
        let mut back;
        if b.fract() == 0. {
            back = self.controls[i..].to_vec();
        } else {
            let (a, z) = split(self.controls[i..i + 4].try_into().unwrap(), b.fract());
            front.extend_from_slice(&a[1..]);
            back = z.to_vec();
            back.extend_from_slice(&self.controls[i + 4..]);
        }
        (Self::new(front, self.tol), Self::new(back, self.tol))
    }
    pub fn three_parts(&self, lo: f64, hi: f64) -> (Self, Self, Self) {
        let (front, remainder) = self.cut(lo);
        let (middle, back) = remainder.cut(ratio(hi - lo, self.len() - lo) * remainder.len());
        (front, middle, back)
    }
    pub fn reversed(mut self) -> Self {
        self.controls.reverse();
        Self::new(self.controls, self.tol)
    }
}

pub(super) fn merge(mut left: Vec<Point>, right: &[Point]) -> Vec<Point> {
    if let Some(last) = left.last_mut() {
        *last = (*last + right[0]) * 0.5;
        left.extend_from_slice(&right[1..]);
        left
    } else {
        right.to_vec()
    }
}
