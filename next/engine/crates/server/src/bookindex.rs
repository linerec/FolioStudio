//! A book-wide index (Generate Index ▸ Include Book Documents, build-b3): the server reads the other documents of the book
//! that holds this one, as the book-wide TOC does (`toc::book_sources`), and gives the generation their printed entries
//! (each with its own page names, as Book ▸ Repaginate numbers them). The generation merges them (`folio_edit` indexes).
use folio_edit::{Command, commands::RegisteredCommand};
use folio_model::indexes::BookIndexDocument;

pub fn prepare(commands: &mut [Command], session: &crate::Session, editor: &folio_edit::Editor) -> Result<(), String> {
    for c in commands {
        match c {
            Command::Registered(RegisteredCommand::GenerateIndex { include_book_documents, book_documents, .. }) => {
                let include = include_book_documents.unwrap_or_else(|| editor.index_includes_book_documents());
                *book_documents = if include { Some(documents(session)?) } else { None };
            }
            // Index panel ▸ Import Topics: the other document's topics, read from its file (given ones are used as they are)
            Command::Registered(RegisteredCommand::ImportIndexTopics { document, topics }) if topics.is_empty() => *topics = topics_of(session, document)?,
            _ => {}
        }
    }
    Ok(())
}

/// The topics of another document of the project (its first index): names, sort orders and subtopics only.
fn topics_of(session: &crate::Session, document: &str) -> Result<Vec<folio_model::indexes::Topic>, String> {
    use std::sync::{Arc, Mutex, OnceLock};
    let folio = session.folio.as_deref().ok_or("주제는 같은 프로젝트의 다른 문서에서 가져옵니다")?;
    let root = folio.opened.project.root.clone();
    if document == folio.opened.rel.as_str() { return Err("지금 문서에서는 주제를 가져올 수 없습니다".into()); }
    let file = folio_project::paths::checked_project_path(&root, document)?;
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::new() });
    let other = crate::load_session(file.to_str().ok_or("invalid document path")?, false, &fonts, &profiles, &|_, _| {})
        .map_err(|e| format!("문서를 읽지 못했습니다: {document}: {e}"))?;
    fn bare(t: &folio_model::indexes::Topic) -> folio_model::indexes::Topic {
        folio_model::indexes::Topic { id: String::new(), name: t.name.clone(), sort_order: t.sort_order.clone(), topics: t.topics.iter().map(bare).collect(), references: Vec::new(), cross_references: Vec::new() }
    }
    let topics: Vec<_> = other.doc.indexes.first().map(|i| i.topics.iter().map(bare).collect()).unwrap_or_default();
    if topics.is_empty() { return Err(format!("가져올 주제가 없습니다: {document}")); }
    Ok(topics)
}

/// The other documents of the open document's project (project paths of `.folio` files; hidden folders are not looked in), for
/// Import Topics.
pub(crate) fn project_documents(session: &crate::Session) -> Vec<String> {
    let Some(folio) = session.folio.as_deref() else { return Vec::new() };
    let root = folio.opened.project.root.clone();
    let rel = folio.opened.rel.as_str().to_owned();
    fn walk(dir: &std::path::Path, root: &std::path::Path, depth: usize, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') { continue; }
            if path.is_dir() {
                if depth < 8 { walk(&path, root, depth + 1, out); }
            } else if name.to_ascii_lowercase().ends_with(".folio")
                && let Ok(r) = path.strip_prefix(root)
            {
                out.push(r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root, &root, 0, &mut out);
    out.retain(|p| *p != rel);
    out.sort();
    out
}

pub(crate) fn dialog_defaults(session: &crate::Session) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let edit = session.edit.as_ref().ok_or("no document open for editing")?.lock().map_err(|_| "edit state")?;
    Ok([("includeBookDocuments".into(), edit.editor.index_includes_book_documents().into())].into_iter().collect())
}

/// The book's documents in order: this one `own`, the others with their printed entries. Empty for a document in no book
/// (the index is the document's own).
fn documents(session: &crate::Session) -> Result<Vec<BookIndexDocument>, String> {
    use std::sync::{Arc, Mutex, OnceLock};
    let Some(folio) = session.folio.as_deref() else { return Ok(Vec::new()) };
    let root = folio.opened.project.root.clone();
    let rel = folio.opened.rel.as_str().to_owned();
    let Some((_, book)) = crate::toc::books_holding(&root, &rel).into_iter().next() else { return Ok(Vec::new()) };
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::new() });
    let mut out = Vec::new();
    for entry in &book.documents {
        if entry.path == rel {
            out.push(BookIndexDocument { document: entry.path.clone(), own: true, entries: Vec::new() });
            continue;
        }
        let file = folio_project::paths::checked_project_path(&root, &entry.path)?;
        let other = crate::load_session(file.to_str().ok_or("invalid document path")?, false, &fonts, &profiles, &|_, _| {})
            .map_err(|e| format!("책 문서를 읽지 못했습니다: {}: {e}", entry.path))?;
        let locations = folio_text::layout::toc::locations(&other.doc, &other.layout);
        let entries = folio_edit::book_index_entries(&other.doc, &locations)
            .map_err(|e| format!("책 문서의 색인을 읽지 못했습니다: {}: {}", entry.path, e.message))?;
        out.push(BookIndexDocument { document: entry.path.clone(), own: false, entries });
        if let Ok(store) = Arc::try_unwrap(other.fonts) {
            *fonts.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(store);
        }
    }
    Ok(out)
}
