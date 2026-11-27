//! Page coordinates, tight Bézier bounds and hit-test outlines for inspection.

use super::{r3, round};
use folio_model::{Affine, ItemKind, Page, PageItem, Point, Rect, Spread, SubPath};

pub(super) fn rect4(r: &Rect) -> [f64; 4] {
    [r3(r.left), r3(r.top), r3(r.right), r3(r.bottom)]
}

pub(super) fn affine6(t: &Affine) -> [f64; 6] {
    [round(t.a, 6), round(t.b, 6), round(t.c, 6), round(t.d, 6), r3(t.tx), r3(t.ty)]
}

pub(super) fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(&b)),
        (a, b) => a.or(b),
    }
}

/// A cubic segment through `t` (straight segments have their handles on their ends).
fn segment(t: &Affine, p: &folio_model::PathPoint, q: &folio_model::PathPoint) -> folio_model::geom::Seg {
    folio_model::geom::Seg { a: t.apply(p.anchor), b: t.apply(p.right), c: t.apply(q.left), d: t.apply(q.anchor) }
}

/// The subpath's segments through `t`, the closing one included for a closed subpath.
fn segments(sp: &SubPath, t: &Affine) -> impl Iterator<Item = folio_model::geom::Seg> {
    let n = sp.points.len();
    let count = if n < 2 { 0 } else if sp.open { n - 1 } else { n };
    (0..count).map(move |i| segment(t, &sp.points[i], &sp.points[(i + 1) % n]))
}

/// Parameters in (0, 1) where one coordinate of a cubic has an extreme.
fn extremes(p0: f64, p1: f64, p2: f64, p3: f64) -> impl Iterator<Item = f64> {
    // derivative / 3 = a t² + b t + c
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let mut out = [f64::NAN; 2];
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            out[0] = -c / b;
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let s = disc.sqrt();
            out = [(-b + s) / (2.0 * a), (-b - s) / (2.0 * a)];
        }
    }
    out.into_iter().filter(|t| *t > 0.0 && *t < 1.0)
}

fn include(r: &mut Option<Rect>, p: Point) {
    let pr = Rect { left: p.x, top: p.y, right: p.x, bottom: p.y };
    *r = Some(r.map_or(pr, |r| r.union(&pr)));
}

/// Tight bounds of paths through `t` (curves at their extremes, not their handles).
pub(super) fn paths_bounds(paths: &[SubPath], t: &Affine) -> Option<Rect> {
    let mut anchors = None;
    for p in paths.iter().flat_map(|sp| &sp.points) {
        include(&mut anchors, t.apply(p.anchor));
    }
    let a = anchors?;
    // A curve stays inside the hull of its points: only a segment with a handle outside the
    // anchors' box can reach beyond it.
    let inside = |p: Point| p.x >= a.left && p.x <= a.right && p.y >= a.top && p.y <= a.bottom;
    let mut r = anchors;
    for sp in paths {
        for s in segments(sp, t) {
            if s.is_line() || (inside(s.b) && inside(s.c)) {
                continue;
            }
            for u in extremes(s.a.x, s.b.x, s.c.x, s.d.x).chain(extremes(s.a.y, s.b.y, s.c.y, s.d.y)) {
                include(&mut r, s.eval(u));
            }
        }
    }
    r
}

/// Geometric bounds of an item through `t` (its inner → target): its paths, or for a group
/// its members'. Strokes are not included.
pub(super) fn geometry_bounds(item: &PageItem, t: &Affine) -> Option<Rect> {
    match &item.kind {
        ItemKind::Group { children } => children.iter().fold(None, |acc, c| union(acc, geometry_bounds(c, &t.then_inner(&c.transform)))),
        _ => paths_bounds(&item.paths, t),
    }
}

/// Subpaths with more anchors than this are outlined by their anchors alone.
const FLATTEN_MAX_ANCHORS: usize = 16;
/// Most points listed for one subpath; detailed art keeps evenly spaced anchors (enough to
/// hit-test it; the bounds stay exact).
const OUTLINE_MAX_POINTS: usize = 64;

pub(super) type Outline = Vec<Vec<[f64; 2]>>;

