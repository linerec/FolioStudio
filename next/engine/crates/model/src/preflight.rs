use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub disabled_rules: Vec<String>,
    pub parameters: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profiles {
    pub active: String,
    pub profiles: Vec<Profile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub page_range: String,
    pub layers: String,
    pub include_pasteboard: bool,
    pub include_nonprinting: bool,
    pub off: bool,
}
impl Default for Settings {
    fn default()->Self {Self{page_range:String::new(),layers:"visiblePrintable".into(),include_pasteboard:false,include_nonprinting:false,off:false}}
}
