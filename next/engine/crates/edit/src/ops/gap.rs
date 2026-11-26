//! Coordinate-driven editing of a gap between page items (the Gap tool).
//!
//! The rules come from InDesign 2025 measured with REAL mouse drags (gaptool-c1; next/oracle/rules/gaptool.ko.md, cases in
//! next/oracle/measured/gap-tool-2025.json, replayed by tests/gap_measured.rs; the 54 posted-message cases of InDesign 2026 are in it again). In short,
//! for a pointer at `from`:
//! - the gap is the empty stretch along the scan line through the pointer, on the axis (x or y) where it has neighbours; a pointer inside an
//!   item does nothing. Items are the LEAVES of the spread (a group is not an item: its children are, InDesign 2025);
//! - a gap needs an item on both sides, except a lone neighbour also works (the page edge or the pasteboard is the other side);
//! - only items that touch the pointer's page move (the rest are walls); the items that move are the nearest items on both sides plus the items whose
//!   facing edge lies on the same line and that touch the same free band (an item crossing the band ends it); Shift = the nearest items only;
//!   Ctrl and Alt make no difference (real keys, three ways of holding them: not told apart);
//! - the drag moves the gap by the pointer delta along its axis (gap width stays), the shrinking items keep at least 8 pt;
//! - a locked neighbour stays where it is while the others move; so does a turned one (not a multiple of 90 degrees: then the items on the same line stay too);
//! - a document WITHOUT facing pages is stricter (InDesign 2025, measured): the tool works inside the page shrunk by 36 pt (whatever the margins and
//!   the zoom): a lone neighbour works only while the pointer lies in the 36 pt strip along the page edge; a neighbour whose width across the gap
//!   leaves that area, whose facing edge lies outside it, or that is turned, stops the whole drag.
//!   The 0.75 pt step of InDesign's pointer is the screen's business (`ui/dragunit.ts`).
use super::*;

/// Smallest size an item keeps while the gap moves into it (measured at 111.6 % and 50 % zoom).
const MIN_SIZE: f64 = 8.0;
const EDGE: f64 = 1e-6;
/// The Gap tool's working area is the page shrunk by this much on every side (InDesign 2025: 36 pt at every zoom, for margins of 0, 36, 72 and 100 pt).
const INSET: f64 = 36.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// A box along the axes (also turned by a multiple of 90 degrees or flipped): its bounds are its box.
    Plain,
    /// Turned by another angle: the Gap tool leaves the whole gap alone when it is a neighbour.
    Turned,
    /// Lines, buttons, sheared items: not built.
    Other,
}

struct Item { id: String, bounds: [f64; 4], to_local: Option<Mat>, kind: Kind, locked: bool, on_page: bool }
struct Gap { axis: usize, before: Option<usize>, after: Option<usize> }

/// The page the pointer is on and, in a document without facing pages, its working area (the page less INSET), in spread coordinates.
struct Area { page: [f64; 4], inner: Option<[f64; 4]> }

fn classify(kind: &str, m: &Mat) -> Kind {
    if !matches!(kind, "Rectangle" | "Oval" | "Polygon" | "TextFrame") { return Kind::Other; }
    let [a, b, c, d, ..] = m.0;
    let scale = a.abs().max(b.abs()).max(c.abs()).max(d.abs());
    if scale < 1e-12 { return Kind::Other; }
    let eps = 1e-6 * scale;
    if (b.abs() < eps && c.abs() < eps) || (a.abs() < eps && d.abs() < eps) { return Kind::Plain; }
    // a pure turn (perpendicular columns of the same length); anything else is a shear
    let (u, v) = (a.hypot(b), c.hypot(d));
    if (a * c + b * d).abs() < 1e-6 * u * v && (u - v).abs() < 1e-6 * u.max(v) { Kind::Turned } else { Kind::Other }
}

