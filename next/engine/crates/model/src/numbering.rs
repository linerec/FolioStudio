//! Named list definitions stored in the document design map.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberingList {
    pub id: String,
    pub name: String,
    pub across_stories: bool,
    pub across_documents: bool,
}
