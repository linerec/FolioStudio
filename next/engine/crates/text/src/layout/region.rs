use super::Column;
use folio_model::geom::{Seg, closed_contours};
use folio_model::{Affine, Point, SubPath};

pub(super) type Interval = (f64, f64);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Outline(pub(super) Vec<Vec<Seg>>);

fn map_segment(s: Seg, f: impl Fn(Point) -> Point) -> Seg {
    Seg {
        a: f(s.a),
        b: f(s.b),
        c: f(s.c),
        d: f(s.d),
    }
}

impl Outline {
    pub(super) fn new(paths: &[SubPath], distance: f64) -> Self {
        Self(closed_contours(paths, distance))
    }

    pub(super) fn map(&self, f: impl Fn(Point) -> Point) -> Self {
        Self(
            self.0
                .iter()
                .map(|ring| ring.iter().map(|&s| map_segment(s, &f)).collect())
                .collect(),
        )
    }

    pub(super) fn transform(&self, m: Affine) -> Self {
        self.map(|p| m.apply(p))
    }

    pub(super) fn polygonal(&self) -> Self {
        Self(
            self.0
                .iter()
                .map(|ring| {
                    ring.iter()
                        .flat_map(|s| {
                            let n = if s.is_line() { 1 } else { 128 };
                            (0..n).map(move |i| {
                                Seg::line(
                                    s.eval(i as f64 / n as f64),
                                    s.eval((i + 1) as f64 / n as f64),
                                )
                            })
                        })
                        .collect()
                })
                .collect(),
        )
    }
}

pub(super) fn rectangle(paths: &[SubPath], m: Affine) -> bool {
    let [p] = paths else { return false };
    if p.open || p.points.len() != 4 {
        return false;
    }
    let axes: Vec<_> = p
        .points
        .iter()
        .zip(p.points.iter().cycle().skip(1))
        .filter_map(|(a, b)| {
            if a.left != a.anchor || a.right != a.anchor {
                return None;
            }
            let (a, b) = (m.apply(a.anchor), m.apply(b.anchor));
            match ((a.x - b.x).abs() < 1e-8, (a.y - b.y).abs() < 1e-8) {
                (true, false) => Some(0),
                (false, true) => Some(1),
                _ => None,
            }
        })
        .collect();
    axes.len() == 4
        && axes
            .iter()
            .zip(axes.iter().cycle().skip(1))
            .all(|(a, b)| a != b)
}

// R3
fn chord_count(s: Seg) -> usize {
    if s.is_line() {
        return 1;
    }
    let difference = |a: f64, b: f64, c: f64| (a - 2.0 * b + c).abs();
    let dd = difference(s.a.x, s.b.x, s.c.x)
        .max(difference(s.b.x, s.c.x, s.d.x))
        .max(difference(s.a.y, s.b.y, s.c.y))
        .max(difference(s.b.y, s.c.y, s.d.y));
    let mut n = 1;
    while (n as f64).powi(2) < dd / 4.0 && n < 65536 {
        n *= 2;
    }
    n
}

fn clip_edge(ring: &[Seg], x: f64, keep_right: bool) -> Vec<Seg> {
    let inside = |p: Point| if keep_right { p.x >= x } else { p.x <= x };
    let mut kept = Vec::new();
    for &s in ring {
        let controls = [s.a, s.b, s.c, s.d];
        if controls.iter().all(|&p| inside(p)) {
            kept.push(s);
            continue;
        }
        if controls.iter().all(|&p| !inside(p)) {
            continue;
        }
        let n = chord_count(s);
        let mut start = (0.0, s.a);
        let mut previous = s.a;
        for i in 1..=n {
            let next = s.eval(i as f64 / n as f64);
            if inside(previous) != inside(next) {
                let fraction = (x - previous.x) / (next.x - previous.x);
                let t = ((i - 1) as f64 + fraction) / n as f64;
                let hit = Point::new(x, previous.y + fraction * (next.y - previous.y));
                if inside(previous) {
                    push_part(&mut kept, s, start, (t, hit));
                }
                start = (t, hit);
            }
            previous = next;
        }
        if inside(s.d) {
            push_part(&mut kept, s, start, (1.0, s.d));
        }
    }
    let mut closed = Vec::new();
    for (s, next) in kept.iter().zip(kept.iter().cycle().skip(1)) {
        closed.push(*s);
        if s.d != next.a {
            closed.push(Seg::line(s.d, next.a));
        }
    }
    closed
}

