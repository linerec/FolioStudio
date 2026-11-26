//! PDF settings endpoints; the PDF crate validates values, project owns storage.
use folio_pdf::{PdfPreset, Preset};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Action { List, Save, Delete, Import, Export }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    action: Action,
    project: Option<PathBuf>,
    name: Option<String>,
    settings: Option<PdfPreset>,
    replace: Option<bool>,
    path: Option<PathBuf>,
}

pub fn handle(req: &Value) -> Option<Value> {
    if req["cmd"] != "pdfPresets" { return None; }
    Some(match run(req) {
        Ok(result) => json!({"id": req["id"], "ok": true, "result": result}),
        Err(error) => json!({"id": req["id"], "ok": false, "error": error}),
    })
}

pub(crate) fn run(value: &Value) -> Result<Value, String> {
    let r: Request = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let root = || r.project.as_deref().ok_or("a project is required".to_string());
    let name = || r.name.as_deref().ok_or("a preset name is required".to_string());
    let settings = || -> Result<&PdfPreset, String> {
        let p = r.settings.as_ref().ok_or("PDF settings are required")?;
        p.validate()?; Ok(p)
    };
    match r.action {
        Action::List => {
            let mut entries: Vec<Value> = Preset::all()
                .map(|p| json!({"id": p.key(), "builtin": true, "available": p != Preset::Small, "settings": PdfPreset::builtin(p)})).collect();
            if let Some(root) = &r.project {
                for (name, preset) in folio_project::presets::list::<PdfPreset>(root)? {
                    preset.validate()?;
                    if preset.name != name { return Err(format!("PDF preset name does not match its file: {name}")); }
                    entries.push(json!({"id": format!("project:{name}"), "builtin": false, "available": preset.standard != Preset::Small, "settings": preset}));
                }
            }
            Ok(json!(entries))
        }
        Action::Save => {
            let p = settings()?;
            let name = name()?;
            if p.name != name { return Err("preset name and file name must agree".into()); }
            if r.replace != Some(true) && folio_project::presets::list::<PdfPreset>(root()?)?.iter().any(|(n,_)| n == name) {
                return Err("a PDF preset with this name already exists".into());
            }
            folio_project::presets::save(root()?, name, p)?; Ok(json!(true))
        }
        Action::Delete => { folio_project::presets::delete(root()?, name()?)?; Ok(json!(true)) }
        Action::Import => {
            let file = r.path.as_deref().ok_or("a JSON file is required")?;
            if file.extension().and_then(|s| s.to_str()) != Some("json") { return Err("only Folio JSON presets can be imported".into()); }
            let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
            let p: PdfPreset = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            p.validate()?;
            Ok(json!(p))
        }
        Action::Export => {
            let p = settings()?;
            let file = r.path.as_deref().ok_or("a JSON file is required")?;
            if file.extension().and_then(|s| s.to_str()) != Some("json") { return Err("use the .json extension".into()); }
            let bytes = serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?;
            crate::print::write_atomically(file, &bytes)?; Ok(json!(true))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_requests_keep_the_same_error_reply() {
        for request in [Value::Null, json!([]), json!({"cmd": "other"})] {
            assert!(handle(&request).is_none());
        }
        for (fields, error) in [
            (json!({"action": "save"}), "PDF settings are required"),
            (json!({"action": "delete"}), "a project is required"),
            (json!({"action": "import"}), "a JSON file is required"),
            (json!({"action": "import", "path": "preset.txt"}), "only Folio JSON presets can be imported"),
        ] {
            let mut request = fields;
            request["cmd"] = json!("pdfPresets");
            request["id"] = json!(7);
            assert_eq!(serde_json::to_vec(&handle(&request).unwrap()).unwrap(),
                serde_json::to_vec(&json!({"id": 7, "ok": false, "error": error})).unwrap());
        }
    }

    #[test]
    fn save_checks_settings_then_name_then_project() {
        let settings = PdfPreset::builtin(Preset::all().next().unwrap());
        let mut request = json!({"action": "save", "settings": settings});
        assert_eq!(run(&request), Err("a preset name is required".into()));
        request["name"] = json!("wrong name");
        assert_eq!(run(&request), Err("preset name and file name must agree".into()));
        request["name"] = json!(settings.name);
        assert_eq!(run(&request), Err("a project is required".into()));
    }
}
