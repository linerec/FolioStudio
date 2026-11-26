//! Object ▸ Pathfinder ▸ Add, Subtract, Intersect, Exclude Overlap, Minus Back on two closed paths of straight segments
//! (oracle: basic-paths-pathfinder, the menu actions on two overlapping squares):
//! - the result is a Polygon with the settings of one of the two objects: Subtract takes the rearmost, the others the front one;
//!   the other object is gone. The result takes the place of the object it follows in the stack;
//! - the paths turn counter-clockwise on the page (a negative shoelace sum); Exclude Overlap and a hole make several paths of
//!   one object, the front object's part first;
//! - NOT the same as InDesign: where a path starts, and so which point is first, is InDesign's own and differs here (known gap
//!   in known-gaps.json "oracle"); the region, the points and the way round are the same.
//! - curves are not built: a path with handles is refused.
//!
//! The method: both boundaries are cut at their crossings; each piece lies inside or outside the other path; the pieces the
//! operation keeps (turned round where it takes the inside of a path away) join into the new boundaries.
use super::*;
use compound::{path_xml, subpaths};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathfinderOp {
    Add,
    Subtract,
    Intersect,
    ExcludeOverlap,
    MinusBack,
}

type P = (f64, f64);
const EPS: f64 = 1e-7;

fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

/// The shoelace sum: negative for a path that turns counter-clockwise on the page.
fn shoelace(poly: &[P]) -> f64 {
    (0..poly.len()).map(|i| cross(poly[i], poly[(i + 1) % poly.len()])).sum::<f64>() / 2.0
}

/// A point inside a path (even-odd); `None` when it lies on the boundary.
fn inside(poly: &[P], p: P) -> Option<bool> {
    let mut hit = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let ab = (b.0 - a.0, b.1 - a.1);
        let len2 = ab.0 * ab.0 + ab.1 * ab.1;
        let t = if len2 > 0.0 { (((p.0 - a.0) * ab.0 + (p.1 - a.1) * ab.1) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let near = (a.0 + ab.0 * t, a.1 + ab.1 * t);
        if (near.0 - p.0).hypot(near.1 - p.1) < 1e-6 {
            return None;
        }
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            hit = !hit;
        }
    }
    Some(hit)
}

/// The edges of `poly` cut where `other` crosses them: each piece with whether it lies inside `other`.
fn pieces(poly: &[P], other: &[P]) -> Result<Vec<(P, P, bool)>, EditError> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let r = (q.0 - p.0, q.1 - p.1);
        let mut cuts = vec![0.0, 1.0];
        for j in 0..other.len() {
            let (a, b) = (other[j], other[(j + 1) % other.len()]);
            let s = (b.0 - a.0, b.1 - a.1);
            let rxs = cross(r, s);
            if rxs.abs() < 1e-12 {
                continue;
            }
            let ap = (a.0 - p.0, a.1 - p.1);
            let (t, u) = (cross(ap, s) / rxs, cross(ap, r) / rxs);
            if (-EPS..=1.0 + EPS).contains(&u) && t > EPS && t < 1.0 - EPS {
                cuts.push(t);
            }
        }
        cuts.sort_by(|a, b| a.total_cmp(b));
        cuts.dedup_by(|a, b| (*a - *b).abs() < EPS);
        for w in cuts.windows(2) {
            let at = |t: f64| (p.0 + r.0 * t, p.1 + r.1 * t);
            let (from, to) = (at(w[0]), at(w[1]));
            let mid = at((w[0] + w[1]) / 2.0);
            let is_in = inside(other, mid).ok_or_else(|| EditError::new("unsupported", "the two paths share a stretch of their boundary: not built yet"))?;
            out.push((from, to, is_in));
        }
    }
    Ok(out)
}

fn key(p: P) -> (i64, i64) {
    ((p.0 * 1e5).round() as i64, (p.1 * 1e5).round() as i64)
}

/// The kept pieces joined into closed boundaries, without the points that lie on a straight stretch.
fn link(edges: &[(P, P)]) -> Vec<Vec<P>> {
    let mut from: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, e) in edges.iter().enumerate() {
        from.entry(key(e.0)).or_default().push(k);
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for s in 0..edges.len() {
        if used[s] {
            continue;
        }
        let mut pts = Vec::new();
        let mut k = s;
        loop {
            used[k] = true;
            pts.push(edges[k].0);
            let next = from.get(&key(edges[k].1)).and_then(|v| v.iter().copied().find(|&n| !used[n]));
            match next {
                Some(n) => k = n,
                None => break,
            }
        }
        // drop the points on a straight stretch
        let n = pts.len();
        let keep: Vec<P> = (0..n)
            .filter(|&i| {
                let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
                cross((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1)).abs() > 1e-9
            })
            .map(|i| pts[i])
            .collect();
        if keep.len() >= 3 {
            loops.push(keep);
        }
    }
    loops
}

