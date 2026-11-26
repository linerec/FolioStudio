//! Resource IDs for an attached edit reply. The screen's selection uses the original item IDs.
use folio_model::Document;
use serde_json::{Value, json};

pub(crate) fn resource_id(op: &Value, before: &Document, after: &Document) -> Option<Value> {
    let id = match op.as_str()? {
        "newLayer" => after.layers.iter()
            .find(|layer| !before.layers.iter().any(|old| old.id == layer.id)).map(|layer| &layer.id),
        "newSwatch" | "newTint" => after.swatches.keys().find(|id| !before.swatches.contains_key(*id)),
        "newGradient" => after.gradients.keys().find(|id| !before.gradients.contains_key(*id)),
        "newParent" | "saveAsParent" => after.master_spreads.iter()
            .find(|spread| !before.master_spreads.iter().any(|old| old.id == spread.id)).map(|spread| &spread.id),
        _ => None,
    }?;
    Some(json!([id]))
}
