use super::*;
use folio_idml::style_identity::{is_reference_attribute, renamed_id};

pub(super) fn rename(w: &mut Work<'_>, cat: &Catalog, id: &str, name: &Value) -> Result<(), EditError> {
    let name = name.as_str().ok_or_else(|| EditError::new("invalid", "style name must be text"))?;
    renamed_id(id, name).ok_or_else(|| EditError::new("invalid", "style name is empty or contains a slash/control"))?;
    // a style in a group keeps the groups before its name (`Heads%3aName`, `Heads:Name`)
    let tag = id.split('/').next().unwrap_or_default();
    let (groups, _) = super::groups::split_id(tag, id);
    let (new_id, full) = super::groups::join_id(tag, &groups, name);
    if new_id == id { return Ok(()); }
    if cat.styles.contains_key(&new_id) {
        return Err(EditError::new("invalid", "style name already exists"));
    }
    retarget(w, id, &new_id, &full)
}

/// The style `id` is now `new_id` with the `Name` `name`: its definition and every reference to it (attributes and object properties of any
/// part, the pending typing formats) follow.
pub(super) fn retarget(w: &mut Work<'_>, id: &str, new_id: &str, name: &str) -> Result<(), EditError> {
    let parts: Vec<_> = w.ed.parts.iter().filter(|(p, _)| p.ends_with(".xml"))
        .map(|(p, t)| (p.clone(), t.clone())).collect();
    for (part, t) in parts {
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for (i, n) in ix.nodes.iter().enumerate() {
            let definition = part == PART && ix.value(&t, i, "Self").as_deref() == Some(id);
            for a in ix.attrs(&t, i) {
                let key = &t[a.name.0..a.name.1];
                let value = unescape(&t[a.value.0..a.value.1]);
                let replacement = if definition && key == "Name" { Some(name) }
                    else if (definition && key == "Self" || is_reference_attribute(key)) && value == id { Some(new_id) }
                    else { None };
                if let Some(value) = replacement { edits.push((a.value.0..a.value.1, escape(value))); }
            }
            if ix.attr(&t, i, "type") == Some("object") && !n.empty && ix.children(i).is_empty()
                && unescape(t[n.open_end..n.close_start].trim()) == id {
                edits.push((n.open_end..n.close_start, escape(new_id)));
            }
            if ix.local(&t, i) == "Story"
                && let Some(story) = ix.value(&t, i, "Self") { w.recompose(&story); }
        }
        w.splice(&part, edits)?;
    }
    for state in w.ed.text_state.values_mut() {
        let mut a = style_attrs(&state.character);
        for (key, value) in &mut a {
            if is_reference_attribute(key) && value.value == id { *value = attr(new_id); }
        }
        state.character = make_style(&state.character, &a);
    }
    Ok(())
}
