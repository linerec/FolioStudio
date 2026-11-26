use super::bezier::*;
use super::samples::{Samples, lengths, window};

fn line(a: Point, b: Point) -> Cubic {
    [a, a.between(b, 1. / 3.), a.between(b, 2. / 3.), b]
}

struct Section<'a> {
    points: &'a [Point],
    lengths: &'a [f64],
}

impl<'a> Section<'a> {
    fn new(s: &'a Samples, a: usize, b: usize) -> Self {
        Self {
            points: &s.points[a..=b],
            lengths: &s.lengths[a..=b],
        }
    }
    fn fractions(&self) -> impl Iterator<Item = f64> + '_ {
        let a = self.lengths[0];
        let total = self.lengths.last().unwrap() - a;
        self.lengths.iter().map(move |s| ratio(s - a, total))
    }
    fn ends(&self) -> (Point, Point) {
        (self.points[0], *self.points.last().unwrap())
    }
    fn quadratic_handle(&self) -> Point {
        let (a, b) = self.ends();
        let mut sum = Point::ZERO;
        let mut square = 0.;
        for (&p, t) in self.points.iter().zip(self.fractions()) {
            let u = 1. - t;
            let w = 2. * t * u;
            sum = sum + (p - a * (u * u) - b * (t * t)) * w;
            square += w * w;
        }
        if square == 0. {
            (a + b) * 0.5
        } else {
            sum * (1. / square)
        }
    }
    fn cubic(&self, first: Point, last: Point) -> Cubic {
        let (a, b) = self.ends();
        match self.points.len() {
            1 | 2 => return line(a, b),
            3 => {
                let h = self.quadratic_handle();
                return [a, a.between(h, 2. / 3.), b.between(h, 2. / 3.), b];
            }
            _ => {}
        }
        // F7
        let mut axes = [Point::ZERO; 4];
        let mut handle = [0; 4];
        let mut base = [Point::ZERO; 2];
        let mut n = 0;
        for (i, (tangent, anchor)) in [(first, a), (last, b)].into_iter().enumerate() {
            if tangent == Point::ZERO {
                axes[n] = Point(1., 0.);
                axes[n + 1] = Point(0., 1.);
                handle[n..n + 2].fill(i);
                n += 2;
            } else {
                base[i] = anchor;
                axes[n] = tangent.unit();
                handle[n] = i;
                n += 1;
            }
        }
        let mut system = [[0.; 5]; 4];
        for (&p, t) in self.points.iter().zip(self.fractions()) {
            let u = 1. - t;
            let weights = [3. * u * u * t, 3. * u * t * t];
            let residual = p - a * (u * u * u) - b * (t * t * t) - base[0] * weights[0] - base[1] * weights[1];
            let columns = std::array::from_fn::<_, 4, _>(|j| axes[j] * weights[handle[j]]);
            for j in 0..n {
                for k in 0..n {
                    system[j][k] += columns[j].dot(columns[k]);
                }
                system[j][n] += columns[j].dot(residual);
            }
        }
        let Some(unknown) = solve(system, n) else {
            return line(a, b);
        };
        for j in 0..n {
            base[handle[j]] = base[handle[j]] + axes[j] * unknown[j];
        }
        [a, base[0], base[1], b]
    }
    fn accepts(&self, c: Cubic, tol: f64) -> bool {
        if self.points.len() <= 2 {
            return true;
        }
        let count = curvature_steps(c, tol / 16.);
        let poly = (0..=count).map(|i| at(c, i as f64 / count as f64)).collect::<Vec<_>>();
        let dist = lengths(&poly);
        let mut i = 0;
        self.points.iter().zip(self.fractions()).all(|(&p, u)| {
            while i < count && ratio(dist[i + 1], dist[count]) < u {
                i += 1;
            }
            (p - poly[i]).length() <= tol
                || (i < count
                    && dist[i + 1] != dist[i]
                    && ((p - poly[i + 1]).length() <= tol
                        || projection(p, poly[i], poly[i + 1])
                            .is_some_and(|(_, q)| (p - q).length() <= tol * 15. / 16.)))
        })
    }
}

