//! Starting values and conversion preview for document color profile dialogs.

use super::{Defaults, Refusal, all_same, app_defaults};
use crate::Session;
use serde_json::{Map, Value, json};

// Edit > Assign Profiles: the document's colour settings as IDML writes them (profiles, policies, intents, Lab spots)
pub(super) fn assign_defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    let prefs = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.color_preferences().map_err(|e| e.to_string())?;
    for (key, value) in prefs { values.insert(key, value); }
    Ok((values, json!({})))
}

// Edit > Convert to Profile: from the document's profiles (the engine's defaults when it names none) to the working spaces, with the
// colour settings' intent and black point compensation (InDesign starts there)
pub(super) fn convert_defaults(s: &Session, requested: &Value) -> Defaults {
    let mut values = Map::new();
    let prefs = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.color_preferences().map_err(|e| e.to_string())?;
    for (key, field) in [("RGBProfile", "sourceRgb"), ("CMYKProfile", "sourceCmyk")] {
        if let Some(name) = prefs.get(key).and_then(Value::as_str).filter(|n| !n.is_empty()) { values.insert(field.into(), json!(name)); }
    }
    // (no app settings to read: the fields' defaults, which are InDesign's)
    let app = app_defaults("userPreferences:colorSettings.", requested).unwrap_or_default();
    for (key, field) in [("workingSpaceRGB", "destRgb"), ("workingSpaceCMYK", "destCmyk"), ("intent", "intent")] {
        if let Some(v) = app.get(key) { values.insert(field.into(), v.clone()); }
    }
    values.insert("bpc".into(), json!(app.get("useBPC") != Some(&json!(false))));
    Ok((values, json!({})))
}

// Convert to Profile: a computed text (`kind: "text"`), the numbers a trial of the values gives as `values` ({rgb}, {cmyk}, … of the text)
pub(super) fn preview(def: &Value, values: &Value, args: Value, session: Option<&Session>) -> Result<Value, Refusal> {
    // (to the profiles the document has: nothing changes, as the screen said without asking)
    if all_same(def, values) { return Ok(json!({"ok": true, "values": {"rgb": 0, "cmyk": 0, "renamed": 0}})); }
    let edit = session.and_then(|s| s.edit.as_ref()).ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation");
    let text = |k: &str| args[k].as_str().unwrap_or_default().to_owned();
    let plan = edit.editor.convert_to_profile_plan(&text("rgbProfile"), &text("cmykProfile"), &text("intent"), args["blackPointCompensation"].as_bool().unwrap_or(true))
        .map_err(|e| e.to_string())?;
    Ok(json!({"ok": true, "values": plan["count"]}))
}
