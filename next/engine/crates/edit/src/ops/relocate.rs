//! Explicit spread relocation (spread-move-*). InDesign retains the spread-local
//! transform, applies the offset, and inserts above the destination layer's last item.
//! A by-only move never selects a different spread (probe-spread.json).
use super::*;
use std::collections::BTreeSet;

pub(super) fn run(w: &mut Work<'_>, ids: &[String], spread: &str, by: [f64; 2]) -> Result<(), EditError> {
    if by.iter().any(|n| !n.is_finite()) { return Err(EditError::new("invalid", "non-finite displacement")); }
    let selected: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let mut ordered = Vec::new();
    for id in &selected {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        check_item(&ix, t, i, id)?;
        if ix.ancestors(i).any(|p| ix.attr(t, p, "Self").is_some_and(|s| selected.contains(s))) { continue; }
        // an item inside a group (the Direct Selection tool picked it alone) leaves the group and stands on the other spread by itself
        if ix.nodes[i].parent.is_none() {
            return Err(EditError::new("invalid", format!("{id} is not an object of a spread")));
        }
        ordered.push((part, ix.nodes[i].start, (*id).to_owned()));
    }
    ordered.sort();
    for (_, _, id) in ordered { one(w, &id, spread, by)?; }
    Ok(())
}

fn one(w: &mut Work<'_>, id: &str, spread: &str, by: [f64; 2]) -> Result<(), EditError> {
    let (from, ix, i) = w.find(id)?;
    let (to, dx, di) = w.find(spread)?;
    let t = w.text(&from)?.to_owned();
    let dest = w.text(&to)?.to_owned();
    if dx.local(&dest, di) != "Spread" || !to.starts_with("Spreads/") {
        return Err(EditError::new("invalid", "destination is not a document spread"));
    }
    if from == to {
        move_items(w, &[id.to_owned()], (by[0], by[1]))?;
        return arrange(w, id, Arrange::Front);
    }
    let n = &ix.nodes[i];
    let mut copy = t[n.start..n.end].to_owned();
    let cx = Index::scan(&copy).map_err(|e| EditError::new("xml", e))?;
    // inside a group the object takes the group's transforms along (screens-doc: a group child dragged alone to another spread)
    let nested = n.parent.is_some_and(|p| ix.local(&t, p) != "Spread");
    let mut m = if nested { parent_transform(&ix, &t, i).then_after(&item_transform(&ix, &t, i)) } else { item_transform(&ix, &t, i) };
    m.0[4] += by[0]; m.0[5] += by[1];
    let (range, value) = attr_edit(&cx, &copy, 0, "ItemTransform", &m.text());
    copy.replace_range(range, &value);
    let own_layer = ix.attr(&t, i, "ItemLayer");
    // a group's children often have no layer of their own: they are on the group's
    let layer = own_layer.or_else(|| if nested { ix.ancestors(i).find_map(|a| ix.attr(&t, a, "ItemLayer")) } else { None });
    if nested && own_layer.is_none() && let Some(l) = layer {
        let cx = Index::scan(&copy).map_err(|e| EditError::new("xml", e))?;
        let (range, value) = attr_edit(&cx, &copy, 0, "ItemLayer", l);
        copy.replace_range(range, &value);
    }
    let dm = w.text("designmap.xml")?;
    let mx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
    let layers: Vec<_> = (0..mx.nodes.len()).filter(|&k| mx.local(dm, k) == "Layer").filter_map(|k| mx.attr(dm, k, "Self")).collect();
    let rank = |l: Option<&str>| layers.iter().position(|s| Some(*s) == l).unwrap_or(0);
    let siblings: Vec<_> = dx.children(di).iter().copied().filter(|&k| is_item(&dx, &dest, k)).collect();
    let at = siblings.iter().rev().find(|&&k| dx.attr(&dest, k, "ItemLayer") == layer)
        .map(|&k| dx.nodes[k].end)
        .or_else(|| siblings.iter().find(|&&k| rank(dx.attr(&dest, k, "ItemLayer")) > rank(layer)).map(|&k| element_span(&dx, &dest, k).start))
        .unwrap_or(dx.nodes[di].close_start);
    w.splice(&to, vec![(at..at, format!("\n\t\t{copy}"))])?;
    w.splice(&from, vec![(element_span(&ix, &t, i), String::new())])?;
    // Threads and their Story part stay; only composition can depend on the new page.
    for k in i..ix.nodes.len() {
        if ix.nodes[k].start >= n.end { break; }
        if ix.local(&t, k) == "TextFrame" && let Some(story) = ix.attr(&t, k, "ParentStory") { w.recompose(story); }
    }
    Ok(())
}
