//! Folio book membership is separate from the IDML of each document.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn chapter_source() -> String { "ContinueFromPreviousDocument".into() }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub path: String,
    pub sha256: String,
}

/// Book Page Numbering Options (InDesign `Book.repaginationOption`, `insertBlankPage`, `automaticPagination`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// `nextPage`, `nextOdd` or `nextEven`: where a document starts after the one before it.
    #[serde(default = "next_page")]
    pub repagination: String,
    /// A blank page ends the document before when the start page would otherwise be skipped.
    #[serde(default)]
    pub insert_blank_page: bool,
    /// Numbers follow at once when documents are added, moved or removed or these options change.
    #[serde(default = "yes")]
    pub automatic: bool,
}

fn next_page() -> String {
    "nextPage".into()
}
fn yes() -> bool {
    true
}

impl Default for Options {
    fn default() -> Self {
        Options { repagination: next_page(), insert_blank_page: false, automatic: true }
    }
}

/// Style kinds Synchronize Book carries (`paragraph`, `character`, `object`, `cell`, `table`).
pub fn default_sync() -> Vec<String> {
    ["paragraph", "character", "object", "cell", "table", "swatch", "numberingList", "conditionalText", "textVariable", "trapStyle", "tableOfContentStyle", "crossReferenceFormat"].map(String::from).to_vec()
}

/// Every kind that can be switched on: the default ones and `masterPage` (parent pages), which is off by default as in InDesign
/// (`Book.synchronizeMasterPage` is false; oracle bookx-c3-master).
pub fn sync_kinds() -> Vec<String> {
    let mut kinds = default_sync();
    kinds.push("masterPage".into());
    kinds
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Book {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub documents: Vec<Entry>,
    /// Page numbering options of the book.
    #[serde(default)]
    pub options: Options,
    /// The entry whose styles the others follow; none means the first document (InDesign's default style source).
    #[serde(default)]
    pub style_source: Option<String>,
    /// The style kinds Synchronize Book carries.
    #[serde(default = "default_sync")]
    pub sync: Vec<String>,
    /// Merge Identically Named Layers on Export: the book's PDF has one Acrobat layer per layer name (InDesign's `Book.mergeIdenticalLayers`,
    /// on by default: buildc2-ocg-probe).
    #[serde(default = "yes")]
    pub merge_layers: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
