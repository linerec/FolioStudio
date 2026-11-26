//! The Layers panel, as InDesign does it (next/oracle/scenarios/basic-layers.json):
//! - designmap lists Layer elements bottom to top; a new layer goes above the reference
//!   (the active layer) and becomes active; its name is "Layer N" (N = layers + 1, the DOM
//!   default; the panel passes a localized name) and its colour the next in COLORS;
//! - a spread lists its page items grouped by layer (bottom to top), each layer's items in
//!   stacking order: moving a layer regroups every spread; items moved to a layer go to the
//!   top of it, keeping their order (to-layer, to-layer-two, layer-move-*);
//! - deleting a layer deletes its items on every spread and parent (stories with the last
//!   frame); the layer above (else below) becomes active (layer-delete, layer-delete-active);
//! - Duplicate Layer: "<name> copy" directly above, its settings and colour, active; every
//!   object on the layer (spreads and parents) copied onto it above the originals, stories
//!   copied and threads between the copies kept (basic-layers2 layer-duplicate-*);
//! - Merge Layers: the other layers' objects go to the target layer keeping their stacking
//!   order; those layers go; the target becomes active if the active one went
//!   (layer-merge-*);
//! - Delete Unused Layers: layers nothing is on go, as Delete Layer (layer-delete-unused);
//! - Unlock All (menu action 11396): every object lock on every spread goes, layer locks
//!   stay; Unlock All on Spread (11395) only the spread's (basic-layers3 unlock-all-*).
use super::*;
use std::collections::{HashMap, HashSet};

/// The colours new layers get, by the number of layers already there. layer-new-many: after
/// LightBlue, Red, Green the turn skips Yellow, Gray and Black. Measured to Purple; the rest
/// of InDesign's UI colours follow in their usual order (not measured).
const COLORS: &[&str] = &[
    "LightBlue", "Red", "Green", "Blue", "Magenta", "Cyan", "Orange", "DarkGreen", "Teal", "Tan", "Brown", "Violet", "Gold",
    "DarkBlue", "Pink", "Lavender", "BrickRed", "OliveGreen", "Peach", "Burgundy", "GrassGreen", "Ochre", "Purple",
    "LightGray", "Charcoal", "GridBlue", "GridOrange", "Fiesta", "LightOlive", "Lipstick", "CuteTeal", "Sulphur", "GridGreen",
];

const DM: &str = "designmap.xml";

