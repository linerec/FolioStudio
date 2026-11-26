//! The control panel's scale percentages (oracle: basic-scale-values), InDesign's default
//! "apply to content": the scale goes into the object, which reads 100 % again. Paths are
//! scaled about their own space's origin and the object's transform moves so that the
//! reference point of its own stroked bounds stays put; a group's members get the same in their
//! own spaces, a placed graphic's transform takes the scale. Each axis scaled down thins the
//! strokes by its factor; scaling up leaves them (oracle: xform-scale-*-stroked).
//!
//! The Scale dialog and Transform Again scale through the object's transform instead (oracle:
//! basic-transform-prefs): Transform panel menu ▸ Adjust Stroke Weight when Scaling multiplies the
//! stroke weight by the scale and Adjust Effects when Scaling multiplies an effect's lengths
//! (effects.rs); both are settings of the application, which the caller passes in `Adjust`.
//!
//! A text frame is scaled by `tfscale` (the frame's settings, and the attributes of the text it shows).
//!
//! Not measured, refused: text frames on their own here, gradients, text wrap offsets, a non-uniform scale of a rotated or sheared member.
use super::*;

/// The Transform panel menu's Adjust Stroke Weight / Adjust Effects when Scaling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjust {
    #[serde(default)]
    pub stroke: bool,
    #[serde(default)]
    pub effects: bool,
}

pub(super) fn scale(w: &mut Work<'_>, id: &str, sx: f64, sy: f64, anchor: Anchor, adjust: Option<Adjust>) -> Result<(), EditError> {
    if !(sx.is_finite() && sy.is_finite() && sx > 0.0 && sy > 0.0) {
        return Err(EditError::new("invalid", "scale: more than 0 %"));
    }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let b = alignment::visible(w, &ix, &t, i, Mat::ID).ok_or_else(|| EditError::new("unsupported", format!("{id} has no bounds")))?;
    let (fx, fy) = anchor.at();
    let a = (b[0] + (b[2] - b[0]) * fx, b[1] + (b[3] - b[1]) * fy);
    scale_at(w, id, sx, sy, a, adjust)
}

/// `scale` about the point `a` of the object's own space.
fn scale_at(w: &mut Work<'_>, id: &str, sx: f64, sy: f64, a: (f64, f64), adjust: Option<Adjust>) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let mut edits = Vec::new();
    bake(w, &ix, &t, i, sx, sy, adjust, &mut edits)?;
    let moved = item_transform(&ix, &t, i).then_after(&Mat([1.0, 0.0, 0.0, 1.0, a.0 - sx * a.0, a.1 - sy * a.1]));
    edits.push(attr_edit(&ix, &t, i, "ItemTransform", &moved.text()));
    w.splice(&part, edits)
}

/// The control panel's scale percentages with several objects selected (oracle: ctl2-scale-many-*, geom-c1 gm-scale-many-*,
/// InDesign's transform on the selection): all about the reference point of the selection's visible box on the spread, every object
/// scaled the way the Scale tool does (paths scaled; an object that is only turned or sheared in its transform is re-split as
/// InDesign writes it, `tooltransform::baked_factors`); the strokes follow the Transform panel menu's settings (`adjust`, as the Scale
/// dialog). Objects on one spread, not in a group, not flipped and not scaled in their transform are measured; the others are refused.
pub(super) fn scale_many(w: &mut Work<'_>, ids: &[String], sx: f64, sy: f64, anchor: Anchor, adjust: Option<Adjust>) -> Result<(), EditError> {
    // a negative factor flips the objects as well (oracle: tfscale-c2 flip probe: the scale of |s| about the reference point, then the
    // mirror about the same point; the text takes |s|)
    if !(sx.is_finite() && sy.is_finite() && sx != 0.0 && sy != 0.0) {
        return Err(EditError::new("invalid", "scale: not 0 %"));
    }
    let refuse = |why: &str| Err(EditError::new("unsupported", format!("scaling several objects: {why} (not measured)")));
    let mut spread = None::<String>;
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        if spread.get_or_insert_with(|| part.clone()) != &part {
            return refuse("objects on different spreads");
        }
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let [a, b, c, d, _, _] = item_transform(&ix, &t, i).0;
        let plain = (a - 1.0).abs() < 1e-9 && (d - 1.0).abs() < 1e-9 && b.abs() < 1e-9 && c.abs() < 1e-9;
        let turned = (a.hypot(b) - 1.0).abs() < 1e-9 && (a * d - b * c - 1.0).abs() < 1e-9;
        if parent_transform(&ix, &t, i) != Mat::ID || !(plain || turned) {
            return refuse("a member of a group, or an object that is scaled in its transform or flipped");
        }
        if !plain && ix.local(&t, i) == "Group" {
            return refuse("a group that is turned or sheared");
        }
    }
    if ids.is_empty() {
        return Ok(());
    }
    let (ox, oy) = reference(w, ids, anchor)?;
    tooltransform::scale_items(w, ids, sx.abs(), sy.abs(), [ox, oy], adjust)?;
    if sx < 0.0 || sy < 0.0 {
        let (mx, my) = (if sx < 0.0 { -1.0 } else { 1.0 }, if sy < 0.0 { -1.0 } else { 1.0 });
        transform_items(w, ids, &Mat([mx, 0.0, 0.0, my, ox - mx * ox, oy - my * oy]))?;
    }
    Ok(())
}

