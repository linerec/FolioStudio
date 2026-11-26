//! Subpath bounds and conversion between path points and cubic segments.

use super::{Seg, point::dist};
use crate::{Point, Rect, SubPath};

impl SubPath {
    pub fn bounds(&self) -> Option<Rect> {
        let mut it = self.points.iter().flat_map(|p| [p.anchor, p.left, p.right]);
        let first = it.next()?;
        let mut r = Rect { left: first.x, top: first.y, right: first.x, bottom: first.y };
        for p in it {
            r.left = r.left.min(p.x);
            r.top = r.top.min(p.y);
            r.right = r.right.max(p.x);
            r.bottom = r.bottom.max(p.y);
        }
        Some(r)
    }
}

/// Bounds of a set of subpaths (inner coordinates).
pub fn paths_bounds(paths: &[SubPath]) -> Option<Rect> {
    paths.iter().filter_map(SubPath::bounds).reduce(|a, b| a.union(&b))
}

/// Segments of a subpath (a closed subpath gets its closing segment).
pub fn segments(sp: &SubPath) -> Vec<Seg> {
    let pts = &sp.points;
    let mut out = Vec::with_capacity(pts.len());
    for w in pts.windows(2) {
        out.push(Seg { a: w[0].anchor, b: w[0].right, c: w[1].left, d: w[1].anchor });
    }
    if !sp.open && pts.len() > 1 {
        let (l, f) = (pts.last().unwrap(), &pts[0]);
        // A closing segment of zero length (a path whose last point repeats the first) adds nothing.
        if !(l.anchor == f.anchor && l.right == l.anchor && f.left == f.anchor) {
            out.push(Seg { a: l.anchor, b: l.right, c: f.left, d: f.anchor });
        }
    }
    out
}

/// Builds subpaths from consecutive segments.
#[derive(Default)]
struct Builder {
    out: Vec<SubPath>,
    cur: Vec<crate::PathPoint>,
}

impl Builder {
    fn start(&mut self, p: Point) {
        self.finish(true);
        self.cur.push(crate::PathPoint { anchor: p, left: p, right: p });
    }
    fn seg(&mut self, s: &Seg) {
        match self.cur.last_mut() {
            Some(last) if dist(last.anchor, s.a) < 1e-9 => last.right = s.b,
            _ => {
                self.finish(true);
                self.cur.push(crate::PathPoint { anchor: s.a, left: s.a, right: s.b });
            }
        }
        self.cur.push(crate::PathPoint { anchor: s.d, left: s.c, right: s.d });
    }
    fn line_to(&mut self, p: Point) {
        let a = self.cur.last().map(|l| l.anchor).unwrap_or(p);
        self.seg(&Seg::line(a, p));
    }
    /// Ends the current subpath (`open`), or closes it onto its first point.
    fn finish(&mut self, open: bool) {
        if self.cur.is_empty() {
            return;
        }
        let mut pts = std::mem::take(&mut self.cur);
        if !open && pts.len() > 1 {
            let last = *pts.last().unwrap();
            if dist(last.anchor, pts[0].anchor) < 1e-9 {
                pts[0].left = last.left;
                pts.pop();
            }
        }
        self.out.push(SubPath { points: pts, open });
    }
    fn subpaths(mut self) -> Vec<SubPath> {
        self.finish(true);
        self.out
    }
}

pub(super) fn to_subpaths(segs: &[Seg], closed: bool) -> SubPath {
    let mut b = Builder::default();
    if let Some(first) = segs.first() {
        b.start(first.a);
    }
    for s in segs {
        if s.is_line() {
            b.line_to(s.d);
        } else {
            b.seg(s);
        }
    }
    b.finish(!closed);
    let mut v = b.out;
    v.pop().unwrap_or_default()
}
