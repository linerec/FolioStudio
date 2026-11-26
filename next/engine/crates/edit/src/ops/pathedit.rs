//! The Pen tool and editing a path's points (oracle: basic-paths). A path made by the Pen tool is what a script's
//! `entirePath` makes: a Polygon (closed) or GraphicLine (open) with the point list as given (create.rs); moving an
//! anchor takes its two handles with it; moving a handle of a smooth point turns the opposite handle to keep the two
//! in line, at its own length, and a corner point's handles move on their own. InDesign tells the two apart when it
//! reads a path: smooth is two handles in line on opposite sides of the anchor (either may be as short as nothing); a
//! cusp, a bend, handles on the same side or a hair off the line make a corner (oracle/indesign/probe-point-types.jsx).
//! Coordinates are the item's own (inner) ones, as the path stores them.
use super::*;

/// One point of a path to create, in page coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathPt {
    pub anchor: [f64; 2],
    #[serde(default)]
    pub left: Option<[f64; 2]>,
    #[serde(default)]
    pub right: Option<[f64; 2]>,
}

impl PathPt {
    pub(super) fn point(&self) -> PathPoint {
        let at = |p: [f64; 2]| (p[0], p[1]);
        PathPoint { anchor: at(self.anchor), left: at(self.left.unwrap_or(self.anchor)), right: at(self.right.unwrap_or(self.anchor)) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HandleSide {
    Left,
    Right,
}

fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

fn len(v: (f64, f64)) -> f64 {
    v.0.hypot(v.1)
}

/// Two handles in line on opposite sides of the anchor (a handle with no length is in line with any other).
fn smooth(p: &PathPoint) -> bool {
    let (l, r) = (sub(p.left, p.anchor), sub(p.right, p.anchor));
    let (ll, rl) = (len(l), len(r));
    if ll < 1e-9 || rl < 1e-9 {
        return true;
    }
    let cross = (l.0 * r.1 - l.1 * r.0) / (ll * rl);
    let dot = (l.0 * r.0 + l.1 * r.1) / (ll * rl);
    cross.abs() < 1e-6 && dot < 0.0
}

/// Four plain points forming a rectangle (either way round).
pub(super) fn is_rectangle(pts: &[PathPoint]) -> bool {
    let [a, b, c, d] = pts else { return false };
    pts.iter().all(|p| p.left == p.anchor && p.right == p.anchor)
        && ((a.anchor.0 == b.anchor.0 && b.anchor.1 == c.anchor.1 && c.anchor.0 == d.anchor.0 && d.anchor.1 == a.anchor.1)
            || (a.anchor.1 == b.anchor.1 && b.anchor.0 == c.anchor.0 && c.anchor.1 == d.anchor.1 && d.anchor.0 == a.anchor.0))
}

/// Every point of the path moved by the same distance: the shape is the same.
fn only_moved(before: &[PathPoint], after: &[PathPoint]) -> bool {
    let Some((b, a)) = before.first().zip(after.first()) else { return true };
    let d = sub(a.anchor, b.anchor);
    before.iter().zip(after).all(|(b, a)| {
        let near = |x: (f64, f64), y: (f64, f64)| (x.0 - y.0).abs() < 1e-9 && (x.1 - y.1).abs() < 1e-9;
        near(sub(a.anchor, b.anchor), d) && near(sub(a.left, b.left), d) && near(sub(a.right, b.right), d)
    })
}

/// A path's item: the file part, its index, the element and its path points (as `own_path` reads them).
struct Target {
    part: String,
    ix: Index,
    t: String,
    i: usize,
    kind: String,
    els: Vec<usize>,
    pts: Vec<PathPoint>,
    closed: bool,
    all: Vec<(Vec<PathPoint>, bool)>,
}

fn target(w: &mut Work<'_>, id: &str, path: usize) -> Result<Target, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let kind = ix.local(&t, i).to_owned();
    if !matches!(kind.as_str(), "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "TextFrame") {
        return Err(EditError::new("unsupported", format!("editing the points of a {kind} is not built yet")));
    }
    if ix.attr(&t, i, "Locked") == Some("true") {
        return Err(EditError::new("locked", format!("{id} is locked")));
    }
    let own = own_path(&ix, &t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let all: Vec<(Vec<PathPoint>, bool)> = own.subpaths.iter().map(|(_, p, c)| (p.clone(), *c)).collect();
    let (els, pts, closed) = own.subpaths.get(path).cloned().ok_or_else(|| EditError::new("notFound", format!("{id} has no path {path}")))?;
    Ok(Target { part, ix, t, i, kind, els, pts, closed, all })
}

impl Target {
    /// Writes the changed points of the path back, and what the frame's new shape changes around it.
    fn write(&self, w: &mut Work<'_>, path: usize, now: &[PathPoint]) -> Result<(), EditError> {
        let mut edits = Vec::new();
        for ((&el, old), new) in self.els.iter().zip(&self.pts).zip(now) {
            for (key, a, b) in [("Anchor", old.anchor, new.anchor), ("LeftDirection", old.left, new.left), ("RightDirection", old.right, new.right)] {
                if a != b {
                    edits.push(attr_edit(&self.ix, &self.t, el, key, &geom::pair(b)));
                }
            }
        }
        if edits.is_empty() {
            return Ok(());
        }
        self.finish(w, path, now, edits)
    }

    /// Writes a path with another number of points: each entry is the old point it keeps (its element stays as it was
    /// when unchanged) or a new one.
    fn write_list(&self, w: &mut Work<'_>, path: usize, now: &[(Option<usize>, PathPoint)]) -> Result<(), EditError> {
        let first = *self.els.first().ok_or_else(|| EditError::new("xml", "a path without points"))?;
        let array = self.ix.nodes[first].parent.ok_or_else(|| EditError::new("xml", "a point outside its array"))?;
        let ind = indent_before(&self.t, self.ix.nodes[first].start);
        let last = *self.els.last().unwrap_or(&first);
        let tail = &self.t[self.ix.nodes[last].end..self.ix.nodes[array].close_start];
        let mut s = String::new();
        for (old, p) in now {
            s.push_str(&format!("\n{ind}"));
            match old {
                Some(k) if self.pts[*k] == *p => {
                    let n = &self.ix.nodes[self.els[*k]];
                    s.push_str(&self.t[n.start..n.end]);
                }
                _ => s.push_str(&convert::point_xml(p)),
            }
        }
        s.push_str(tail);
        let edit = (self.ix.nodes[array].open_end..self.ix.nodes[array].close_start, s);
        let pts: Vec<PathPoint> = now.iter().map(|(_, p)| *p).collect();
        self.finish(w, path, &pts, vec![edit])
    }

    fn finish(&self, w: &mut Work<'_>, path: usize, now: &[PathPoint], mut edits: Vec<(std::ops::Range<usize>, String)>) -> Result<(), EditError> {
        // the frame's box after the edit: a placed graphic's crop and a text frame's column follow it, as in a resize
        let mut shapes = self.all.clone();
        shapes[path].0 = now.to_vec();
        let mut story = None;
        if let Some(b) = geom::path_bounds(&shapes) {
            if self.kind == "TextFrame" {
                if let Some(e) = column_width(w, &self.ix, &self.t, self.i, b[2] - b[0]) {
                    edits.push(e);
                }
                story = self.ix.attr(&self.t, self.i, "ParentStory").map(str::to_owned);
            }
            edits.extend(fitting_crops(&self.ix, &self.t, self.i, b));
        }
        // A frame that holds a placed graphic stops being a Rectangle or an Oval when its path no longer is one (InDesign
        // writes a Polygon); an empty frame keeps its class (oracle: path-edit-rect-with-image, path-edit-empty-frame).
        let graphic = self.ix.children(self.i).iter().any(|&c| GRAPHICS.contains(&self.ix.local(&self.t, c)));
        let still = if self.kind == "Rectangle" { is_rectangle(now) } else { self.pts.len() == now.len() && only_moved(&self.pts, now) };
        if graphic && matches!(self.kind.as_str(), "Rectangle" | "Oval") && !still {
            let n = &self.ix.nodes[self.i];
            edits.push((n.name.clone(), "Polygon".into()));
            edits.push((n.close_start + 2..n.close_start + 2 + self.kind.len(), "Polygon".into()));
        }
        w.splice(&self.part, edits)?;
        // a text frame that is no rectangle has one inset, all round (as in Convert Shape)
        if self.kind == "TextFrame" && !is_rectangle(now) {
            convert::one_inset(w, &self.part, self.i)?;
        }
        if let Some(s) = story {
            w.recompose(&s);
        }
        Ok(())
    }

    fn point(&self, index: usize) -> Result<PathPoint, EditError> {
        self.pts.get(index).copied().ok_or_else(|| EditError::new("notFound", format!("point {index}")))
    }
}

/// Moves anchors and, with each, its two handles (dragging selected points with the Direct Selection tool; a script
/// setting `anchor`).
pub(super) fn move_points(w: &mut Work<'_>, id: &str, path: usize, indices: &[usize], by: [f64; 2]) -> Result<(), EditError> {
    if indices.is_empty() || by.iter().any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "no points to move"));
    }
    let tg = target(w, id, path)?;
    let mut now = tg.pts.clone();
    for &k in indices {
        tg.point(k)?;
        let p = &mut now[k];
        for q in [&mut p.anchor, &mut p.left, &mut p.right] {
            *q = (q.0 + by[0], q.1 + by[1]);
        }
    }
    tg.write(w, path, &now)
}

/// Moves one handle to `to`. A smooth point turns its other handle to stay opposite, at its own length (unless the
/// tangent is `independent`, Alt while dragging).
pub(super) fn move_handle(w: &mut Work<'_>, id: &str, path: usize, index: usize, side: HandleSide, to: Option<[f64; 2]>, by: Option<[f64; 2]>, independent: bool) -> Result<(), EditError> {
    let tg = target(w, id, path)?;
    let old = tg.point(index)?;
    let from = if side == HandleSide::Left { old.left } else { old.right };
    let to = match (to, by) {
        (Some(t), None) => t,
        (None, Some(b)) => [from.0 + b[0], from.1 + b[1]],
        _ => return Err(EditError::new("invalid", "a handle moves to a place or by a distance")),
    };
    if to.iter().any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "handle position"));
    }
    let was_smooth = smooth(&old);
    let mut p = old;
    let to = (to[0], to[1]);
    match side {
        HandleSide::Left => p.left = to,
        HandleSide::Right => p.right = to,
    }
    if was_smooth && !independent {
        let dir = sub(to, p.anchor);
        let d = len(dir);
        if d > 1e-9 {
            let other = if side == HandleSide::Left { p.right } else { p.left };
            let keep = len(sub(other, p.anchor));
            let moved = (p.anchor.0 - dir.0 / d * keep, p.anchor.1 - dir.1 / d * keep);
            if side == HandleSide::Left {
                p.right = moved;
            } else {
                p.left = moved;
            }
        }
    }
    let mut now = tg.pts.clone();
    now[index] = p;
    tg.write(w, path, &now)
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Add Anchor Point: a point on the segment that starts at point `segment` (`t` of the way along it, 0 to 1). The curve keeps
/// its shape — the segment is split where it is, so both halves are the same curve — and a straight segment gets a corner
/// point with no handles. A closed path's last segment runs back to its first point.
pub(super) fn add_point(w: &mut Work<'_>, id: &str, path: usize, segment: usize, t: f64) -> Result<(), EditError> {
    let tg = target(w, id, path)?;
    let n = tg.pts.len();
    if !(t > 0.0 && t < 1.0) || segment + usize::from(!tg.closed) >= n {
        return Err(EditError::new("invalid", "no such segment, or a place outside it"));
    }
    let (a, b) = (tg.pts[segment], tg.pts[(segment + 1) % n]);
    let straight = a.right == a.anchor && b.left == b.anchor;
    let (mut a2, mut b2) = (a, b);
    let fresh = if straight {
        let p = lerp(a.anchor, b.anchor, t);
        PathPoint { anchor: p, left: p, right: p }
    } else {
        // de Casteljau
        let (q0, q1, q2) = (lerp(a.anchor, a.right, t), lerp(a.right, b.left, t), lerp(b.left, b.anchor, t));
        let (r0, r1) = (lerp(q0, q1, t), lerp(q1, q2, t));
        a2.right = q0;
        b2.left = q2;
        PathPoint { anchor: lerp(r0, r1, t), left: r0, right: r1 }
    };
    let mut now: Vec<(Option<usize>, PathPoint)> = Vec::with_capacity(n + 1);
    for k in 0..n {
        let p = if k == segment { a2 } else if k == (segment + 1) % n { b2 } else { tg.pts[k] };
        now.push((Some(k), p));
        if k == segment {
            now.push((None, fresh));
        }
    }
    tg.write_list(w, path, &now)
}

