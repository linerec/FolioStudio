//! Book ▸ Update All Cross-References: every document of the book updates its cross-references (the editor's own
//! `updateCrossReference` with no id, which is "all"), with the page and paragraph-number locations of the document's composed
//! pages as the cross-reference commands of one document get them (xrefloc.rs). The documents are written like Synchronize Book
//! writes them: all or none, atomically, the pins renewed, a document someone has open stops the run. A `.idml` document of the
//! book is checked but never written.
use super::{Doc, load, write_all};
use folio_edit::commands::RegisteredCommand;
use folio_edit::Editor;
use folio_project::book;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// The updated document, or None when nothing in it changes.
fn updated(d: &Doc, fonts: &crate::Fonts, profiles: &Arc<crate::Profiles>) -> Result<Option<Editor>, String> {
    let session = crate::load_session(d.file.to_str().ok_or("invalid document path")?, false, fonts, profiles, &|_, _| {})?;
    let locations = folio_text::layout::toc::sources(&session.doc, &session.layout);
    if let Ok(store) = Arc::try_unwrap(session.fonts) {
        *fonts.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(store);
    }
    let mut editor = Editor::new(d.parts.clone());
    editor.apply("update-cross-references", &[RegisteredCommand::UpdateCrossReference { id: String::new(), locations }.into()]).map_err(|e| e.to_string())?;
    Ok((editor.parts() != &d.parts).then_some(editor))
}

pub(super) fn run(root: &Path, path: &str, profiles: &crate::Profiles) -> Result<Value, String> {
    let book = book::read(root, path)?;
    if book.documents.is_empty() { return Err("book has no documents".into()); }
    let docs = load(root, &book)?;
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::from(profiles.get()) });
    let mut edited = BTreeMap::new();
    let mut errors: BTreeMap<usize, String> = BTreeMap::new();
    for (i, d) in docs.iter().enumerate() {
        match updated(d, &fonts, &profiles) {
            Ok(Some(editor)) => { edited.insert(i, editor); }
            Ok(None) => {}
            Err(error) => { errors.insert(i, error); }
        }
    }
    let written = write_all(root, path, &docs, &edited)?;
    let rows: Vec<Value> = docs.iter().enumerate().map(|(i, d)| {
        json!({"id": d.entry.id, "path": d.entry.path, "changed": edited.contains_key(&i), "writable": d.writable, "written": written.contains(&d.entry.id), "error": errors.get(&i)})
    }).collect();
    Ok(json!({"documents": rows}))
}
