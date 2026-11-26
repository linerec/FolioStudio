//! Point construction and vector operations.

use crate::Point;

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

pub(crate) fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
pub(crate) fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
pub(crate) fn mul(a: Point, k: f64) -> Point {
    Point::new(a.x * k, a.y * k)
}
pub(super) fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
pub(crate) fn len(a: Point) -> f64 {
    a.x.hypot(a.y)
}
pub(crate) fn dist(a: Point, b: Point) -> f64 {
    len(sub(a, b))
}
pub(super) fn unit(a: Point) -> Option<Point> {
    let l = len(a);
    (l > 1e-12).then(|| mul(a, 1.0 / l))
}
/// The left normal of a direction in y-down coordinates (left of the direction of travel).
pub(crate) fn left(d: Point) -> Point {
    Point::new(d.y, -d.x)
}

pub(crate) fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}
