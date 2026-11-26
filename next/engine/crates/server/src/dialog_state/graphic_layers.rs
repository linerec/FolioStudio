//! Starting values, layer comp selection and edit requests for Object Layer Options.

use super::{Defaults, graphic_of};
use crate::Session;
use serde_json::{Map, Value, json};

// a placed picture's layer options: its layers (the top ones, as the dialog lists them), the comp last chosen, the comps' layers and the link policy
pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a frame id")?;
    let o = graphic_of(s, id).and_then(|g| g.layer_options.as_ref()).ok_or_else(|| format!("{id} has no object layer options"))?;
    values.insert("layers".into(), json!(o.layers.iter().map(|l| json!({"id": l.id, "name": l.name, "original": l.original, "visible": l.visible})).collect::<Vec<_>>()));
    values.insert("comp".into(), json!(o.applied_comp.to_string()));
    values.insert("hasComps".into(), json!(!o.comps.is_empty()));
    values.insert("compsShown".into(), json!(o.comps.iter().map(|c| &c.shown).collect::<Vec<_>>()));
    values.insert("policy".into(), json!(if o.update_link == "ApplicationSettings" { "ApplicationSettings" } else { "KeepOverrides" }));
    Ok((values, json!({})))
}

/// A Layer Comp popup value: a position in the comps, -1 Last Document State, -2 [Custom].
fn layer_comp(value: &Value) -> i32 { value.as_str().and_then(|t| t.parse().ok()).unwrap_or(-1) }

/// Whether a layer row is shown by a comp (Last Document State: as the file saved it).
fn comp_visible(values: &Value, comp: i32, row: &Value) -> Value {
    if comp < 0 { return row["original"].clone(); }
    json!(values["compsShown"][comp as usize].as_array().is_some_and(|shown| shown.contains(&row["id"])))
}

/// Object Layer Options sends what the screen sent: the comp when another one (or Last Document State) is chosen, then each layer that differs
/// from what the comp gives (or from the picture when no comp was chosen), then the link policy when it changed.
pub(super) fn graphic_layer_edits(values: &Value, initial: &Value, ids: &Value) -> Vec<Value> {
    let comp = layer_comp(&values["comp"]);
    let chosen = (comp >= 0 || comp == -1) && comp != layer_comp(&initial["comp"]);
    let mut out = vec![];
    if chosen { out.push(json!({"op": "setGraphicLayerComp", "ids": ids, "comp": comp, "comps": null, "layers": null})); }
    for row in values["layers"].as_array().into_iter().flatten() {
        let base = if chosen { comp_visible(values, comp, row) } else {
            initial["layers"].as_array().into_iter().flatten().find(|l| l["id"] == row["id"]).map_or(Value::Null, |l| l["visible"].clone())
        };
        if row["visible"] != base { out.push(json!({"op": "setGraphicLayerVisibility", "ids": ids, "layer": row["id"], "visible": row["visible"], "layers": null})); }
    }
    if values["policy"] != initial["policy"] { out.push(json!({"op": "setGraphicLayerUpdateLink", "ids": ids, "policy": values["policy"], "layers": null})); }
    out
}

// Object Layer Options: a comp chosen shows its layers (Last Document State: as the file saved them); a layer switched by hand makes the comp [Custom]
pub(super) fn link_comp(values: &mut Value, changed: &str) {
    match changed {
        "comp" => {
            let comp = layer_comp(&values["comp"]);
            if comp != -2 {
                let shown: Vec<Value> = values["layers"].as_array().into_iter().flatten().map(|row| comp_visible(values, comp, row)).collect();
                if let Some(rows) = values["layers"].as_array_mut() { for (row, visible) in rows.iter_mut().zip(shown) { row["visible"] = visible; } }
            }
        }
        "layers" if values["hasComps"] == true => values["comp"] = json!("-2"),
        _ => {}
    }
}
