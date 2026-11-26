//! Starting values for new and existing hyperlinks and cross-references.

use super::{Defaults, refuse};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn new_defaults(provider: &str, s: &Session, requested: &Value) -> Defaults {
    let mut values = Map::new();
    // the text the link is made in comes with the request (`story`, `start`, `end`); the name continues the numbering of the document's links
    let cross = provider == "newCrossReference";
    let _story = requested["story"].as_str().filter(|t| !t.is_empty()).ok_or_else(|| refuse("story", "required", "the dialog needs the story the link is made in".into()))?;
    for key in if cross { vec!["start"] } else { vec!["start", "end"] } {
        if !requested[key].is_number() { return Err(refuse(key, "required", format!("the dialog needs {key}, the position in the story"))); }
    }
    let links = &s.doc.hyperlinks;
    let prefix = requested["namePrefix"].as_str().filter(|p| !p.is_empty()).unwrap_or(if cross { "Cross-Reference" } else { "Hyperlink" });
    let name = (1..).map(|n| format!("{prefix} {n}")).find(|name| links.links.iter().all(|l| &l.name != name)).unwrap_or_default();
    values.insert("name".into(), json!(name));
    if cross {
        values.insert("destination".into(), json!(links.destinations.iter().find(|d| !d.story.is_empty()).map_or("", |d| d.id.as_str())));
        values.insert("format".into(), json!(links.formats.iter().find(|f| f.name == "Paragraph Text").or(links.formats.first()).map_or("", |f| f.id.as_str())));
    }
    Ok((values, json!({})))
}

pub(super) fn edit_defaults(provider: &str, s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a link id")?;
    let links = &s.doc.hyperlinks;
    let link = links.links.iter().find(|l| l.id == id).ok_or_else(|| format!("unknown link: {id}"))?;
    values.insert("name".into(), json!(link.name));
    if provider == "editHyperlink" {
        values.insert("url".into(), json!(links.destinations.iter().find(|d| d.id == link.destination).map_or("", |d| d.value.as_str())));
    } else {
        values.insert("destination".into(), json!(link.destination));
        values.insert("format".into(), json!(links.sources.iter().find(|x| x.id == link.source).map_or("", |x| x.format.as_str())));
    }
    Ok((values, json!({})))
}