fn find(items: &[Item], at: [f64; 2], area: &Area) -> Option<Gap> {
    let candidates: Vec<Gap> = (0..2).filter_map(|axis| {
        let cross = 1 - axis;
        let band = |b: &[f64; 4]| b[cross] < at[cross] && at[cross] < b[cross + 2];
        if items.iter().any(|i| band(&i.bounds) && i.bounds[axis] <= at[axis] && at[axis] <= i.bounds[axis + 2]) { return None; }
        let before = items.iter().enumerate().filter(|(_, i)| band(&i.bounds) && i.bounds[axis + 2] < at[axis])
            .max_by(|(_, a), (_, b)| a.bounds[axis + 2].total_cmp(&b.bounds[axis + 2])).map(|(n, _)| n);
        let after = items.iter().enumerate().filter(|(_, i)| band(&i.bounds) && i.bounds[axis] > at[axis])
            .min_by(|(_, a), (_, b)| a.bounds[axis].total_cmp(&b.bounds[axis])).map(|(n, _)| n);
        // one side only (without facing pages): the page edge is the other side while the pointer lies in the strip between the inset and the page edge (measured)
        let in_strip = match (area.inner, before, after) {
            (Some(inner), Some(_), None) => at[axis] > inner[axis + 2] && at[axis] <= area.page[axis + 2],
            (Some(inner), None, Some(_)) => at[axis] < inner[axis] && at[axis] >= area.page[axis],
            _ => true,
        };
        (in_strip && (before.is_some() || after.is_some())).then_some(Gap { axis, before, after })
    }).collect();
    // a gap with items on both sides wins (the narrower one); otherwise the axis whose neighbour is nearer
    let key = |g: &Gap| match (g.before, g.after) {
        (Some(b), Some(a)) => (0, items[a].bounds[g.axis] - items[b].bounds[g.axis + 2]),
        (Some(b), None) => (1, at[g.axis] - items[b].bounds[g.axis + 2]),
        (None, Some(a)) => (1, items[a].bounds[g.axis] - at[g.axis]),
        (None, None) => (2, 0.0),
    };
    candidates.into_iter().min_by(|a, b| { let (ka, kb) = (key(a), key(b)); ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)) })
}

/// The items on each side that move with the nearest ones: those whose facing edge lies on the same line and that touch the free run of the band
/// (the strip between the two edges, seen from the pointer along the other axis) around the pointer.
fn groups(items: &[Item], g: &Gap, at: [f64; 2], nearest: bool) -> (Vec<usize>, Vec<usize>) {
    let axis = g.axis;
    let cross = 1 - axis;
    let (mut before, mut after) = (g.before.into_iter().collect::<Vec<_>>(), g.after.into_iter().collect::<Vec<_>>());
    if nearest { return (before, after); }
    let left = g.before.map(|n| items[n].bounds[axis + 2]);
    let right = g.after.map(|n| items[n].bounds[axis]);
    let band = match (left, right) {
        (Some(l), Some(r)) => (l, r),
        (Some(l), None) => (l, l + 2.0 * EDGE),
        (None, Some(r)) => (r - 2.0 * EDGE, r),
        (None, None) => return (before, after),
    };
    // an item that overlaps the band ends the run
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for (n, i) in items.iter().enumerate() {
        if Some(n) == g.before || Some(n) == g.after { continue; }
        // a turned item does not end the run (InDesign 2025: the items beside a turned one still move)
        if i.kind == Kind::Turned { continue; }
        if !(i.bounds[axis] < band.1 - EDGE && i.bounds[axis + 2] > band.0 + EDGE) { continue; }
        let (a, b) = (i.bounds[cross], i.bounds[cross + 2]);
        if a < at[cross] && at[cross] < b { return (before, after); }
        if b <= at[cross] { lo = lo.max(b); } else if a >= at[cross] { hi = hi.min(a); }
    }
    let touches = |i: &Item| i.bounds[cross] < hi - EDGE && i.bounds[cross + 2] > lo + EDGE;
    for (n, i) in items.iter().enumerate() {
        if before.contains(&n) || after.contains(&n) || !touches(i) { continue; }
        if left.is_some_and(|l| (i.bounds[axis + 2] - l).abs() < EDGE) { before.push(n); }
        else if right.is_some_and(|r| (i.bounds[axis] - r).abs() < EDGE) { after.push(n); }
    }
    (before, after)
}

/// Whether the width of an item across the gap lies inside the working area.
fn within_across(i: &Item, cross: usize, area: &Area) -> bool {
    area.inner.is_none_or(|inner| i.bounds[cross] >= inner[cross] - EDGE && i.bounds[cross + 2] <= inner[cross + 2] + EDGE)
}

