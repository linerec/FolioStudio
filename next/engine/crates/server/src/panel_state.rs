//! Panel placement is a user setting. Drag calculations remain in the dock layout owner.
use serde_json::{Value, json};
use std::collections::HashSet;

fn spec() -> Value { serde_json::from_str(include_str!("../../../../shared/panels.json")).expect("panel schema") }

fn validate(state: &Value) -> Result<(), String> {
    let schema = spec();
    let dock = &state["dock"];
    if !state["hidden"].is_boolean() || !dock["columnCollapsed"].is_boolean() { return Err("hidden and columnCollapsed must be boolean".into()); }
    let mut seen = HashSet::new();
    let mut group = |list: &Value, active: &Value| -> Result<(), String> {
        let ids = list.as_array().ok_or("panel group must be an array")?;
        if ids.is_empty() || !ids.contains(active) { return Err("active panel must belong to its group".into()); }
        for id in ids {
            let id = id.as_str().ok_or("panel id must be a string")?;
            if schema["panels"].get(id).is_none() || !seen.insert(id.to_owned()) { return Err(format!("unknown or duplicate panel: {id}")); }
        }
        Ok(())
    };
    for zone in ["strip", "column"] {
        for list in dock[zone].as_array().ok_or("dock zone must be an array")? {
            let first = list[0].as_str().ok_or("empty panel group")?;
            group(list, &dock["active"][first])?;
        }
    }
    for floating in dock["floating"].as_array().ok_or("floating must be an array")? {
        group(&floating["panels"], &floating["active"])?;
        for (key, min) in [("x",0.),("y",0.),("w",120.),("h",80.)] {
            if floating[key].as_f64().is_none_or(|v| !v.is_finite() || v < min) { return Err(format!("invalid floating {key}")); }
        }
    }
    if let Some(folded) = dock.get("folded") {
        if folded.as_object().is_none_or(|v| v.values().any(|v| !v.is_boolean())) { return Err("folded must map groups to booleans".into()); }
    }
    if !state["flyout"].is_null() {
        let f = &state["flyout"];
        let index = f["group"].as_u64().ok_or("flyout group must be an index")? as usize;
        if dock["strip"].get(index).and_then(Value::as_array).is_none_or(|g| !g.contains(&f["active"])) { return Err("flyout panel must belong to its strip group".into()); }
    }
    Ok(())
}

pub fn run(req: &Value) -> Result<Value, String> {
    let settings = crate::user_settings::update(|settings, _| {
        if req["cmd"] == "setPanelState" {
            let patch = req["patch"].as_object().ok_or("patch must be an object")?;
            if patch.keys().any(|k| !["dock","hidden","flyout"].contains(&k.as_str())) { return Err("unknown panel state field".into()); }
            let mut state = state(&Value::Object(settings.clone()));
            for (key, value) in patch { state[key] = value.clone(); }
            validate(&state)?;
            settings.insert("dock".into(), state["dock"].clone());
            settings.insert("panelVisibility".into(), json!({"hidden":state["hidden"], "flyout":state["flyout"]}));
        }
        Ok(())
    })?;
    let mut result = json!({"panelState":state(&settings)});
    if req["cmd"] == "setPanelState" { result["_settingsEvent"] = crate::user_settings::event(&settings, &req["origin"]); }
    Ok(result)
}

fn state(settings: &Value) -> Value {
    json!({"dock":settings.get("dock").filter(|v|!v.is_null()).cloned().unwrap_or_else(||spec()["default"].clone()),
        "hidden":settings["panelVisibility"]["hidden"].as_bool().unwrap_or(false),
        "flyout":settings["panelVisibility"]["flyout"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_layouts_that_the_screen_cannot_display() {
        let mut s = state(&json!({}));
        assert!(validate(&s).is_ok());
        assert!(validate(&state(&json!({"dock":null}))).is_ok());
        s["flyout"] = json!({"group":0,"active":"pages"});
        assert!(validate(&s).is_ok());
        s["flyout"]["active"] = json!("project");
        assert!(validate(&s).is_err());
        s["flyout"] = Value::Null;
        s["dock"]["strip"][0][0] = json!("unknown");
        assert!(validate(&s).is_err());
    }
}
