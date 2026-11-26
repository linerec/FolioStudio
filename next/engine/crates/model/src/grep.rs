//! Ordered character-style rules attached to a paragraph style.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrepStyle {
    pub style: String,
    pub expression: String,
    /// Original list position when editing an imported record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<usize>,
}