/// The leaves of a spread (groups are looked through), without the hidden items: bounds in spread coordinates, the matrix back into the path's own space.
fn collect(ix: &Index, t: &str, dm: &str, dx: &Index, parent: usize, inherited_locked: bool, page: &[f64; 4], items: &mut Vec<Item>) {
    for &i in ix.children(parent) {
        if !is_item(ix, t, i) { continue; }
        let layer = ix.attr(t, i, "ItemLayer");
        let li = (0..dx.nodes.len()).find(|&k| dx.local(dm, k) == "Layer" && dx.attr(dm, k, "Self") == layer);
        // a hidden item is not there; an item on a hidden layer behaves as one on a locked layer (measured: the same results), i.e. as a locked item
        if ix.attr(t, i, "Visible") == Some("false") { continue; }
        let locked = inherited_locked || ix.attr(t, i, "Locked") == Some("true")
            || li.is_some_and(|k| dx.attr(dm, k, "Locked") == Some("true") || dx.attr(dm, k, "Visible") == Some("false"));
        let kind = ix.local(t, i);
        if kind == "Group" { collect(ix, t, dm, dx, i, locked, page, items); continue; }
        let Some(bounds) = spread_bounds(ix, t, i) else { continue; };
        let full = parent_transform(ix, t, i).then_after(&item_transform(ix, t, i));
        if let Some(id) = ix.attr(t, i, "Self") {
            let on_page = bounds[0] <= page[2] && bounds[2] >= page[0] && bounds[1] <= page[3] && bounds[3] >= page[1];
            items.push(Item { id: id.into(), bounds, to_local: full.invert(), kind: classify(kind, &full), locked, on_page });
        }
    }
}

pub(crate) fn adjust(w: &mut Work<'_>, page: &str, from: &[f64; 2], to: &[f64; 2], nearest: &bool, _resize_gap: &bool, _move_items: &bool) -> Result<(), EditError> {
    // Ctrl (resize the gap) and Alt (move the items) make no difference to a drag in InDesign 2025 (real keys, before, during and over the gap): both flags are ignored.
    adjust_keys(w, page, from, to, nearest, &false, &false)
}