/// The outline through `t`: each subpath's anchors, the curved segments of a simple subpath
/// flattened into a few points, at most [`OUTLINE_MAX_POINTS`] per subpath; `None` for a
/// plain rectangle (its bounds say it all).
pub(super) fn outline(paths: &[SubPath], t: &Affine, bounds: Option<&Rect>) -> Option<Outline> {
    if paths.is_empty() {
        return None;
    }
    if let (Some(b), [sp]) = (bounds, paths)
        && !sp.open
        && sp.points.len() == 4
        && sp.points.iter().all(|p| p.left == p.anchor && p.right == p.anchor)
    {
        let on_corner = |p: Point| {
            let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
            (near(p.x, b.left) || near(p.x, b.right)) && (near(p.y, b.top) || near(p.y, b.bottom))
        };
        if sp.points.iter().all(|p| on_corner(t.apply(p.anchor))) {
            return None;
        }
    }
    let mut out = Vec::with_capacity(paths.len());
    for sp in paths {
        let Some(first) = sp.points.first() else { continue };
        let n = sp.points.len();
        // detailed art: its anchors, evenly spaced ones when there are many
        if n > FLATTEN_MAX_ANCHORS {
            let step = n.div_ceil(OUTLINE_MAX_POINTS).max(1);
            let kept = sp.points.iter().enumerate().filter(|(i, _)| i % step == 0 || (sp.open && *i == n - 1));
            out.push(kept.map(|(_, p)| t.apply(p.anchor)).map(|p| [r3(p.x), r3(p.y)]).collect());
            continue;
        }
        // simple shapes (ovals, rounded corners): their curves flattened
        let mut pts = vec![t.apply(first.anchor)];
        for (i, s) in segments(sp, t).enumerate() {
            if !s.is_line() {
                let hull = (s.b.x - s.a.x).hypot(s.b.y - s.a.y)
                    + (s.c.x - s.b.x).hypot(s.c.y - s.b.y)
                    + (s.d.x - s.c.x).hypot(s.d.y - s.c.y);
                let steps = (hull / 6.0).ceil().clamp(2.0, 8.0) as usize;
                for k in 1..steps {
                    pts.push(s.eval(k as f64 / steps as f64));
                }
            }
            // the closing segment ends where the subpath started
            if i + 1 < n {
                pts.push(s.d);
            }
        }
        let step = pts.len().div_ceil(OUTLINE_MAX_POINTS).max(1);
        let last = pts.len() - 1;
        let kept = pts.iter().enumerate().filter(|(i, _)| i % step == 0 || (sp.open && *i == last));
        out.push(kept.map(|(_, p)| [r3(p.x), r3(p.y)]).collect());
    }
    Some(out)
}

/// Pages of a spread in spread coordinates.
pub(super) fn page_rects(spread: &Spread) -> Vec<Rect> {
    spread.pages.iter().map(|p| p.transform.apply_rect(&p.bounds)).collect()
}

/// The page that holds point `c`: the first whose bounds contain it, else the nearest.
pub(super) fn page_for(rects: &[Rect], c: Point) -> usize {
    if let Some(i) = rects.iter().position(|r| c.x >= r.left && c.x <= r.right && c.y >= r.top && c.y <= r.bottom) {
        return i;
    }
    let dist = |r: &Rect| {
        let dx = (r.left - c.x).max(c.x - r.right).max(0.0);
        let dy = (r.top - c.y).max(c.y - r.bottom).max(0.0);
        dx * dx + dy * dy
    };
    (0..rects.len()).min_by(|&a, &b| dist(&rects[a]).total_cmp(&dist(&rects[b]))).unwrap_or(0)
}

/// Left or right of the spine (x = 0 in spread coordinates), as `folio_render` decides it;
/// "single" without facing pages.
pub(super) fn side(facing: bool, spread: &Spread, page: &Page) -> &'static str {
    if !facing {
        return "single";
    }
    let r = page.transform.apply_rect(&page.bounds);
    let right = if spread.pages.len() == 1 { r.left >= -1.0 } else { (r.left + r.right) / 2.0 > 0.0 };
    if right { "right" } else { "left" }
}
