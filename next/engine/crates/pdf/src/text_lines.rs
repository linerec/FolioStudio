use folio_model::{PathPoint, Point, SubPath};

/// Recover a straight stroke from a rectangular text decoration outline. Keeping the
/// stroke operator lets PDF rasterizers apply their thin-line treatment consistently.
pub(crate) fn horizontal_stroke(paths: &[SubPath]) -> Option<(f64, SubPath)> {
    let [path] = paths else { return None };
    if path.open || path.points.len() != 4 { return None; }
    let p = &path.points;
    if p.iter().any(|p| p.left != p.anchor || p.right != p.anchor) { return None; }
    let x0 = p.iter().map(|p| p.anchor.x).fold(f64::INFINITY, f64::min);
    let x1 = p.iter().map(|p| p.anchor.x).fold(f64::NEG_INFINITY, f64::max);
    let y0 = p.iter().map(|p| p.anchor.y).fold(f64::INFINITY, f64::min);
    let y1 = p.iter().map(|p| p.anchor.y).fold(f64::NEG_INFINITY, f64::max);
    if y1 <= y0 || x1 - x0 < y1 - y0 { return None; }
    for i in 0..4 {
        let (a, b) = (p[i].anchor, p[(i + 1) % 4].anchor);
        if (a.x != x0 && a.x != x1) || (a.y != y0 && a.y != y1) || (a.x != b.x && a.y != b.y) { return None; }
    }
    let point = |x| { let a = Point::new(x, (y0 + y1) / 2.); PathPoint { anchor: a, left: a, right: a } };
    Some((y1 - y0, SubPath { points: vec![point(x0), point(x1)], open: true }))
}
