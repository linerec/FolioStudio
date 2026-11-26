//! Object Layer Options: which layers of a placed PDF or PSD are shown, which layer comp of a PSD
//! is chosen, and what a link update does to the choices. A picture's `GraphicLayerOption` is made
//! on first use for a PSD (InDesign writes none until the layers are touched; measured
//! objlayer-c4/psd-hide-green): every layer of the file, the changed one with its new
//! `CurrentVisibility`, then `LayerCompOption`. Any change gives the picture and its layers new
//! `Self` ids (objlayer-c3).
//!
//! `LayerCompOption/@AppliedLayerComp` (objlayer-c5, the dialog measured): the position of the
//! chosen comp in the file's comp list, -1 for "Last Document State" (the layers as the file
//! saved them), -2 for "[Custom]" (a layer switched by hand in a file that has comps; a file
//! without comps keeps -1). Choosing a comp sets every layer the way the comp holds it.
use super::*;
use folio_model::graphic_layers::{Comp, Layer};

/// One layer row; InDesign names a layer `<parent>i<hex id>` and nests a group's layers inside the group's row.
fn row(parent: &str, l: &Layer, visible: &dyn Fn(&Layer) -> bool) -> String {
    let (id, name, original, adjustment) = (l.id, escape(&l.name), l.original, l.adjustment);
    let this = format!("{parent}i{id:x}");
    let current = visible(l);
    let head = format!(r#"<GraphicLayer Self="{this}" Name="{name}" OriginalVisibility="{original}" CurrentVisibility="{current}" SeparatorLayer="false" AdjustmentLayer="{adjustment}" FXLayer="false" Locked="false" HasViewState="false" ViewState="false" HasExportState="false" ExportState="false" HasPrintState="false" PrintState="false" Id="{id}""#);
    if l.children.is_empty() {
        return format!("{head}/>");
    }
    let kids = l.children.iter().map(|c| row(&this, c, visible)).collect::<String>();
    format!("{head}>{kids}</GraphicLayer>")
}

/// Every `GraphicLayer` below `node`, nested ones included.
fn layer_nodes(ix: &crate::xml::Index, text: &str, node: usize, out: &mut Vec<usize>) {
    for child in ix.named(text, node, "GraphicLayer") {
        out.push(child);
        layer_nodes(ix, text, child, out);
    }
}

fn find_layer(list: &[Layer], id: u32) -> bool {
    list.iter().any(|l| l.id == id || find_layer(&l.children, id))
}

fn flat_ids(list: &[Layer], out: &mut Vec<u32>) {
    for l in list {
        out.push(l.id);
        flat_ids(&l.children, out);
    }
}

/// What a command changes: the layers' visibility, the applied comp, the link update policy.
struct Change<'a> {
    /// The new visibility of a layer: (id, saved by the file, now) -> shown.
    visible: &'a dyn Fn(u32, bool, bool) -> bool,
    /// The new `AppliedLayerComp` (None: leave it, -1 when the options are made).
    comp: Option<i32>,
    /// The new `UpdateLinkOption` (None: leave it, KeepOverrides when the options are made).
    policy: Option<&'a str>,
    /// A layer the command names, which must exist.
    needs: Option<u32>,
}