/// Delete Anchor Point: the point goes and its neighbours keep their handles, so the path reshapes between them (a script's
/// `remove()`, oracle: path-delete-*). A path keeps at least two points.
pub(super) fn delete_point(w: &mut Work<'_>, id: &str, path: usize, index: usize) -> Result<(), EditError> {
    let tg = target(w, id, path)?;
    tg.point(index)?;
    if tg.pts.len() <= 2 {
        return Err(EditError::new("invalid", "a path keeps at least two points: delete the object instead"));
    }
    let now: Vec<(Option<usize>, PathPoint)> = tg.pts.iter().enumerate().filter(|(k, _)| *k != index).map(|(k, p)| (Some(k), *p)).collect();
    tg.write_list(w, path, &now)
}

/// Convert Direction Point: a click on a point draws both handles into the anchor (a script's `pointType = PLAIN`).
pub(super) fn plain_point(w: &mut Work<'_>, id: &str, path: usize, index: usize) -> Result<(), EditError> {
    let tg = target(w, id, path)?;
    let mut p = tg.point(index)?;
    p.left = p.anchor;
    p.right = p.anchor;
    let mut now = tg.pts.clone();
    now[index] = p;
    tg.write(w, path, &now)
}

/// Convert Direction Point dragged out of a point: both handles to the given places (the tool makes them one each way).
pub(super) fn set_handles(w: &mut Work<'_>, id: &str, path: usize, index: usize, left: [f64; 2], right: [f64; 2]) -> Result<(), EditError> {
    if left.iter().chain(&right).any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "handle positions"));
    }
    let tg = target(w, id, path)?;
    let mut p = tg.point(index)?;
    p.left = (left[0], left[1]);
    p.right = (right[0], right[1]);
    let mut now = tg.pts.clone();
    now[index] = p;
    tg.write(w, path, &now)
}

