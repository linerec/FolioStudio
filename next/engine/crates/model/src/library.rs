//! A Folio local library is separate from the documents into which its assets are placed.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub parts: BTreeMap<String, String>,
    pub ids: Vec<String>,
    #[serde(default)]
    pub links: BTreeMap<String, Dependency>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub sha256: String,
    /// What the item is, as InDesign's Item Information tells it: `geometry`, `text` or `image` (empty: not known, an older container).
    #[serde(default)]
    pub kind: String,
    /// The item's script label (InDesign `Asset.label`).
    #[serde(default)]
    pub label: String,
    /// The item's keyed script labels (InDesign `Asset.insertLabel(key, value)` / `extractLabel(key)`): apart from `label`, kept with the item
    /// across closing the library and renaming the item, and not carried to the object placed (probe bookx-c4-labels).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// When the item was stored (RFC 3339 UTC); the library lists the newest first, as InDesign does.
    #[serde(default)]
    pub created: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub assets: Vec<Asset>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