/// The boundaries of `a` (rearmost) and `b` (front) under `op`.
pub(crate) fn combine(a: &[P], b: &[P], op: PathfinderOp) -> Result<Vec<Vec<P>>, EditError> {
    // both turn counter-clockwise on the page
    let ccw = |p: &[P]| if shoelace(p) > 0.0 { p.iter().rev().copied().collect::<Vec<P>>() } else { p.to_vec() };
    let (a, b) = (ccw(a), ccw(b));
    let (pa, pb) = (pieces(&a, &b)?, pieces(&b, &a)?);
    let pick = |ps: &[(P, P, bool)], want_inside: bool, turn: bool| -> Vec<(P, P)> {
        ps.iter().filter(|e| e.2 == want_inside).map(|e| if turn { (e.1, e.0) } else { (e.0, e.1) }).collect()
    };
    let mut edges: Vec<(P, P)> = Vec::new();
    match op {
        PathfinderOp::Add => {
            edges.extend(pick(&pa, false, false));
            edges.extend(pick(&pb, false, false));
        }
        PathfinderOp::Intersect => {
            edges.extend(pick(&pa, true, false));
            edges.extend(pick(&pb, true, false));
        }
        PathfinderOp::Subtract => {
            edges.extend(pick(&pa, false, false));
            edges.extend(pick(&pb, true, true));
        }
        PathfinderOp::MinusBack => {
            edges.extend(pick(&pb, false, false));
            edges.extend(pick(&pa, true, true));
        }
        PathfinderOp::ExcludeOverlap => {
            edges.extend(pick(&pb, false, false));
            edges.extend(pick(&pa, true, true));
            edges.extend(pick(&pa, false, false));
            edges.extend(pick(&pb, true, true));
        }
    }
    let loops = link(&edges);
    if loops.is_empty() {
        return Err(EditError::new("invalid", "nothing is left of the paths"));
    }
    Ok(loops)
}

fn check(ix: &Index, t: &str, i: usize, id: &str) -> Result<(), EditError> {
    check_item(ix, t, i, id)?;
    let tag = ix.local(t, i);
    if !matches!(tag, "Rectangle" | "Oval" | "Polygon") {
        return Err(EditError::new("unsupported", format!("{id} ({tag}): only closed shapes can be combined")));
    }
    if ix.nodes[i].parent.is_none_or(|p| ix.local(t, p) != "Spread") {
        return Err(EditError::new("unsupported", format!("{id} is inside a group")));
    }
    if ix.children(i).iter().any(|&c| GRAPHICS.contains(&ix.local(t, c))) {
        return Err(EditError::new("unsupported", format!("{id} holds a graphic")));
    }
    if ix.attr(t, i, "Locked") == Some("true") {
        return Err(EditError::new("locked", format!("{id} is locked")));
    }
    Ok(())
}

