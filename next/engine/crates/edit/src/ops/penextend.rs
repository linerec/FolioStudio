//! The Pen tool on an open path (screens-doc): a click on an end point of a selected open path picks the path up, the clicks after it add points to
//! that end (the path goes on being drawn), a click on an end point of another open path joins the two there, and a click on the other end point of
//! the same path closes it. One command takes what the screen collected: `anchors` / `lefts` / `rights` are the new points in page coordinates of
//! `page` (a handle equal to its anchor is no handle; the same shape as the points of a new path), `at_start` says which end of `id` is continued.
//! The path keeps its element, settings and place; a joined path's element is gone (as Object ▸ Paths ▸ Join leaves it, pathjoin.rs).
//! Measured against InDesign 2025 with the real mouse where the session allowed (artifacts/screens-doc-20261023); the result is the path a script's
//! `pathPoints` would hold, so the points of the old path never move.
use super::*;
use super::pathjoin::turned;

/// How near two anchors must be to count as the same place.
const SAME: f64 = 1e-4;

fn finite(list: &[[f64; 2]]) -> bool {
    list.iter().all(|p| p[0].is_finite() && p[1].is_finite())
}

/// `id` continued at its start (`at_start`) or end with the points, then joined to the end point of `join` (`join_at_start`: its first point; empty:
/// no join; `join == id`: the path is closed instead).
pub(crate) fn run(w: &mut Work<'_>, id: &str, page: &str, at_start: &bool, anchors: &[[f64; 2]], lefts: &[[f64; 2]], rights: &[[f64; 2]], join: &str, join_at_start: &bool) -> Result<(), EditError> {
    if anchors.len() != lefts.len() || anchors.len() != rights.len() {
        return Err(EditError::new("invalid", "the new points need an anchor, a left and a right handle each"));
    }
    if !finite(anchors) || !finite(lefts) || !finite(rights) {
        return Err(EditError::new("invalid", "the new points need coordinates that are numbers"));
    }
    if anchors.is_empty() && join.is_empty() {
        return Err(EditError::new("invalid", "there is nothing to continue the path with"));
    }
    let (pts, m, part) = pathedit::open_path(w, id, 0)?;
    let (ppart, pix, pi) = w.find(page)?;
    if pix.local(w.text(&ppart)?, pi) != "Page" || ppart != part {
        return Err(EditError::new("invalid", "the page is not on the spread of the path"));
    }
    let pt = w.text(&ppart)?.to_owned();
    let inv = m.invert().ok_or_else(|| EditError::new("xml", "a path with a singular transform"))?;
    let (ox, oy) = pencil::page_origin(&pix, &pt, pi)?;
    let mapped = |p: [f64; 2]| inv.apply((p[0] + ox, p[1] + oy));
    // the end that is continued is the last point from here on
    let mut out = if *at_start { turned(&pts) } else { pts.clone() };
    for k in 0..anchors.len() {
        out.push(PathPoint { anchor: mapped(anchors[k]), left: mapped(lefts[k]), right: mapped(rights[k]) });
    }
    let close = !join.is_empty() && join == id;
    if !join.is_empty() && !close {
        let (bpts, bm, bpart) = pathedit::open_path(w, join, 0)?;
        if bpart != part {
            return Err(EditError::new("invalid", "the other path is on another spread"));
        }
        let to_a = inv.then_after(&bm);
        let mut b: Vec<PathPoint> = bpts.iter().map(|p| PathPoint { anchor: to_a.apply(p.anchor), left: to_a.apply(p.left), right: to_a.apply(p.right) }).collect();
        // the end that is joined is B's first point from here on
        if !*join_at_start {
            b = turned(&b);
        }
        let mut rest = b.into_iter();
        if let Some(head) = rest.next() {
            let last = out.len() - 1;
            if (out[last].anchor.0 - head.anchor.0).hypot(out[last].anchor.1 - head.anchor.1) < SAME {
                out[last].right = head.right;
            } else {
                out.push(head);
            }
        }
        out.extend(rest);
    }
    // the continued path keeps its direction: a path continued at its start is turned back
    if *at_start && !close {
        out = turned(&out);
    }
    if out.len() < 2 {
        return Err(EditError::new("invalid", "a path needs two points"));
    }
    if close && out.len() < 3 {
        return Err(EditError::new("invalid", "a closed path needs three points"));
    }
    pathedit::replace_with_pieces(w, id, 0, &[out])?;
    if close {
        convert::convert_shape(w, &[id.to_owned()], convert::Shape::ClosedPath)?;
    }
    if !join.is_empty() && !close {
        let (bpart, bix, bi) = w.find(join)?;
        let bt = w.text(&bpart)?.to_owned();
        w.splice(&bpart, vec![(element_span(&bix, &bt, bi), String::new())])?;
    }
    w.created.push(id.to_owned());
    Ok(())
}
