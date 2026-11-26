use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnLayout {
    #[default]
    Single,
    Span,
    Split,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParagraphColumns {
    pub layout: ColumnLayout,
    /// None means all frame columns.
    pub count: Option<u32>,
    pub before: f64,
    pub after: f64,
    pub inside: f64,
    pub outside: f64,
}

impl Default for ParagraphColumns {
    fn default() -> Self {
        Self { layout: ColumnLayout::Single, count: None, before: 0.0, after: 0.0, inside: 6.0, outside: 0.0 }
    }
}
