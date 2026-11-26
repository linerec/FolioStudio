//! IDML hyperlinks and cross-reference definitions. Unknown XML remains in the package.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hyperlinks {
    pub links: Vec<Hyperlink>,
    pub sources: Vec<TextSource>,
    /// Hyperlink sources that are page items (IDML `HyperlinkPageItemSource`): the object, a frame or a placed graphic's frame, is the link.
    pub item_sources: Vec<ItemSource>,
    pub destinations: Vec<Destination>,
    pub formats: Vec<CrossReferenceFormat>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hyperlink {
    pub id: String,
    pub name: String,
    pub source: String,
    pub destination: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextSource {
    pub id: String,
    pub story: String,
    pub start: usize,
    pub end: usize,
    pub format: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ItemSource {
    pub id: String,
    /// The page item (IDML `SourcePageItem`) the link starts from.
    pub item: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub value: String,
    pub story: String,
    pub at: usize,
    /// A page destination's zoom setting as IDML writes it (`ViewSetting`: Fixed, FitView, FitWidth, FitHeight, FitVisible, InheritZoom); empty when absent.
    pub view_setting: String,
    /// A page destination's fixed magnification in percent (`ViewPercentage`); absent when the file has none.
    pub view_percentage: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CrossReferenceFormat {
    pub id: String,
    pub name: String,
    pub character_style: String,
    pub blocks: Vec<BuildingBlock>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuildingBlock {
    pub kind: String,
    pub text: String,
    pub character_style: String,
    pub delimiter: String,
    pub include_delimiter: bool,
}
