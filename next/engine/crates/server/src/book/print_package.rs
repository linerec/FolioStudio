//! Book ▸ Package Book For Print…: every document of the book is packaged the way File ▸ Package packages one document (its own session,
//! the same plan, copies, report and PDF choice), into a folder of its own under the folder the person chose: `01 <name>`, `02 <name>`…
//! (InDesign collects the book into one folder; Folio keeps a package per document, each complete and each with its own report).
//! The folder must exist and be one the package rules allow; nothing is created outside it. A document whose package fails is a row with
//! the error (`exists`, `protected`, `space`…), the others are still made. Progress events are not sent (one reply at the end).
use std::{path::Path, sync::{Arc, Mutex, OnceLock}};
use serde_json::{Value, json};
use folio_project::{book, paths::checked_project_path};

pub(super) fn run(root: &Path, path: &str, request: &Value, profiles: &crate::Profiles) -> Result<Value, String> {
    let book = book::read(root, path)?;
    if book.documents.is_empty() { return Err("book has no documents".into()); }
    let dir = Path::new(super::field(request, "dir")?);
    if !dir.is_dir() { return Err(format!("folder_missing: {}", dir.display())); }
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::from(profiles.get()) });
    let mut rows = Vec::new();
    for (i, entry) in book.documents.iter().enumerate() {
        let name = Path::new(&entry.path).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let folder = dir.join(format!("{:02} {name}", i + 1));
        let opened = book::inspect(root, entry).and_then(|_| {
            let source = checked_project_path(root, &entry.path)?;
            crate::load_session(source.to_str().ok_or("invalid document path")?, false, &fonts, &profiles, &|_, _| {})
        });
        rows.push(match opened {
            Ok(session) => {
                let mut one = request.clone();
                one["dir"] = json!(folder.to_string_lossy());
                one["id"] = Value::Null;
                let reply = crate::package::package_command(&one, &session, None);
                if let Ok(store) = Arc::try_unwrap(session.fonts) { *fonts.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(store); }
                if reply["ok"] == json!(true) {
                    json!({"id": entry.id, "path": entry.path, "ok": true, "folder": folder.to_string_lossy(), "result": reply["result"]})
                } else {
                    json!({"id": entry.id, "path": entry.path, "ok": false, "folder": folder.to_string_lossy(), "code": reply["code"], "error": reply["error"], "suggestion": reply["suggestion"]})
                }
            }
            Err(error) => json!({"id": entry.id, "path": entry.path, "ok": false, "error": error}),
        });
    }
    Ok(json!({"dir": dir.to_string_lossy(), "documents": rows}))
}
