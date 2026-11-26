//! Book > Synchronize, parent pages (oracle `bookx-c3-master`, InDesign 2025, `Book.synchronizeMasterPage` switched on; off is the default):
//! a parent page of the style source replaces the document's parent of the same name (margins, items and the stories of its text frames
//! come from the source, as new objects; the pages that used the parent use the new one), a parent the document lacks is added with the
//! source's prefix, and a parent only the document has stays. Unlike Load Parent Pages the source's margins are kept and nothing is renamed.
//! Not measured, so refused rather than guessed: a parent based on another parent.
use super::*;
use std::collections::BTreeMap;

pub(crate) fn sync(w: &mut Work<'_>, parts: &BTreeMap<String, String>) -> Result<(), EditError> {
    let source = clipboard::Source::of(&crate::Editor::new(parts.clone()));
    let Some(dm) = parts.get("designmap.xml") else { return Err(EditError::new("invalid", "missing source designmap")) };
    let ix = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
    let mut spreads = Vec::new();
    for i in (0..ix.nodes.len()).filter(|&i| ix.local(dm, i) == "MasterSpread") {
        let Some(part) = ix.attr(dm, i, "src") else { continue };
        let text = parts.get(part).ok_or_else(|| EditError::new("invalid", "missing parent definition"))?;
        let sx = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
        let root = (0..sx.nodes.len()).find(|&j| sx.local(text, j) == "MasterSpread" && sx.attr(text, j, "Self").is_some())
            .ok_or_else(|| EditError::new("xml", "missing parent"))?;
        let xml = text[sx.nodes[root].start..sx.nodes[root].end].to_owned();
        let based: Vec<usize> = (0..sx.nodes.len()).filter(|&j| sx.local(text, j) == "Page" && sx.attr(text, j, "AppliedMaster").is_some_and(|m| m != "n")).collect();
        if !based.is_empty() { return Err(EditError::new("unsupported", "book synchronization of a parent page based on another parent is not measured")); }
        spreads.push(xml);
    }
    let mut map = Vec::new();
    parent_load::layer_map(w, dm, &ix, &mut map)?;
    let before = w.parts_named("");
    let mut importer = xdoc::Importer::new(&source);
    for xml in spreads {
        let sx = Index::scan(&xml).map_err(|e| EditError::new("xml", e))?;
        let name = sx.value(&xml, 0, "Name").unwrap_or_default();
        let existing = parents::parents(w)?;
        let named = |part: &str| -> bool {
            w.text(part).is_ok_and(|t| Index::scan(t).is_ok_and(|px| (0..px.nodes.len()).any(|j| px.local(t, j) == "MasterSpread" && px.value(t, j, "Name").as_deref() == Some(&name))))
        };
        let same = existing.iter().find(|(part, _, _)| named(part)).cloned();
        let prefix = match &same {
            Some((_, _, prefix)) => prefix.clone(),
            None => {
                let wanted = sx.value(&xml, 0, "NamePrefix").unwrap_or_default();
                let taken: Vec<String> = existing.iter().map(|p| p.2.clone()).collect();
                if !wanted.is_empty() && !taken.contains(&wanted) { wanted } else { parents::free_prefix(&taken) }
            }
        };
        let xml = importer.rewrite(w, &xml)?;
        parents::copy_parent_from(w, &xml, &source, map.clone(), &prefix)?;
        let Some((_, old, _)) = same else { continue };
        let new = parents::parents(w)?.into_iter().map(|p| p.1).find(|id| !existing.iter().any(|e| e.1 == *id))
            .ok_or_else(|| EditError::new("xml", "the copied parent is missing"))?;
        // the pages that used the parent use the new one, then the old one goes (its stories with it)
        for p in w.parts_named("Spreads/").into_iter().chain(w.parts_named("MasterSpreads/")) {
            let t = w.text(&p)?.to_owned();
            let px = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
            let edits: Vec<_> = (0..px.nodes.len()).filter(|&k| px.local(&t, k) == "Page" && px.attr(&t, k, "AppliedMaster") == Some(old.as_str()))
                .map(|k| attr_edit(&px, &t, k, "AppliedMaster", &new)).collect();
            if !edits.is_empty() { w.splice(&p, edits)?; }
        }
        parents::delete_parent(w, &old)?;
    }
    for p in w.parts_named("Stories/").into_iter().filter(|p| !before.contains(p)) {
        let text = w.text(&p)?.to_owned();
        let imported = importer.rewrite(w, &text)?;
        w.splice(&p, vec![(0..text.len(), imported)])?;
    }
    Ok(())
}
