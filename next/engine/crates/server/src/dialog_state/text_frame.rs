//! Starting values and linked insets for Text Frame Options.

use super::{Defaults, item};
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    // every item named is a text frame (the screen offers the dialog only then); the values are the first one's options as the
    // reader gives them, and every frame gets what changed
    let mut first = None;
    for id in ids.as_array().into_iter().flatten() {
        let id = id.as_str().ok_or("dialog needs text frame ids")?;
        let frame = item(s, id).ok_or_else(|| format!("unknown item: {id}"))?;
        let folio_model::ItemKind::TextFrame(tf) = &frame.kind else { return Err(format!("not a text frame: {id}").into()) };
        first.get_or_insert((frame, tf));
    }
    let (frame, tf) = first.ok_or("dialog needs a text frame id")?;
    let facts = crate::inspect::text_frame_dialog_facts(frame, tf);
    let o = &facts["options"];
    let inset = |k: usize| o["insets"][k].as_f64().unwrap_or(0.);
    let (columns, gutter) = (facts["columns"].as_f64().unwrap_or(1.).max(1.), o["gutter"].as_f64().unwrap_or(0.));
    // a column's width: the fixed one, or what the frame gives each column (as the screen worked it out)
    let width = o["fixedWidth"].as_f64().unwrap_or_else(|| (facts["width"].as_f64().unwrap_or(0.) - inset(1) - inset(3) - gutter * (columns - 1.)) / columns);
    values.insert("columns".into(), facts["columns"].clone());
    values.insert("gutter".into(), o["gutter"].clone());
    values.insert("fixed".into(), json!(!o["fixedWidth"].is_null()));
    values.insert("columnWidth".into(), json!(width));
    for (k, key) in ["insetTop", "insetLeft", "insetBottom", "insetRight"].into_iter().enumerate() { values.insert(key.into(), json!(inset(k))); }
    values.insert("same".into(), json!((1..4).all(|k| inset(k) == inset(0))));
    values.insert("shaped".into(), facts["shaped"].clone());
    for key in ["vertical", "ignoreWrap", "firstBaseline", "minFirstBaseline", "autoSize", "autoSizePoint", "noLineBreaks"] { values.insert(key.into(), o[key].clone()); }
    for (used, key) in [("useMinHeight", "minHeight"), ("useMinWidth", "minWidth")] {
        values.insert(used.into(), json!(!o[key].is_null()));
        values.insert(key.into(), json!(o[key].as_f64().unwrap_or(0.)));
    }
    Ok((values, json!({})))
}

// a text frame's insets: "make all settings the same" (or a shaped corner, one inset) sets the four together; turning it on
// gives all four the top one
pub(super) fn link_insets(values: &mut Value, changed: &str) {
    const INSETS: [&str; 4] = ["insetTop", "insetLeft", "insetBottom", "insetRight"];
    let together = values["same"] == true || values["shaped"] == true;
    let from = if changed == "same" && values["same"] == true { Some("insetTop") } else { INSETS.into_iter().find(|k| *k == changed && together) };
    if let Some(from) = from { let v = values[from].clone(); for k in INSETS { values[k] = v.clone(); } }
}
