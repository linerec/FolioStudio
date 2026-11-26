//! A scaled text frame: its own settings, and the text it shows (oracle: tfscale-c1; InDesign 2025 transform() on text frames).
//!
//! The frame's path and stroke scale as any shape's (`scale::bake_with`). Of its text frame options: the column gutter scales by sx;
//! the insets scale by their own axis (top and bottom by sy, left and right by sx) plus the stroke's share when the two axes differ:
//! `weight · (axis / smaller − 1)` on the axis scaled more (measured on a stroke of 1, 2 and 6, and on scales (1.5, 0.8), (2, 1),
//! (1, 2), (0.5, 2), (3, 0.5): the share is that, and 0 when the axes are scaled alike); the column width is the frame's
//! again. Everything else of the options stays. The text is `text::format::scale_text`'s, for the characters the frame shows
//! (`Editor::set_frame_spans`: the composition's).
use super::*;
use textframe::Prefs;

/// A text frame the measurement did not cover is refused: one that is scaled or flipped in its own transform, or in a group that is
/// transformed. A frame that is only turned or sheared is measured: it takes the scale along its own axes (`baked_factors`).
pub(super) fn refuse_unmeasured(ix: &Index, t: &str, i: usize, baked: bool) -> Result<(), EditError> {
    if !baked {
        let id = ix.attr(t, i, "Self").unwrap_or_default();
        return Err(EditError::new("unsupported", format!("scaling {id}: a text frame that is scaled or flipped in its transform, or in a group that is moved or turned (not measured)")));
    }
    Ok(())
}

/// The text frames in the group `i` (at any depth), by id.
pub(super) fn members(ix: &Index, t: &str, i: usize) -> Vec<String> {
    let mut out = Vec::new();
    for &c in ix.children(i) {
        match ix.local(t, c) {
            "TextFrame" => out.extend(ix.attr(t, c, "Self").map(str::to_owned)),
            "Group" => out.extend(members(ix, t, c)),
            _ => {}
        }
    }
    out
}

/// What the stroke of a text frame is: its weight (0 when it shows nothing) and how far it reaches inside the frame's outline.
fn stroke_of(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> (f64, f64) {
    let weight: f64 = stroke_attr(w, ix, t, i, "StrokeWeight").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let visible = weight > 0.0 && stroke_attr(w, ix, t, i, "StrokeColor").is_some_and(|c| c != "Swatch/None");
    if visible { (weight, textframe::stroke_inset(w, ix, t, i)) } else { (0.0, 0.0) }
}

/// The geometric width and height of the frame's outline (in its own space).
fn outline(ix: &Index, t: &str, i: usize) -> Result<[f64; 2], EditError> {
    let id = ix.attr(t, i, "Self").unwrap_or_default();
    let path = own_path(ix, t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, q, c)| (q.clone(), *c)).collect();
    let [x0, y0, x1, y1] = geom::path_bounds(&shapes).ok_or_else(|| EditError::new("unsupported", format!("{id} has an empty path")))?;
    Ok([x1 - x0, y1 - y0])
}

/// The scale the text and the settings of a text frame take, across and down. It is the scale itself, except when strokes are not
/// scaled and the frame has a visible stroke: then InDesign takes the new inner size as the outer size scaled less twice the stroke
/// weight, `factor = (s · outer − 2 · weight) / inner`, with the outer and inner length of the stroke on that axis (centred stroke:
/// the outline plus and minus the weight; inside: the outline and less twice the weight; outside: plus twice the weight and the
/// outline). Measured on strokes of 2, 4, 6 and 10 in all three alignments, scales from 0.25 to 3 (oracle: tfscale-c2
/// `measurement/tfscale-c2-nostroke*.json`, `align.json`): the 309 numeric story properties and the insets and gutter follow it.
pub(super) fn factors(w: &Work<'_>, ix: &Index, t: &str, i: usize, sx: f64, sy: f64, adjust: Option<Adjust>) -> Result<(f64, f64), EditError> {
    let (weight, inner) = stroke_of(w, ix, t, i);
    if weight <= 0.0 || adjust.is_none_or(|a| a.stroke) {
        return Ok((sx, sy));
    }
    let id = ix.attr(t, i, "Self").unwrap_or_default();
    let outer = weight - inner;
    let [width, height] = outline(ix, t, i)?;
    let along = |s: f64, len: f64| -> Result<f64, EditError> {
        let (full, room) = (len + 2.0 * outer, len - 2.0 * inner);
        let f = (s * full - 2.0 * weight) / room;
        if room <= 1e-9 || f <= 0.0 {
            return Err(EditError::new("invalid", format!("scaling {id}: the stroke is too heavy for the frame")));
        }
        Ok(f)
    };
    Ok((along(sx, width)?, along(sy, height)?))
}

