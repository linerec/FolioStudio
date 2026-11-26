//! Object ▸ Effects ▸ Clear Effects and Clear All Transparency, Object ▸ Transform ▸ Clear
//! Transformations, Object ▸ Content ▸ Graphic / Unassigned, as InDesign's menu commands do them
//! (oracle: basic-object-menu2).
use super::*;
use blending::{CHILDREN, child_edit};

/// The transparency levels an object keeps as its own elements.
const LEVELS: [&str; 4] = ["TransparencySetting", "StrokeTransparencySetting", "FillTransparencySetting", "ContentTransparencySetting"];

/// Clear Effects: the object level's effects go, settings and all (obj-clear-effects-values: turned
/// on again, a shadow has InDesign's values); its opacity and blending mode stay (obj-clear-effects).
/// A group: its own effects (obj-clear-effects-group). InDesign offers the command only when the
/// object level has an effect; the other levels are left alone.
pub(super) fn clear_effects(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        for child in &CHILDREN[1..] {
            let (part, ix, i) = w.find(id)?;
            let t = w.text(&part)?.to_owned();
            check_item(&ix, &t, i, id)?;
            if let Some(e) = child_edit(&ix, &t, i, "TransparencySetting", child, &[]) {
                w.splice(&part, vec![e])?;
            }
        }
    }
    Ok(())
}

/// Clear All Transparency: every level back to InDesign's (no elements): the object, stroke, fill
/// and text levels, and a placed graphic's own (its content level: obj-clear-all-transparency-image).
pub(super) fn clear_all_transparency(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let mut gone: Vec<usize> = ix.children(i).iter().copied().filter(|&c| LEVELS.contains(&ix.local(&t, c))).collect();
        for &g in ix.children(i).iter().filter(|&&c| GRAPHICS.contains(&ix.local(&t, c))) {
            gone.extend(ix.children(g).iter().copied().filter(|&c| ix.local(&t, c) == "TransparencySetting"));
        }
        if !gone.is_empty() {
            w.splice(&part, gone.into_iter().map(|c| (element_span(&ix, &t, c), String::new())).collect())?;
        }
    }
    Ok(())
}

/// Clear Transformations: the object's transform loses its rotation, shear, scale and flip and
/// keeps the middle of its path where it is (obj-clear-transformations-*: a frame, a group, a
/// placed graphic's frame, a flipped oval). Paths, members and contents are not changed.
pub(super) fn clear_transformations(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let b = own_bounds(&ix, &t, i, &Mat::ID).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
        let c = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
        let (x, y) = item_transform(&ix, &t, i).apply(c);
        let m = Mat([1.0, 0.0, 0.0, 1.0, x - c.0, y - c.1]);
        w.splice(&part, vec![attr_edit(&ix, &t, i, "ItemTransform", &m.text())])?;
    }
    Ok(())
}

/// Object ▸ Content ▸ Graphic / Text / Unassigned of an empty frame. Graphic and Unassigned only
/// change the `ContentType` attribute (obj-content-graphic, obj-content-graphic-oval,
/// obj-content-unassigned); Text makes the shape a text frame with a story of its own
/// (create::to_text_frame, obj-content-text-*), and an empty text frame becomes its shape again
/// (create::from_text_frame, obj-content-text-back, -oval-back, -poly-back). InDesign does not
/// offer any of it for a frame that holds text or a graphic.
pub(super) fn set_content_type(w: &mut Work<'_>, ids: &[String], kind: ContentKind) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        match (ix.local(&t, i), kind) {
            ("TextFrame", ContentKind::Text) => {}
            ("TextFrame", _) => create::from_text_frame(w, id, kind == ContentKind::Graphic)?,
            ("Rectangle" | "Oval" | "Polygon", _) if ix.children(i).iter().any(|&c| GRAPHICS.contains(&ix.local(&t, c)) || is_item(&ix, &t, c)) => {
                return Err(EditError::new("unsupported", format!("{id}: only an empty frame's content type changes")));
            }
            ("Rectangle" | "Oval" | "Polygon", ContentKind::Text) => create::to_text_frame(w, id)?,
            ("Rectangle" | "Oval" | "Polygon", _) => {
                w.splice(&part, vec![attr_edit(&ix, &t, i, "ContentType", if kind == ContentKind::Graphic { "GraphicType" } else { "Unassigned" })])?;
            }
            _ => return Err(EditError::new("unsupported", format!("{id}: only an empty frame's content type changes"))),
        }
    }
    Ok(())
}