fn scan(w: &Work<'_>, part: &str) -> Result<(String, Index), EditError> {
    let t = w.text(part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    Ok((t, ix))
}

/// Layer elements of designmap, bottom to top.
fn layer_nodes(ix: &Index, t: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Layer" && ix.attr(t, i, "Self").is_some()).collect()
}

fn document_node(ix: &Index, t: &str) -> Result<usize, EditError> {
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Document").ok_or_else(|| EditError::new("invalid", "designmap has no Document"))
}

fn layer_index(ix: &Index, t: &str, nodes: &[usize], id: &str) -> Result<usize, EditError> {
    nodes.iter().position(|&i| ix.attr(t, i, "Self") == Some(id)).ok_or_else(|| EditError::new("notFound", format!("layer {id}")))
}

fn spread_parts(w: &Work<'_>) -> Vec<String> {
    w.ed.parts.keys().filter(|n| n.starts_with("Spreads/") || n.starts_with("MasterSpreads/")).cloned().collect()
}

/// The page items of a spread part: direct children of its Spread/MasterSpread with an
/// ItemLayer, in document order.
fn spread_items(ix: &Index, t: &str) -> Vec<usize> {
    // the Spread element itself (Self), not the idPkg:Spread wrapper around it
    let Some(root) = (0..ix.nodes.len()).find(|&i| matches!(ix.local(t, i), "Spread" | "MasterSpread") && ix.attr(t, i, "Self").is_some()) else { return Vec::new() };
    (0..ix.nodes.len()).filter(|&i| ix.nodes[i].parent == Some(root) && ix.attr(t, i, "ItemLayer").is_some()).collect()
}

/// Puts a spread part's items in layer order (bottom to top), keeping each layer's order;
/// `moved` items go to the top of their layer.
fn regroup(w: &mut Work<'_>, part: &str, rank: &HashMap<String, usize>, moved: &HashSet<String>) -> Result<(), EditError> {
    let (t, ix) = scan(w, part)?;
    let items = spread_items(&ix, &t);
    let key = |i: usize| {
        let layer = ix.attr(&t, i, "ItemLayer").unwrap_or_default();
        let id = ix.attr(&t, i, "Self").unwrap_or_default();
        (rank.get(layer).copied().unwrap_or(usize::MAX), moved.contains(id))
    };
    let mut order: Vec<usize> = items.clone();
    order.sort_by_key(|&i| key(i)); // stable: each layer keeps its order
    if order == items {
        return Ok(());
    }
    let spans: Vec<_> = items.iter().map(|&i| element_span(&ix, &t, i)).collect();
    let edits = spans.iter().zip(&order).map(|(slot, &i)| (slot.clone(), t[element_span(&ix, &t, i)].to_owned())).collect();
    w.splice(part, edits)
}

fn layer_ranks(w: &Work<'_>) -> Result<HashMap<String, usize>, EditError> {
    let (t, ix) = scan(w, DM)?;
    Ok(layer_nodes(&ix, &t).iter().enumerate().map(|(k, &i)| (ix.attr(&t, i, "Self").unwrap().to_owned(), k)).collect())
}

fn regroup_all(w: &mut Work<'_>, moved: &HashSet<String>) -> Result<(), EditError> {
    let rank = layer_ranks(w)?;
    for part in spread_parts(w) {
        regroup(w, &part, &rank, moved)?;
    }
    Ok(())
}

/// Every spread's items in layer order again (after items changed layer).
pub(super) fn regroup_spreads(w: &mut Work<'_>) -> Result<(), EditError> {
    regroup_all(w, &HashSet::new())
}

/// New layer above `above` (default: the active layer); it becomes the active layer.
pub(super) fn new_layer(w: &mut Work<'_>, name: Option<&str>, color: Option<&str>, above: Option<&str>) -> Result<(), EditError> {
    let (t, ix) = scan(w, DM)?;
    let nodes = layer_nodes(&ix, &t);
    let doc = document_node(&ix, &t)?;
    let reference = match above.or(ix.attr(&t, doc, "ActiveLayer")) {
        Some(id) => nodes[layer_index(&ix, &t, &nodes, id)?],
        None => *nodes.last().ok_or_else(|| EditError::new("invalid", "designmap has no layer"))?,
    };
    let names: HashSet<String> = nodes.iter().filter_map(|&i| ix.value(&t, i, "Name")).collect();
    let name = match name {
        Some(n) => n.to_owned(),
        None => (nodes.len() + 1..).map(|k| format!("Layer {k}")).find(|n| !names.contains(n)).unwrap(),
    };
    let color = color.unwrap_or(COLORS[nodes.len() % COLORS.len()]);
    let id = w.new_id();
    let ind = indent_before(&t, ix.nodes[reference].start).to_owned();
    let el = format!(
        "\n{ind}<Layer Self=\"{id}\" Name=\"{}\" Visible=\"true\" Locked=\"false\" IgnoreWrap=\"false\" ShowGuides=\"true\" LockGuides=\"false\" UI=\"true\" Expendable=\"true\" Printable=\"true\">\n{ind}\t<Properties>\n{ind}\t\t<LayerColor type=\"enumeration\">{}</LayerColor>\n{ind}\t</Properties>\n{ind}</Layer>",
        escape(&name), escape(color)
    );
    let at = ix.nodes[reference].end;
    let active = attr_edit(&ix, &t, doc, "ActiveLayer", &id);
    // not in `created`: that lists new page items (the UI selects them)
    w.splice(DM, vec![(at..at, el), active])
}

/// Layer options beyond show/lock/print: name, colour, guides.
pub(super) fn set_options(w: &mut Work<'_>, id: &str, name: Option<&str>, color: Option<&str>, show_guides: Option<bool>, lock_guides: Option<bool>) -> Result<(), EditError> {
    let (t, ix) = scan(w, DM)?;
    let nodes = layer_nodes(&ix, &t);
    let i = nodes[layer_index(&ix, &t, &nodes, id)?];
    let mut edits = Vec::new();
    if let Some(n) = name {
        if nodes.iter().any(|&k| k != i && ix.value(&t, k, "Name").as_deref() == Some(n)) {
            return Err(EditError::new("invalid", format!("a layer named {n} already exists")));
        }
        edits.push(attr_edit(&ix, &t, i, "Name", n));
    }
    for (attr, v) in [("ShowGuides", show_guides), ("LockGuides", lock_guides)] {
        if let Some(v) = v {
            edits.push(attr_edit(&ix, &t, i, attr, if v { "true" } else { "false" }));
        }
    }
    if let Some(c) = color {
        let colour = ix.own_descendants(&t, i, "LayerColor").first().copied()
            .ok_or_else(|| EditError::new("unsupported", "layer without LayerColor"))?;
        let n = &ix.nodes[colour];
        edits.push((n.start..n.end, format!("<LayerColor type=\"enumeration\">{}</LayerColor>", escape(c))));
    }
    w.splice(DM, edits)
}

/// Moves a layer directly above `above` or directly below `below`; spreads regroup.
pub(super) fn move_layer(w: &mut Work<'_>, id: &str, above: Option<&str>, below: Option<&str>) -> Result<(), EditError> {
    let (t, ix) = scan(w, DM)?;
    let nodes = layer_nodes(&ix, &t);
    let me = nodes[layer_index(&ix, &t, &nodes, id)?];
    let (reference, after) = match (above, below) {
        (Some(r), None) => (nodes[layer_index(&ix, &t, &nodes, r)?], true),
        (None, Some(r)) => (nodes[layer_index(&ix, &t, &nodes, r)?], false),
        _ => return Err(EditError::new("invalid", "moveLayer needs above or below")),
    };
    if reference == me {
        return Ok(());
    }
    let span = element_span(&ix, &t, me);
    let text = t[span.clone()].to_owned();
    let at = if after { ix.nodes[reference].end } else { element_span(&ix, &t, reference).start };
    w.splice(DM, vec![(span, String::new()), (at..at, text)])?;
    regroup_all(w, &HashSet::new())
}

/// Deletes a layer and everything on it.
pub(super) fn delete_layer(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    {
        let (t, ix) = scan(w, DM)?;
        let nodes = layer_nodes(&ix, &t);
        layer_index(&ix, &t, &nodes, id)?;
        if nodes.len() < 2 {
            return Err(EditError::new("invalid", "a document keeps at least one layer"));
        }
    }
    // the layer's objects, with what goes with them (threads, stories, group fills)
    for part in spread_parts(w) {
        loop {
            let (t, ix) = scan(w, &part)?;
            let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(&t, i, "ItemLayer") == Some(id)) else { break };
            let sid = ix.attr(&t, i, "Self").map(str::to_owned);
            match sid {
                Some(sid) if is_item(&ix, &t, i) => delete(w, &sid)?,
                _ => w.splice(&part, vec![(element_span(&ix, &t, i), String::new())])?, // guides and the like
            }
        }
    }
    let (t, ix) = scan(w, DM)?;
    let nodes = layer_nodes(&ix, &t);
    let k = layer_index(&ix, &t, &nodes, id)?;
    let doc = document_node(&ix, &t)?;
    let mut edits = vec![(element_span(&ix, &t, nodes[k]), String::new())];
    if ix.attr(&t, doc, "ActiveLayer") == Some(id) {
        let next = if k + 1 < nodes.len() { nodes[k + 1] } else { nodes[k - 1] };
        edits.push(attr_edit(&ix, &t, doc, "ActiveLayer", ix.attr(&t, next, "Self").unwrap()));
    }
    w.splice(DM, edits)
}

