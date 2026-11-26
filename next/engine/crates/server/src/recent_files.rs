//! Recent paths belong to this computer's settings, including migration from the old shell files.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Project { root: String, #[serde(default)] id: String, #[serde(default)] name: String }

fn projects(value: &Value) -> Result<Vec<Project>, String> {
    let list: Vec<Project> = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if list.iter().any(|p| p.root.is_empty()) { return Err("project root must not be empty".into()); }
    Ok(list)
}
fn documents(value: &Value) -> Result<Vec<String>, String> {
    let list: Vec<String> = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if list.iter().any(String::is_empty) { return Err("document path must not be empty".into()); }
    Ok(list)
}
fn same_path(a: &str, b: &str) -> bool {
    if cfg!(target_os = "windows") { a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/")) } else { a == b }
}
fn legacy(root: &Path, name: &str) -> Value {
    std::fs::read(root.join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!([]))
}

pub fn run(req: &Value) -> Result<Value, String> {
    if req["cmd"] != "setRecentFiles" && ["documents", "projects"].iter().any(|k| req.get(k).is_some()) {
        return Err("use setRecentFiles to replace lists".into());
    }
    let settings = crate::user_settings::update(|settings, root| {
        let docs = settings.entry("recentDocuments").or_insert_with(|| {
            let value = legacy(root, "recent.json");
            if documents(&value).is_ok() { value } else { json!([]) }
        }).clone();
        let projs = settings.entry("recentProjects").or_insert_with(|| {
            let value = legacy(root, "recent-projects.json");
            if projects(&value).is_ok() { value } else { json!([]) }
        }).clone();
        let mut docs = documents(req.get("documents").unwrap_or(&docs))?;
        let mut projs = projects(req.get("projects").unwrap_or(&projs))?;
        if req["cmd"] == "rememberRecent" {
            if req.get("document").is_some() == req.get("project").is_some() { return Err("provide document or project".into()); }
            if let Some(value) = req.get("document") {
                let path = documents(&json!([value]))?.remove(0);
                docs.retain(|p| !same_path(p, &path));
                docs.insert(0, path);
            }
            if let Some(value) = req.get("project") {
                let p = projects(&json!([value]))?.remove(0);
                projs.retain(|q| !same_path(&q.root, &p.root) && (p.id.is_empty() || p.id != q.id));
                projs.insert(0, p);
            }
        }
        let mut unique_docs = vec![];
        for p in docs { if !unique_docs.iter().any(|q: &String| same_path(q, &p)) { unique_docs.push(p); } }
        unique_docs.truncate(8);
        let mut unique_projs: Vec<Project> = vec![];
        for p in projs { if !unique_projs.iter().any(|q| same_path(&q.root, &p.root) || (!p.id.is_empty() && p.id == q.id)) { unique_projs.push(p); } }
        unique_projs.truncate(8);
        settings.insert("recentDocuments".into(), json!(unique_docs));
        settings.insert("recentProjects".into(), json!(unique_projs));
        Ok(())
    })?;
    let mut result = json!({"documents": settings["recentDocuments"], "projects": settings["recentProjects"]});
    if req["cmd"] != "recentFiles" { result["_settingsEvent"] = crate::user_settings::event(&settings, &req["origin"]); }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_paths_and_project_shape() {
        assert!(documents(&json!([null])).is_err());
        assert!(documents(&json!([""])).is_err());
        assert!(projects(&json!([{"root": ""}])).is_err());
        assert!(projects(&json!([{"root": "project", "name": "test"}])).is_ok());
        assert!(!same_path("a", "b"));
    }
}