/// What Object ▸ Convert Point turns the selected points into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PointKind {
    Plain,
    Corner,
    Smooth,
    Symmetrical,
}

/// The handles a script's `pointType = SMOOTH` gives a point (oracle/indesign/probe-convert-smooth.jsx): along the bisector of
/// the way in and the way out (the sum of the two unit directions), a quarter of the distance to the neighbour on each side.
/// The neighbours wrap round, an open path's ends too (measured); where the two directions cancel, the tangent points up
/// (0, -1).
fn auto_smooth(prev: (f64, f64), p: &PathPoint, next: (f64, f64)) -> ((f64, f64), (f64, f64), f64, f64) {
    let (vi, vo) = (sub(p.anchor, prev), sub(next, p.anchor));
    let (di, dout) = (len(vi), len(vo));
    let unit = |v: (f64, f64), d: f64| if d > 1e-12 { (v.0 / d, v.1 / d) } else { (0.0, 0.0) };
    let (ui, uo) = (unit(vi, di), unit(vo, dout));
    let sum = (ui.0 + uo.0, ui.1 + uo.1);
    let s = len(sum);
    let dir = if s > 1e-9 { (sum.0 / s, sum.1 / s) } else { (0.0, -1.0) };
    let (li, lo) = (di * 0.25, dout * 0.25);
    ((p.anchor.0 - dir.0 * li, p.anchor.1 - dir.1 * li), (p.anchor.0 + dir.0 * lo, p.anchor.1 + dir.1 * lo), li, lo)
}

