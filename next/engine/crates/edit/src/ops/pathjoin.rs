//! Object ▸ Paths ▸ Join (oracle: basic-paths-join, the menu action on the selected open paths):
//! - one open path: it is closed (its first and last points meet by a straight segment); the object stays what it was;
//! - two or more: the first selected object takes the others' paths and keeps its element, settings and place, the others are
//!   gone. The two ends that lie nearest each other meet: the end of A and the start of B (A + B), the two ends (A + B turned
//!   round), the two starts (B turned round + A) or the start of A and the end of B (B + A). Ends at the same place become
//!   one point, with the way in of the first and the way out of the second;
//!   otherwise a straight segment joins them. With more than two paths each one joins the result so far in the same way.
//! - the result is open, in the first object's space.
use super::*;
use compound::{path_xml, subpaths};

/// How near two ends must be to count as the same place.
const SAME: f64 = 1e-4;

pub(super) fn turned(pts: &[PathPoint]) -> Vec<PathPoint> {
    pts.iter().rev().map(|p| PathPoint { anchor: p.anchor, left: p.right, right: p.left }).collect()
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// `a` and `b` joined at their nearest ends.
fn join_two(a: Vec<PathPoint>, b: Vec<PathPoint>) -> Vec<PathPoint> {
    let (a0, a1) = (a[0].anchor, a[a.len() - 1].anchor);
    let (b0, b1) = (b[0].anchor, b[b.len() - 1].anchor);
    // end of A with start of B first, as the ties go
    let options = [(dist(a1, b0), 0), (dist(a1, b1), 1), (dist(a0, b0), 2), (dist(a0, b1), 3)];
    let mut best = options[0];
    for o in &options[1..] {
        if o.0 < best.0 {
            best = *o;
        }
    }
    let (first, second) = match best.1 {
        0 => (a, b),
        1 => (a, turned(&b)),
        2 => (turned(&b), a),
        _ => (b, a),
    };
    let mut out = first;
    let mut rest = second.into_iter();
    if let Some(head) = rest.next() {
        let last = out.len() - 1;
        if dist(out[last].anchor, head.anchor) < SAME {
            out[last].right = head.right;
        } else {
            out.push(head);
        }
    }
    out.extend(rest);
    out
}

fn check(ix: &Index, t: &str, i: usize, id: &str) -> Result<(), EditError> {
    check_item(ix, t, i, id)?;
    let tag = ix.local(t, i);
    if !matches!(tag, "GraphicLine" | "Polygon") {
        return Err(EditError::new("unsupported", format!("{id} ({tag}): only open paths can be joined")));
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

pub(crate) fn join(w: &mut Work<'_>, ids_in: &[String]) -> Result<(), EditError> {
    let mut ids: Vec<&String> = Vec::new();
    for id in ids_in {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let first = ids.first().ok_or_else(|| EditError::new("invalid", "nothing to join"))?;
    let (part, ix, _) = w.find(first)?;
    let t = w.text(&part)?.to_owned();
    let mut items = Vec::new();
    for id in &ids {
        let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("invalid", format!("{id} is not on the same spread")))?;
        check(&ix, &t, i, id)?;
        items.push(i);
    }
    let base = items[0];
    let base_id = ix.attr(&t, base, "Self").unwrap_or_default().to_owned();
    let geo = ix.path(&t, base, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let paths = ix.named(&t, geo, "GeometryPathType");
    let mine = subpaths(&ix, &t, base)?;
    let [(points, closed)] = mine.as_slice() else {
        return Err(EditError::new("invalid", format!("{base_id} is not a single path")));
    };
    if *closed {
        return Err(EditError::new("invalid", format!("{base_id} is closed already")));
    }
    let inverse = item_transform(&ix, &t, base).invert().ok_or_else(|| EditError::new("unsupported", format!("{base_id} cannot be inverted")))?;
    let mut now = points.clone();
    let mut edits = Vec::new();
    for &i in &items[1..] {
        let m = inverse.then_after(&item_transform(&ix, &t, i));
        let other = subpaths(&ix, &t, i)?;
        let [(pts, closed)] = other.as_slice() else {
            return Err(EditError::new("invalid", "only single paths can be joined"));
        };
        if *closed {
            return Err(EditError::new("invalid", "a closed path cannot be joined"));
        }
        let mapped: Vec<PathPoint> = pts.iter().map(|p| PathPoint { anchor: m.apply(p.anchor), left: m.apply(p.left), right: m.apply(p.right) }).collect();
        now = join_two(now, mapped);
        edits.push((element_span(&ix, &t, i), String::new()));
    }
    if now.len() < 2 {
        return Err(EditError::new("invalid", "nothing to join"));
    }
    let ind = indent_before(&t, ix.nodes[geo].start).to_owned();
    // one path: it is closed; several: they are one open path
    edits.push((element_span(&ix, &t, paths[0]), path_xml(&now, items.len() == 1, &ind)));
    w.splice(&part, edits)?;
    w.created.push(base_id);
    Ok(())
}
