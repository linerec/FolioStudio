//! Ephemeral print requests. Device destinations never belong to a document.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PrintSelection {
    All,
    Range { value: String },
    Pages { ids: Vec<crate::Id> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintJob {
    pub selection: PrintSelection,
    pub spreads: bool,
    pub print_blank_pages: bool,
}