/// The Gap tool with the modifier keys InDesign's help describes (adjustGapKeys, tools-c2; the real-key measurement above found InDesign 2025
/// not telling them apart, so this follows the help): `resize_gap` (Ctrl) the gap gets wider by the drag (each side by half of it) and the items beside
/// it change size; `move_items` (Alt) the gap and the items beside it move by the drag; both (Ctrl+Alt) the items move apart by half the drag each.
pub(crate) fn adjust_keys(w: &mut Work<'_>, page: &str, from: &[f64; 2], to: &[f64; 2], nearest: &bool, resize_gap: &bool, move_items_too: &bool) -> Result<(), EditError> {
    if from.iter().chain(to).any(|n| !n.is_finite()) { return Err(EditError::new("invalid", "gap coordinates must be finite")); }
    let (part, ix, pi) = w.find(page)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, pi) != "Page" { return Err(EditError::new("invalid", "gap destination must be a page")); }
    let pm = item_transform(&ix, &t, pi);
    if pm.0[..4] != [1.0, 0.0, 0.0, 1.0] { return Err(EditError::new("unsupported", "gap on a transformed page")); }
    let pb: Vec<f64> = ix.attr(&t, pi, "GeometricBounds").unwrap_or("").split_whitespace().filter_map(|s| s.parse().ok()).collect();
    if pb.len() != 4 { return Err(EditError::new("xml", "invalid page bounds")); }
    let origin = pm.apply((pb[1], pb[0]));
    let at = [from[0] + origin.0, from[1] + origin.1];
    let page_box = [origin.0, origin.1, origin.0 + (pb[3] - pb[1]), origin.1 + (pb[2] - pb[0])];
    let facing = w.text("Resources/Preferences.xml").ok().and_then(|prefs| {
        let px = Index::scan(prefs).ok()?;
        (0..px.nodes.len()).find(|&k| px.local(prefs, k) == "DocumentPreference").map(|k| px.attr(prefs, k, "FacingPages") == Some("true"))
    }).unwrap_or(false);
    let area = Area { page: page_box, inner: (!facing).then_some([page_box[0] + INSET, page_box[1] + INSET, page_box[2] - INSET, page_box[3] - INSET]) };
    let spread = ix.nodes[pi].parent.ok_or_else(|| EditError::new("xml", "page without spread"))?;
    let dm = w.text("designmap.xml")?;
    let dx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
    let mut items = Vec::new();
    collect(&ix, &t, dm, &dx, spread, false, &page_box, &mut items);
    let g = find(&items, at, &area).ok_or_else(|| EditError::new("notFound", "no gap at this position"))?;
    let axis = g.axis;
    let cross = 1 - axis;
    let mut delta = to[axis] - from[axis];
    if delta.abs() < 1e-10 { return Ok(()); }
    // Without facing pages a neighbour the tool cannot work with stops the drag (InDesign 2025 does nothing): one that is turned, or whose width across the
    // gap or whose facing edge lies outside the working area (the facing edge of a lone neighbour is free: the pointer is in the page-edge strip then).
    let lone = g.before.is_none() || g.after.is_none();
    let mut nearest = *nearest;
    for (n, is_before) in [(g.before, true), (g.after, false)] {
        let Some(n) = n else { continue; };
        let i = &items[n];
        if let Some(inner) = area.inner {
            let facing_edge = if is_before { i.bounds[axis + 2] } else { i.bounds[axis] };
            let facing_ok = lone || (facing_edge >= inner[axis] - EDGE && facing_edge <= inner[axis + 2] + EDGE);
            if i.kind == Kind::Turned || !within_across(i, cross, &area) || !facing_ok {
                return Err(EditError::new("notFound", "a neighbour of the gap cannot be moved (turned, or outside the page's 36 pt working area)"));
            }
        } else if i.kind == Kind::Turned {
            // with facing pages a turned neighbour stays and the items on the same line stay with it: only the nearest item on the other side moves
            nearest = true;
        }
        if i.kind == Kind::Other && !i.locked { return Err(EditError::new("unsupported", "gap touches a line, a sheared item or an object that is not built")); }
    }
    let (before, after) = groups(&items, &g, at, nearest);
    // a locked item stays where it is (measured): the others still move; so does an item on the same line that is turned or leaves the working area
    let movable = |n: &usize| { let i = &items[*n]; !i.locked && i.kind == Kind::Plain && i.to_local.is_some() && i.on_page && within_across(i, cross, &area) };
    let (before, after): (Vec<usize>, Vec<usize>) = (before.into_iter().filter(movable).collect(), after.into_iter().filter(movable).collect());
    // the items the gap moves into keep MIN_SIZE (measured: 92 of 100 pt, 42 of 50 pt)
    let room = |group: &[usize]| group.iter().map(|&n| (items[n].bounds[axis + 2] - items[n].bounds[axis] - MIN_SIZE).max(0.0)).fold(f64::INFINITY, f64::min);
    // the modifier keys (adjustGapKeys): how far the edge before the gap and the edge after it go for the drag, and whether the items move whole
    let width = match (g.before, g.after) { (Some(b), Some(a)) => (items[a].bounds[axis] - items[b].bounds[axis + 2]).max(0.0), _ => f64::INFINITY };
    let (shift_before, shift_after) = match (*resize_gap, *move_items_too) {
        (false, false) => {
            delta = if delta > 0.0 { delta.min(room(&after)) } else { delta.max(-room(&before)) };
            (delta, delta)
        }
        (false, true) => (delta, delta),
        (true, moving) => {
            // a narrower gap stops at no gap; a wider one stops where an item beside it keeps MIN_SIZE (when the items change size)
            delta = delta.max(-width);
            if !moving && delta > 0.0 { delta = delta.min(2.0 * room(&before).min(room(&after))); }
            (-delta / 2.0, delta / 2.0)
        }
    };
    if delta.abs() < 1e-10 { return Ok(()); }
    if *move_items_too {
        // the items keep their size and move whole (their content with them)
        let ids = |group: &[usize]| group.iter().map(|&n| items[n].id.clone()).collect::<Vec<_>>();
        let by = |d: f64| if axis == 0 { (d, 0.0) } else { (0.0, d) };
        if !before.is_empty() { move_items(w, &ids(&before), by(shift_before))?; }
        if !after.is_empty() { move_items(w, &ids(&after), by(shift_after))?; }
        return Ok(());
    }
    let mut changes = Vec::new();
    for (members, is_before) in [(&before, true), (&after, false)] {
        for &n in members {
            let mut bounds = items[n].bounds;
            bounds[axis + if is_before { 2 } else { 0 }] += if is_before { shift_before } else { shift_after };
            if bounds[axis + 2] - bounds[axis] <= 1e-6 { return Err(EditError::new("invalid", "gap drag would collapse an item")); }
            changes.push((&items[n], bounds));
        }
    }
    for (item, bounds) in changes {
        let inv = item.to_local.ok_or_else(|| EditError::new("invalid", "singular item transform"))?;
        let (a, b) = (inv.apply((bounds[0], bounds[1])), inv.apply((bounds[2], bounds[3])));
        // a box turned by a quarter or flipped swaps the corners: the item's own box is the span of the two
        resize(w, &item.id, Target::Box([a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]))?;
    }
    Ok(())
}
