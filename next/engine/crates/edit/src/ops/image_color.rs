use super::{attr_edit, check_item, escape};
use crate::{EditError, Work};

pub(crate) fn set(w: &mut Work<'_>, ids: &[String], profile: &str, intent: &str) -> Result<(), EditError> {
    if !folio_model::image_color::INTENTS.contains(&intent) {
        return Err(EditError::new("invalid", "unknown image rendering intent"));
    }
    if profile.trim().is_empty() || profile.chars().any(char::is_control) {
        return Err(EditError::new("invalid", "image profile name required"));
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let g = ix.children(i).iter().copied().find(|&c| ix.local(&t, c) == "Image")
            .ok_or_else(|| EditError::new("invalid", "select a raster image"))?;
        let mut edits = vec![attr_edit(&ix, &t, g, "ImageRenderingIntent", intent)];
        if let Some(p) = ix.path(&t, g, &["Properties", "Profile"]) {
            let n = &ix.nodes[p];
            if n.empty {
                let open = t[n.start..n.end].trim_end_matches("/>");
                edits.push((n.start..n.end, format!("{open}>{}</Profile>", escape(profile))));
            } else {
                edits.push((n.open_end..n.close_start, escape(profile)));
            }
        } else {
            let profile = format!("<Profile type=\"string\">{}</Profile>", escape(profile));
            if let Some(p) = ix.path(&t, g, &["Properties"]) {
                let n = &ix.nodes[p];
                if n.empty {
                    let open = t[n.start..n.end].trim_end_matches("/>");
                    edits.push((n.start..n.end, format!("{open}>{profile}</Properties>")));
                } else {
                    edits.push((n.close_start..n.close_start, profile));
                }
            } else {
                let at = ix.nodes[g].open_end;
                edits.push((at..at, format!("<Properties>{profile}</Properties>")));
            }
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}
