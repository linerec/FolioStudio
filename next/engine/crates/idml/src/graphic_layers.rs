use crate::xml::Element;
use folio_model::graphic_layers::{Layer, Options};

pub(crate) fn read(g: &Element<'_>) -> Option<Options> {
    let options = g.child("GraphicLayerOption")?;
    let layers = layers(options);
    let applied_comp = g.child("LayerCompOption").and_then(|c| c.attr("AppliedLayerComp")).and_then(|v| v.parse().ok()).unwrap_or(-1);
    (!layers.is_empty()).then(|| Options { update_link: options.attr("UpdateLinkOption").unwrap_or("KeepOverrides").into(), layers, applied_comp, comps: Vec::new() })
}

fn layers(parent: &Element<'_>) -> Vec<Layer> {
    parent.elements().filter(|e| e.name == "GraphicLayer").map(|e| Layer {
        id: e.attr("Id").and_then(|v| v.parse().ok()).unwrap_or(0),
        name: e.attr("Name").unwrap_or("").strip_prefix("$ID/").unwrap_or(e.attr("Name").unwrap_or("")).into(),
        original: e.attr("OriginalVisibility") != Some("false"),
        visible: e.attr("CurrentVisibility") != Some("false"),
        children: layers(e),
        adjustment: e.attr("AdjustmentLayer") == Some("true"),
    }).collect()
}
