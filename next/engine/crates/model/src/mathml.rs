//! Imported equation metadata. Missing properties stay missing; no layout is inferred.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MathObject {
    pub id: crate::Id,
    /// Decoded MathML XML from `MathMLDescription`.
    pub source: Option<String>,
    pub font_size: Option<f64>,
    pub rgb: Option<[f64; 3]>,
    pub tint: Option<f64>,
    /// Stored swatch reference, or `Nothing` when there is no applied swatch.
    pub swatch: Option<String>,
    pub is_mathml: Option<bool>,
    /// Opaque `MathmlSvgData` JSON, including fields this version does not know.
    /// This is document content, not a source of reusable font outlines.
    pub svg_data: Option<String>,
}
