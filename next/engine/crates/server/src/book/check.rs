//! Book ▸ Preflight Book…: the preflight of every document of the book. Each document is opened the way the book's PDF export opens it
//! (a session of its own; nothing of the document on screen is touched) and checked with the preflight panel's own rules and profile;
//! the findings come back in the panel's shape. A document that cannot be opened is a row with its error, not the end of the run.
use std::{path::Path, sync::{Arc, Mutex, OnceLock}};
use serde_json::{Value, json};
use folio_project::{book, paths::checked_project_path};

pub(super) fn run(root: &Path, path: &str, profiles: &crate::Profiles) -> Result<Value, String> {
    let book = book::read(root, path)?;
    if book.documents.is_empty() { return Err("book has no documents".into()); }
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::from(profiles.get()) });
    let mut rows = Vec::new();
    for entry in &book.documents {
        let opened = book::inspect(root, entry).and_then(|_| {
            let source = checked_project_path(root, &entry.path)?;
            crate::load_session(source.to_str().ok_or("invalid document path")?, false, &fonts, &profiles, &|_, _| {})
        });
        rows.push(match opened {
            Ok(session) => {
                let report = crate::print::preflight_view(&session.preflight(true), session.locations());
                if let Ok(store) = Arc::try_unwrap(session.fonts) { *fonts.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(store); }
                json!({"id": entry.id, "path": entry.path, "ok": true, "report": report})
            }
            Err(error) => json!({"id": entry.id, "path": entry.path, "ok": false, "error": error}),
        });
    }
    Ok(json!({"documents": rows}))
}
