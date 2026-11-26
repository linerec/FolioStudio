use super::*;
use crate::geom::{offset_curve, trim_corner};

#[derive(Clone, Debug)]
pub(super) struct Correspondence {
    source: PathMeasure,
    parallel: PathMeasure,
    source_start: f64,
    parallel_start: f64,
}

fn rounded_corner(center: Point, a: Point, b: Point) -> Vec<Seg> {
    let (u, v) = (sub(a, center), sub(b, center));
    let angle = cross(u, v).atan2(dot(u, v));
    let count = (angle.abs() / 0.03).ceil().max(1.0) as usize;
    let mut last = a;
    (1..=count)
        .map(|i| {
            let (sin, cos) = (angle * i as f64 / count as f64).sin_cos();
            let end = if i == count {
                b
            } else {
                add(
                    center,
                    Point::new(u.x * cos - u.y * sin, u.y * cos + u.x * sin),
                )
            };
            let edge = Seg::line(last, end);
            last = end;
            edge
        })
        .collect()
}

impl PathMeasure {
    pub fn offset(&self, height: f64) -> Option<Self> {
        Some(self.offset_with(height, OffsetJoin::Round)?.path)
    }

    pub fn offset_with(&self, height: f64, join: OffsetJoin) -> Option<Offset> {
        if !height.is_finite() {
            return None;
        }
        let n = self.segments.len();
        let mut sides: Vec<_> = self
            .segments
            .iter()
            .map(|s| offset_curve(s, height))
            .collect();
        let mut after = vec![Vec::new(); n];
        for i in (if self.closed { 0 } else { 1 })..n {
            let prev = (i + n - 1) % n;
            let a = sides[prev].last()?.d;
            let b = sides[i].first()?.a;
            if dist(a, b) < 1e-9 {
                continue;
            }
            let prev_dir = self.segments[prev].end_dir()?;
            let next_dir = self.segments[i].start_dir()?;
            let center = self.segments[i].a;
            if let Some((along, tip)) = geom::tangent_intersection(a, prev_dir, b, next_dir) {
                if along < 0.0 {
                    let (incoming, outgoing) = trim_corner(*sides[prev].last()?, sides[i][0], tip);
                    *sides[prev].last_mut()? = incoming;
                    sides[i][0] = outgoing;
                    continue;
                }
                if let OffsetJoin::Miter(limit) = join {
                    if dist(tip, center) / height.abs() < limit - 1e-10 {
                        after[prev] = vec![Seg::line(a, tip), Seg::line(tip, b)];
                        continue;
                    }
                }
            }
            after[prev] = match join {
                OffsetJoin::Round => rounded_corner(center, a, b),
                _ => vec![Seg::line(a, b)],
            };
        }
        let mut segments = Vec::new();
        let mut pieces = Vec::new();
        let (mut source_start, mut parallel_start) = (0.0, 0.0);
        for ((&s, side), corner) in self.segments.iter().zip(sides).zip(after) {
            let source = Self::from_segments(vec![s], false)?;
            let parallel = Self::from_segments(side.clone(), false)?;
            let next_start = parallel_start + parallel.length();
            let next_source = source_start + source.length();
            pieces.push(Correspondence {
                source,
                parallel,
                source_start,
                parallel_start,
            });
            segments.extend(side);
            parallel_start = next_start + corner.iter().map(|s| dist(s.a, s.d)).sum::<f64>();
            source_start = next_source;
            segments.extend(corner);
        }
        Some(Offset {
            height,
            path: Self::from_segments(segments, self.closed)?,
            pieces,
            length: self.length(),
        })
    }
}

impl Offset {
    fn split_laps(&self, d: f64, length: f64) -> (f64, f64) {
        if self.path.closed {
            ((d / length).floor(), d.rem_euclid(length))
        } else {
            (0.0, d.clamp(0.0, length))
        }
    }

    pub fn map(&self, d: f64) -> f64 {
        let (laps, d) = self.split_laps(d, self.length);
        let i = self
            .pieces
            .partition_point(|p| p.source_start <= d)
            .saturating_sub(1);
        let p = &self.pieces[i];
        let (q, t) = p.source.at(d - p.source_start);
        let q = add(q, mul(left(t), self.height));
        let hit = p.parallel.nearest(q);
        let mut near = (hit.distance, p.parallel_start + hit.at);
        for j in [
            i.checked_sub(1),
            (i + 1 < self.pieces.len()).then_some(i + 1),
        ]
        .into_iter()
        .flatten()
        {
            let adjacent = &self.pieces[j];
            let (first, next) = if j < i { (adjacent, p) } else { (p, adjacent) };
            let a = first.source.segments.last().and_then(Seg::end_dir);
            let b = next.source.segments.first().and_then(Seg::start_dir);
            if a.zip(b).is_some_and(|(a, b)| dot(a, b) > 1.0 - 1e-10) {
                let hit = adjacent.parallel.nearest(q);
                if hit.distance < near.0 {
                    near = (hit.distance, adjacent.parallel_start + hit.at);
                }
            }
        }
        laps * self.path.length() + near.1
    }

    pub fn unmap(&self, d: f64) -> f64 {
        let (laps, d) = self.split_laps(d, self.path.length());
        let i = self
            .pieces
            .partition_point(|p| p.parallel_start + p.parallel.length() < d);
        let at = if let Some(p) = self.pieces.get(i) {
            if d < p.parallel_start {
                p.source_start
            } else {
                let (q, t) = p.parallel.at(d - p.parallel_start);
                let q = add(q, mul(left(t), -self.height));
                p.source_start + p.source.nearest(q).at
            }
        } else {
            self.length
        };
        laps * self.length + at
    }
}
