//! The control panel's rotation and shear angles, set as values (oracle:
//! basic-transform-values). InDesign reads an object's transform on the page as rotation ·
//! shear · scale — the rotation from the first column, the shear as `c = −tan φ · scale y` once
//! the rotation is taken out — and sets one angle keeping the other parts. The reference point
//! is on the object's own bounds (stroke included, in its own space, so it turns with the
//! object) and stays where it is.
use super::*;

/// Rotation (counter-clockwise) and shear of `full` in degrees, and its scale; `None` for a
/// mirrored or flat transform.
pub(super) fn angles(full: &Mat) -> Option<(f64, f64, f64, f64)> {
    let [a, b, c, d, _, _] = full.0;
    let sx = a.hypot(b);
    if sx < 1e-9 || a * d - b * c <= 1e-12 {
        return None;
    }
    let theta = (-b).atan2(a);
    let (cs, sn) = (theta.cos(), theta.sin());
    // the rotation taken out: [[sx, k], [0, sy]]
    let k = cs * c - sn * d;
    let sy = sn * c + cs * d;
    Some((theta.to_degrees(), (-k / sy).atan().to_degrees(), sx, sy))
}

/// `totals` (the Transform panel's "Transformations are Totals", measured in InDesign 20.5.5.119: rules/transform-totals.ko.md): on,
/// the angles are those of the object's whole transform on the page, its group's included, and a typed angle sets that total (the
/// object's own transform makes up the difference); off, they are those of the object's own transform, relative to its group.
pub(super) fn set_angles(w: &mut Work<'_>, id: &str, rotation: Option<f64>, shear: Option<f64>, anchor: Anchor, content: bool, totals: bool) -> Result<(), EditError> {
    if rotation.is_some_and(|r| !r.is_finite()) || shear.is_some_and(|s| !(s.is_finite() && s.abs() < 90.0)) {
        return Err(EditError::new("invalid", "shear angle: between −90° and 90°"));
    }
    if content {
        return set_content_angles(w, id, rotation, shear, anchor);
    }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    // the space whose angles are read and set: the page (totals) or the object's group (off: the group's own transform stays as it is)
    let parent = if totals { parent_transform(&ix, &t, i) } else { Mat::ID };
    let full = parent.then_after(&item_transform(&ix, &t, i));
    let (theta, phi, sx, sy) = angles(&full).ok_or_else(|| EditError::new("unsupported", format!("{id} is mirrored or flat: its angles are not measured")))?;
    let theta = rotation.unwrap_or(theta).to_radians();
    let phi = shear.unwrap_or(phi).to_radians();
    let (cs, sn) = (theta.cos(), theta.sin());
    let k = -phi.tan() * sy;
    let lin = Mat([cs * sx, -sn * sx, cs * k + sn * sy, -sn * k + cs * sy, 0.0, 0.0]);
    // the reference point of the object's own bounds keeps its place on the page
    let b = alignment::visible(w, &ix, &t, i, Mat::ID).ok_or_else(|| EditError::new("unsupported", format!("{id} has no bounds")))?;
    let (fx, fy) = anchor.at();
    let q = (b[0] + (b[2] - b[0]) * fx, b[1] + (b[3] - b[1]) * fy);
    let (p, moved) = (full.apply(q), lin.apply(q));
    let new_full = Mat([lin.0[0], lin.0[1], lin.0[2], lin.0[3], p.0 - moved.0, p.1 - moved.1]);
    let own = parent.invert().ok_or_else(|| EditError::new("unsupported", format!("{id}: its group cannot be inverted")))?.then_after(&new_full);
    let e = attr_edit(&ix, &t, i, "ItemTransform", &own.text());
    w.splice(&part, vec![e])
}

/// The same for the placed graphic of the frame `id` (the control panel with the content selected;
/// oracle: ctl2-shear-content-*, InDesign's `shearAngle` of the Image): the graphic's transform
/// in its frame takes the angle, its own bounds hold the reference point, and the frame's fitting
/// crops follow.
fn set_content_angles(w: &mut Work<'_>, id: &str, rotation: Option<f64>, shear: Option<f64>, anchor: Anchor) -> Result<(), EditError> {
    let (part, ix, f) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, f, id)?;
    let c = content_of(&ix, &t, f).ok_or_else(|| EditError::new("invalid", format!("{id} holds no placed graphic")))?;
    let frame = parent_transform(&ix, &t, f).then_after(&item_transform(&ix, &t, f));
    let full = frame.then_after(&c.m);
    let (theta, phi, sx, sy) = angles(&full).ok_or_else(|| EditError::new("unsupported", format!("the graphic of {id} is mirrored or flat: its angles are not measured")))?;
    let theta = rotation.unwrap_or(theta).to_radians();
    let phi = shear.unwrap_or(phi).to_radians();
    let (cs, sn) = (theta.cos(), theta.sin());
    let k = -phi.tan() * sy;
    let lin = Mat([cs * sx, -sn * sx, cs * k + sn * sy, -sn * k + cs * sy, 0.0, 0.0]);
    let (fx, fy) = anchor.at();
    let q = (c.bounds[0] + (c.bounds[2] - c.bounds[0]) * fx, c.bounds[1] + (c.bounds[3] - c.bounds[1]) * fy);
    let (p, moved) = (full.apply(q), lin.apply(q));
    let new_full = Mat([lin.0[0], lin.0[1], lin.0[2], lin.0[3], p.0 - moved.0, p.1 - moved.1]);
    let own = frame.invert().ok_or_else(|| EditError::new("unsupported", format!("{id} cannot be inverted")))?.then_after(&new_full);
    write_content(w, &part, &ix, &t, &c, own, None, true)
}