/// The reference point of a scale of several objects. One turned or sheared object has its own bounds turned with it: the point is a
/// corner (or an edge middle, or the centre) of the bounds in its own space, put through its transform (oracle: tfscale-c2
/// rect-scale-rotated, tf-scale-rotated, tf-scale-sheared: InDesign scaled about the frame's own top left corner, not the corner of the
/// box around the turned frame). Several objects: the reference point of their boxes together (`union_anchor`).
fn reference(w: &mut Work<'_>, ids: &[String], anchor: Anchor) -> Result<(f64, f64), EditError> {
    if let [id] = ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        let total = parent_transform(&ix, &t, i).then_after(&item_transform(&ix, &t, i));
        let [_, b, c, _, _, _] = total.0;
        if (b.abs() > 1e-9 || c.abs() > 1e-9) && ix.local(&t, i) != "Group"
            && let Some(own) = alignment::visible(w, &ix, &t, i, Mat::ID) {
            let (fx, fy) = anchor.at();
            return Ok(total.apply((own[0] + (own[2] - own[0]) * fx, own[1] + (own[3] - own[1]) * fy)));
        }
    }
    union_anchor(w, ids, anchor)
}

/// Transform panel menu ▸ Redefine Scaling as 100 %: the scale in an object's transform goes into
/// its path (it looks the same and reads 100 % again); rotation, shear and a flip stay in the
/// transform. A scale in the transform is what InDesign's "Adjust Scaling Percentage" leaves, or
/// a file made by something else. Strokes and effects keep their values.
pub(super) fn redefine(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let [a, b, c, d, e, f] = item_transform(&ix, &t, i).0;
    let (sx, sy) = (a.hypot(b), c.hypot(d));
    if !(sx > 1e-12 && sy > 1e-12) {
        return Err(EditError::new("invalid", format!("{id} has no area to scale")));
    }
    if (sx - 1.0).abs() < 1e-9 && (sy - 1.0).abs() < 1e-9 {
        return Ok(());
    }
    let mut edits = Vec::new();
    bake(w, &ix, &t, i, sx, sy, Some(Adjust::default()), &mut edits)?;
    edits.push(attr_edit(&ix, &t, i, "ItemTransform", &Mat([a / sx, b / sx, c / sy, d / sy, e, f]).text()));
    w.splice(&part, edits)
}

/// Edits that scale item `i` by (sx, sy) in its own space, about that space's origin.
pub(super) fn bake(w: &Work<'_>, ix: &Index, t: &str, i: usize, sx: f64, sy: f64, adjust: Option<Adjust>, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Result<(), EditError> {
    bake_with(w, ix, t, i, sx, sy, adjust, false, edits)
}

