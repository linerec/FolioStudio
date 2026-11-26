use folio_model::{Affine, DirectionalFeather, FollowShape, PageItem, Point, Rect};

use super::super::{Alpha, Part, Shapes, noise, silhouette_through};
use super::square;

// R18
struct Ramp {
    lo: u64,
    hi: u64,
}

impl Ramp {
    fn from_width(width: f64, choke: f64) -> Option<Self> {
        if width <= 1e-8 {
            return None;
        }
        let edge = (width + 1.0) as f32;
        let lo = (edge * choke.clamp(0.0, 1.0) as f32 * 256.0) as u64;
        let hi = (edge * 256.0) as u64;
        Some(Self { lo, hi })
    }

    fn at(&self, pixels: usize, coverage: u8) -> u8 {
        let v = pixels as u64 * 256 + u64::from(coverage);
        if v >= self.hi {
            255
        } else if v <= self.lo {
            0
        } else {
            square(((v - self.lo) * 255 / (self.hi - self.lo)) as u32)
        }
    }
}

#[derive(Clone, Copy)]
struct Run {
    start: usize,
    end: usize,
    first: u8,
    last: u8,
}

fn covered_bounds(a: &Alpha) -> Option<[Run; 2]> {
    let mut left = a.w;
    let mut right = 0;
    let mut top = a.h;
    let mut bottom = 0;
    for (y, row) in a.data.chunks_exact(a.w).enumerate() {
        if let Some(x) = row.iter().position(|&v| v != 0) {
            left = left.min(x);
            right = right.max(row.iter().rposition(|&v| v != 0).unwrap());
            top = top.min(y);
            bottom = y;
        }
    }
    (left < a.w).then_some([
        Run {
            start: left,
            end: right,
            first: 255,
            last: 255,
        },
        Run {
            start: top,
            end: bottom,
            first: 255,
            last: 255,
        },
    ])
}

fn for_each_run(
    len: usize,
    at: impl Fn(usize) -> u8,
    follow: FollowShape,
    bounds: Option<Run>,
    mut emit: impl FnMut(Run),
) {
    match follow {
        FollowShape::FirstEdge => {
            if let Some(run) = bounds {
                emit(run);
            }
        }
        FollowShape::LeadingEdge => {
            if let Some(start) = (0..len).find(|&i| at(i) != 0) {
                let end = (start..len).rfind(|&i| at(i) != 0).unwrap();
                emit(Run {
                    start,
                    end,
                    first: at(start),
                    last: at(end),
                });
            }
        }
        FollowShape::AllEdges => {
            let mut i = 0;
            while i < len {
                if at(i) == 0 {
                    i += 1;
                    continue;
                }
                let start = i;
                i += 1;
                while i < len && at(i) != 0 {
                    i += 1;
                }
                let end = i - 1;
                emit(Run {
                    start,
                    end,
                    first: at(start),
                    last: at(end),
                });
            }
        }
    }
}

fn axis(
    a: &Alpha,
    follow: FollowShape,
    bounds: Option<Run>,
    leading: Option<&Ramp>,
    trailing: Option<&Ramp>,
    vertical: bool,
) -> Option<Vec<u8>> {
    if leading.is_none() && trailing.is_none() {
        return None;
    }
    let (lines, len) = if vertical { (a.w, a.h) } else { (a.h, a.w) };
    let mut values = vec![0; a.data.len()];
    for line in 0..lines {
        let index = |i| {
            if vertical {
                i * a.w + line
            } else {
                line * a.w + i
            }
        };
        for_each_run(
            len,
            |i| a.data[index(i)],
            follow,
            bounds,
            |run| {
                for i in run.start..=run.end {
                    let lo = leading.map_or(255, |r| r.at(i - run.start, run.first));
                    let hi = trailing.map_or(255, |r| r.at(run.end - i, run.last));
                    values[index(i)] = lo.min(hi);
                }
            },
        );
    }
    Some(values)
}

fn feather_lines(a: &mut Alpha, ramps: &[Option<Ramp>; 4], follow: FollowShape) {
    let bounds = if follow == FollowShape::FirstEdge {
        covered_bounds(a)
    } else {
        None
    };
    let rows = axis(
        a,
        follow,
        bounds.map(|b| b[0]),
        ramps[0].as_ref(),
        ramps[1].as_ref(),
        false,
    );
    let cols = axis(
        a,
        follow,
        bounds.map(|b| b[1]),
        ramps[2].as_ref(),
        ramps[3].as_ref(),
        true,
    );
    match (rows, cols) {
        (Some(mut rows), Some(cols)) => {
            for (row, col) in rows.iter_mut().zip(cols) {
                *row = (*row).min(col);
            }
            a.data = rows;
        }
        (Some(values), None) | (None, Some(values)) => a.data = values,
        (None, None) => {}
    }
}

fn sample(a: &Alpha, x: f64, y: f64) -> u8 {
    let (ix, iy) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = (x - ix as f64, y - iy as f64);
    let at = |i: isize, j: isize| {
        if i >= 0 && i < a.w as isize && j >= 0 && j < a.h as isize {
            f64::from(a.data[j as usize * a.w + i as usize])
        } else {
            0.0
        }
    };
    let top = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
    let bottom = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
    (top * (1.0 - fy) + bottom * fy).round() as u8
}

pub(super) fn apply(
    item: &PageItem,
    shapes: &Shapes<'_, '_>,
    a: &mut Alpha,
    ext: &Rect,
    ppu: f64,
    d: &DirectionalFeather,
    part: Part,
) {
    let res = f64::from((ppu * 72.0) as f32);
    let ramps =
        [d.left, d.right, d.top, d.bottom].map(|pt| Ramp::from_width(res * pt / 72.0, d.choke));
    if d.angle == 0.0 || ramps.iter().all(Option::is_none) {
        feather_lines(a, &ramps, d.follow);
        noise::feather(a, d.noise);
        return;
    }
    let cx = (ext.left + ext.right) / 2.0;
    let cy = (ext.top + ext.bottom) / 2.0;
    let (sin, cos) = d.angle.to_radians().sin_cos();
    let turn = Affine::translate(cx, cy)
        .then_inner(&Affine::new(cos, sin, -sin, cos, 0.0, 0.0))
        .then_inner(&Affine::translate(-cx, -cy));
    let rect = turn.apply_rect(&a.rect);
    let Some(mut turned) = silhouette_through(item, shapes, &rect, ppu, &turn, part) else {
        return;
    };
    feather_lines(&mut turned, &ramps, d.follow);
    noise::feather(&mut turned, d.noise);
    for (y, row) in a.data.chunks_exact_mut(a.w).enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            let p = turn.apply(Point::new(
                a.rect.left + (x as f64 + 0.5) / ppu,
                a.rect.top + (y as f64 + 0.5) / ppu,
            ));
            *value = sample(
                &turned,
                (p.x - rect.left) * ppu - 0.5,
                (p.y - rect.top) * ppu - 0.5,
            );
        }
    }
}
