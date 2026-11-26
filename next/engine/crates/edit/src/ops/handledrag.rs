//! Dragging a selection handle, and turning a selection from just outside a handle, as one engine command each.
//!
//! The screen sends only where the pointer went and which modifier keys were down; the new box and the angle are worked out
//! here, so that a script (or an LLM) can do the same drag with the same command.
//!
//! Rules (measured in InDesign 2025 with real mouse input, `artifacts/docinput-c2-20261023`, `next/oracle/rules/docinput-c2.ko.md`;
//! only the 1 pt minimum is Folio's own choice: InDesign lets the box turn inside out):
//! - A handle is `[fx, fy]`, each 0 (left/top edge), 0.5 (middle) or 1 (right/bottom edge); `[0.5, 0.5]` is no handle.
//! - A corner moves two edges, a side handle one. The opposite edge stays, or with `from_center` (Alt) the middle stays.
//! - `keep_proportions` (Shift) keeps the width : height of the box. On a corner the LARGER of the two factors (new size / old size)
//!   wins, also when both shrink (the smaller shrink wins) or one grows and the other shrinks (the growing one wins); on a side handle
//!   the dragged side decides and the other side scales about its middle line.
//! - Past the opposite edge (the middle with Alt) the object is turned inside out: the box lies on the other side of the staying edge, as
//!   far from it as the pointer, and the item is mirrored about that axis (`ItemTransform` gets a -1). Measured with real mouse input in InDesign
//!   2025 (`artifacts/docinput-c3-20261023`); only the 0.1 pt floor is Folio's own.
//! - One object is resized in its own (possibly turned) frame, as its handles are drawn; a selection of several objects in
//!   the spread's axes, about the box that holds them all.
//! - Turning: InDesign always turns about the middle of the selection box (a rotated single object: its own middle; several objects:
//!   the middle of the box that holds them), whatever the Control panel proxy says, so the screen sends `anchor` [0.5, 0.5]; the
//!   command still takes any reference point. `constrain` (Shift) rounds the turn (not the object's angle) to a multiple of 45°.
use super::*;

/// The smallest width or height a handle drag leaves (points). InDesign has no such floor of its own: the smallest it was seen to leave
/// is 0.2 pt (a box that is 0 pt wide cannot be made), so this only keeps the box from vanishing.
pub const MIN_SIZE: f64 = 0.1;

fn side(f: f64) -> f64 {
    if f < 0.25 { -1.0 } else if f > 0.75 { 1.0 } else { 0.0 }
}

fn sign(v: f64) -> f64 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

/// The box `b` (`[x0, y0, x1, y1]`) after its `handle` is dragged by `d` (in the box's own axes), and for each axis whether the
/// handle went past the opposite edge (or, with `center`, past the middle): the box is then on the other side of that edge and
/// mirrored. InDesign 2025 (flip measured with real mouse input): the size is the signed distance from the staying edge to the
/// pointer, its absolute value is the new size, and a negative one turns the object inside out about that axis. Shift takes the larger
/// of the two absolute factors on a corner and each axis keeps its own sign.
pub fn handle_box(b: [f64; 4], handle: [f64; 2], d: [f64; 2], keep: bool, center: bool) -> ([f64; 4], [bool; 2]) {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if !(w > 0.0 && h > 0.0) {
        return (b, [false, false]);
    }
    let (sx, sy) = (side(handle[0]), side(handle[1]));
    let k = if center { 2.0 } else { 1.0 };
    // signed sizes: negative is past the staying edge
    let mut nw = if sx == 0.0 { w } else { w + sx * d[0] * k };
    let mut nh = if sy == 0.0 { h } else { h + sy * d[1] * k };
    if keep && (sx != 0.0 || sy != 0.0) {
        let (fw, fh) = (nw / w, nh / h);
        // a corner: the larger factor (measured in InDesign 2025, see above); a side: the dragged side decides
        let s = match (sx != 0.0, sy != 0.0) {
            (true, true) => fw.abs().max(fh.abs()),
            (true, false) => fw.abs(),
            _ => fh.abs(),
        }
        .max(MIN_SIZE / w)
        .max(MIN_SIZE / h);
        nw = w * s * if sx != 0.0 { sign(fw) } else { 1.0 };
        nh = h * s * if sy != 0.0 { sign(fh) } else { 1.0 };
    } else {
        if sx != 0.0 && nw.abs() < MIN_SIZE {
            nw = MIN_SIZE * sign(nw);
        }
        if sy != 0.0 && nh.abs() < MIN_SIZE {
            nh = MIN_SIZE * sign(nh);
        }
    }
    // the edge (or middle) that stays; a negative size puts the box on the other side of it
    let span = |lo: f64, hi: f64, n: f64, s: f64| -> (f64, f64) {
        if center || s == 0.0 {
            let m = (lo + hi) / 2.0;
            (m - n.abs() / 2.0, m + n.abs() / 2.0)
        } else if s > 0.0 {
            if n >= 0.0 { (lo, lo + n) } else { (lo + n, lo) }
        } else if n >= 0.0 {
            (hi - n, hi)
        } else {
            (hi, hi - n)
        }
    };
    let (x0, x1) = span(b[0], b[2], nw, sx);
    let (y0, y1) = span(b[1], b[3], nh, sy);
    ([x0, y0, x1, y1], [sx != 0.0 && nw < 0.0, sy != 0.0 && nh < 0.0])
}

