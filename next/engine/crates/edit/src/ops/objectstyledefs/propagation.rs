use super::*;

pub(super) fn update(w: &mut Work<'_>, old: &str, old_ix: &Index, parent: usize, replacement: &str) -> Result<(), EditError> {
    update_inner(w, old, old_ix, parent, replacement, &mut std::collections::HashSet::new(), &[])
}

/// Redefine Style: a based-on style follows what the redefine captured only through the categories the redefined style has switched on (oracle
/// os-redefine-menu-*: with the anchored object, story and paragraph style categories off nothing of them reaches `Oracle Box Blue`; with only the
/// anchored object category on, the anchor point, pin and vertical alignment follow and the story settings and paragraph style still do not).
pub(super) fn update_redefined(w: &mut Work<'_>, old: &str, old_ix: &Index, parent: usize, replacement: &str) -> Result<(), EditError> {
    let next = Index::scan(replacement).map_err(|e| EditError::new("xml", e))?;
    let mut skip: Vec<&str> = Vec::new();
    for (switch, names) in [("EnableParagraphStyle", "AppliedParagraphStyle"), ("EnableAnchoredObjectOptions", "AnchoredObjectSetting"), ("EnableStoryOptions", "StoryPreference")] {
        if next.attr(replacement, 0, switch) != Some("true") { skip.push(names); }
    }
    update_inner(w, old, old_ix, parent, replacement, &mut std::collections::HashSet::new(), &skip)
}

fn update_inner(w: &mut Work<'_>, old: &str, old_ix: &Index, parent: usize, replacement: &str, visited: &mut std::collections::HashSet<usize>, skip: &[&str]) -> Result<(), EditError> {
    if !visited.insert(parent) { return Err(EditError::new("invalid", "cyclic object style bases")); }
    let id = old_ix.value(old, parent, "Self").unwrap();
    let combined = format!("{old}\n{replacement}");
    let ix = Index::scan(&combined).map_err(|e| EditError::new("xml", e))?;
    let new = (0..ix.nodes.len()).find(|&i| ix.nodes[i].start >= old.len() && ix.local(&combined, i) == "ObjectStyle").unwrap();
    let children: Vec<usize> = (0..old_ix.nodes.len()).filter(|&i| {
        old_ix.local(old, i) == "ObjectStyle" && old_ix.path(old, i, &["Properties", "BasedOn"])
            .is_some_and(|b| crate::xml::unescape(element_text(old_ix, old, b)) == id)
    }).collect();
    for child in children {
        let n = &ix.nodes[child];
        let mut edits = Vec::new();
        deletion::rebase_node_skipping(&combined, &ix, child, parent, Some(new), &mut edits, skip);
        if edits.is_empty() { continue; }
        let mut changed = old[n.start..n.end].to_owned();
        local_edit(&mut changed, edits.into_iter().map(|(r, s)| (r.start-n.start..r.end-n.start, s)).collect());
        update_inner(w, old, old_ix, child, &changed, visited, skip)?;
        let (current, current_ix) = scan(w)?;
        let child_id = old_ix.value(old, child, "Self").unwrap();
        let i = find(&current, &current_ix, &child_id)?;
        w.splice(PART, vec![(current_ix.nodes[i].start..current_ix.nodes[i].end, changed)])?;
    }
    Ok(())
}

pub(super) fn preserve_opacity(w: &mut Work<'_>, style: &str, old: &str, old_ix: &Index, source: usize, replacement: &str) -> Result<(), EditError> {
    let next = Index::scan(replacement).map_err(|e| EditError::new("xml", e))?;
    let path = ["ObjectStyleObjectEffectsCategorySettings"];
    let enabled = |ix: &Index, t: &str, i| ix.path(t, i, &path).and_then(|n| ix.attr(t, n, "EnableTransparency")) != Some("false");
    if !enabled(old_ix, old, source) || enabled(&next, replacement, 0) { return Ok(()); }
    let Some(opacity) = old_ix.path(old, source, &["TransparencySetting", "BlendingSetting"]).and_then(|n| old_ix.value(old, n, "Opacity")) else { return Ok(()); };
    let parts: Vec<_> = w.ed.parts.keys().filter(|p| p.starts_with("Spreads/") || p.starts_with("MasterSpreads/")).cloned().collect();
    for part in parts {
        let t = w.text(&part)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for i in 0..ix.nodes.len() {
            if ix.value(&t, i, "AppliedObjectStyle").as_deref() != Some(style) { continue; }
            let mut attrs = blending::child_attrs(&ix, &t, i, "TransparencySetting", "BlendingSetting");
            if attrs.iter().any(|(k, _)| k == "Opacity") { continue; }
            attrs.push(("Opacity".to_owned(), opacity.clone()));
            if let Some(edit) = blending::child_edit(&ix, &t, i, "TransparencySetting", "BlendingSetting", &attrs) { edits.push(edit); }
        }
        if !edits.is_empty() { w.splice(&part, edits)?; }
    }
    Ok(())
}
