//! Object ▸ Paths ▸ Make Compound Path and Release Compound Path (oracle basic-compound, measured with the DOM's
//! `makeCompoundPath(with)` and `releaseCompoundPath()`):
//! - the rearmost object (first in the spread) is the compound path: it keeps its element (a Rectangle stays a Rectangle), its
//!   transform and all its settings; the other objects' paths join its `PathGeometry`, back to front, written in its own space
//!   (their transform, then the inverse of its transform), every other path reversed (the second, the fourth…); those objects
//!   are gone;
//! - Release keeps the first path in the object and makes each other path an object of its own: the same element with the same
//!   settings and transform, directly above it, one path in its `PathGeometry`.
use super::*;
use convert::point_xml;

/// The geometry of `i` ... its subpaths as `(points, closed)` in the item's own space.
pub(super) fn subpaths(ix: &Index, t: &str, i: usize) -> Result<Vec<(Vec<PathPoint>, bool)>, EditError> {
    let path = own_path(ix, t, i).ok_or_else(|| EditError::new("unsupported", "an object without a path"))?;
    Ok(path.subpaths.into_iter().map(|(_, pts, closed)| (pts, closed)).collect())
}

/// `<GeometryPathType>` of `points` as InDesign lays it out inside a `PathGeometry` indented `ind`.
pub(super) fn path_xml(points: &[PathPoint], closed: bool, ind: &str) -> String {
    let mut s = format!("\n{ind}\t<GeometryPathType PathOpen=\"{}\">\n{ind}\t\t<PathPointArray>", !closed);
    for p in points {
        s.push_str(&format!("\n{ind}\t\t\t{}", point_xml(p)));
    }
    s.push_str(&format!("\n{ind}\t\t</PathPointArray>\n{ind}\t</GeometryPathType>"));
    s
}

const SHAPES: &[&str] = &["Rectangle", "Oval", "Polygon"];

fn check(ix: &Index, t: &str, i: usize, id: &str) -> Result<(), EditError> {
    check_item(ix, t, i, id)?;
    let tag = ix.local(t, i);
    if !SHAPES.contains(&tag) {
        return Err(EditError::new("unsupported", format!("{id} ({tag}): only rectangles, ovals and polygons make a compound path")));
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

pub(super) fn make(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    let mut ids: Vec<&String> = ids.iter().collect();
    ids.dedup();
    if ids.len() < 2 {
        return Err(EditError::new("invalid", "a compound path needs at least two objects"));
    }
    // all in one spread, rearmost first
    let (part, ix, _) = w.find(ids[0])?;
    let t = w.text(&part)?.to_owned();
    let mut items: Vec<usize> = Vec::new();
    for id in &ids {
        let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("invalid", format!("{id} is not on the same spread")))?;
        check(&ix, &t, i, id)?;
        if !items.contains(&i) {
            items.push(i);
        }
    }
    items.sort_unstable();
    let base = items[0];
    let base_id = ix.attr(&t, base, "Self").unwrap_or_default().to_owned();
    let inverse = item_transform(&ix, &t, base).invert().ok_or_else(|| EditError::new("unsupported", format!("{base_id} cannot be inverted")))?;
    let geo = ix.path(&t, base, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let ind = indent_before(&t, ix.nodes[geo].start).to_owned();
    let mut added = String::new();
    let mut edits = Vec::new();
    // the paths alternate in direction, the base's first: every other one is reversed (points the other way round, directions
    // swapped), whether it lies inside the one before or not (compound-front)
    let mut count = ix.named(&t, geo, "GeometryPathType").len();
    for &i in &items[1..] {
        let m = inverse.then_after(&item_transform(&ix, &t, i));
        for (points, closed) in subpaths(&ix, &t, i)? {
            let mut mapped: Vec<PathPoint> = points.iter().map(|p| PathPoint { anchor: m.apply(p.anchor), left: m.apply(p.left), right: m.apply(p.right) }).collect();
            if count % 2 == 1 {
                mapped = mapped.iter().rev().map(|p| PathPoint { anchor: p.anchor, left: p.right, right: p.left }).collect();
            }
            count += 1;
            added.push_str(&path_xml(&mapped, closed, &ind));
        }
        edits.push((element_span(&ix, &t, i), String::new()));
    }
    // after the last path of the base
    let last = *ix.named(&t, geo, "GeometryPathType").last().ok_or_else(|| EditError::new("xml", "a path geometry without a path"))?;
    let at = ix.nodes[last].end;
    edits.push((at..at, added));
    w.splice(&part, edits)?;
    w.created.push(base_id);
    Ok(())
}

pub(super) fn release(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check(&ix, &t, i, id)?;
    let geo = ix.path(&t, i, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let paths = ix.named(&t, geo, "GeometryPathType");
    if paths.len() < 2 {
        return Err(EditError::new("invalid", format!("{id} is not a compound path")));
    }
    let whole = ix.nodes[i].clone();
    let element = &t[whole.start..whole.end];
    let geo_n = &ix.nodes[geo];
    // the element once per path: the first stays, the others copy it with their own path
    let with_path = |keep: usize| {
        let mut s = String::new();
        s.push_str(&element[..geo_n.open_end - whole.start]);
        let g = &ix.nodes[paths[keep]];
        s.push('\n');
        s.push_str(t[element_span(&ix, &t, paths[keep]).start..g.end].trim_start_matches(['\n', '\r']));
        s.push('\n');
        s.push_str(indent_before(&t, geo_n.start));
        s.push_str(&element[geo_n.close_start - whole.start..]);
        s
    };
    let mut after = String::new();
    let indent = indent_before(&t, whole.start).to_owned();
    let mut created = Vec::new();
    for k in 1..paths.len() {
        let new_id = w.new_id();
        let copy = with_path(k).replacen(&format!("Self=\"{id}\""), &format!("Self=\"{new_id}\""), 1);
        after.push_str(&format!("\n{indent}{copy}"));
        created.push(new_id);
    }
    let first = with_path(0);
    w.splice(&part, vec![(whole.start..whole.end, format!("{first}{after}"))])?;
    w.created.extend(created);
    Ok(())
}
