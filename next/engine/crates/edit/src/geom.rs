//! Matrices, path points and numbers as IDML writes them.

/// IDML `ItemTransform` `a b c d tx ty`: (x, y) → (a·x + c·y + tx, b·x + d·y + ty).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat(pub [f64; 6]);

impl Mat {
    pub const ID: Mat = Mat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn parse(s: &str) -> Option<Mat> {
        let v: Vec<f64> = s.split_ascii_whitespace().map(|t| t.parse().ok()).collect::<Option<_>>()?;
        (v.len() == 6).then(|| Mat([v[0], v[1], v[2], v[3], v[4], v[5]]))
    }

    pub fn text(&self) -> String {
        self.0.iter().map(|&v| num(v)).collect::<Vec<_>>().join(" ")
    }

    /// `self ∘ o`: `o` first, then `self`.
    pub fn then_after(&self, o: &Mat) -> Mat {
        let [a1, b1, c1, d1, e1, f1] = self.0;
        let [a2, b2, c2, d2, e2, f2] = o.0;
        Mat([a1 * a2 + c1 * b2, b1 * a2 + d1 * b2, a1 * c2 + c1 * d2, b1 * c2 + d1 * d2, a1 * e2 + c1 * f2 + e1, b1 * e2 + d1 * f2 + f1])
    }

    pub fn apply(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// A vector in the space this maps to, brought back into the space it maps from.
    pub fn unapply_vector(&self, (x, y): (f64, f64)) -> Option<(f64, f64)> {
        let [a, b, c, d, _, _] = self.0;
        let det = a * d - b * c;
        (det.abs() > 1e-12).then(|| ((d * x - c * y) / det, (-b * x + a * y) / det))
    }

    pub fn invert(&self) -> Option<Mat> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() <= 1e-12 {
            return None;
        }
        let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
        Some(Mat([ia, ib, ic, id, -(ia * e + ic * f), -(ib * e + id * f)]))
    }
}

/// A number as InDesign writes it in IDML: the shortest text that reads back as the same
/// double (`5`, `-410.945`, `0.8660254037844387`), `0` for negative zero.
pub fn num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    format!("{v}")
}

pub fn parse_pair(s: &str) -> Option<(f64, f64)> {
    let mut it = s.split_ascii_whitespace().map(|t| t.parse::<f64>());
    let (x, y) = (it.next()?.ok()?, it.next()?.ok()?);
    it.next().is_none().then_some((x, y))
}

pub fn pair(p: (f64, f64)) -> String {
    format!("{} {}", num(p.0), num(p.1))
}

/// One `PathPointType`: anchor, left and right direction points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint {
    pub anchor: (f64, f64),
    pub left: (f64, f64),
    pub right: (f64, f64),
}

/// Bounds of a closed or open path of cubic segments (the curve, not its control points):
/// `[x0, y0, x1, y1]`.
pub fn path_bounds(paths: &[(Vec<PathPoint>, bool)]) -> Option<[f64; 4]> {
    let mut b: Option<[f64; 4]> = None;
    let mut add = |(x, y): (f64, f64)| {
        b = Some(match b {
            None => [x, y, x, y],
            Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
        });
    };
    for (pts, closed) in paths {
        if pts.is_empty() {
            continue;
        }
        add(pts[0].anchor);
        let n = pts.len();
        let segs = if *closed { n } else { n - 1 };
        for i in 0..segs {
            let (p, q) = (&pts[i], &pts[(i + 1) % n]);
            let c = [p.anchor, p.right, q.left, q.anchor];
            add(q.anchor);
            for axis in 0..2 {
                let v: [f64; 4] = c.map(|pt| if axis == 0 { pt.0 } else { pt.1 });
                for t in cubic_extrema(v) {
                    add(cubic_at(&c, t));
                }
            }
        }
    }
    b
}

fn cubic_at(c: &[(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * c[0].0 + b * c[1].0 + cc * c[2].0 + d * c[3].0, a * c[0].1 + b * c[1].1 + cc * c[2].1 + d * c[3].1)
}

/// Parameters in (0, 1) where one coordinate of a cubic has a zero derivative.
fn cubic_extrema([p0, p1, p2, p3]: [f64; 4]) -> Vec<f64> {
    // derivative / 3: a t² + b t + c
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let mut out = Vec::new();
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            out.push(-c / b);
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let s = disc.sqrt();
            out.push((-b + s) / (2.0 * a));
            out.push((-b - s) / (2.0 * a));
        }
    }
    out.retain(|t| *t > 0.0 && *t < 1.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_like_indesign() {
        assert_eq!(num(5.0), "5");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(-415.945 + 5.0), "-410.945");
        assert_eq!(num(56.848168735422206 + 10.0), "66.8481687354222");
        assert_eq!(num(-84.76537180435963 + 10.0), "-74.76537180435963");
        assert_eq!(Mat::parse("1 0 0 1 5 -415.945").unwrap().text(), "1 0 0 1 5 -415.945");
    }

    #[test]
    fn inverse_and_vectors() {
        let m = Mat([0.8660254037844387, -0.5, 0.5, 0.8660254037844387, 3.0, 4.0]);
        let p = m.apply((10.0, 20.0));
        let back = m.invert().unwrap().apply(p);
        assert!((back.0 - 10.0).abs() < 1e-9 && (back.1 - 20.0).abs() < 1e-9);
        let v = m.unapply_vector((1.0, 0.0)).unwrap();
        let (ax, ay) = (m.0[0] * v.0 + m.0[2] * v.1, m.0[1] * v.0 + m.0[3] * v.1);
        assert!((ax - 1.0).abs() < 1e-12 && ay.abs() < 1e-12);
    }

    #[test]
    fn curve_bounds_use_the_curve() {
        // a circle of radius 50 around (100, 100) as InDesign writes it
        let k = 27.614237491539665;
        let pt = |a: (f64, f64), l: (f64, f64), r: (f64, f64)| PathPoint { anchor: a, left: l, right: r };
        let circle = vec![
            pt((50.0, 100.0), (50.0, 100.0 + k), (50.0, 100.0 - k)),
            pt((100.0, 50.0), (100.0 - k, 50.0), (100.0 + k, 50.0)),
            pt((150.0, 100.0), (150.0, 100.0 - k), (150.0, 100.0 + k)),
            pt((100.0, 150.0), (100.0 + k, 150.0), (100.0 - k, 150.0)),
        ];
        let b = path_bounds(&[(circle, true)]).unwrap();
        for (got, want) in b.iter().zip([50.0, 50.0, 150.0, 150.0]) {
            assert!((got - want).abs() < 1e-9, "{b:?}");
        }
    }
}