fn solve(mut system: [[f64; 5]; 4], n: usize) -> Option<[f64; 4]> {
    for i in 0..n {
        let pivot = (i..n)
            .max_by(|&j, &k| system[j][i].abs().total_cmp(&system[k][i].abs()))
            .unwrap();
        system.swap(i, pivot);
        if system[i][i].abs() < 1e-24 {
            return None;
        }
        for j in i + 1..n {
            let factor = system[j][i] / system[i][i];
            #[allow(clippy::needless_range_loop, reason = "the pivot and target rows share the same column index")]
            for k in i + 1..=n {
                system[j][k] -= factor * system[i][k];
            }
        }
    }
    let mut result = [0.; 4];
    for i in (0..n).rev() {
        let mut value = system[i][n];
        for j in i + 1..n {
            value -= system[i][j] * result[j];
        }
        result[i] = value / system[i][i];
    }
    Some(result)
}

fn tangent(s: &Samples, i: usize, lo: usize, hi: usize, tol: f64) -> Point {
    let (a, b) = window(&s.lengths[lo..=hi], i - lo, 4. * tol);
    let (a, b) = (a + lo, b + lo);
    if s.lengths[a] == s.lengths[b] {
        return Point::ZERO;
    }
    let section = Section::new(s, a, b);
    let h = section.quadratic_handle();
    let t = (s.lengths[i] - s.lengths[a]) / (s.lengths[b] - s.lengths[a]);
    (h - s.points[a]).between(s.points[b] - h, t) * 2.
}

fn fit_section(s: &Samples, a: usize, b: usize, first: Point, last: Point, tol: f64, out: &mut Vec<Point>) {
    let section = Section::new(s, a, b);
    let c = section.cubic(first, last);
    if section.accepts(c, tol) {
        out.extend_from_slice(&c[1..]);
    } else {
        let mid = a + (b - a) / 2;
        let shared = tangent(s, mid, 0, s.points.len() - 1, tol);
        fit_section(s, a, mid, first, shared, tol, out);
        fit_section(s, mid, b, shared, last, tol, out);
    }
}

pub(super) fn fit(points: &[Point], tol: f64, first: Point, last: Point, sigma: f64) -> Vec<Point> {
    let s = Samples::prepare(points, tol, sigma);
    if s.points.len() < 2 {
        return s.points;
    }
    let end = s.points.len() - 1;
    let mut result = vec![s.points[0]];
    let mut a = 0;
    for b in s.corners.iter().copied().chain(std::iter::once(end)) {
        while a < b {
            let next = (a + 200).min(b);
            let start_tangent = if a == 0 && first != Point::ZERO {
                first
            } else {
                tangent(&s, a, if s.corners.binary_search(&a).is_ok() { a } else { 0 }, end, tol)
            };
            let end_tangent = if next == end && last != Point::ZERO {
                last
            } else {
                tangent(
                    &s,
                    next,
                    0,
                    if s.corners.binary_search(&next).is_ok() {
                        next
                    } else {
                        end
                    },
                    tol,
                )
            };
            fit_section(&s, a, next, start_tangent, end_tangent, tol, &mut result);
            a = next;
        }
    }
    result
}

pub(super) fn new_stroke(points: &[Point], tol: f64, smoothness: f64) -> Vec<Point> {
    let d = if smoothness < 1e-8 { 0. } else { smoothness * 0.4 + 40. };
    let curve = fit(
        points,
        tol,
        Point::ZERO,
        Point::ZERO,
        10f64.powf(-(100. - d) * 1.2 / 20.),
    );
    if curve.len() <= 4 {
        return curve;
    }
    let mut kept = Vec::new();
    for c in segments(&curve) {
        if c.iter().any(|p| (*p - c[0]).length() > 0.0283464566929) {
            kept.extend_from_slice(&c[..3]);
        }
    }
    kept.push(*curve.last().unwrap());
    kept
}
