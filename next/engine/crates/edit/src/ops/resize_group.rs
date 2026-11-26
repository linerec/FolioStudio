//! Group/selection resizing, measured by resize-group-* and resize-many-*.
//! Transforms, stroke weights and corner radii stay unchanged. Child path points change,
//! with each child's stroke outset retained; text columns and graphic crops follow.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}
fn box_of(w: &Work<'_>, ix: &Index, t: &str, i: usize, m: Mat, stroke: bool, hair: bool) -> Option<[f64; 4]> {
    if stroke { alignment::visible_hair(w, ix, t, i, m, hair) } else { own_bounds(ix, t, i, &m) }
}
fn destination(b: [f64; 4], target: Target) -> Result<[f64; 4], EditError> {
    let d = match target {
        Target::Box(d) => d,
        Target::Size((w, h), a, _) => {
            let (ax, ay) = a.at();
            let x = b[0] + ((b[2] - b[0]) - w) * ax;
            let y = b[1] + ((b[3] - b[1]) - h) * ay;
            [x, y, x + w, y + h]
        }
    };
    if d.iter().any(|v| !v.is_finite()) || d[2] <= d[0] || d[3] <= d[1] || b[2] <= b[0] || b[3] <= b[1] {
        return Err(EditError::new("invalid", "selection has no positive size"));
    }
    Ok(d)
}

pub(super) fn group(w: &mut Work<'_>, id: &str, target: Target) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let stroke = matches!(target, Target::Size(_, _, true));
    let b = box_of(w, &ix, &t, i, Mat::ID, stroke, false).ok_or_else(|| EditError::new("unsupported", "empty group"))?;
    let d = destination(b, target)?;
    let mut edits = Vec::new(); let mut stories = Vec::new();
    leaves(w, &ix, &t, i, Mat::ID, b, d, stroke, false, &mut edits, &mut stories)?;
    w.splice(&part, edits)?;
    for s in stories { w.recompose(&s); }
    Ok(())
}

pub(super) fn many(w: &mut Work<'_>, ids: &[String], size: [f64; 2], anchor: Anchor, stroke: bool) -> Result<(), EditError> {
    let selected: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let mut members = Vec::new();
    let mut box_all = None;
    let mut spread = None;
    for id in &selected {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        if ix.ancestors(i).any(|p| ix.attr(&t, p, "Self").is_some_and(|s| selected.contains(s))) { continue; }
        check_item(&ix, &t, i, id)?;
        if spread.as_ref().is_some_and(|p| p != &part) { return Err(EditError::new("invalid", "resize selection must be in one spread")); }
        spread = Some(part.clone());
        let m = parent_transform(&ix, &t, i).then_after(&item_transform(&ix, &t, i));
        let b = box_of(w, &ix, &t, i, m, stroke, false).ok_or_else(|| EditError::new("unsupported", "item has no bounds"))?;
        box_all = Some(box_all.map_or(b, |a| union(a, b)));
        members.push((part, ix, t, i, m));
    }
    let b = box_all.ok_or_else(|| EditError::new("invalid", "empty selection"))?;
    let d = destination(b, Target::Size((size[0], size[1]), anchor, stroke))?;
    let mut by_part = BTreeMap::<String, Vec<_>>::new(); let mut stories = Vec::new();
    for (part, ix, t, i, m) in members {
        leaves(w, &ix, &t, i, m, b, d, stroke, false, by_part.entry(part).or_default(), &mut stories)?;
    }
    for (part, edits) in by_part { w.splice(&part, edits)?; }
    for s in stories { w.recompose(&s); }
    Ok(())
}

