use std::{path::Path, sync::{Arc, Mutex, OnceLock}};
use serde_json::{Value, json};
use folio_project::{book, paths::checked_project_path};

pub(super) fn run(root: &Path, path: &str, request: &Value, profiles: &crate::Profiles) -> Result<Value, String> {
    let book = book::read(root, path)?;
    if book.documents.is_empty() { return Err("book has no documents".into()); }
    let output = super::field(request, "output")?;
    if !Path::new(output).extension().is_some_and(|s| s.eq_ignore_ascii_case("pdf")) { return Err("PDF output required".into()); }
    let destination = checked_project_path(root, output)?;
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::from(profiles.get()) });
    let mut documents = Vec::new();
    let mut reports = Vec::new();
    for entry in &book.documents {
        book::inspect(root, entry)?;
        let source = checked_project_path(root, &entry.path)?;
        let session = crate::load_session(source.to_str().ok_or("invalid document path")?, false, &fonts, &profiles, &|_, _| {})?;
        let (exported, pages, ms) = crate::render_pdf(&session, request, &|_| {})?;
        reports.push(json!({"id":entry.id,"pages":pages,"report":crate::print::export_view(&exported.report, Path::new(output), pages, ms)}));
        documents.push(exported.pdf);
        if let Ok(store) = Arc::try_unwrap(session.fonts) { *fonts.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(store); }
    }
    // Acrobat layers of the documents: one group per name with Merge Identically Named Layers on Export (the book's option), each
    // document's own otherwise; the lists are labelled with the book's and the documents' file names
    let file_name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_owned();
    let names: Vec<String> = book.documents.iter().map(|e| file_name(&e.path)).collect();
    let book_name = file_name(path);
    let layers = folio_pdf::book::BookLayers { book_name: &book_name, document_names: &names, merge: book.merge_layers };
    let (bytes, pages) = folio_pdf::book::join_layers(&documents, Some(&layers))?;
    crate::print::write_atomically(&destination, &bytes)?;
    Ok(json!({"path":output,"pages":pages,"bytes":bytes.len(),"documents":reports}))
}