/// Object ▸ Convert Point ▸ Plain / Corner / Smooth / Symmetrical on the points `indices` of one path.
/// - Plain: both handles into the anchor.
/// - Corner: the handles stay where they are. IDML stores no point type (it is read from the handles: see `smooth`), so the
///   geometry does not change; a drag of one handle still turns the other (known gap: InDesign remembers the type for the
///   session only).
/// - Smooth: a point that is smooth already keeps its handles; a plain point or a corner gets the automatic handles of
///   `auto_smooth` (measured with the DOM's `pointType`; handles on the same side of the anchor are not reproduced).
/// - Symmetrical: not measurable by script (a path point cannot be selected for the menu action). Decision: the same tangent
///   as Smooth (the point's own tangent if it is smooth already), both handles the same length (the mean of the two).
pub(crate) fn convert_points(w: &mut Work<'_>, id: &str, path: &usize, indices: &[usize], kind: &PointKind) -> Result<(), EditError> {
    let (path, kind) = (*path, *kind);
    if indices.is_empty() {
        return Err(EditError::new("invalid", "no points to convert"));
    }
    let tg = target(w, id, path)?;
    let n = tg.pts.len();
    let mut now = tg.pts.clone();
    for &k in indices {
        let old = tg.point(k)?;
        let flat = old.left == old.anchor && old.right == old.anchor;
        let (prev, next) = (tg.pts[(k + n - 1) % n].anchor, tg.pts[(k + 1) % n].anchor);
        now[k] = match kind {
            PointKind::Plain => PathPoint { anchor: old.anchor, left: old.anchor, right: old.anchor },
            PointKind::Corner => old,
            PointKind::Smooth if !flat && smooth(&old) => old,
            PointKind::Smooth => {
                let (left, right, ..) = auto_smooth(prev, &old, next);
                PathPoint { anchor: old.anchor, left, right }
            }
            PointKind::Symmetrical => {
                let (l, r) = if !flat && smooth(&old) {
                    (len(sub(old.left, old.anchor)), len(sub(old.right, old.anchor)))
                } else {
                    let (_, _, li, lo) = auto_smooth(prev, &old, next);
                    (li, lo)
                };
                let m = (l + r) / 2.0;
                // the tangent: the point's own if it has one, else the automatic one
                let (al, ar, ..) = auto_smooth(prev, &old, next);
                let own = if !flat && smooth(&old) {
                    let (a, b) = (sub(old.right, old.anchor), sub(old.left, old.anchor));
                    let (la, lb) = (len(a), len(b));
                    if la > 1e-9 { Some((a.0 / la, a.1 / la)) } else if lb > 1e-9 { Some((-b.0 / lb, -b.1 / lb)) } else { None }
                } else {
                    None
                };
                let dir = own.unwrap_or_else(|| {
                    let d = sub(ar, al);
                    let dl = len(d);
                    if dl > 1e-12 { (d.0 / dl, d.1 / dl) } else { (0.0, -1.0) }
                });
                PathPoint { anchor: old.anchor, left: (old.anchor.0 - dir.0 * m, old.anchor.1 - dir.1 * m), right: (old.anchor.0 + dir.0 * m, old.anchor.1 + dir.1 * m) }
            }
        };
    }
    tg.write(w, path, &now)
}

