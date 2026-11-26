//! The New Document dialog's own ties (`"link": "newDocument"`; also the document preset editor, which has its fields):
//! the page size list and the orientation follow the typed size, the sides of the margins, bleed and slug follow one another
//! while their "all the same" box is on, the document preset is loaded when it is chosen and becomes [Custom] once a setting
//! differs, and the font styles are those of the chosen family. Lengths stay in points; the unit only changes how they are shown.
use crate::document_presets;
use serde_json::{Map, Value, json};
use std::sync::Mutex;

/// The installed fonts as `newDocumentFonts` answers them (families with their styles, the default), given when the dialog opens
/// (main.rs holds the font store); the dialog's family and style lists come from here.
static FONTS: Mutex<Value> = Mutex::new(Value::Null);

pub fn set_fonts(fonts: Value) { if let Ok(mut f) = FONTS.lock() { *f = fonts; } }
fn fonts() -> Value { FONTS.lock().map(|f| f.clone()).unwrap_or(Value::Null) }

/// The value `docpreset` holds while the settings are no preset's.
pub const CUSTOM: &str = "__custom";

/// The id of a page size of the list: its name in lower case, a run of other characters a hyphen ("Letter - Half" → `letter-half`).
fn size_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() { id.push(c) } else if !id.ends_with('-') { id.push('-') }
    }
    id.trim_matches('-').to_owned()
}

/// The page size of the list a typed size is, either way up (within 0.05 pt, as the screen took typed sizes), else `custom`.
fn size_of(w: f64, h: f64) -> String {
    folio_edit::PAGE_SIZE_PRESETS.iter()
        .find(|(_, pw, ph)| ((pw - w).abs() < 0.05 && (ph - h).abs() < 0.05) || ((ph - w).abs() < 0.05 && (pw - h).abs() < 0.05))
        .map_or_else(|| "custom".to_owned(), |(name, _, _)| size_id(name))
}

fn num(values: &Value, key: &str) -> f64 { values[key].as_f64().unwrap_or(0.) }

const SIDES: [(&str, [&str; 4]); 3] = [
    ("marginLink", ["top", "bottom", "inside", "outside"]),
    ("bleedLink", ["bleedTop", "bleedBottom", "bleedInside", "bleedOutside"]),
    ("slugLink", ["slugTop", "slugBottom", "slugInside", "slugOutside"]),
];

/// The values a preset gives the dialog (and `loaded` / `docpreset` naming it, when the dialog has them).
fn load(values: &mut Value, name: &str, preset: &Value) {
    for (key, value) in document_presets::fields_of(preset) { values[key] = value; }
    if values.get("docpreset").is_some() { values["docpreset"] = json!(name); values["loaded"] = json!(name); }
}

/// What follows from the values as they are: the size in the list, the orientation, the preset or [Custom].
pub fn settle(values: &mut Value) {
    let (w, h) = (num(values, "width"), num(values, "height"));
    values["preset"] = json!(size_of(w, h));
    values["orientation"] = json!(if w > h { "landscape" } else { "portrait" });
    if values.get("docpreset").is_some() {
        let loaded = values["loaded"].as_str().unwrap_or("").to_owned();
        let same = document_presets::named(&loaded).is_some_and(|p| document_presets::same(values, &p));
        values["docpreset"] = json!(if same { loaded } else { CUSTOM.to_owned() });
    }
}

/// The styles of a family, `[{value, label}]` (none when the family is not installed).
fn styles_of(family: &str) -> Vec<Value> {
    fonts()["fonts"].as_array().into_iter().flatten().find(|f| f["family"] == family)
        .and_then(|f| f["styles"].as_array()).into_iter().flatten()
        .filter_map(|s| s["style"].as_str()).map(|s| json!({"value": s, "label": s})).collect()
}

/// The style list of the chosen family; a style it does not have becomes Regular, or its first one (as the screen did).
pub fn fit_style(values: &mut Value, choices: &mut Value) {
    if values.get("family").is_none() { return; }
    let styles = styles_of(values["family"].as_str().unwrap_or(""));
    if !styles.iter().any(|s| s["value"] == values["style"]) {
        let pick = styles.iter().find(|s| s["value"] == "Regular").or(styles.first()).map_or_else(|| json!(""), |s| s["value"].clone());
        values["style"] = pick;
    }
    choices["fontStyles"] = Value::Array(styles);
}

