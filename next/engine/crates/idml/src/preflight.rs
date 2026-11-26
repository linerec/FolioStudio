use folio_model::preflight::Profiles;
pub const SETTINGS: &str = "folio.preflightSettings";
pub const LABEL: &str = "folio.preflightProfiles";

pub fn read(source: &str) -> Result<Profiles, String> {
    let root = crate::xml::parse(source).map_err(|e|e.0)?;
    Ok(from_element(&root))
}

pub(crate) fn from_element(root: &crate::xml::Element<'_>) -> Profiles {
    let pair = root.child("Properties").and_then(|p|p.child("Label"))
        .and_then(|l|l.elements().find(|e|e.attr("Key")==Some(LABEL)));
    match pair.and_then(|e|e.attr("Value")) {
        Some(json) => serde_json::from_str(json).unwrap_or_default(),
        None => Profiles::default(),
    }
}

pub(crate) fn settings(root:&crate::xml::Element<'_>)->folio_model::preflight::Settings {
    root.child("Properties").and_then(|p|p.child("Label"))
        .and_then(|l|l.elements().find(|e|e.attr("Key")==Some(SETTINGS)))
        .and_then(|e|e.attr("Value")).and_then(|v|serde_json::from_str(v).ok()).unwrap_or_default()
}
