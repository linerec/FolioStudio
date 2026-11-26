//! Starting values for page margins, document size and orientation.

use super::fields::number;
use super::{Defaults, document_info};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn margins(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs a page id")?;
    let info = document_info(s)?;
    let page = info["pages"].as_array().and_then(|p| p.iter().find(|p| p["id"] == id)).ok_or_else(|| format!("unknown page: {id}"))?;
    let text = crate::inspect::command(&json!({"cmd":"pageItems","page":page["index"]}), &s.doc, &s.layout, &s.colors, &s.inspect)?;
    let items: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let items = &items["items"];
    for (key, at) in [("top", 0), ("left", 1), ("bottom", 2), ("right", 3)] { values.insert(key.into(), items["margins"][at].clone()); }
    values.insert("columns".into(), items["columns"]["count"].clone());
    values.insert("gutter".into(), items["columns"]["gutter"].clone());
    // IDML Left/Right are the inside and outside margins on facing pages
    let context = json!({"facing": items["side"].as_str() != Some("single"), "pages": ids.as_array().map_or(0, Vec::len)});
    Ok((values, context))
}

pub(super) fn size(s: &Session) -> Defaults {
    let mut values = Map::new();
    // the page size the document is set up with (the first page's when it names none)
    for key in ["width", "height"] {
        let size = s.summary["pageSettings"][key].as_f64().or_else(|| s.summary["pages"][0][key].as_f64()).ok_or("the document has no page size")?;
        values.insert(key.into(), json!(size));
    }
    let landscape = values["width"].as_f64() > values["height"].as_f64();
    values.insert("orientation".into(), json!(if landscape { "landscape" } else { "portrait" }));
    Ok((values, json!({})))
}

// a page is landscape when it is wider than high; choosing the other orientation turns the size
pub(super) fn link_orientation(values: &mut Value, changed: &str) {
    let (w, h) = (number(values, "width"), number(values, "height"));
    if changed == "orientation" {
        if (w > h) != (values["orientation"] == "landscape") { values["width"] = json!(h); values["height"] = json!(w); }
    } else if changed == "width" || changed == "height" {
        values["orientation"] = json!(if w > h { "landscape" } else { "portrait" });
    }
}
