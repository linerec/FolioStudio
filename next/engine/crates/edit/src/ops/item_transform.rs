//! Move, rotate, shear, flip and resize page items in their coordinate spaces.

use super::*;

/// Transform from an item's parent space to spread coordinates (the page items around it;
/// the spread's own transform places the spread on the pasteboard and is left out).
pub(super) fn parent_transform(ix: &Index, t: &str, i: usize) -> Mat {
    let mut m = Mat::ID;
    for a in ix.ancestors(i) {
        if is_item(ix, t, a) && let Some(at) = ix.attr(t, a, "ItemTransform").and_then(Mat::parse) {
            m = at.then_after(&m);
        }
    }
    m
}

pub(super) fn item_transform(ix: &Index, t: &str, i: usize) -> Mat {
    ix.attr(t, i, "ItemTransform").and_then(Mat::parse).unwrap_or(Mat::ID)
}

/// What Object ▸ Transform Again can repeat: a move, rotation, scale or flip, the last three
/// relative to the reference point of the objects they are applied to (oracle:
/// basic-transform-again: InDesign remembers the matrix with its anchor, not the page position).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Move([f64; 2]),
    Rotate(f64, Anchor),
    Scale(f64, f64, Anchor, Option<scale::Adjust>),
    Shear(f64, Anchor),
    Flip(Axis, Anchor),
}

impl Step {
    pub(crate) fn of(c: &Command) -> Option<Step> {
        match c {
            Command::Move { by, .. } => Some(Step::Move(*by)),
            Command::Rotate { by, anchor, .. } => Some(Step::Rotate(*by, *anchor)),
            Command::Scale { sx, sy, anchor, adjust, .. } => Some(Step::Scale(*sx, *sy, *anchor, *adjust)),
            Command::Shear { by, anchor, .. } => Some(Step::Shear(*by, *anchor)),
            Command::Flip { axis, anchor, .. } => Some(Step::Flip(*axis, *anchor)),
            _ => None,
        }
    }
}

/// `"@copies"` in `ids` stands for the objects the Duplicate before it in the same step made: a
/// transformation with Copy (the Move, Scale, Rotate and Shear dialogs' Copy button) is one step.
pub(super) fn expand(w: &Work<'_>, ids: &[String]) -> Vec<String> {
    ids.iter().flat_map(|i| if i == "@copies" { w.created.clone() } else { vec![i.clone()] }).collect()
}

pub(super) fn turn(w: &mut Work<'_>, ids: &[String], by: f64, anchor: Anchor) -> Result<(), EditError> {
    let r = by.to_radians();
    let m = about(w, ids, anchor, [r.cos(), -r.sin(), r.sin(), r.cos()])?;
    transform_items(w, ids, &m)
}

pub(super) fn turn_about(w: &mut Work<'_>, ids: &[String], by: f64, point: [f64; 2]) -> Result<(), EditError> {
    if !by.is_finite() || !point.iter().all(|v| v.is_finite()) {
        return Err(EditError::new("invalid", "a rotation needs an angle and a point"));
    }
    let r = by.to_radians();
    let [a, b, c, d] = [r.cos(), -r.sin(), r.sin(), r.cos()];
    let [ox, oy] = point;
    transform_items(w, ids, &Mat([a, b, c, d, ox - (a * ox + c * oy), oy - (b * ox + d * oy)]))
}

fn lean(w: &mut Work<'_>, ids: &[String], by: f64, anchor: Anchor) -> Result<(), EditError> {
    tooltransform::shear_items(w, ids, by, anchor, None)
}

pub(super) fn mirror(w: &mut Work<'_>, ids: &[String], axis: Axis, anchor: Anchor) -> Result<(), EditError> {
    let (sx, sy) = match axis {
        Axis::Horizontal => (-1.0, 1.0),
        Axis::Vertical => (1.0, -1.0),
        Axis::Both => (-1.0, -1.0),
    };
    let m = about(w, ids, anchor, [sx, 0.0, 0.0, sy])?;
    transform_items(w, ids, &m)
}