/// Every element on the layer (page items, guides) in the spread and parent parts.
fn used_layers(w: &Work<'_>) -> Result<HashSet<String>, EditError> {
    let mut used = HashSet::new();
    for part in spread_parts(w) {
        let (t, ix) = scan(w, &part)?;
        for i in 0..ix.nodes.len() {
            if let Some(l) = ix.attr(&t, i, "ItemLayer") {
                used.insert(l.to_owned());
            }
        }
    }
    Ok(used)
}

/// Layers panel ▸ Delete Unused Layers: the layers nothing is on, as Delete Layer does
/// (the active one passes to the layer above); one layer always stays.
pub(super) fn delete_unused(w: &mut Work<'_>) -> Result<(), EditError> {
    let used = used_layers(w)?;
    let (t, ix) = scan(w, DM)?;
    let unused: Vec<String> = layer_nodes(&ix, &t).iter().filter_map(|&i| ix.attr(&t, i, "Self")).filter(|id| !used.contains(*id)).map(str::to_owned).collect();
    let total = layer_nodes(&ix, &t).len();
    for (k, id) in unused.iter().enumerate() {
        if total - k < 2 {
            break;
        }
        delete_layer(w, id)?;
    }
    Ok(())
}

/// Layers panel ▸ Merge Layers: the objects (and guides) of `ids` go to `into`, keeping their
/// stacking order; the layers of `ids` go; `into` becomes active when the active one went.
pub(super) fn merge_layers(w: &mut Work<'_>, into: &str, ids: &[String]) -> Result<(), EditError> {
    let merged: HashSet<&str> = ids.iter().map(String::as_str).filter(|id| *id != into).collect();
    {
        let (t, ix) = scan(w, DM)?;
        let nodes = layer_nodes(&ix, &t);
        layer_index(&ix, &t, &nodes, into)?;
        for id in &merged {
            layer_index(&ix, &t, &nodes, id)?;
        }
    }
    for part in spread_parts(w) {
        let (t, ix) = scan(w, &part)?;
        let edits: Vec<_> = (0..ix.nodes.len())
            .filter(|&i| ix.attr(&t, i, "ItemLayer").is_some_and(|l| merged.contains(l)))
            .map(|i| attr_edit(&ix, &t, i, "ItemLayer", into))
            .collect();
        if !edits.is_empty() {
            w.splice(&part, edits)?;
        }
    }
    let (t, ix) = scan(w, DM)?;
    let nodes = layer_nodes(&ix, &t);
    let doc = document_node(&ix, &t)?;
    let mut edits: Vec<_> = nodes.iter().filter(|&&i| ix.attr(&t, i, "Self").is_some_and(|s| merged.contains(s))).map(|&i| (element_span(&ix, &t, i), String::new())).collect();
    if ix.attr(&t, doc, "ActiveLayer").is_some_and(|a| merged.contains(a)) {
        edits.push(attr_edit(&ix, &t, doc, "ActiveLayer", into));
    }
    w.splice(DM, edits)?;
    regroup_all(w, &HashSet::new())
}

