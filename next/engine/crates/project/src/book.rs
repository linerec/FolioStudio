//! Portable book membership. Mutations only replace the book container.
use crate::{atomic, hash, paths::checked_project_path};
use folio_model::book::{Book, Entry, Options};
use std::path::{Path, PathBuf};

fn container(root: &Path, path: &str) -> Result<PathBuf, String> {
    if !path.ends_with(".foliobook") { return Err("use a .foliobook file".into()); }
    checked_project_path(root, path)
}
fn save(root: &Path, path: &str, book: &Book) -> Result<(), String> {
    atomic::write_bytes(&container(root, path)?, &serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn create(root: &Path, path: &str, name: &str) -> Result<Book, String> {
    if container(root, path)?.exists() { return Err("book already exists".into()); }
    if name.trim().is_empty() { return Err("book name required".into()); }
    let book = Book { format: "folio-book".into(), version: 1, name: name.into(), documents: vec![], options: Options::default(), style_source: None, sync: folio_model::book::default_sync(), merge_layers: true, extra: Default::default() };
    save(root, path, &book)?;
    Ok(book)
}
/// What File > New > Book... made: the project-relative path, the name (the file name, as InDesign names a book) and whether it replaced a file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Created { pub path: String, pub name: String, pub replaced: bool }
/// The answer of the New Book dialog: an empty book in `folder` (project-relative, `""` is the project folder) named `file_name` (`.foliobook` is
/// added when it is not typed). A file of that name needs `replace` (InDesign asks "already exists, replace it?"; Yes makes a new empty book over it),
/// otherwise the error is `exists: <path>`; `open` lists the project-relative paths that are open as books (the screen knows them): replacing one of them is
/// `open: <path>`. See `newfile` for the other error codes.
pub fn create_in(root: &Path, folder: &str, file_name: &str, replace: bool, open: &[String]) -> Result<Created, String> {
    let target = crate::newfile::resolve(root, folder, file_name, ".foliobook")?;
    if target.exists && !replace { return Err(format!("exists: {}", target.path)); }
    // a file that is open as a book is not saved over (InDesign: "Cannot save to <path> because it is already open"), after the replace question
    if target.exists && open.iter().any(|p| crate::paths::collision_key(p) == crate::paths::collision_key(&target.path)) { return Err(format!("open: {}", target.path)); }
    let book = Book { format: "folio-book".into(), version: 1, name: target.file_name.clone(), documents: vec![], options: Options::default(), style_source: None, sync: folio_model::book::default_sync(), merge_layers: true, extra: Default::default() };
    save(root, &target.path, &book)?;
    Ok(Created { path: target.path, name: target.file_name, replaced: target.exists })
}
pub fn read(root: &Path, path: &str) -> Result<Book, String> {
    let book: Book = serde_json::from_slice(&std::fs::read(container(root, path)?).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if book.format != "folio-book" || book.version != 1 { return Err("unsupported book format".into()); }
    for entry in &book.documents { crate::RelPath::parse(&entry.path).map_err(|e| e.to_string())?; }
    Ok(book)
}
fn document(root: &Path, path: &str) -> Result<(String, usize), String> {
    if !path.ends_with(".folio") && !path.ends_with(".idml") { return Err("use a .folio or .idml document".into()); }
    let native = checked_project_path(root, path)?;
    let sha = hash::sha256_file(&native).map_err(|e| e.to_string())?.0;
    let doc = folio_idml::read_package(&native, None).map_err(|e| e.to_string())?;
    Ok((sha, doc.spreads.iter().map(|s| s.pages.len()).sum()))
}
pub fn inspect(root: &Path, entry: &Entry) -> Result<usize, String> {
    let (sha, count) = document(root, &entry.path)?;
    if sha != entry.sha256 { return Err("document hash changed".into()); }
    Ok(count)
}
fn ensure_unique(root: &Path, book: &Book, path: &str, except: Option<&str>) -> Result<(), String> {
    let target = checked_project_path(root, path)?.canonicalize().map_err(|e| e.to_string())?;
    for entry in &book.documents {
        if Some(entry.id.as_str()) == except { continue; }
        if entry.path == path || checked_project_path(root, &entry.path).ok().and_then(|p| p.canonicalize().ok()).as_ref() == Some(&target) {
            return Err("document already in book".into());
        }
    }
    Ok(())
}
pub fn add(root: &Path, path: &str, document_path: &str) -> Result<Entry, String> {
    let mut book = read(root, path)?;
    ensure_unique(root, &book, document_path, None)?;
    let (sha256, _) = document(root, document_path)?;
    let entry = Entry { id: crate::util::short_id(), path: document_path.into(), sha256 };
    book.documents.push(entry.clone());save(root, path, &book)?;Ok(entry)
}
pub fn move_to(root: &Path, path: &str, id: &str, index: usize) -> Result<(), String> {
    let mut book = read(root, path)?;
    if index >= book.documents.len() { return Err("book position out of range".into()); }
    let old = book.documents.iter().position(|e| e.id == id).ok_or("book entry not found")?;
    let entry = book.documents.remove(old);book.documents.insert(index, entry);save(root, path, &book)
}
pub fn remove(root: &Path, path: &str, id: &str) -> Result<(), String> {
    let mut book = read(root, path)?;
    let index = book.documents.iter().position(|e| e.id == id).ok_or("book entry not found")?;
    book.documents.remove(index);save(root, path, &book)
}
pub fn relink(root: &Path, path: &str, id: &str, replacement: &str) -> Result<(), String> {
    let mut book = read(root, path)?;
    let entry = book.documents.iter_mut().find(|e| e.id == id).ok_or("book entry not found")?;
    let mut candidate = entry.clone();candidate.path = replacement.into();inspect(root, &candidate)?;
    ensure_unique(root, &book, replacement, Some(id))?;
    book.documents.iter_mut().find(|e| e.id == id).expect("the entry was found above and the book has not changed").path = replacement.into();save(root, path, &book)
}

/// Book Page Numbering Options. Only the book container is replaced.
pub fn set_options(root: &Path, path: &str, options: Options) -> Result<(), String> {
    if !["nextPage", "nextOdd", "nextEven"].contains(&options.repagination.as_str()) {
        return Err("repagination must be nextPage, nextOdd or nextEven".into());
    }
    let mut book = read(root, path)?;
    book.options = options;
    save(root, path, &book)
}
/// Merge Identically Named Layers on Export. Only the book container is replaced.
pub fn set_merge_layers(root: &Path, path: &str, merge: bool) -> Result<(), String> {
    let mut book = read(root, path)?;
    book.merge_layers = merge;
    save(root, path, &book)
}
/// The document whose styles Synchronize Book carries to the others (none: the first document).
pub fn set_style_source(root: &Path, path: &str, entry: Option<&str>) -> Result<(), String> {
    let mut book = read(root, path)?;
    if let Some(id) = entry
        && !book.documents.iter().any(|e| e.id == id)
    {
        return Err("book entry not found".into());
    }
    book.style_source = entry.map(str::to_owned);
    save(root, path, &book)
}
/// The style kinds Synchronize Book carries (`paragraph`, `character`, `object`, `cell`, `table`).
pub fn set_sync(root: &Path, path: &str, kinds: &[String]) -> Result<(), String> {
    let allowed = folio_model::book::sync_kinds();
    if let Some(bad) = kinds.iter().find(|k| !allowed.contains(k)) {
        return Err(format!("unknown style kind {bad}"));
    }
    let mut book = read(root, path)?;
    book.sync = allowed.into_iter().filter(|k| kinds.contains(k)).collect();
    save(root, path, &book)
}
/// New pins after the book rewrote some documents (the numbering or the styles). One write of the container.
pub fn update_hashes(root: &Path, path: &str, hashes: &[(String, String)]) -> Result<(), String> {
    let mut book = read(root, path)?;
    for (id, sha) in hashes {
        book.documents.iter_mut().find(|e| &e.id == id).ok_or("book entry not found")?.sha256 = sha.clone();
    }
    save(root, path, &book)
}
/// The entry that Synchronize Book follows.
pub fn style_source(book: &Book) -> Option<&Entry> {
    book.style_source.as_ref().and_then(|id| book.documents.iter().find(|e| &e.id == id)).or_else(|| book.documents.first())
}