fn push_part(out: &mut Vec<Seg>, s: Seg, start: (f64, Point), end: (f64, Point)) {
    if end.0 <= start.0 {
        return;
    }
    if s.is_line() {
        out.push(Seg::line(start.1, end.1));
        return;
    }
    let part = s.sub(start.0, end.0);
    let shifted =
        |h: Point, old: Point, new: Point| Point::new(h.x + (new.x - old.x), h.y + (new.y - old.y));
    out.push(Seg {
        a: start.1,
        b: shifted(part.b, part.a, start.1),
        c: shifted(part.c, part.d, end.1),
        d: end.1,
    });
}

fn monotone(s: Seg) -> Vec<Seg> {
    let (p, q, r) = (s.b.y - s.a.y, s.c.y - s.b.y, s.d.y - s.c.y);
    if (p >= 0.0 && q >= 0.0 && r >= 0.0) || (p <= 0.0 && q <= 0.0 && r <= 0.0) {
        return vec![s];
    }
    let (a, b) = (p - 2.0 * q + r, q - p);
    let disc = b * b - a * p;
    let roots = if a.abs() < 1e-14 {
        [-0.5 * p / b, f64::NAN]
    } else {
        [(-b - disc.sqrt()) / a, (-b + disc.sqrt()) / a]
    };
    let mut cuts = vec![0.0, 1.0];
    cuts.extend(roots.into_iter().filter(|&t| t > 0.0 && t < 1.0));
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    cuts.windows(2).map(|w| s.sub(w[0], w[1])).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Sample {
    Column,
    Obstacle,
    Inverse,
}

fn crossing(s: &Seg, y: f64, sample: Sample) -> Option<f64> {
    if y < s.a.y.min(s.d.y) || y >= s.a.y.max(s.d.y) {
        return None;
    }
    if y == s.a.y {
        return Some(s.a.x);
    }
    if y == s.d.y {
        return Some(s.d.x);
    }
    if s.is_line() {
        let slope = (s.d.x - s.a.x) / (s.d.y - s.a.y);
        let x = s.a.x + (y - s.a.y) * slope;
        return Some(if sample == Sample::Column && slope > 0.0 {
            x.next_down()
        } else {
            x
        });
    }
    let increasing = s.a.y < s.d.y;
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..44 {
        let mid = (lo + hi) * 0.5;
        if (s.eval(mid).y < y) == increasing {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(s.eval((lo + hi) * 0.5).x)
}

pub(super) fn intersect(a: &[Interval], b: &[Interval]) -> Vec<Interval> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        let (l, r) = (a[i].0.max(b[j].0), a[i].1.min(b[j].1));
        if l <= r {
            out.push((l, r));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scan {
    rings: Vec<Vec<Seg>>,
    ymin: f64,
    ymax: f64,
}

impl Scan {
    pub(super) fn new(outline: &Outline) -> Self {
        let rings: Vec<Vec<_>> = outline
            .0
            .iter()
            .map(|ring| ring.iter().copied().flat_map(monotone).collect())
            .collect();
        let heights = rings.iter().flatten().flat_map(|s| [s.a.y, s.d.y]);
        let ymin = heights.clone().fold(f64::INFINITY, f64::min);
        let ymax = heights.fold(f64::NEG_INFINITY, f64::max);
        Self { rings, ymin, ymax }
    }

    fn intervals(&self, y: f64, sample: Sample) -> Vec<Interval> {
        let mut xs = Vec::new();
        for ring in &self.rings {
            let start = xs.len();
            xs.extend(ring.iter().filter_map(|s| crossing(s, y, sample)).map(|x| {
                if sample == Sample::Inverse {
                    x
                } else {
                    x.floor()
                }
            }));
            if (xs.len() - start) % 2 != 0 {
                xs.truncate(start);
            }
        }
        xs.sort_by(f64::total_cmp);
        xs.chunks_exact(2).map(|v| (v[0], v[1])).collect()
    }

    fn band(&self, top: f64, bottom: f64, width: f64) -> Vec<Interval> {
        if top < self.ymin - 1e-4 || bottom > self.ymax + 1e-4 {
            return Vec::new();
        }
        let mut rooms = vec![(0.0, width.floor())];
        let mut y = top;
        loop {
            let mut cut = self.intervals(y, Sample::Column);
            if cut.is_empty() {
                cut = self.intervals(if y == top { top + 1.0 } else { y - 0.5 }, Sample::Column);
            }
            rooms = intersect(&rooms, &cut);
            if rooms.is_empty() || y >= bottom {
                break;
            }
            y = (y.floor() + 1.0).min(bottom);
        }
        rooms
    }

    pub(super) fn inverse_room(&self, top: f64, bottom: f64, room: Interval) -> Vec<Interval> {
        let (top, bottom) = (top - 0.5, bottom + 0.5);
        let mut rooms = vec![room];
        for y in [top, bottom].into_iter().chain(
            self.rings
                .iter()
                .flatten()
                .map(|s| s.a.y)
                .filter(|&y| y > top && y < bottom),
        ) {
            rooms = intersect(&rooms, &self.intervals(y, Sample::Inverse));
            if rooms.is_empty() {
                break;
            }
        }
        rooms.iter_mut().for_each(|(l, r)| {
            *l = l.round();
            *r = r.round();
        });
        rooms
    }

    pub(super) fn blocked(&self, top: f64, bottom: f64) -> Vec<Interval> {
        let mut y = top.max(self.ymin);
        let end = bottom.min(self.ymax);
        if y > end {
            return Vec::new();
        }
        let mut pieces = Vec::new();
        loop {
            pieces.extend(self.intervals(y, Sample::Obstacle));
            if y >= end {
                break;
            }
            y = (y.floor() + 1.0).min(end);
        }
        pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<Interval> = Vec::new();
        for (left, right) in pieces {
            if let Some(last) = merged.last_mut().filter(|last| last.1 >= left) {
                last.1 = last.1.max(right);
            } else {
                merged.push((left, right));
            }
        }
        merged
    }
}

#[derive(Clone, Debug)]
pub struct Shape {
    outline: Outline,
    columns: Vec<(Column, Scan)>,
}

impl Shape {
    pub(super) fn new(outline: Outline, columns: Vec<Column>) -> Self {
        let columns = columns
            .into_iter()
            .map(|c| (c, Self::scan(&outline, &c)))
            .collect();
        Self { outline, columns }
    }

    fn scan(outline: &Outline, c: &Column) -> Scan {
        let grid = |v: f64| (v * 10000.0).trunc() / 10000.0;
        let rings = outline
            .0
            .iter()
            .map(|r| clip_edge(&clip_edge(r, c.left, true), c.right, false))
            .collect();
        Scan::new(&Outline(rings).map(|p| Point::new(grid(p.x - c.left), grid(p.y - c.top))))
    }

    pub(super) fn rooms(&self, c: &Column, top: f64, bottom: f64) -> Vec<Interval> {
        let fresh;
        let scan = match self
            .columns
            .iter()
            .find(|(k, _)| k.left == c.left && k.right == c.right && k.top == c.top)
        {
            Some((_, s)) => s,
            None => {
                fresh = Self::scan(&self.outline, c);
                &fresh
            }
        };
        scan.band(top - c.top, bottom - c.top, c.width())
            .into_iter()
            .map(|(l, r)| (l + c.left, r + c.left))
            .collect()
    }
}
