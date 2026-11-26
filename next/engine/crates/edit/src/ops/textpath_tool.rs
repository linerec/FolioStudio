//! Pointer-driven edits of text on a path: the Selection tool dragging a bracket (start, end, centre) of an open path.
//!
//! Rules measured in InDesign 2025 with real mouse input (next/oracle/rules/typepath-c8.ko.md section 4, 12 drags on a 360 pt line):
//! - the start and end bracket take the pointer's projection onto the path (its distance along it); a bracket stops at the other bracket
//!   and at the ends of the path; pulling the pointer off the path sideways does not change the value;
//! - the centre bracket moves both brackets by the pointer's travel along the path (a pointer that crosses to the other side of the path
//!   flips the text and swaps the two values; the model cannot hold that yet, so it is refused here, not approximated).
use super::*;
use std::collections::BTreeMap;

/// InDesign lets a bracket stop on the other one; the model reads a range only when it has some length, so it stops this far short of it.
const GAP: f64 = 0.01;
/// A centre-bracket drag whose pointer ends no more than this far above the path (points, glyph-top side) flips the text: InDesign flipped it with the
/// pointer on the path and did not with it 0.7 pt above.
const FLIP_SIDE: f64 = 0.3;

/// A point in spread coordinates in the item's own coordinates (through the item's and its groups' transforms).
pub(super) fn to_inner(ix: &Index, t: &str, i: usize, point: [f64; 2]) -> Result<(f64, f64), EditError> {
    let mut m = item_transform(ix, t, i);
    for a in ix.ancestors(i) {
        if is_item(ix, t, a) { m = item_transform(ix, t, a).then_after(&m); }
    }
    let inverse = m.invert().ok_or_else(|| EditError::new("invalid", "the item's transform cannot be undone"))?;
    Ok(inverse.apply((point[0], point[1])))
}

/// The brackets `(StartBracket, EndBracket)` a drag of `which` to `point` would give the text path of `id` (spread coordinates, `from` where the centre drag
/// began), without changing anything: the Selection tool's live preview and the edit itself read the same numbers.
pub(crate) fn drag_values(w: &mut Work<'_>, id: &str, which: &str, point: &[f64; 2], from: &[f64; 2]) -> Result<crate::BracketDrag, EditError> {
    if point.iter().chain(from).any(|v| !v.is_finite()) { return Err(EditError::new("invalid", "the drag needs finite points")); }
    if !matches!(which, "start" | "end" | "center") { return Err(EditError::new("invalid", "which is start, end or center")); }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let source = &t[ix.nodes[i].start..ix.nodes[i].end];
    let model = folio_idml::textpath::supported(source).ok_or_else(|| EditError::new("unsupported", "select an unthreaded open text path"))?;
    let measure = folio_idml::textpath::measure_of(source).ok_or_else(|| EditError::new("unsupported", "the path has no length"))?;
    let closed = model.curve.as_ref().is_some_and(|c| !c.open);
    if model.flipped { return Err(EditError::new("unsupported", "bracket drags of a flipped text path are not measured")); }
    let child = ix.children(i).iter().copied().find(|&c| ix.local(&t, c) == "TextPath").ok_or_else(|| EditError::new("invalid", "item has no text path"))?;
    let number = |name: &str| ix.attr(&t, child, name).and_then(|v| v.parse::<f64>().ok()).ok_or_else(|| EditError::new("invalid", format!("text path has no {name}")));
    let (start, end, length) = (number("StartBracket")?, number("EndBracket")?, measure.length());
    let landing = |p: [f64; 2]| -> Result<folio_model::textpath::Nearest, EditError> {
        let inner = to_inner(&ix, &t, i, p)?;
        Ok(measure.nearest(folio_model::Point::new(inner.0, inner.1)))
    };
    let mut flip = false;
    let now = if closed {
        // A closed shape (InDesign 2025, rules/typepath-c14.ko.md section 3): a bracket takes the pointer's distance round the outline, past the other bracket
        // too (the range then wraps over the path's start); the centre moves both by the shorter way round between the press and the pointer.
        let round = |d: f64| d.rem_euclid(length);
        let apart = |a: f64, b: f64| { let d = (a - b).rem_euclid(length); d.min(length - d) };
        match which {
            "start" => { let at = round(landing(*point)?.at); (if apart(at, end) < GAP { start } else { at }, end) }
            "end" => { let at = round(landing(*point)?.at); (start, if apart(at, start) < GAP { end } else { at }) }
            _ => {
                let (now, before) = (landing(*point)?, landing(*from)?);
                // (a pointer exactly on the outline flipped on some edges and not on others in InDesign: only one clearly inside is refused)
                if now.side < -0.5 && before.side >= -0.5 {
                    return Err(EditError::new("unsupported", "dragging the centre bracket across the outline of a closed shape flips the text; how InDesign draws that is not measured yet"));
                }
                let travel = (now.at - before.at + length / 2.0).rem_euclid(length) - length / 2.0;
                (round(start + travel), round(end + travel))
            }
        }
    } else { match which {
        "start" => (landing(*point)?.at.clamp(0.0, end - GAP), end),
        "end" => (start, landing(*point)?.at.clamp(start + GAP, length)),
        _ => {
            let (now, before) = (landing(*point)?, landing(*from)?);
            let travel = (now.at - before.at).clamp(-start, length - end);
            // A pointer that ends on the path or beyond it flips the text and swaps the brackets (InDesign 2025, typepath-c8 section 4, c14 p5/p6: 60 / 300 dragged
            // 28.51 along and below became 328.51 / 88.51, flipped); one that stays above the path just carries them.
            if now.side < FLIP_SIDE {
                flip = true;
                (end + travel, start + travel)
            } else { (start + travel, end + travel) }
        }
    } };
    Ok(crate::BracketDrag { start: now.0, end: now.1, was_start: start, was_end: end, flip })
}

/// Drag one bracket of the text path of `id`: `which` is "start", "end" or "center" (both); `point` where the pointer is (spread
/// coordinates), `from` where the centre drag began (only the centre uses it).
pub(crate) fn set_bracket(w: &mut Work<'_>, id: &str, which: &str, point: &[f64; 2], from: &[f64; 2]) -> Result<(), EditError> {
    let crate::BracketDrag { start, end, flip, .. } = drag_values(w, id, which, point, from)?;
    let mut props = BTreeMap::new();
    if flip { props.insert("FlipPathEffect".to_owned(), "Flipped".to_owned()); }
    match which {
        "start" => { props.insert("StartBracket".to_owned(), start.to_string()); }
        "end" => { props.insert("EndBracket".to_owned(), end.to_string()); }
        _ => {
            props.insert("StartBracket".to_owned(), start.to_string());
            props.insert("EndBracket".to_owned(), end.to_string());
        }
    }
    super::textpath::set_options(w, &[id.to_owned()], &props)
}