/// The box after the drag (see [`handle_box`]), without the mirroring.
pub fn handle_rect(b: [f64; 4], handle: [f64; 2], d: [f64; 2], keep: bool, center: bool) -> [f64; 4] {
    handle_box(b, handle, d, keep, center).0
}

/// The turn (degrees, counter-clockwise, y down) that takes the pointer from `from` to `to` about `center`; `constrain` rounds
/// it to a multiple of 45°.
pub fn turn_angle(center: [f64; 2], from: [f64; 2], to: [f64; 2], constrain: bool) -> Option<f64> {
    if (from[0] - center[0]).hypot(from[1] - center[1]) < 1e-6 || (to[0] - center[0]).hypot(to[1] - center[1]) < 1e-6 {
        return None;
    }
    let a = ((center[1] - to[1]).atan2(to[0] - center[0]) - (center[1] - from[1]).atan2(from[0] - center[0])).to_degrees();
    let a = (a + 540.0).rem_euclid(360.0) - 180.0;
    Some(if constrain { (a / 45.0).round() * 45.0 } else { a })
}

fn anchor_of(fx: f64, fy: f64) -> Anchor {
    match (side(fy) as i32, side(fx) as i32) {
        (-1, -1) => Anchor::TopLeft,
        (-1, 0) => Anchor::Top,
        (-1, _) => Anchor::TopRight,
        (0, -1) => Anchor::Left,
        (0, 0) => Anchor::Center,
        (0, _) => Anchor::Right,
        (_, -1) => Anchor::BottomLeft,
        (_, 0) => Anchor::Bottom,
        _ => Anchor::BottomRight,
    }
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

/// Dragging `handle` of the selection by `by` (spread coordinates): `resizeByHandle`.
pub(crate) fn resize(w: &mut Work<'_>, ids: &[String], handle: &[f64; 2], by: &[f64; 2], keep: &bool, center: &bool) -> Result<(), EditError> {
    if !finite(handle) || !finite(by) || handle.iter().any(|f| ![0.0, 0.5, 1.0].contains(f)) || (handle[0] == 0.5 && handle[1] == 0.5) {
        return Err(EditError::new("invalid", "a handle is [0, 0.5 or 1, 0, 0.5 or 1] (not the middle), the drag two finite numbers"));
    }
    let ids = expand(w, ids);
    if ids.is_empty() {
        return Err(EditError::new("invalid", "nothing is selected"));
    }
    if ids.len() == 1 {
        let id = &ids[0];
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let b = if ix.local(&t, i) == "Group" {
            own_bounds(&ix, &t, i, &Mat::ID)
        } else {
            own_path(&ix, &t, i).and_then(|p| geom::path_bounds(&p.subpaths.iter().map(|(_, pts, closed)| (pts.clone(), *closed)).collect::<Vec<_>>()))
        }
        .ok_or_else(|| EditError::new("unsupported", format!("{id} has no path to resize")))?;
        let m = parent_transform(&ix, &t, i).then_after(&item_transform(&ix, &t, i));
        let (dx, dy) = m.unapply_vector((by[0], by[1])).ok_or_else(|| EditError::new("unsupported", format!("{id} cannot be resized: its transform cannot be inverted")))?;
        let (rect, flip) = handle_box(b, *handle, [dx, dy], *keep, *center);
        resize_box(w, id, rect)?;
        return if flip[0] || flip[1] { mirror_frame(w, id, rect, flip) } else { Ok(()) };
    }
    // several objects: the box that holds them, in spread axes, resized about the edge opposite the handle (or the middle)
    let mut u: Option<[f64; 4]> = None;
    for id in &ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        if ix.ancestors(i).any(|a| ix.attr(&t, a, "Self").is_some_and(|s| ids.iter().any(|x| x == s))) {
            continue;
        }
        let m = parent_transform(&ix, &t, i).then_after(&item_transform(&ix, &t, i));
        let b = own_bounds(&ix, &t, i, &m).ok_or_else(|| EditError::new("unsupported", format!("{id} has no bounds")))?;
        u = Some(u.map_or(b, |p| [p[0].min(b[0]), p[1].min(b[1]), p[2].max(b[2]), p[3].max(b[3])]));
    }
    let u = u.ok_or_else(|| EditError::new("invalid", "nothing to resize"))?;
    let (r, flip) = handle_box(u, *handle, *by, *keep, *center);
    let anchor = if *center { Anchor::Center } else { anchor_of(1.0 - handle[0], 1.0 - handle[1]) };
    resize_group::many(w, &ids, [r[2] - r[0], r[3] - r[1]], anchor, false)?;
    // turned inside out: the objects are mirrored about the edge that stayed (the middle with Alt)
    match flip {
        [false, false] => Ok(()),
        [true, false] => mirror(w, &ids, Axis::Horizontal, anchor),
        [false, true] => mirror(w, &ids, Axis::Vertical, anchor),
        [true, true] => mirror(w, &ids, Axis::Both, anchor),
    }
}