pub(super) fn again(w: &mut Work<'_>, ids: &[String], sequence: bool, individually: bool) -> Result<(), EditError> {
    let steps: Vec<Step> = if sequence { w.ed.again.clone() } else { w.ed.again.last().cloned().into_iter().collect() };
    if steps.is_empty() {
        return Err(EditError::new("nothing", "there is no transformation to repeat"));
    }
    let groups: Vec<Vec<String>> = if individually { ids.iter().map(|i| vec![i.clone()]).collect() } else { vec![ids.to_vec()] };
    for g in &groups {
        for s in &steps {
            match s {
                Step::Move(by) => move_items(w, g, (by[0], by[1]))?,
                Step::Rotate(by, a) => turn(w, g, *by, *a)?,
                Step::Shear(by, a) => lean(w, g, *by, *a)?,
                Step::Flip(axis, a) => mirror(w, g, *axis, *a)?,
                Step::Scale(sx, sy, a, adj) => match g.as_slice() {
                    [id] => scale::scale(w, id, *sx, *sy, *a, *adj)?,
                    _ => return Err(EditError::new("unsupported", "scaling several objects together is not measured: use Transform Again Individually")),
                },
            }
        }
    }
    Ok(())
}

/// InDesign adds the distance to the translation of the item's `ItemTransform`, in its
/// parent's space (a group's child: the group's), and changes nothing else — not the path,
/// not the placed graphic inside (oracle: move-*).
pub(super) fn move_items(w: &mut Work<'_>, ids: &[String], by: (f64, f64)) -> Result<(), EditError> {
    // an item moved along with a selected group is not moved again
    let mut todo: Vec<(String, String)> = Vec::new();
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        check_item(&ix, t, i, id)?;
        let inside_other = ix.ancestors(i).any(|a| ix.attr(t, a, "Self").is_some_and(|s| ids.iter().any(|x| x == s)));
        if !inside_other {
            todo.push((id.clone(), part));
        }
    }
    let mut by_part: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (id, part) in todo {
        by_part.entry(part).or_default().push(id);
    }
    for (part, ids) in by_part {
        let ix = w.index(&part)?;
        let t = w.text(&part)?;
        let mut edits = Vec::new();
        for id in &ids {
            let i = ix.by_self(t, id).ok_or_else(|| EditError::new("notFound", id.clone()))?;
            let v = parent_transform(&ix, t, i)
                .unapply_vector(by)
                .ok_or_else(|| EditError::new("unsupported", format!("{id}: its group cannot be inverted")))?;
            let mut m = item_transform(&ix, t, i);
            m.0[4] += v.0;
            m.0[5] += v.1;
            edits.push(attr_edit(&ix, t, i, "ItemTransform", &m.text()));
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}

pub(super) struct OwnPath {
    /// (PathPointType elements, points) per subpath, and whether it is closed.
    pub(super) subpaths: Vec<(Vec<usize>, Vec<PathPoint>, bool)>,
}

pub(super) fn own_path(ix: &Index, t: &str, i: usize) -> Option<OwnPath> {
    let geo = ix.path(t, i, &["Properties", "PathGeometry"])?;
    let mut subpaths = Vec::new();
    for g in ix.named(t, geo, "GeometryPathType") {
        let closed = ix.attr(t, g, "PathOpen") != Some("true");
        let arr = ix.named(t, g, "PathPointArray").into_iter().next()?;
        let mut els = Vec::new();
        let mut pts = Vec::new();
        for p in ix.named(t, arr, "PathPointType") {
            let get = |k| ix.attr(t, p, k).and_then(geom::parse_pair);
            let anchor = get("Anchor")?;
            pts.push(PathPoint { anchor, left: get("LeftDirection").unwrap_or(anchor), right: get("RightDirection").unwrap_or(anchor) });
            els.push(p);
        }
        subpaths.push((els, pts, closed));
    }
    Some(OwnPath { subpaths })
}

/// A stroke attribute of an item: its own, else its object style's (following `BasedOn`),
/// as InDesign resolves it — an item keeps no attribute its style already gives
/// (oracle: resize-plain-*, a 1 pt stroke from `[Normal Graphics Frame]`).
pub(super) fn stroke_attr(w: &Work<'_>, ix: &Index, t: &str, i: usize, name: &str) -> Option<String> {
    ix.value(t, i, name).or_else(|| objectstyle::from_style(w, ix, t, i, &[], name, "EnableStroke"))
}

/// How far the stroke reaches outside the path, in the item's own space.
pub(super) fn stroke_outset(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> f64 {
    let weight: f64 = stroke_attr(w, ix, t, i, "StrokeWeight").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    if weight <= 0.0 || stroke_attr(w, ix, t, i, "StrokeColor").is_none_or(|c| c == "Swatch/None") {
        return 0.0;
    }
    match stroke_attr(w, ix, t, i, "StrokeAlignment").as_deref() {
        Some("InsideAlignment") => 0.0,
        Some("OutsideAlignment") => weight,
        _ => weight / 2.0,
    }
}

/// Width and height in the control panel include the stroke (InDesign's default
/// `dimensionsIncludeStrokeWeight`); the path is scaled in the item's own space about the
/// reference point of that box and the transform stays as it was (oracle: resize-*).
/// Side effects InDesign writes with it: a text frame's fixed column width, a frame's
/// fitting crops of the graphic inside.
pub(super) enum Target {
    /// Control panel width and height about a reference point; stroke included or (the last
    /// field off) the path's own.
    Size((f64, f64), Anchor, bool),
    /// New path bounds in the item's own space.
    Box([f64; 4]),
}

pub(super) fn resize(w: &mut Work<'_>, id: &str, target: Target) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    check_item(&ix, t, i, id)?;
    let kind = ix.local(t, i);
    if kind == "Group" { return resize_group::group(w, id, target); }
    // a line is resized by its box (a handle drag, measured in InDesign 2025); the control panel's size of a line is not built
    let by_box_line = kind == "GraphicLine" && matches!(target, Target::Box(_));
    if !matches!(kind, "Rectangle" | "Oval" | "Polygon" | "TextFrame") && !by_box_line {
        return Err(EditError::new("unsupported", format!("resizing a {kind} is not built yet")));
    }
    let path = own_path(&ix, t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, p, c)| (p.clone(), *c)).collect();
    let [x0, y0, x1, y1] = geom::path_bounds(&shapes).ok_or_else(|| EditError::new("unsupported", format!("{id} has an empty path")))?;
    let (gw, gh) = (x1 - x0, y1 - y0);
    // a straight line along an axis has no extent across (Adjust Layout moves it by its box)
    if (gw <= 0.0 && gh <= 0.0) || (!by_box_line && (gw <= 0.0 || gh <= 0.0)) {
        return Err(EditError::new("unsupported", format!("{id} has no width or height")));
    }
    // the new box: its corner and the scale of the path in its own space
    let (nx0, ny0, sx, sy) = match target {
        Target::Size(size, anchor, include_stroke) => {
            let o = if include_stroke { stroke_outset(w, &ix, t, i) } else { 0.0 };
            let (nw, nh) = (size.0 - 2.0 * o, size.1 - 2.0 * o);
            if !(nw > 0.0 && nh > 0.0) {
                return Err(EditError::new("invalid", format!("size {size:?} is too small for {id}")));
            }
            let (fx, fy) = anchor.at();
            // the reference point of the stroke box stays: the geometric box keeps the same point
            let (ax, ay) = (x0 + gw * fx, y0 + gh * fy);
            let (sx, sy) = (nw / gw, nh / gh);
            (ax + (x0 - ax) * sx, ay + (y0 - ay) * sy, sx, sy)
        }
        Target::Box([nx0, ny0, nx1, ny1]) => {
            if !((nx1 - nx0 > 0.0 || (gw <= 0.0 && nx1 == nx0)) && (ny1 - ny0 > 0.0 || (gh <= 0.0 && ny1 == ny0))) {
                return Err(EditError::new("invalid", format!("box {:?} is empty", [nx0, ny0, nx1, ny1])));
            }
            (nx0, ny0, if gw > 0.0 { (nx1 - nx0) / gw } else { 1.0 }, if gh > 0.0 { (ny1 - ny0) / gh } else { 1.0 })
        }
    };
    let map = |(x, y): (f64, f64)| (nx0 + (x - x0) * sx, ny0 + (y - y0) * sy);
    let mut edits = Vec::new();
    for (els, pts, _) in &path.subpaths {
        for (&el, p) in els.iter().zip(pts) {
            edits.push(attr_edit(&ix, t, el, "Anchor", &geom::pair(map(p.anchor))));
            edits.push(attr_edit(&ix, t, el, "LeftDirection", &geom::pair(map(p.left))));
            edits.push(attr_edit(&ix, t, el, "RightDirection", &geom::pair(map(p.right))));
        }
    }
    let new_box = [nx0, ny0, nx0 + gw * sx, ny0 + gh * sy];
    let mut story = None;
    if kind == "TextFrame" {
        if let Some(e) = column_width(w, &ix, t, i, new_box[2] - new_box[0]) {
            edits.push(e);
        }
        story = ix.attr(t, i, "ParentStory").map(str::to_owned);
    }
    let auto_fit = framefitting::automatic(w, &ix, t, i);
    if !auto_fit { edits.extend(fitting_crops(&ix, t, i, new_box)); }
    w.splice(&part, edits)?;
    if auto_fit { framefitting::set(w, &[id.to_owned()], &Default::default())?; }
    // the frame's lines change: InDesign's saved lines of its story no longer hold
    if let Some(s) = story {
        w.recompose(&s);
    }
    Ok(())
}

pub(crate) fn outline_bounds(w: &Work<'_>, ix: &Index, t: &str, i: usize, inner: bool) -> Option<[f64; 4]> {
    let m = if inner { Mat::ID } else { parent_transform(ix, t, i).then_after(&item_transform(ix, t, i)) };
    alignment::visible(w, ix, t, i, m)
}

/// The path's own bounds (no stroke), in inner or spread coordinates: Dimensions Include Stroke Weight off.
pub(crate) fn path_bounds_of(ix: &Index, t: &str, i: usize, inner: bool) -> Option<[f64; 4]> {
    let m = if inner { Mat::ID } else { parent_transform(ix, t, i).then_after(&item_transform(ix, t, i)) };
    own_bounds(ix, t, i, &m)
}

/// An item's path bounds in spread coordinates (a group: its members'), or None.
pub(crate) fn spread_bounds(ix: &Index, t: &str, i: usize) -> Option<[f64; 4]> {
    let m = parent_transform(ix, t, i).then_after(&item_transform(ix, t, i));
    own_bounds(ix, t, i, &m)
}

pub(super) fn own_bounds(ix: &Index, t: &str, i: usize, m: &Mat) -> Option<[f64; 4]> {
    if ix.local(t, i) == "Group" {
        let mut out: Option<[f64; 4]> = None;
        for &c in ix.children(i) {
            if !is_item(ix, t, c) {
                continue;
            }
            let cm = m.then_after(&item_transform(ix, t, c));
            if let Some(b) = own_bounds(ix, t, c, &cm) {
                out = Some(match out {
                    None => b,
                    Some(o) => [o[0].min(b[0]), o[1].min(b[1]), o[2].max(b[2]), o[3].max(b[3])],
                });
            }
        }
        return out;
    }
    let path = own_path(ix, t, i)?;
    // the curve's bounds through the transform: control points mapped, bounds of the mapped curve
    let mapped: Vec<(Vec<PathPoint>, bool)> = path
        .subpaths
        .iter()
        .map(|(_, pts, closed)| (pts.iter().map(|p| PathPoint { anchor: m.apply(p.anchor), left: m.apply(p.left), right: m.apply(p.right) }).collect(), *closed))
        .collect();
    geom::path_bounds(&mapped)
}

// ---------------------------------------------------------------------------------------
// transform (rotate, flip)

/// A linear map about the reference point of the objects' bounds together (stroke included).
fn about(w: &mut Work<'_>, ids: &[String], anchor: Anchor, [a, b, c, d]: [f64; 4]) -> Result<Mat, EditError> {
    let (ox, oy) = union_anchor(w, ids, anchor)?;
    Ok(Mat([a, b, c, d, ox - (a * ox + c * oy), oy - (b * ox + d * oy)]))
}

/// The reference point of the objects' bounds together (stroke included), in spread coordinates.
pub(crate) fn union_anchor(w: &mut Work<'_>, ids: &[String], anchor: Anchor) -> Result<(f64, f64), EditError> {
    let mut u: Option<[f64; 4]> = None;
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        // A group's pivot includes its children's stroke outlines, too
        // (oracle: align-group-stroke, rotation before alignment).
        let bx = alignment::visible(w, &ix, t, i, parent_transform(&ix, t, i).then_after(&item_transform(&ix, t, i)))
            .ok_or_else(|| EditError::new("unsupported", format!("{id} has no bounds")))?;
        u = Some(match u {
            None => bx,
            Some(p) => [p[0].min(bx[0]), p[1].min(bx[1]), p[2].max(bx[2]), p[3].max(bx[3])],
        });
    }
    let u = u.ok_or_else(|| EditError::new("invalid", "nothing to transform"))?;
    let (fx, fy) = anchor.at();
    Ok((u[0] + (u[2] - u[0]) * fx, u[1] + (u[3] - u[1]) * fy))
}

/// New transform = parent⁻¹ · X · parent · old (X in spread coordinates).
pub(super) fn transform_items(w: &mut Work<'_>, ids: &[String], x: &Mat) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        check_item(&ix, t, i, id)?;
        if ix.ancestors(i).any(|a| ix.attr(t, a, "Self").is_some_and(|s| ids.iter().any(|x| x == s))) {
            continue;
        }
        let p = parent_transform(&ix, t, i);
        let pinv = p.invert().ok_or_else(|| EditError::new("unsupported", format!("{id}: its group cannot be inverted")))?;
        let m = pinv.then_after(&x.then_after(&p.then_after(&item_transform(&ix, t, i))));
        let e = attr_edit(&ix, t, i, "ItemTransform", &m.text());
        w.splice(&part, vec![e])?;
    }
    Ok(())
}