// Recursive group traversal shares XML context and collects edits and affected stories separately.
#[allow(clippy::too_many_arguments)]
fn leaves(
    w: &Work<'_>, ix: &Index, t: &str, i: usize, m: Mat, b: [f64; 4], d: [f64; 4], stroke: bool, hair: bool,
    edits: &mut Vec<(std::ops::Range<usize>, String)>, stories: &mut Vec<String>,
) -> Result<(), EditError> {
    if ix.local(t, i) == "Group" {
        for &k in ix.children(i) {
            if is_item(ix, t, k) { leaves(w, ix, t, k, m.then_after(&item_transform(ix, t, k)), b, d, stroke, hair, edits, stories)?; }
        }
        return Ok(());
    }
    let path = own_path(ix, t, i).ok_or_else(|| EditError::new("unsupported", "group member has no editable path"))?;
    let g = own_bounds(ix, t, i, &m).ok_or_else(|| EditError::new("unsupported", "empty path"))?;
    let v = box_of(w, ix, t, i, m, stroke, hair).ok_or_else(|| EditError::new("unsupported", "empty outline"))?;
    let scale = [(d[2] - d[0]) / (b[2] - b[0]), (d[3] - d[1]) / (b[3] - b[1])];
    let mut to = [0.0; 4];
    for k in 0..4 {
        let axis = k % 2;
        to[k] = d[axis] + (v[k] - b[axis]) * scale[axis] + g[k] - v[k];
    }
    let inv = m.invert().ok_or_else(|| EditError::new("unsupported", "singular group member"))?;
    let mut factors = [1.0; 2];
    for k in 0..2 {
        if g[k + 2] > g[k] {
            if to[k + 2] <= to[k] { return Err(EditError::new("invalid", "selection size is too small for a child's stroke")); }
            factors[k] = (to[k + 2] - to[k]) / (g[k + 2] - g[k]);
        }
    }
    // Nonuniform resizing changes the angles of a rotated/sheared path, hence its
    // miter outline too. InDesign solves for the requested *new* outline, not the old
    // outset (resize-group-mixed-axes / resize-many-mixed-axes).
    let desired = [
        d[0] + (v[0] - b[0]) * scale[0], d[1] + (v[1] - b[1]) * scale[1],
        d[0] + (v[2] - b[0]) * scale[0], d[1] + (v[3] - b[1]) * scale[1],
    ];
    if stroke && (stroke_outset(w, ix, t, i) > 0.0 || (hair && v != g)) {
        let measure = |f: [f64; 2]| -> Result<[f64; 4], EditError> {
            let mut temp = t.to_owned();
            let mut changes = Vec::new();
            for (els, pts, _) in &path.subpaths {
                for (&el, p) in els.iter().zip(pts) {
                    for (key, p) in [("Anchor", p.anchor), ("LeftDirection", p.left), ("RightDirection", p.right)] {
                        let (x, y) = m.apply(p);
                        let q = inv.apply((g[0] + (x - g[0]) * f[0], g[1] + (y - g[1]) * f[1]));
                        changes.push(attr_edit(ix, t, el, key, &geom::pair(q)));
                    }
                }
            }
            changes.sort_by_key(|e| std::cmp::Reverse(e.0.start));
            for (r, v) in changes { temp.replace_range(r, &v); }
            let tx = Index::scan(&temp).map_err(|e| EditError::new("xml", e))?;
            alignment::visible_hair(w, &tx, &temp, i, m, hair).ok_or_else(|| EditError::new("unsupported", "no resized outline"))
        };
        let target = [desired[2] - desired[0], desired[3] - desired[1]];
        let mut outline = measure(factors)?;
        for _ in 0..12 {
            let error = [outline[2] - outline[0] - target[0], outline[3] - outline[1] - target[1]];
            if error.iter().all(|v| v.abs() < 1e-8) { break; }
            let eps = 1e-5;
            let x = measure([factors[0] + eps, factors[1]])?;
            let y = measure([factors[0], factors[1] + eps])?;
            let a = (x[2] - x[0] - (outline[2] - outline[0])) / eps;
            let c = (y[2] - y[0] - (outline[2] - outline[0])) / eps;
            let b = (x[3] - x[1] - (outline[3] - outline[1])) / eps;
            let d = (y[3] - y[1] - (outline[3] - outline[1])) / eps;
            let det = a * d - b * c;
            if det.abs() < 1e-12 { return Err(EditError::new("unsupported", "outline cannot reach that size")); }
            factors[0] -= (d * error[0] - c * error[1]) / det;
            factors[1] -= (a * error[1] - b * error[0]) / det;
            if factors.iter().any(|f| !f.is_finite() || *f <= 0.0) { return Err(EditError::new("invalid", "size is too small for the outline")); }
            outline = measure(factors)?;
        }
        if (outline[2] - outline[0] - target[0]).abs() > 1e-6 || (outline[3] - outline[1] - target[1]).abs() > 1e-6 {
            return Err(EditError::new("unsupported", "outline did not converge"));
        }
        to[0] = g[0] + desired[0] - outline[0];
        to[1] = g[1] + desired[1] - outline[1];
    }
    let map = |p| {
        let (x, y) = m.apply(p);
        inv.apply((to[0] + (x - g[0]) * factors[0], to[1] + (y - g[1]) * factors[1]))
    };
    let mut changed = Vec::new();
    for (els, pts, closed) in path.subpaths {
        let mut new_pts = Vec::new();
        for (el, p) in els.into_iter().zip(pts) {
            let p = PathPoint { anchor: map(p.anchor), left: map(p.left), right: map(p.right) };
            edits.push(attr_edit(ix, t, el, "Anchor", &geom::pair(p.anchor)));
            edits.push(attr_edit(ix, t, el, "LeftDirection", &geom::pair(p.left)));
            edits.push(attr_edit(ix, t, el, "RightDirection", &geom::pair(p.right)));
            new_pts.push(p);
        }
        changed.push((new_pts, closed));
    }
    let own = geom::path_bounds(&changed).ok_or_else(|| EditError::new("unsupported", "empty changed path"))?;
    if ix.local(t, i) == "TextFrame" {
        if let Some(e) = column_width(w, ix, t, i, own[2] - own[0]) { edits.push(e); }
        if let Some(s) = ix.attr(t, i, "ParentStory") { stories.push(s.to_owned()); }
    }
    edits.extend(fitting_crops(ix, t, i, own));
    Ok(())
}

/// Adjust Layout (layoutadj-c4): the visible bounds (stroke included, a hairline reaching half a point) of an item or group on the
/// spread go to `d`: every member keeps its stroke reach and its path scales by what the visible box scales by.
pub(super) fn visible_to(w: &mut Work<'_>, id: &str, d: [f64; 4]) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let m = parent_transform(&ix, &t, i).then_after(&item_transform(&ix, &t, i));
    let b = box_of(w, &ix, &t, i, m, true, true).ok_or_else(|| EditError::new("unsupported", "item has no bounds"))?;
    let d = destination(b, Target::Box(d))?;
    let (mut edits, mut stories) = (Vec::new(), Vec::new());
    leaves(w, &ix, &t, i, m, b, d, true, true, &mut edits, &mut stories)?;
    w.splice(&part, edits)?;
    for s in stories { w.recompose(&s); }
    Ok(())
}