/// Scissors: cuts the path at point `index` (the tool's click on a point; on a segment the caller adds a point first).
/// Decisions, not measured: a script cannot drive the tool.
/// - a closed path becomes open the way Paths ▸ Open Path makes it (measured, convert.rs): the path turned to start at the cut
///   point, which is then at both ends (a full copy at the end);
/// - an open path becomes two open paths with the cut point at the end of the first and the start of the second (a full copy
///   each). The first stays in the object, the second is a new object right above with the same settings (as Release
///   Compound Path makes them);
/// - a cut at an end of an open path cuts nothing.
pub(crate) fn cut_point(w: &mut Work<'_>, id: &str, path: &usize, index: &usize) -> Result<(), EditError> {
    let (path, index) = (*path, *index);
    let tg = target(w, id, path)?;
    tg.point(index)?;
    if tg.all.len() != 1 {
        return Err(EditError::new("unsupported", "cutting a compound path is not built"));
    }
    let n = tg.pts.len();
    if tg.closed {
        let now: Vec<(Option<usize>, PathPoint)> = (0..n).map(|k| (Some((index + k) % n), tg.pts[(index + k) % n])).collect();
        tg.write_list(w, path, &now)?;
        return convert::convert_shape(w, &[id.to_owned()], convert::Shape::OpenPath);
    }
    if index == 0 || index + 1 >= n {
        return Err(EditError::new("invalid", "a cut at the end of an open path cuts nothing"));
    }
    if tg.kind == "TextFrame" || tg.ix.children(tg.i).iter().any(|&c| GRAPHICS.contains(&tg.ix.local(&tg.t, c))) {
        return Err(EditError::new("unsupported", "only a plain path can be cut in two"));
    }
    let geo = tg.ix.path(&tg.t, tg.i, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let first_path = *tg.ix.named(&tg.t, geo, "GeometryPathType").first().ok_or_else(|| EditError::new("xml", "no path"))?;
    let ind = indent_before(&tg.t, tg.ix.nodes[geo].start).to_owned();
    let span = element_span(&tg.ix, &tg.t, first_path);
    let whole = tg.ix.nodes[tg.i].clone();
    let element = &tg.t[whole.start..whole.end];
    let new_id = w.new_id();
    let rel = span.start - whole.start..span.end - whole.start;
    let copy = format!("{}{}{}", &element[..rel.start], compound::path_xml(&tg.pts[index..], false, &ind), &element[rel.end..])
        .replacen(&format!("Self=\"{id}\""), &format!("Self=\"{new_id}\""), 1);
    let indent = indent_before(&tg.t, whole.start).to_owned();
    let edits = vec![(span, compound::path_xml(&tg.pts[..=index], false, &ind)), (whole.end..whole.end, format!("\n{indent}{copy}"))];
    w.splice(&tg.part, edits)?;
    w.created.push(new_id);
    Ok(())
}

/// For the tools that edit an open path's curve (Erase; ops/pencilerase.rs): the points of the path `path` of an open plain item, the
/// matrix that takes its own coordinates to the spread's, and the file part the item is in.
pub(super) fn open_path(w: &mut Work<'_>, id: &str, path: usize) -> Result<(Vec<PathPoint>, Mat, String), EditError> {
    let tg = target(w, id, path)?;
    if tg.closed {
        return Err(EditError::new("unsupported", "only an open path can be edited with the pencil tools"));
    }
    if tg.all.len() != 1 {
        return Err(EditError::new("unsupported", "a compound path cannot be edited with the pencil tools"));
    }
    if tg.kind == "TextFrame" || tg.ix.children(tg.i).iter().any(|&c| GRAPHICS.contains(&tg.ix.local(&tg.t, c))) {
        return Err(EditError::new("unsupported", "only a plain path can be edited with the pencil tools"));
    }
    let m = parent_transform(&tg.ix, &tg.t, tg.i).then_after(&item_transform(&tg.ix, &tg.t, tg.i));
    Ok((tg.pts.clone(), m, tg.part.clone()))
}

/// For the Erase and Smooth tools, which also work on a closed path and on one path of a compound path (ops/pencilerase.rs): every path of a
/// plain item as `(points, closed)`, the matrix that takes its own coordinates to the spread's, and the file part the item is in.
pub(super) fn curve_paths(w: &mut Work<'_>, id: &str) -> Result<(Vec<(Vec<PathPoint>, bool)>, Mat, String), EditError> {
    let tg = target(w, id, 0)?;
    if tg.kind == "TextFrame" || tg.ix.children(tg.i).iter().any(|&c| GRAPHICS.contains(&tg.ix.local(&tg.t, c))) {
        return Err(EditError::new("unsupported", "only a plain path can be edited with the pencil tools"));
    }
    let m = parent_transform(&tg.ix, &tg.t, tg.i).then_after(&item_transform(&tg.ix, &tg.t, tg.i));
    Ok((tg.all.clone(), m, tg.part.clone()))
}

/// Puts `with` (paths as `(points, closed)`) in place of the path `path` of a plain item, in the same object: a closed path the Erase tool
/// opened, a path smoothed, the pieces of one path of a compound path. A path that was closed and is open now loses its corner radii (as
/// Object ▸ Paths ▸ Open Path, convert.rs).
pub(super) fn replace_path(w: &mut Work<'_>, id: &str, path: usize, with: &[(Vec<PathPoint>, bool)]) -> Result<(), EditError> {
    let tg = target(w, id, path)?;
    if with.is_empty() || with.iter().any(|(p, _)| p.len() < 2) {
        return Err(EditError::new("invalid", "nothing would be left of the path"));
    }
    let geo = tg.ix.path(&tg.t, tg.i, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let el = *tg.ix.named(&tg.t, geo, "GeometryPathType").get(path).ok_or_else(|| EditError::new("notFound", format!("{id} has no path {path}")))?;
    let ind = indent_before(&tg.t, tg.ix.nodes[geo].start).to_owned();
    let xml: String = with.iter().map(|(p, closed)| compound::path_xml(p, *closed, &ind)).collect();
    let mut edits = vec![(element_span(&tg.ix, &tg.t, el), xml)];
    if tg.closed && tg.all.len() == 1 && with.iter().all(|(_, closed)| !closed) {
        for key in ["CornerRadius", "TopLeftCornerRadius", "TopRightCornerRadius", "BottomLeftCornerRadius", "BottomRightCornerRadius"] {
            #[allow(clippy::unnecessary_map_or, reason = "keep the parse-failure branch and floating-point zero test explicit")]
            if tg.ix.attr(&tg.t, tg.i, key).is_some_and(|v| v.parse::<f64>().map_or(true, |r| r != 0.0)) {
                edits.push(attr_edit(&tg.ix, &tg.t, tg.i, key, "0"));
            }
        }
    }
    w.splice(&tg.part, edits)
}

/// Puts open paths in place of the open path `path` of an item: the first stays in the object, the second (when there is one) is a new
/// object right above with the same settings (as Release Compound Path and the Scissors make them). Returns the new object's id.
pub(super) fn replace_with_pieces(w: &mut Work<'_>, id: &str, path: usize, pieces: &[Vec<PathPoint>]) -> Result<(), EditError> {
    let tg = target(w, id, path)?;
    match pieces {
        [] => Err(EditError::new("invalid", "nothing would be left of the path")),
        [one] => {
            let now: Vec<(Option<usize>, PathPoint)> = one.iter().map(|p| (None, *p)).collect();
            tg.write_list(w, path, &now)
        }
        [first, second] => {
            let geo = tg.ix.path(&tg.t, tg.i, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
            let first_path = *tg.ix.named(&tg.t, geo, "GeometryPathType").first().ok_or_else(|| EditError::new("xml", "no path"))?;
            let ind = indent_before(&tg.t, tg.ix.nodes[geo].start).to_owned();
            let span = element_span(&tg.ix, &tg.t, first_path);
            let whole = tg.ix.nodes[tg.i].clone();
            let element = &tg.t[whole.start..whole.end];
            let new_id = w.new_id();
            let rel = span.start - whole.start..span.end - whole.start;
            let copy = format!("{}{}{}", &element[..rel.start], compound::path_xml(second, false, &ind), &element[rel.end..])
                .replacen(&format!("Self=\"{id}\""), &format!("Self=\"{new_id}\""), 1);
            let indent = indent_before(&tg.t, whole.start).to_owned();
            let edits = vec![(span, compound::path_xml(first, false, &ind)), (whole.end..whole.end, format!("\n{indent}{copy}"))];
            w.splice(&tg.part, edits)?;
            w.created.push(new_id);
            Ok(())
        }
        _ => Err(EditError::new("invalid", "an erase leaves two paths at most")),
    }
}