/// Layers panel ▸ Duplicate Layer.
pub(super) fn duplicate_layer(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    // the new layer: the original's element with its own Self and name, right above it
    let (t, ix) = scan(w, DM)?;
    let nodes = layer_nodes(&ix, &t);
    let me = nodes[layer_index(&ix, &t, &nodes, id)?];
    let names: HashSet<String> = nodes.iter().filter_map(|&i| ix.value(&t, i, "Name")).collect();
    let base = format!("{} copy", ix.value(&t, me, "Name").unwrap_or_default());
    let name = std::iter::once(base.clone()).chain((2..).map(|k| format!("{base} {k}"))).find(|n| !names.contains(n)).unwrap();
    let new_layer = w.new_id();
    let mut el = t[ix.nodes[me].start..ix.nodes[me].end].to_owned();
    let ex = Index::scan(&el).map_err(|e| EditError::new("xml", e))?;
    let mut e = vec![attr_edit(&ex, &el, 0, "Self", &new_layer), attr_edit(&ex, &el, 0, "Name", &name)];
    e.sort_by_key(|x| std::cmp::Reverse(x.0.start));
    for (r, s) in e {
        el.replace_range(r, &s);
    }
    let ind = indent_before(&t, ix.nodes[me].start).to_owned();
    let doc = document_node(&ix, &t)?;
    let at = ix.nodes[me].end;
    w.splice(DM, vec![(at..at, format!("\n{ind}{el}")), attr_edit(&ix, &t, doc, "ActiveLayer", &new_layer)])?;

    // the layer's objects in every spread and parent, copied together (threads between
    // them kept, also across spreads), each part's copies appended to it
    let parts = spread_parts(w);
    let mut wrapper = String::from("<Copy>");
    for (k, part) in parts.iter().enumerate() {
        let (t, ix) = scan(w, part)?;
        let items: Vec<usize> = spread_items(&ix, &t).into_iter().filter(|&i| ix.attr(&t, i, "ItemLayer") == Some(id) && ix.attr(&t, i, "Self").is_some()).collect();
        if items.is_empty() {
            continue;
        }
        wrapper.push_str(&format!("<In part=\"{k}\">"));
        for i in items {
            check_item(&ix, &t, i, ix.attr(&t, i, "Self").unwrap_or_default())?;
            wrapper.push_str("<One>");
            wrapper.push_str(&t[ix.nodes[i].start..ix.nodes[i].end]);
            wrapper.push_str("</One>");
        }
        wrapper.push_str("</In>");
    }
    wrapper.push_str("</Copy>");
    let cx = Index::scan(&wrapper).map_err(|e| EditError::new("xml", e))?;
    let mut map: Vec<(String, String)> = Vec::new();
    for k in 0..cx.nodes.len() {
        if let Some(s) = cx.attr(&wrapper, k, "Self") {
            map.push((s.to_owned(), w.new_id()));
        }
    }
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let source = super::clipboard::Source::of(w.ed);
    story_copy::copy_stories(w, &source, &cx, &wrapper, &mut map, &mut edits)?;
    for k in 0..cx.nodes.len() {
        let top = cx.nodes[k].parent.is_some_and(|p| cx.local(&wrapper, p) == "One");
        for a in cx.attrs(&wrapper, k) {
            let (name, v) = (&wrapper[a.name.0..a.name.1], &wrapper[a.value.0..a.value.1]);
            if top && name == "ItemLayer" {
                edits.push((a.value.0..a.value.1, new_layer.clone()));
            } else if let Some((_, new)) = map.iter().find(|(old, _)| old == v) {
                edits.push((a.value.0..a.value.1, new.clone()));
            }
        }
        if let Some(edit) = clipboard::copied_profile(&cx, &wrapper, k) { edits.push(edit); }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut copy = wrapper.clone();
    for (r, s) in edits {
        copy.replace_range(r, &s);
    }
    let cx = Index::scan(&copy).map_err(|e| EditError::new("xml", e))?;
    for k in (0..cx.nodes.len()).filter(|&k| cx.local(&copy, k) == "In") {
        let part = &parts[cx.attr(&copy, k, "part").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0)];
        let inner: Vec<&str> = cx.children(k).iter().filter(|&&o| cx.local(&copy, o) == "One")
            .filter_map(|&o| cx.children(o).first().map(|&c| &copy[cx.nodes[c].start..cx.nodes[c].end])).collect();
        let (t, ix) = scan(w, part)?;
        let Some(&last) = spread_items(&ix, &t).last() else { continue };
        let indent = indent_before(&t, ix.nodes[last].start).to_owned();
        let at = ix.nodes[last].end;
        let text: String = inner.iter().map(|s| format!("\n{indent}{s}")).collect();
        w.splice(part, vec![(at..at, text)])?;
        let (t2, ix2) = scan(w, part)?;
        for s in &inner {
            if let Ok(sx) = Index::scan(s)
                && let Some(new) = sx.attr(s, 0, "Self")
                && ix2.by_self(&t2, new).is_some()
            {
                w.created.push(new.to_owned());
            }
        }
    }
    regroup_all(w, &HashSet::new())
}