fn apply(w: &mut Work<'_>, ids: &[String], layers: &Option<Vec<Layer>>, change: &Change) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let text = w.text(&part)?.to_owned();
        check_item(&ix, &text, i, id)?;
        let content = content_of(&ix, &text, i).ok_or_else(|| EditError::new("invalid", "select a picture frame"))?;
        let kind = ix.local(&text, content.graphic);
        let psd = kind == "Image" && ix.attr(&text, content.graphic, "ImageTypeName") == Some("$ID/Photoshop");
        if !matches!(kind, "PDF" | "ImportedPage") && !psd {
            return Err(EditError::new("unsupported", "layer options are supported for PDF and PSD pictures"));
        }
        let old = ix.attr(&text, content.graphic, "Self").ok_or_else(|| EditError::new("invalid", "picture has no identifier"))?;
        let new = w.new_id();
        let mut edits = vec![attr_edit(&ix, &text, content.graphic, "Self", &new)];
        let comp_node = ix.named(&text, content.graphic, "LayerCompOption").first().copied();
        if let Some(options) = ix.named(&text, content.graphic, "GraphicLayerOption").first().copied() {
            let mut all = Vec::new();
            layer_nodes(&ix, &text, options, &mut all);
            if let Some(n) = change.needs
                && !all.iter().any(|node| ix.attr(&text, *node, "Id").and_then(|v| v.parse::<u32>().ok()) == Some(n))
            {
                return Err(EditError::new("invalid", "picture layer does not exist"));
            }
            for node in &all {
                let lid = ix.attr(&text, *node, "Id").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                let saved = ix.attr(&text, *node, "OriginalVisibility") != Some("false");
                let now = ix.attr(&text, *node, "CurrentVisibility") != Some("false");
                let shown = (change.visible)(lid, saved, now);
                if shown != now {
                    edits.push(attr_edit(&ix, &text, *node, "CurrentVisibility", if shown { "true" } else { "false" }));
                }
                if let Some(suffix) = ix.attr(&text, *node, "Self").and_then(|s| s.strip_prefix(old)) {
                    edits.push(attr_edit(&ix, &text, *node, "Self", &format!("{new}{suffix}")));
                }
            }
            if let Some(policy) = change.policy {
                edits.push(attr_edit(&ix, &text, options, "UpdateLinkOption", policy));
            }
            // only a PSD has layer comps (a placed PDF keeps no LayerCompOption)
            if let Some(value) = change.comp.filter(|_| psd) {
                match comp_node {
                    Some(c) => edits.push(attr_edit(&ix, &text, c, "AppliedLayerComp", &value.to_string())),
                    None => {
                        let at = ix.nodes[options].end;
                        edits.push((at..at, format!(r#"<LayerCompOption AppliedLayerComp="{value}"/>"#)));
                    }
                }
            }
        } else if psd {
            let list = layers.as_ref().filter(|l| !l.is_empty()).ok_or_else(|| EditError::new("invalid", "picture has no layer options: pass the layers of its file"))?;
            let mut flat = Vec::new();
            flat_ids(list, &mut flat);
            let mut seen = std::collections::BTreeSet::new();
            if !flat.iter().all(|l| seen.insert(*l)) || change.needs.is_some_and(|n| !find_layer(list, n)) {
                return Err(EditError::new("invalid", "picture layer does not exist"));
            }
            let shown = |l: &Layer| (change.visible)(l.id, l.original, l.visible);
            let rows = list.iter().map(|l| row(&format!("{new}GraphicLayerOption1"), l, &shown)).collect::<String>();
            let policy = change.policy.unwrap_or("KeepOverrides");
            let value = change.comp.unwrap_or(-1);
            let block = format!(r#"<GraphicLayerOption UpdateLinkOption="{policy}">{rows}</GraphicLayerOption>"#);
            match comp_node {
                Some(c) => {
                    edits.push((ix.nodes[c].start..ix.nodes[c].start, block));
                    if change.comp.is_some() {
                        edits.push(attr_edit(&ix, &text, c, "AppliedLayerComp", &value.to_string()));
                    }
                }
                None => {
                    let at = ix.nodes[content.graphic].close_start;
                    edits.push((at..at, format!(r#"{block}<LayerCompOption AppliedLayerComp="{value}"/>"#)));
                }
            }
        } else {
            return Err(EditError::new("invalid", "picture has no layer options"));
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}

pub(crate) fn set(w: &mut Work<'_>, ids: &[String], layer: &u32, visible: &bool, layers: &Option<Vec<Layer>>, has_comps: &Option<bool>) -> Result<(), EditError> {
    let (target, value) = (*layer, *visible);
    // a layer switched by hand in a file that has layer comps is "[Custom]" (-2); the DOM path of a file without comps writes -1
    let comp = if has_comps.unwrap_or(false) { -2 } else { -1 };
    apply(w, ids, layers, &Change { visible: &|id, _, now| if id == target { value } else { now }, comp: Some(comp), policy: None, needs: Some(target) })
}

/// Chooses layer comp `comp` (its position in the file's list), -1 for Last Document State. A comp
/// shows the layers it lists, hides the others; Last Document State shows the layers as saved.
pub(crate) fn set_comp(w: &mut Work<'_>, ids: &[String], comp: &i32, comps: &Option<Vec<Comp>>, layers: &Option<Vec<Layer>>) -> Result<(), EditError> {
    let list = comps.as_deref().unwrap_or(&[]);
    let value = *comp;
    let shown: Option<&Vec<u32>> = if value >= 0 {
        Some(&list.get(value as usize).ok_or_else(|| EditError::new("invalid", "layer comp does not exist"))?.shown)
    } else if value == -1 {
        None
    } else {
        return Err(EditError::new("invalid", "layer comp must be a position in the comp list or -1"));
    };
    apply(w, ids, layers, &Change { visible: &|id, saved, _| shown.map_or(saved, |s| s.contains(&id)), comp: Some(value), policy: None, needs: None })
}

/// What a link update does to the layers a placement switched: `KeepOverrides` or `ApplicationSettings`.
pub(crate) fn set_update_link(w: &mut Work<'_>, ids: &[String], policy: &str, layers: &Option<Vec<Layer>>) -> Result<(), EditError> {
    if !matches!(policy, "KeepOverrides" | "ApplicationSettings") {
        return Err(EditError::new("invalid", "link update policy must be KeepOverrides or ApplicationSettings"));
    }
    apply(w, ids, layers, &Change { visible: &|_, _, now| now, comp: None, policy: Some(policy), needs: None })
}