/// Mirrors an item in its own axes about the middle of `rect` (its path box, own coordinates): the transform gets the flip after it.
fn mirror_frame(w: &mut Work<'_>, id: &str, rect: [f64; 4], flip: [bool; 2]) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
    let f = Mat([if flip[0] { -1.0 } else { 1.0 }, 0.0, 0.0, if flip[1] { -1.0 } else { 1.0 }, if flip[0] { 2.0 * cx } else { 0.0 }, if flip[1] { 2.0 * cy } else { 0.0 }]);
    let m = item_transform(&ix, &t, i).then_after(&f);
    let e = attr_edit(&ix, &t, i, "ItemTransform", &m.text());
    w.splice(&part, vec![e])
}

fn resize_box(w: &mut Work<'_>, id: &str, rect: [f64; 4]) -> Result<(), EditError> {
    super::resize(w, id, Target::Box(rect))
}

/// Turning the selection by dragging from just outside a handle: `rotateByDrag`. `from` and `to` are the pointer's first and
/// last place (spread coordinates), `anchor` the reference point `[fx, fy]` (0, 0.5 or 1) of the selection that stays.
pub(crate) fn rotate(w: &mut Work<'_>, ids: &[String], from: &[f64; 2], to: &[f64; 2], constrain: &bool, anchor: &[f64; 2]) -> Result<(), EditError> {
    if !finite(from) || !finite(to) || !finite(anchor) {
        return Err(EditError::new("invalid", "a turn needs finite points"));
    }
    let ids = expand(w, ids);
    if ids.is_empty() {
        return Err(EditError::new("invalid", "nothing is selected"));
    }
    let anchor = anchor_of(anchor[0], anchor[1]);
    let (cx, cy) = union_anchor(w, &ids, anchor)?;
    let by = turn_angle([cx, cy], *from, *to, *constrain)
        .ok_or_else(|| EditError::new("invalid", "the pointer is on the reference point: there is no angle"))?;
    if by == 0.0 {
        return Ok(());
    }
    turn(w, &ids, by, anchor)
}
