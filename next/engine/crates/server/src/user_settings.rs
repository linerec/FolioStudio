//! App settings share Electron's user profile, never a document or project folder.
use serde_json::{Map, Value, json};
use std::fs::{self, OpenOptions};
use std::path::Path;

pub fn run(request: &Value) -> Result<Value, String> {
    let root = std::env::var_os("FOLIO_USER_DATA").ok_or("FOLIO_USER_DATA is not configured")?;
    access(Path::new(&root), request).map(|settings| json!({"settings": settings}))
}

pub fn command(request: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let mut result = run(request)?;
    if session.is_some() && ["patch", "values", "migrate"].iter().any(|key| request.get(key).is_some()) {
        if let Some(value) = result["settings"]["preferences"].get("folio.showHiddenSpreads") {
            let view = crate::view_state::patch(&json!({"patch": {"showHiddenSpreads": value == "1"}, "origin": request["origin"]}), session)?;
            if let Some(event) = view.get("_viewEvent") { result["_viewEvent"] = event.clone(); }
        }
    }
    if ["patch", "values", "migrate"].iter().any(|key| request.get(key).is_some()) {
        result["_settingsEvent"] = event(&result["settings"], &request["origin"]);
    }
    Ok(result)
}

pub fn event(settings: &Value, origin: &Value) -> Value {
    json!({"event": "user-settings", "detail": {"settings": settings, "origin": origin}})
}

pub fn update(change: impl FnOnce(&mut Map<String, Value>, &Path) -> Result<(), String>) -> Result<Value, String> {
    let root = std::env::var_os("FOLIO_USER_DATA").ok_or("FOLIO_USER_DATA is not configured")?;
    access_with(Path::new(&root), &json!({}), change)
}

fn object<'a>(request: &'a Value, key: &str) -> Result<Option<&'a Map<String, Value>>, String> {
    request.get(key).map(|value| value.as_object().ok_or_else(|| format!("{key} must be an object"))).transpose()
}

fn access(root: &Path, request: &Value) -> Result<Value, String> {
    access_with(root, request, |_, _| Ok(()))
}

fn access_with(root: &Path, request: &Value, change: impl FnOnce(&mut Map<String, Value>, &Path) -> Result<(), String>) -> Result<Value, String> {
    let patch = object(request, "patch")?;
    let values = object(request, "values")?;
    let migrate = object(request, "migrate")?;
    if [patch.is_some(), values.is_some(), migrate.is_some()].into_iter().filter(|v| *v).count() > 1 {
        return Err("use only one of patch, values or migrate".into());
    }
    for entries in [values, migrate].into_iter().flatten() {
        if entries.values().any(|v| !v.is_string() && !v.is_boolean() && !v.is_number()) {
            return Err("preference values must be strings, booleans or numbers".into());
        }
    }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .open(root.join("settings.lock")).map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let path = root.join("settings.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let parsed = bytes.as_deref().and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
    let mut settings = parsed.as_ref().and_then(Value::as_object).cloned().unwrap_or_default();
    let before = settings.clone();
    if let Some(patch) = patch { settings.extend(patch.clone()); }
    if let Some(entries) = values.or(migrate).filter(|entries| !entries.is_empty()) {
        let preferences = settings.entry("preferences").or_insert_with(|| json!({}));
        if !preferences.is_object() { *preferences = json!({}); }
        if let Some(preferences) = preferences.as_object_mut() {
            for (key, value) in entries {
                if migrate.is_none() || !preferences.contains_key(key) {
                    preferences.insert(key.clone(), value.clone());
                }
            }
        }
    }
    change(&mut settings, root)?;
    if settings != before {
        if parsed.as_ref().is_none_or(|v| !v.is_object()) {
            if let Some(bytes) = &bytes {
                folio_project::atomic::write_bytes(&root.join("settings.corrupt.json"), bytes).map_err(|e| e.to_string())?;
            }
        }
        let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
        folio_project::atomic::write_bytes(&path, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(Value::Object(settings))
}

#[cfg(test)]
#[path = "user_settings_tests.rs"]
mod tests;
