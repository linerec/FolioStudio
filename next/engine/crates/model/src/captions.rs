use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptionRow {
    pub before: String,
    pub metadata: String,
    pub after: String,
}
impl Default for CaptionRow {
    fn default() -> Self { Self { before:String::new(), metadata:"Name".into(), after:String::new() } }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all="camelCase")]
pub struct CaptionSetup {
    pub rows: Vec<CaptionRow>,
    pub position: String,
    pub offset: f64,
    pub paragraph_style: String,
    pub layer: String,
    pub group: bool,
}
impl Default for CaptionSetup {
    fn default() -> Self { Self { rows:vec![CaptionRow::default()],position:"below".into(),offset:1.0,paragraph_style:String::new(),layer:String::new(),group:false } }
}