pub(crate) fn run(w: &mut Work<'_>, ids: &[String], kind: &PathfinderOp) -> Result<(), EditError> {
    let op = kind;
    let mut want: Vec<&String> = Vec::new();
    for id in ids {
        if !want.contains(&id) {
            want.push(id);
        }
    }
    if want.len() != 2 {
        return Err(EditError::new("invalid", "the Pathfinder needs two objects (more are not built yet)"));
    }
    let (part, ix, _) = w.find(want[0])?;
    let t = w.text(&part)?.to_owned();
    let mut items = Vec::new();
    for id in &want {
        let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("invalid", format!("{id} is not on the same spread")))?;
        check(&ix, &t, i, id)?;
        items.push(i);
    }
    // rearmost first
    items.sort_unstable();
    let (back, front) = (items[0], items[1]);
    let base = if *op == PathfinderOp::Subtract { back } else { front };
    let other = if base == back { front } else { back };
    let base_id = ix.attr(&t, base, "Self").unwrap_or_default().to_owned();
    let inverse = item_transform(&ix, &t, base).invert().ok_or_else(|| EditError::new("unsupported", format!("{base_id} cannot be inverted")))?;
    let polygon = |i: usize| -> Result<Vec<P>, EditError> {
        let m = if i == base { Mat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]) } else { inverse.then_after(&item_transform(&ix, &t, i)) };
        let subs = subpaths(&ix, &t, i)?;
        let [(pts, closed)] = subs.as_slice() else {
            return Err(EditError::new("unsupported", "a compound path is not built yet"));
        };
        if !closed {
            return Err(EditError::new("invalid", "an open path cannot be combined"));
        }
        if pts.iter().any(|p| p.left != p.anchor || p.right != p.anchor) {
            return Err(EditError::new("unsupported", "curves are not built yet: only straight segments"));
        }
        Ok(pts.iter().map(|p| m.apply(p.anchor)).collect())
    };
    let loops = combine(&polygon(back)?, &polygon(front)?, *op)?;
    let geo = ix.path(&t, base, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let paths = ix.named(&t, geo, "GeometryPathType");
    let ind = indent_before(&t, ix.nodes[geo].start).to_owned();
    let mut xml = String::new();
    for l in &loops {
        let pts: Vec<PathPoint> = l.iter().map(|&p| PathPoint { anchor: p, left: p, right: p }).collect();
        xml.push_str(&path_xml(&pts, true, &ind));
    }
    let (first, last) = (paths[0], *paths.last().unwrap_or(&paths[0]));
    let mut edits = vec![(element_span(&ix, &t, first).start..ix.nodes[last].end, xml), (element_span(&ix, &t, other), String::new())];
    // a Rectangle or an Oval is a Polygon once its path is another
    let tag = ix.local(&t, base).to_owned();
    if tag != "Polygon" {
        let n = &ix.nodes[base];
        edits.push((n.name.clone(), "Polygon".into()));
        edits.push((n.close_start + 2..n.close_start + 2 + tag.len(), "Polygon".into()));
    }
    w.splice(&part, edits)?;
    w.created.push(base_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // two overlapping squares as the oracle cases have them (item space): A 95..195 (rearmost), B 145..245 (front)
    const A: [P; 4] = [(95.0, 95.0), (195.0, 95.0), (195.0, 195.0), (95.0, 195.0)];
    const B: [P; 4] = [(145.0, 145.0), (245.0, 145.0), (245.0, 245.0), (145.0, 245.0)];

    /// The same closed boundary whatever the point it starts at.
    fn same_cycle(a: &[P], b: &[P]) -> bool {
        a.len() == b.len() && (0..a.len()).any(|s| (0..a.len()).all(|k| (a[(s + k) % a.len()].0 - b[k].0).abs() < 1e-6 && (a[(s + k) % a.len()].1 - b[k].1).abs() < 1e-6))
    }

    // InDesign's results (goldens/basic/path-pf-*-overlap)
    #[test]
    fn matches_indesign_regions() {
        let cases: [(PathfinderOp, Vec<Vec<P>>); 5] = [
            (PathfinderOp::Add, vec![vec![(245., 145.), (195., 145.), (195., 95.), (95., 95.), (95., 195.), (145., 195.), (145., 245.), (245., 245.)]]),
            (PathfinderOp::Subtract, vec![vec![(195., 145.), (195., 95.), (95., 95.), (95., 195.), (145., 195.), (145., 145.)]]),
            (PathfinderOp::Intersect, vec![vec![(145., 195.), (195., 195.), (195., 145.), (145., 145.)]]),
            (
                PathfinderOp::ExcludeOverlap,
                vec![
                    vec![(245., 145.), (195., 145.), (195., 195.), (145., 195.), (145., 245.), (245., 245.)],
                    vec![(145., 145.), (195., 145.), (195., 95.), (95., 95.), (95., 195.), (145., 195.)],
                ],
            ),
            (PathfinderOp::MinusBack, vec![vec![(245., 245.), (245., 145.), (195., 145.), (195., 195.), (145., 195.), (145., 245.)]]),
        ];
        for (op, want) in cases {
            let got = combine(&A, &B, op).unwrap();
            assert_eq!(got.len(), want.len(), "{op:?}: number of paths");
            for w in &want {
                assert!(got.iter().any(|g| same_cycle(g, w)), "{op:?}: {w:?} not in {got:?}");
            }
        }
    }

    #[test]
    fn disjoint_and_nested() {
        let far: Vec<P> = A.iter().map(|p| (p.0 + 500.0, p.1)).collect();
        assert!(combine(&A, &far, PathfinderOp::Intersect).is_err());
        assert_eq!(combine(&A, &far, PathfinderOp::Add).unwrap().len(), 2);
        let small: Vec<P> = vec![(120., 120.), (150., 120.), (150., 150.), (120., 150.)];
        // a hole: two paths turning against each other
        let hole = combine(&A, &small, PathfinderOp::Subtract).unwrap();
        assert_eq!(hole.len(), 2);
        assert!(shoelace(&hole[0]) * shoelace(&hole[1]) < 0.0);
    }
}
