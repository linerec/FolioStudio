//! The window preferences available through an attached MCP connection.
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

const PROGRAMS: &str = "folio.editWith.programs";
static KEYS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../shared/renderer-preferences.json"))
        .expect("renderer preference keys")
});

pub fn run(request: &Value) -> Result<Value, String> {
    let values = request
        .get("values")
        .map(|value| value.as_object().ok_or("values must be an object"))
        .transpose()?;
    for (key, value) in values.into_iter().flatten() {
        if !KEYS.contains(&key.as_str()) {
            return Err(format!("unknown renderer preference: {key}"));
        }
        if !value.is_string() {
            return Err(format!("{key} must be a string"));
        }
    }

    let settings = crate::user_settings::update(|settings, _| {
        let Some(values) = values.filter(|values| !values.is_empty()) else {
            return Ok(());
        };
        let preferences = settings.entry("preferences").or_insert_with(|| json!({}));
        if let Some(value) = values.get(PROGRAMS) {
            let current = preferences
                .get(PROGRAMS)
                .and_then(Value::as_str)
                .and_then(|value| serde_json::from_str::<Vec<Value>>(value).ok())
                .unwrap_or_default();
            let requested: Vec<String> = serde_json::from_str(value.as_str().unwrap())
                .map_err(|_| "Edit With programs must be a JSON array of strings")?;
            // A saved program may be launched by the user from Edit With. Keep program selection in the file picker.
            if requested
                .iter()
                .any(|path| !current.iter().any(|entry| entry.as_str() == Some(path)))
            {
                return Err("new Edit With programs must be picked in the app".into());
            }
            if requested
                .iter()
                .enumerate()
                .any(|(index, path)| requested[..index].contains(path))
            {
                return Err("duplicate Edit With program".into());
            }
        }
        if !preferences.is_object() {
            *preferences = json!({});
        }
        preferences.as_object_mut().unwrap().extend(values.clone());
        Ok(())
    })?;

    let saved: Map<String, Value> = KEYS
        .iter()
        .filter_map(|key| {
            settings["preferences"]
                .get(*key)
                .map(|value| ((*key).to_owned(), value.clone()))
        })
        .collect();
    let mut result = json!({"values": saved});
    if values.is_some_and(|values| !values.is_empty()) {
        result["_settingsEvent"] = crate::user_settings::event(&settings, &request["origin"]);
    }
    Ok(result)
}
