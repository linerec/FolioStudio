//! Project-local portable JSON presets. The caller owns the settings schema.
use crate::{RelPath, atomic, paths};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

fn folder(root: &Path, create: bool) -> Result<PathBuf, String> {
    crate::manifest::load(root).map_err(|e| e.to_string())?;
    let mut dir = root.to_path_buf();
    for part in ["presets", "pdf"] {
        dir.push(part);
        if dir.exists() {
            let meta = std::fs::symlink_metadata(&dir).map_err(|e| e.to_string())?;
            if !meta.is_dir() || meta.file_type().is_symlink() || !dir.canonicalize().map_err(|e| e.to_string())?.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
                return Err("preset folder must be inside the project, without links".into());
            }
        } else if create { std::fs::create_dir(&dir).map_err(|e| e.to_string())?; }
    }
    Ok(dir)
}

fn path(root: &Path, name: &str, create: bool) -> Result<PathBuf, String> {
    paths::check_name(name).map_err(|e| e.to_string())?;
    let file = format!("{name}.json");
    let relative = RelPath::from_names(&["presets", "pdf", &file]).map_err(|e| e.to_string())?;
    let dir = folder(root, create)?;
    if dir.exists() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let existing = entry.file_name().to_string_lossy().to_string();
            if paths::collision_key(&existing) == paths::collision_key(&file) {
                if existing != file { return Err("preset name differs only in case or normalization".into()); }
                let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
                if !meta.is_file() || meta.file_type().is_symlink() { return Err("preset must be a regular file".into()); }
            }
        }
    }
    Ok(relative.to_native(root))
}

pub fn list<T: DeserializeOwned>(root: &Path) -> Result<Vec<(String, T)>, String> {
    let dir = folder(root, false)?;
    if !dir.exists() { return Ok(Vec::new()); }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("json") {
            names.push(p.file_stem().expect("a path with the checked json extension has a file stem").to_string_lossy().to_string());
        }
    }
    names.sort();
    names.into_iter().map(|name| {
        let data = std::fs::read(path(root, &name, false)?).map_err(|e| e.to_string())?;
        let preset = serde_json::from_slice(&data).map_err(|e| format!("{name}: {e}"))?;
        Ok((name, preset))
    }).collect()
}

pub fn save<T: Serialize>(root: &Path, name: &str, value: &T) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let target = path(root, name, true)?;
    atomic::write_with(&target, &mut |w| std::io::Write::write_all(w, &data)).map_err(|e| e.to_string())
}

pub fn delete(root: &Path, name: &str) -> Result<(), String> {
    std::fs::remove_file(path(root, name, false)?).map_err(|e| e.to_string())
}
