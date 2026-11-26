//! Condition definitions in the document designmap.
use crate::xml::{self, Element};
use folio_model::conditions::Condition;
use std::collections::BTreeMap;

/// A UI colour as its IDML name (`Red`) or a custom colour as `r,g,b` (`<IndicatorColor type="list">` of three doubles).
fn color(e: &Element<'_>) -> String {
    if e.attr("type") == Some("list") {
        return e.children_named("ListItem").map(|i| i.text().trim().to_owned()).collect::<Vec<_>>().join(",");
    }
    e.text()
}

pub(crate) fn from_element(dm: &Element<'_>) -> Vec<Condition> {
    dm.children_named("Condition").map(|c| {
        let value = |key| c.attr(key).unwrap_or_default().to_owned();
        Condition {
            id: value("Self"), name: value("Name"), visible: c.attr("Visible") != Some("false"),
            indicator_method: value("IndicatorMethod"), underline_appearance: value("UnderlineIndicatorAppearance"),
            indicator_color: c.child("Properties").and_then(|p| p.child("IndicatorColor")).map(color).unwrap_or_default(),
        }
    }).collect()
}

pub fn read(parts: &BTreeMap<String, String>) -> Result<Vec<Condition>, String> {
    let dm = xml::parse(parts.get("designmap.xml").ok_or("missing designmap.xml")?).map_err(|e| e.0)?;
    Ok(from_element(&dm))
}