pub(super) fn set_active(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, DM)?;
    layer_index(&ix, &t, &layer_nodes(&ix, &t), id)?;
    let doc = document_node(&ix, &t)?;
    let e = attr_edit(&ix, &t, doc, "ActiveLayer", id);
    w.splice(DM, vec![e])
}

/// Layers panel ▸ Unlock All: every object lock in the document goes (all spreads; layer locks
/// stay) - `page`: Object ▸ Unlock All on Spread, only the spread of that page. Parent pages
/// are left alone (not measured) (basic-layers3 unlock-all-items, unlock-all-on-spread).
pub(super) fn unlock_all(w: &mut Work<'_>, page: Option<&str>) -> Result<(), EditError> {
    let mut parts: Vec<String> = match page {
        Some(p) => vec![w.find(p)?.0],
        None => w.ed.parts.keys().filter(|n| n.starts_with("Spreads/")).cloned().collect(),
    };
    parts.sort();
    for part in parts {
        let (t, ix) = scan(w, &part)?;
        let edits: Vec<_> = (0..ix.nodes.len())
            .filter(|&i| is_item(&ix, &t, i) && ix.attr(&t, i, "Locked") == Some("true"))
            .map(|i| attr_edit(&ix, &t, i, "Locked", "false"))
            .collect();
        if !edits.is_empty() {
            w.splice(&part, edits)?;
        }
    }
    Ok(())
}

/// Moves objects (spread-level items, not group members) to the top of `layer`.
pub(super) fn to_layer(w: &mut Work<'_>, ids: &[String], layer: &str) -> Result<(), EditError> {
    {
        let (t, ix) = scan(w, DM)?;
        layer_index(&ix, &t, &layer_nodes(&ix, &t), layer)?;
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        if ix.attr(&t, i, "ItemLayer").is_none() {
            return Err(EditError::new("unsupported", format!("{id} is inside a group: the group's layer applies")));
        }
        set_attrs(w, id, &[("ItemLayer", layer.to_owned())])?;
    }
    let moved: HashSet<String> = ids.iter().cloned().collect();
    regroup_all(w, &moved)
}