/// After `changed` was set: the values it ties.
pub fn linked(values: &mut Value, choices: &mut Value, changed: &str) {
    match changed {
        "docpreset" => {
            let name = values["docpreset"].as_str().unwrap_or("").to_owned();
            if name != CUSTOM && let Some(preset) = document_presets::named(&name) { load(values, &name, &preset); }
        }
        "preset" => {
            let id = values["preset"].as_str().unwrap_or("").to_owned();
            let landscape = num(values, "width") > num(values, "height");
            if let Some((name, _, _)) = folio_edit::PAGE_SIZE_PRESETS.iter().find(|(name, _, _)| size_id(name) == id)
                && let Some((w, h)) = folio_edit::page_size_of_preset(name, landscape) {
                values["width"] = json!(w);
                values["height"] = json!(h);
            }
        }
        "orientation" => {
            let (w, h) = (num(values, "width"), num(values, "height"));
            if (w > h) != (values["orientation"] == "landscape") { values["width"] = json!(h); values["height"] = json!(w); }
        }
        "family" => fit_style(values, choices),
        // a side typed while its group is linked: the other sides take it
        other => for (link, sides) in SIDES {
            if sides.contains(&other) && values[link] == true { let v = values[other].clone(); for side in sides { values[side] = v.clone(); } }
        },
    }
    settle(values);
}

/// The values the dialog opens with (`defaults` provider): `newDocument` from the preset asked for (`values.docpreset`, else
/// [Default]) and the default installed font; `documentPreset` from the preset being edited (`values.replacing`, else [Default]).
pub fn defaults(provider: &str, requested: &Value) -> Result<Map<String, Value>, String> {
    let mut values = json!({});
    match provider {
        "newDocument" => {
            let name = requested["docpreset"].as_str().unwrap_or("").to_owned();
            let preset = document_presets::named(&name).ok_or_else(|| format!("no document preset {name}"))?;
            values["docpreset"] = json!("");
            load(&mut values, &name, &preset);
            let font = fonts()["default"].clone();
            values["family"] = font["family"].clone();
            values["style"] = font["style"].clone();
        }
        "documentPreset" => {
            let name = requested["replacing"].as_str().unwrap_or("").to_owned();
            let preset = document_presets::named(&name).ok_or_else(|| format!("no document preset {name}"))?;
            load(&mut values, &name, &preset);
            values["presetName"] = json!(name);
        }
        other => return Err(format!("unknown app dialog provider {other}")),
    }
    Ok(values.as_object().cloned().unwrap_or_default())
}

/// The lists of the dialog's choices that are not the document's: the user's presets, the installed families and styles.
pub fn choice_list(source: &str, requested: &Value) -> Option<Vec<Value>> {
    match source {
        "documentPresets" => Some(document_presets::list().iter().map(|p| json!({"value": p["name"], "label": p["name"]})).collect()),
        "fonts" => Some(fonts()["fonts"].as_array().into_iter().flatten().map(|f| json!({"value": f["family"], "label": f["family"]})).collect()),
        "fontStyles" => Some(styles_of(requested["family"].as_str().or(fonts()["default"]["family"].as_str()).unwrap_or(""))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_orientation_and_linked_sides_follow_what_was_set() {
        assert_eq!((size_id("Letter - Half"), size_id("Compact Disc"), size_id("iPad")), ("letter-half".into(), "compact-disc".into(), "ipad".into()));
        let mut values = Value::Object(document_presets::fields_of(&document_presets::default_preset()));
        let mut choices = json!({});
        settle(&mut values);
        assert_eq!((values["preset"].clone(), values["orientation"].clone()), (json!("letter"), json!("portrait")));
        values["preset"] = json!("a4");
        linked(&mut values, &mut choices, "preset");
        assert_eq!((values["width"].clone(), values["height"].clone()), (json!(595.275590551), json!(841.889763778)));
        values["orientation"] = json!("landscape");
        linked(&mut values, &mut choices, "orientation");
        assert_eq!((values["width"].clone(), values["preset"].clone()), (json!(841.889763778), json!("a4")));
        // a listed size keeps the orientation; a size typed near one (0.05 pt) is it, any other is custom
        values["preset"] = json!("b5");
        linked(&mut values, &mut choices, "preset");
        assert_eq!(values["width"], json!(708.661417323));
        values["width"] = json!(708.7);
        linked(&mut values, &mut choices, "width");
        assert_eq!(values["preset"], "b5");
        values["width"] = json!(709);
        linked(&mut values, &mut choices, "width");
        assert_eq!(values["preset"], "custom");
        // the margins follow the top while linked, not once the box is off
        values["top"] = json!(72);
        linked(&mut values, &mut choices, "top");
        assert_eq!((values["outside"].clone(), values["bleedTop"].clone()), (json!(72), json!(0)));
        values["marginLink"] = json!(false);
        values["inside"] = json!(10);
        linked(&mut values, &mut choices, "inside");
        assert_eq!((values["inside"].clone(), values["outside"].clone()), (json!(10), json!(72)));
    }
}
