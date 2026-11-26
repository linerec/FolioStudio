//! Pointer transformations about a point in spread coordinates.
use super::*;

/// What a linear map `l` of the spread (x' = la·x + lc·y, y' = lb·x + ld·y) bakes into an object's path, and so into its
/// strokes. An object whose transform only turns and shears (first column of length 1, determinant 1) gets the length of its
/// mapped first axis and the factor that keeps the determinant; the transform keeps the rest (oracle: geom-c1 gm-scale-many-*,
/// gm-shear-many-*, measured on a turned and a sheared rectangle: the same geometry as mapping the transform, written in
/// InDesign's split). `None` for anything else.
fn baked_factors(ix: &Index, t: &str, i: usize, [a, b, c, d]: [f64; 4], [la, lb, lc, ld]: [f64; 4]) -> Option<(f64, f64)> {
    let plain = parent_transform(ix, t, i) == Mat::ID && ix.local(t, i) != "Group";
    if !(plain && (a.hypot(b) - 1.0).abs() < 1e-9 && (a * d - b * c - 1.0).abs() < 1e-9) {
        return None;
    }
    let kx = (la * a + lc * b).hypot(lb * a + ld * b);
    let det = la * ld - lb * lc;
    (kx > 1e-12 && det > 1e-12).then_some((kx, det / kx))
}

pub(crate) fn scale(w: &mut Work<'_>, ids: &[String], sx: &f64, sy: &f64, point: &[f64; 2], stroke: &bool, effects: &bool) -> Result<(), EditError> {
    if !sx.is_finite() || !sy.is_finite() || *sx <= 0.0 || *sy <= 0.0 || point.iter().any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "scale needs positive factors and a finite point"));
    }
    scale_items(w, &expand(w, ids), *sx, *sy, *point, Some(Adjust { stroke: *stroke, effects: *effects }))
}

/// A scale by (sx, sy) about `point` (spread coordinates) of each object, as the Scale tool and dialog give it; `adjust` is what the
/// Transform panel menu says about strokes and effects (`None`: the control panel's rule, strokes thin when scaled down).
pub(crate) fn scale_items(w: &mut Work<'_>, ids: &[String], sx: f64, sy: f64, point: [f64; 2], adjust: Option<Adjust>) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let mut edits = Vec::new();
        let [a, b, c, d, e, f] = item_transform(&ix, &t, i).0;
        let baked = baked_factors(&ix, &t, i, [a, b, c, d], [sx, 0.0, 0.0, sy]);
        let (kx, ky) = baked.unwrap_or((sx, sy));
        // a text frame: its settings and the text it shows scale with it, along its own axes when it is turned or sheared (oracle:
        // tfscale-c1, tfscale-c2); the text frames of a group take the group's scale
        let text = ix.local(&t, i) == "TextFrame";
        let mut taken = (kx, ky);
        let mut follow = None;
        let mut inside = Vec::new();
        if text {
            tfscale::refuse_unmeasured(&ix, &t, i, baked.is_some())?;
            taken = tfscale::factors(w, &ix, &t, i, kx, ky, adjust)?;
            let (frame, fixed) = tfscale::frame_edits(w, &ix, &t, i, kx, ky, taken, adjust)?;
            edits.extend(frame);
            follow = fixed;
        } else if ix.local(&t, i) == "Group" {
            for m in tfscale::members(&ix, &t, i) {
                let (mp, mix, mi) = w.find(&m)?;
                let mt = w.text(&mp)?.to_owned();
                let factors = tfscale::factors(w, &mix, &mt, mi, sx, sy, adjust)?;
                inside.push((m, factors));
            }
        }
        scale::bake_with(w, &ix, &t, i, kx, ky, adjust, text, &mut edits)?;
        let [x, y] = point;
        let m = Mat([sx * a / kx, sy * b / kx, sx * c / ky, sy * d / ky, x + (e - x) * sx, y + (f - y) * sy]);
        edits.push(attr_edit(&ix, &t, i, "ItemTransform", &m.text()));
        w.splice(&part, edits)?;
        if text {
            tfscale::scale_text(w, id, taken.0, taken.1)?;
            if let Some(width) = follow {
                tfscale::follow_columns(w, id, width)?;
            }
        }
        for (m, (tx, ty)) in inside {
            tfscale::scale_text(w, &m, tx, ty)?;
        }
    }
    Ok(())
}

pub(crate) fn shear(w: &mut Work<'_>, ids: &[String], by: &f64, point: &[f64; 2]) -> Result<(), EditError> {
    if !by.is_finite() || by.abs() >= 90.0 || point.iter().any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "shear needs an angle between -90 and 90 and a finite point"));
    }
    let k = -by.to_radians().tan();
    transform_items(w, &expand(w, ids), &Mat([1.0, 0.0, k, 1.0, -k * point[1], 0.0]))
}

/// Shear… on a selection (oracle: basic-transform-shear, geom-c1 gm-shear-many-*, ctl2-shear-many-*): about the reference point of
/// the objects' bounds together, by `by` degrees (positive leans right). A turned object is re-split as InDesign writes it (`baked_factors`).
pub(crate) fn shear_items(w: &mut Work<'_>, ids: &[String], by: f64, anchor: Anchor, adjust: Option<Adjust>) -> Result<(), EditError> {
    if !(by.is_finite() && by.abs() < 90.0) {
        return Err(EditError::new("invalid", "shear angle: between −90° and 90°"));
    }
    let (_, oy) = union_anchor(w, ids, anchor)?;
    let k = -by.to_radians().tan();
    let x = Mat([1.0, 0.0, k, 1.0, -k * oy, 0.0]);
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        if ix.ancestors(i).any(|a| ix.attr(&t, a, "Self").is_some_and(|s| ids.iter().any(|x| x == s))) {
            continue;
        }
        let [a, b, c, d, e, f] = item_transform(&ix, &t, i).0;
        if let Some((kx, ky)) = baked_factors(&ix, &t, i, [a, b, c, d], [1.0, 0.0, k, 1.0]).filter(|(kx, ky)| (kx - 1.0).abs() > 1e-9 || (ky - 1.0).abs() > 1e-9) {
            let mut edits = Vec::new();
            if scale::bake(w, &ix, &t, i, kx, ky, Some(adjust.unwrap_or(Adjust { stroke: true, effects: true })), &mut edits).is_ok() {
                let m = Mat([(a + k * b) / kx, b / kx, (c + k * d) / ky, d / ky, e + k * f - k * oy, f]);
                edits.push(attr_edit(&ix, &t, i, "ItemTransform", &m.text()));
                w.splice(&part, edits)?;
                continue;
            }
        }
        transform_items(w, std::slice::from_ref(id), &x)?;
    }
    Ok(())
}
