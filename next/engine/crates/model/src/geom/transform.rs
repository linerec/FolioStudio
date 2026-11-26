//! Affine transforms of points and rectangle bounds.

use crate::{Affine, Point, Rect};

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };
    pub const fn new(a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64) -> Self {
        Self { a, b, c, d, tx, ty }
    }
    pub fn translate(tx: f64, ty: f64) -> Self {
        Self { tx, ty, ..Self::IDENTITY }
    }
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self { a: sx, d: sy, ..Self::IDENTITY }
    }
    pub fn apply(&self, p: Point) -> Point {
        Point { x: self.a * p.x + self.c * p.y + self.tx, y: self.b * p.x + self.d * p.y + self.ty }
    }
    /// `self` after `inner`: first `inner`, then `self` (parent ∘ child).
    pub fn then_inner(&self, inner: &Affine) -> Affine {
        Affine {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            tx: self.a * inner.tx + self.c * inner.ty + self.tx,
            ty: self.b * inner.tx + self.d * inner.ty + self.ty,
        }
    }
    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine { a, b, c, d, tx: -(a * self.tx + c * self.ty), ty: -(b * self.tx + d * self.ty) })
    }
    pub fn apply_rect(&self, r: &Rect) -> Rect {
        let pts = [
            self.apply(Point::new(r.left, r.top)),
            self.apply(Point::new(r.right, r.top)),
            self.apply(Point::new(r.right, r.bottom)),
            self.apply(Point::new(r.left, r.bottom)),
        ];
        let mut out = Rect { left: f64::MAX, top: f64::MAX, right: f64::MIN, bottom: f64::MIN };
        for p in pts {
            out.left = out.left.min(p.x);
            out.top = out.top.min(p.y);
            out.right = out.right.max(p.x);
            out.bottom = out.bottom.max(p.y);
        }
        out
    }
}