/// The frame's own settings after the scale (text frame options), as edits of its part; `(tx, ty)` is the scale the settings take
/// (`factors`). With fixed-width columns the frame's width follows the columns: the width it must take is the second result.
pub(super) fn frame_edits(w: &Work<'_>, ix: &Index, t: &str, i: usize, sx: f64, sy: f64, (tx, ty): (f64, f64), adjust: Option<Adjust>) -> Result<(Vec<(std::ops::Range<usize>, String)>, Option<f64>), EditError> {
    let id = ix.attr(t, i, "Self").unwrap_or_default().to_owned();
    let Some(p) = Prefs::read(w, ix, t, i) else { return Ok((Vec::new(), None)) };
    let (weight, _) = stroke_of(w, ix, t, i);
    // with strokes scaled the insets on the axis scaled more also grow by the stroke's share; with them kept the settings take
    // the text's scale (`factors`) and no more
    let kept = weight > 0.0 && adjust.is_some_and(|a| !a.stroke);
    let smaller = sx.min(sy);
    let extra = |axis: f64| if kept { 0.0 } else { weight * (axis / smaller - 1.0) };
    let mut insets = [p.insets[0] * ty + extra(sy), p.insets[1] * tx + extra(sx), p.insets[2] * ty + extra(sy), p.insets[3] * tx + extra(sx)];
    // a frame with a corner option has one inset, which takes the mean of the two scales and no share of the stroke (measured:
    // tfscale-c2 corner / corner2 probes, strokes on and off)
    if corners::shaped(ix, t, i) {
        if p.insets.iter().any(|v| (v - p.insets[0]).abs() > 1e-9) {
            return Err(EditError::new("unsupported", format!("scaling {id}: a frame with a corner option and four different insets (not measured)")));
        }
        insets = [p.insets[0] * (tx * ty).sqrt(); 4];
    }
    let gutter = p.gutter * tx;
    // the stroke's reach into the frame follows the new weight
    let thin = match adjust {
        None => sx.min(1.0) * sy.min(1.0),
        Some(a) if a.stroke => smaller,
        Some(_) => 1.0,
    };
    let stroke = p.stroke * thin;
    let [width, _] = outline(ix, t, i)?;
    // fixed-width columns keep their width scaled by the text's scale, and the frame follows them (its left edge stays)
    let (column, follow) = if p.fixed {
        let column = p.fixed_width * tx;
        (column, Some(column * p.count + gutter * (p.count - 1.0) + insets[1] + insets[3] + 2.0 * stroke))
    } else {
        ((width * sx - 2.0 * stroke - insets[1] - insets[3] - gutter * (p.count - 1.0)) / p.count, None)
    };
    if column <= 0.0 {
        return Err(EditError::new("invalid", format!("scaling {id}: the columns would not fit in the frame")));
    }
    // (InDesign leaves out a column width that is the default, 144, when no object style says another; measured, tf-scale-stroked-nostroke-outside)
    let mut edits = Vec::new();
    let width_base = textframe::inherited(w, ix, t, i, "TextColumnFixedWidth");
    let column_text = geom::num(column);
    if same_number(&column_text, if width_base.is_empty() { "144" } else { &width_base }) {
        edits.extend(attr_removal(ix, t, p.pref, "TextColumnFixedWidth"));
    } else {
        edits.push(attr_edit(ix, t, p.pref, "TextColumnFixedWidth", &column_text));
    }
    let base = textframe::inherited(w, ix, t, i, "TextColumnGutter");
    let value = geom::num(gutter);
    if same_number(&value, &base) {
        edits.extend(attr_removal(ix, t, p.pref, "TextColumnGutter"));
    } else {
        edits.push(attr_edit(ix, t, p.pref, "TextColumnGutter", &value));
    }
    edits.extend(corners::write_insets(ix, t, i, insets));
    Ok((edits, follow))
}

/// A frame with fixed-width columns takes the width its columns give (left edge kept), after its path was scaled.
pub(super) fn follow_columns(w: &mut Work<'_>, id: &str, width: f64) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let [x0, y0, x1, y1] = {
        let path = own_path(&ix, &t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
        let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, q, c)| (q.clone(), *c)).collect();
        geom::path_bounds(&shapes).ok_or_else(|| EditError::new("unsupported", format!("{id} has an empty path")))?
    };
    if (x1 - x0 - width).abs() > 1e-9 {
        super::resize(w, id, super::Target::Box([x0, y0, x0 + width, y1]))?;
    }
    Ok(())
}

/// The text the frame `id` shows takes the scale (after the frame's own edits).
pub(super) fn scale_text(w: &mut Work<'_>, id: &str, sx: f64, sy: f64) -> Result<(), EditError> {
    if (sx - 1.0).abs() < 1e-12 && (sy - 1.0).abs() < 1e-12 {
        return Ok(());
    }
    let Some(span) = w.ed.frame_spans.get(id).copied() else {
        return Err(EditError::new("unsupported", format!("scaling {id}: the composition's text range of the frame is needed (set_frame_spans)")));
    };
    let story = {
        let (part, ix, i) = w.find(id)?;
        ix.attr(w.text(&part)?, i, "ParentStory").map(str::to_owned)
    };
    if let (Some((start, end)), Some(story)) = (span, story) {
        crate::text::format::scale_text::scale_range(w, &story, start, end, sx, sy)?;
    }
    Ok(())
}
