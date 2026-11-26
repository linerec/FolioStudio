//! Axis-aligned rectangle dimensions, intersection, and union.

use crate::Rect;

impl Rect {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
    /// From IDML `GeometricBounds="y1 x1 y2 x2"`.
    pub fn from_idml_bounds(v: [f64; 4]) -> Self {
        Self { top: v[0], left: v[1], bottom: v[2], right: v[3] }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect {
            left: self.left.min(o.left),
            top: self.top.min(o.top),
            right: self.right.max(o.right),
            bottom: self.bottom.max(o.bottom),
        }
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.left < o.right && o.left < self.right && self.top < o.bottom && o.top < self.bottom
    }
}
