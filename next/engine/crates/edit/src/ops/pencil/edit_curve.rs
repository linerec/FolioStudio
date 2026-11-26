use super::bezier::*;
use super::chain::{Chain, merge};
use super::fit::fit;
use super::join::{close_loop, join};

pub(super) struct Contact {
    start: f64,
    end: f64,
    distance: f64,
}
impl Contact {
    pub fn new(path: &Chain, first: Point, last: Point) -> Self {
        let start = path.nearest(first);
        Self {
            start,
            end: path.nearest(last),
            distance: (path.point(start) - first).length(),
        }
    }
    fn parts(&self, path: &Chain) -> (Chain, Chain, Chain) {
        path.three_parts(self.start.min(self.end), self.start.max(self.end))
    }
    pub fn distant(&self) -> bool {
        self.distance >= 12.
    }
    pub fn erase(&self, path: &Chain, drag: &[Point], closed: bool) -> Vec<Vec<Point>> {
        let (head, tail) = if self.distant() {
            if closed {
                return vec![path.controls.clone()];
            }
            let b = Chain::new(fit(drag, path.tol, Point::ZERO, Point::ZERO, 0.), path.tol);
            let (front, back) = path.cut(self.end);
            let kept = if b.direction(b.len()).dot(path.direction(self.end)) >= 0. {
                back
            } else {
                front
            };
            return [kept]
                .into_iter()
                .filter(|c| c.len() > 0.)
                .map(|c| c.controls)
                .collect();
        } else {
            let (front, _, back) = self.parts(path);
            (front, back)
        };
        if closed && head.len() > 0. && tail.len() > 0. {
            return vec![merge(tail.controls, &head.controls)];
        }
        [head, tail]
            .into_iter()
            .filter(|c| c.len() > 0.)
            .map(|c| c.controls)
            .collect()
    }
    pub fn smooth(&self, path: &Chain) -> Chain {
        let (front, middle, back) = self.parts(path);
        let first = if front.len() > 0. {
            middle.tangent_start()
        } else {
            Point::ZERO
        };
        let last = if back.len() > 0. {
            middle.tangent_end()
        } else {
            Point::ZERO
        };
        let refit = fit(
            &middle.sample_points().collect::<Vec<_>>(),
            path.tol,
            first,
            last,
            72.,
        );
        join(join(front, Chain::new(refit, path.tol)), back)
    }
}

fn align_start(path: Chain, drag: &Chain) -> (Chain, f64, f64, bool) {
    let start = path.nearest(drag.first());
    let distance = (path.point(start) - drag.first()).length();
    let reverse = drag.direction(0.).dot(path.direction(start)) < 0.;
    if reverse {
        let fraction = ratio(path.len() - start, path.len());
        let path = path.reversed();
        let start = fraction * path.len();
        (path, start, distance, true)
    } else {
        (path, start, distance, false)
    }
}

pub(super) fn extend(path: Chain, mut drag: Chain, reach: f64) -> Chain {
    let (mut path, mut start, mut distance, mut reversed) = align_start(path, &drag);
    if distance >= reach || start <= 3. * distance {
        drag = drag.reversed();
        let aligned = align_start(path, &drag);
        (path, start, distance) = (aligned.0, aligned.1, aligned.2);
        reversed ^= aligned.3;
    }
    if distance >= reach || start <= 3. * distance {
        return drag;
    }
    let end = path.nearest(drag.last());
    let gap = (path.point(end) - drag.last()).length();
    let tail_length = if start <= end {
        path.len() - end
    } else {
        drag.len() + (start - end)
    };
    let attach = gap < 1.5 * reach
        && tail_length > 3. * gap
        && drag.direction(drag.len()).dot(path.direction(end)) >= 0.;
    let (front, back) = path.cut(start);
    let result = if !attach {
        join(front, drag)
    } else if start <= end {
        let (_, tail) = back.cut(ratio(end - start, path.len() - start) * back.len());
        join(join(front, drag), tail)
    } else {
        let (_, middle) = front.cut(ratio(end, start) * front.len());
        close_loop(join(middle, drag))
    };
    if reversed { result.reversed() } else { result }
}