/// `bake`; with `text` a text frame is scaled too (its path and stroke as a rectangle's: the frame's own settings and its text are
/// `tfscale`'s).
pub(super) fn bake_with(w: &Work<'_>, ix: &Index, t: &str, i: usize, sx: f64, sy: f64, adjust: Option<Adjust>, text: bool, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Result<(), EditError> {
    let id = ix.attr(t, i, "Self").unwrap_or_default().to_owned();
    let kind = ix.local(t, i);
    let refuse = |why: &str| Err(EditError::new("unsupported", format!("scaling {id}: {why} (not measured)")));
    match kind {
        "Group" => {
            if adjust.is_some_and(|a| a.effects) {
                edits.extend(effects::scale_lengths(w, ix, t, i, &id, sx, sy)?);
            }
            for c in ix.children(i).iter().copied().filter(|&c| is_item(ix, t, c)) {
                let m = item_transform(ix, t, c);
                if sx != sy && (m.0[1].abs() > 1e-12 || m.0[2].abs() > 1e-12) {
                    return refuse("a rotated or sheared member scaled unevenly");
                }
                // a text frame in the group takes the group's scale as it would alone (oracle: tfscale-c2 tf-scale-group-*)
                let text_member = ix.local(t, c) == "TextFrame";
                if text_member {
                    // (measured with members that are at most turned or sheared in their own transform)
                    let [ma, mb, mc, md, _, _] = m.0;
                    if (ma.hypot(mb) - 1.0).abs() > 1e-9 || (ma * md - mb * mc - 1.0).abs() > 1e-9 {
                        return refuse("a text frame in the group that is scaled or flipped in its own transform");
                    }
                    let taken = tfscale::factors(w, ix, t, c, sx, sy, adjust)?;
                    let (frame, fixed) = tfscale::frame_edits(w, ix, t, c, sx, sy, taken, adjust)?;
                    if fixed.is_some() {
                        return refuse("a text frame with fixed-width columns in a group");
                    }
                    edits.extend(frame);
                }
                bake_with(w, ix, t, c, sx, sy, adjust, text_member, edits)?;
                let [a, b, cc, d, e, f] = m.0;
                edits.push(attr_edit(ix, t, c, "ItemTransform", &Mat([a, b, cc, d, e * sx, f * sy]).text()));
            }
            return Ok(());
        }
        "Rectangle" | "Oval" | "Polygon" | "GraphicLine" => {}
        "TextFrame" if text => {}
        "TextFrame" => return refuse("a text frame, whose text InDesign scales too"),
        k => return refuse(&format!("a {k}")),
    }
    // corner options (rounded, bevel, inverse rounded, fancy, inset): the sizes scale by the smaller factor, whatever the Transform
    // panel menu says (oracle: tfscale-c2 corner2 probe, rectangles in all four modes)
    for (option, radius) in [("TopLeftCornerOption", "TopLeftCornerRadius"), ("TopRightCornerOption", "TopRightCornerRadius"),
        ("BottomLeftCornerOption", "BottomLeftCornerRadius"), ("BottomRightCornerOption", "BottomRightCornerRadius"), ("CornerOption", "CornerRadius")] {
        if stroke_attr(w, ix, t, i, option).is_some_and(|v| v != "None") {
            // the size is the item's own, else the object style's, else InDesign's default (12); a result that is the one the
            // style or the default gives is not written
            let inherited = objectstyle::from_style(w, ix, t, i, &[], radius, "EnableStroke").and_then(|v| v.parse::<f64>().ok()).unwrap_or(12.0);
            let r = ix.attr(t, i, radius).and_then(|v| v.parse::<f64>().ok()).unwrap_or(inherited);
            let now = r * sx.min(sy);
            if (now - inherited).abs() < 1e-9 {
                edits.extend(attr_removal(ix, t, i, radius));
            } else {
                edits.push(attr_edit(ix, t, i, radius, &geom::num(now)));
            }
        }
    }
    if ["FillColor", "StrokeColor"].iter().any(|k| stroke_attr(w, ix, t, i, k).is_some_and(|v| v.starts_with("Gradient/"))) {
        return refuse("a gradient");
    }
    if let Some(o) = ix.path(t, i, &["TextWrapPreference", "Properties", "TextWrapOffset"])
        && ["Top", "Left", "Bottom", "Right"].iter().any(|k| ix.attr(t, o, k).and_then(|v| v.parse::<f64>().ok()).is_some_and(|v| v != 0.0))
    {
        return refuse("text wrap offsets");
    }
    let path = own_path(ix, t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let map = |(x, y): (f64, f64)| (x * sx, y * sy);
    for (els, pts, _) in &path.subpaths {
        for (&el, p) in els.iter().zip(pts) {
            edits.push(attr_edit(ix, t, el, "Anchor", &geom::pair(map(p.anchor))));
            edits.push(attr_edit(ix, t, el, "LeftDirection", &geom::pair(map(p.left))));
            edits.push(attr_edit(ix, t, el, "RightDirection", &geom::pair(map(p.right))));
        }
    }
    // the control panel's percentages thin a stroke scaled down; the dialog's transform scales it
    // by the smaller factor when Adjust Stroke Weight is on and leaves it otherwise
    let thin = match adjust {
        None => sx.min(1.0) * sy.min(1.0),
        Some(a) if a.stroke => sx.min(sy),
        Some(_) => 1.0,
    };
    if adjust.is_some_and(|a| a.effects) {
        edits.extend(effects::scale_lengths(w, ix, t, i, &id, sx, sy)?);
    }
    let weight: f64 = stroke_attr(w, ix, t, i, "StrokeWeight").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    if thin != 1.0 && weight > 0.0 {
        let new = geom::num(weight * thin);
        if style_attr(w, ix, t, i, "StrokeWeight", "EnableStroke").is_some_and(|s| same_number(&s, &new)) {
            edits.extend(attr_removal(ix, t, i, "StrokeWeight"));
        } else {
            edits.push(attr_edit(ix, t, i, "StrokeWeight", &new));
        }
    }
    // a placed graphic takes the scale in its transform; its effective resolution follows
    if let Some(&g) = ix.children(i).iter().find(|&&c| GRAPHICS.contains(&ix.local(t, c))) {
        let m = Mat([sx, 0.0, 0.0, sy, 0.0, 0.0]).then_after(&item_transform(ix, t, g));
        edits.push(attr_edit(ix, t, g, "ItemTransform", &m.text()));
        if let Some(ppi) = ix.attr(t, g, "ActualPpi").and_then(geom::parse_pair) {
            let (gx, gy) = (m.0[0].hypot(m.0[1]), m.0[2].hypot(m.0[3]));
            if gx > 0.0 && gy > 0.0 {
                edits.push(attr_edit(ix, t, g, "EffectivePpi", &format!("{} {}", (ppi.0 / gx).round(), (ppi.1 / gy).round())));
            }
        }
    }
    Ok(())
}
