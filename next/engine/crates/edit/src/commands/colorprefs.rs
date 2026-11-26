use crate::{EditError, Work, xml::Index};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const PART: &str = "designmap.xml";
const PROFILES: &[&str] = &["RGBProfile", "CMYKProfile"];
const POLICIES: &[&str] = &["RGBPolicy", "CMYKPolicy"];
const INTENTS: &[&str] = &["SolidColorIntent", "AfterBlendingIntent", "DefaultImageIntent"];
const POLICY_VALUES: &[&str] = &["ColorPolicyOff", "PreserveEmbeddedProfiles", "ConvertToWorkingSpace", "CombinationOfPreserveAndSafeCmyk"];
const INTENT_VALUES: &[&str] = &["UseColorSettings", "Perceptual", "Saturation", "RelativeColorimetric", "AbsoluteColorimetric"];
fn index(t: &str) -> Result<(Index, usize), EditError> {
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e.to_string()))?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(t,i)=="Document").ok_or_else(|| EditError::new("xml","missing Document"))?;
    Ok((ix,i))
}
pub(crate) fn set(w: &mut Work<'_>, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    for (key, value) in props {
        let valid = if PROFILES.contains(&key.as_str()) { value.as_str().is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_control)) }
        else if POLICIES.contains(&key.as_str()) { value.as_str().is_some_and(|s| POLICY_VALUES.contains(&s)) }
        else if INTENTS.contains(&key.as_str()) { value.as_str().is_some_and(|s| INTENT_VALUES.contains(&s)) }
        else { key=="AccurateLABSpots" && value.is_boolean() };
        if !valid { return Err(EditError::new("invalid",format!("invalid document color preference: {key}"))); }
    }
    let t = w.text(PART)?.to_owned();
    let (ix,i) = index(&t)?;
    let edits = props.iter().filter_map(|(key,v)| {
        let value = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
        (ix.attr(&t,i,key)!=Some(value.as_str())).then(|| crate::ops::attr_edit(&ix,&t,i,key,&value))
    }).collect();
    w.splice(PART,edits)
}
impl crate::Editor {
    pub fn color_preferences(&self) -> Result<BTreeMap<String,Value>,EditError> {
        let t = self.part(PART).ok_or_else(|| EditError::new("notFound",PART))?;
        let (ix,i) = index(t)?;
        let mut out = BTreeMap::new();
        for key in PROFILES.iter().chain(POLICIES).chain(INTENTS) {
            let default = if INTENTS.contains(key) { "UseColorSettings" } else if POLICIES.contains(key) { "ColorPolicyOff" } else { "" };
            out.insert((*key).into(),json!(ix.attr(t,i,key).map(crate::xml::unescape).unwrap_or_else(|| default.into())));
        }
        out.insert("AccurateLABSpots".into(),json!(ix.attr(t,i,"AccurateLABSpots")==Some("true")));
        Ok(out)
    }
}
