//! Book actions: the membership (the `.foliobook` container only), the page numbering of the documents (Repaginate) and
//! Synchronize Book. The numbering and the styles are the editor's own commands run on each document, so the rules are the
//! ones the documents already follow; this file orders them and writes the result.
//!
//! InDesign rewrites the documents of a book when it numbers them or synchronizes them (measured: the hash of a closed
//! document changes), so Folio does too, with three differences: the documents are `.folio` files of the project, every
//! document is written atomically and all of them are put back if one cannot be written, and the book's pins are renewed in
//! the same step. A `.idml` document of the book is numbered in the plan but never written.
use folio_edit::commands::RegisteredCommand;
use folio_edit::{Command, Editor};
use folio_model::book::{Book, Entry, Options};
use folio_project::book;
use folio_project::book_pagination::{self, DocPlan, Facts};
use folio_project::format::{self, PartAction};
use folio_project::{atomic, hash, lock, paths::checked_project_path};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
mod check;
mod export;
mod print_package;
mod xref;

fn field<'a>(r: &'a Value, key: &str) -> Result<&'a str, String> {
    r[key].as_str().ok_or_else(|| format!("{key} required"))
}

/// A document of the book as it is on disk.
struct Doc {
    entry: Entry,
    file: PathBuf,
    parts: BTreeMap<String, String>,
    writable: bool,
}

fn load(root: &Path, book: &Book) -> Result<Vec<Doc>, String> {
    let mut out = Vec::new();
    for entry in &book.documents {
        book::inspect(root, entry).map_err(|e| format!("{}: {e}", entry.path))?;
        let file = checked_project_path(root, &entry.path)?;
        let parts = crate::edit::package_parts(&file)?;
        out.push(Doc { entry: entry.clone(), writable: entry.path.ends_with(".folio"), file, parts });
    }
    Ok(out)
}

/// Every document that changed is written, or none: the originals are put back when one write fails.
fn write_all(root: &Path, book_path: &str, docs: &[Doc], edited: &BTreeMap<usize, Editor>) -> Result<Vec<String>, String> {
    let mut changed: Vec<(usize, BTreeMap<String, Option<String>>)> = Vec::new();
    for (&i, editor) in edited {
        if !docs[i].writable {
            continue;
        }
        let touched: BTreeMap<String, Option<String>> = editor.touched().iter().map(|p| (p.clone(), editor.part(p).map(str::to_owned))).collect();
        if !touched.is_empty() {
            changed.push((i, touched));
        }
    }
    // a document someone has open is not written under them
    for (i, _) in &changed {
        let (m, _) = format::read_manifest(&docs[*i].file).map_err(|e| e.to_string())?;
        if lock::holder(root, &m.id).is_some_and(|h| !lock::is_stale(&h)) {
            return Err(format!("document_open: {}", docs[*i].entry.path));
        }
    }
    let mut originals: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut hashes = Vec::new();
    let restore = |originals: &[(PathBuf, Vec<u8>)]| {
        for (file, bytes) in originals.iter().rev() {
            let _ = atomic::write_bytes(file, bytes);
        }
    };
    for (i, touched) in &changed {
        let doc = &docs[*i];
        let result = (|| -> Result<String, String> {
            originals.push((doc.file.clone(), std::fs::read(&doc.file).map_err(|e| e.to_string())?));
            let (mut manifest, _) = format::read_manifest(&doc.file).map_err(|e| e.to_string())?;
            manifest.saved = folio_project::util::now_rfc3339();
            let report = read_part(&doc.file, format::REPORT_PART);
            let added: Vec<(&str, &str)> = touched.iter().filter(|(n, t)| t.is_some() && !doc.parts.contains_key(*n)).map(|(n, t)| (n.as_str(), t.as_deref().unwrap_or(""))).collect();
            let mut action = |name: &str, _: &mut dyn FnMut() -> std::io::Result<String>| -> std::io::Result<PartAction> {
                Ok(match touched.get(name) {
                    Some(Some(text)) => PartAction::Replace(text.clone()),
                    Some(None) => PartAction::Drop,
                    None => PartAction::Keep,
                })
            };
            // the saved lines are the composed text of the old numbers: they are made again when the document opens
            format::write_folio_with_parts(&doc.file, &doc.file, &mut action, &manifest, None, report.as_deref(), &added).map_err(|e| e.to_string())?;
            Ok(hash::sha256_file(&doc.file).map_err(|e| e.to_string())?.0)
        })();
        match result {
            Ok(sha) => hashes.push((doc.entry.id.clone(), sha)),
            Err(e) => {
                restore(&originals);
                return Err(format!("{}: {e}", doc.entry.path));
            }
        }
    }
    if let Err(e) = book::update_hashes(root, book_path, &hashes) {
        restore(&originals);
        return Err(e);
    }
    Ok(changed.iter().map(|(i, _)| docs[*i].entry.id.clone()).collect())
}

fn read_part(file: &Path, name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(file).ok()?)).ok()?;
    let mut e = z.by_name(name).ok()?;
    let mut out = Vec::new();
    e.read_to_end(&mut out).ok()?;
    Some(out)
}

/// Numbers the documents in order. `apply`: the documents are written (the plan is returned either way).
fn repaginate(root: &Path, path: &str, apply: bool) -> Result<Value, String> {
    let book = book::read(root, path)?;
    let docs = load(root, &book)?;
    let mut edited: BTreeMap<usize, Editor> = BTreeMap::new();
    let plans: Vec<DocPlan> = book_pagination::chain(&book.options, docs.len(), |i, start, blanks| {
        let mut editor = Editor::new(docs[i].parts.clone());
        let mut commands = Vec::new();
        if blanks > 0 {
            commands.push(Command::InsertPages { count: blanks as usize, before: None, master: None });
        }
        commands.push(RegisteredCommand::SetBookPageStart { start }.into());
        editor.apply("book", &commands).map_err(|e| format!("{}: {e}", docs[i].entry.path))?;
        let n = editor.numbering().map_err(|e| format!("{}: {e}", docs[i].entry.path))?;
        let (first, last) = n.pages.first().zip(n.pages.last()).ok_or_else(|| format!("{}: no pages", docs[i].entry.path))?;
        let facts = Facts { first_name: first.name.clone(), last_name: last.name.clone(), first_number: first.number, last_number: last.number, pages: n.pages.len(), own_start: !n.first_continues };
        edited.insert(i, editor);
        Ok::<Facts, String>(facts)
    })?;
    let mut previous = None;
    let mut chapters = Vec::new();
    for i in 0..docs.len() {
        let editor = edited.get_mut(&i).ok_or("missing book document")?;
        let number = editor.update_book_chapter(previous)?;
        previous = Some(number);
        chapters.push(number);
    }
    // a document that comes out as it went in is not rewritten
    edited.retain(|&i, editor| editor.parts() != &docs[i].parts);
    let written = if apply { write_all(root, path, &docs, &edited)? } else { Vec::new() };
    let rows: Vec<Value> = plans
        .iter()
        .zip(&docs)
        .enumerate()
        .map(|(i, (p, d))| json!({"id": d.entry.id, "range": p.range, "start": p.start, "end": p.end, "pages": p.pages, "chapterNumber": chapters[i], "blanks": p.blanks, "ownStart": p.own_start, "writable": d.writable, "written": written.contains(&d.entry.id)}))
        .collect();
    Ok(json!({"applied": apply, "documents": rows}))
}

/// Synchronize Book: the styles of the style source go into the other documents (a style of the same name is replaced by the
/// source's, a style the source has is added, a style only the document has stays: oracle probe `bookx-c1-sync`, g1-g3).
fn synchronize(root: &Path, path: &str) -> Result<Value, String> {
    let book = book::read(root, path)?;
    let docs = load(root, &book)?;
    let source = book::style_source(&book).ok_or("book has no documents")?.id.clone();
    let source_parts = docs.iter().find(|d| d.entry.id == source).ok_or("style source not found")?.parts.clone();
    let mut edited = BTreeMap::new();
    for (i, d) in docs.iter().enumerate() {
        if d.entry.id == source || book.sync.is_empty() {
            continue;
        }
        let mut editor = Editor::new(d.parts.clone());
        let (styles, resources): (Vec<_>, Vec<_>) = book.sync.iter().cloned().partition(|k| ["paragraph", "character", "object", "cell", "table"].contains(&k.as_str()));
        let mut commands = Vec::new();
        if !resources.is_empty() { commands.push(RegisteredCommand::SyncBookResources { kinds: resources, parts: source_parts.clone() }.into()); }
        if !styles.is_empty() { commands.push(RegisteredCommand::LoadStyles { kinds: styles, parts: source_parts.clone(), clash: "overwrite".into(), only: vec![] }.into()); }
        editor.apply("synchronize", &commands).map_err(|e| format!("{}: {e}", d.entry.path))?;
        if editor.parts() != &d.parts {
            edited.insert(i, editor);
        }
    }
    let written = write_all(root, path, &docs, &edited)?;
    let rows: Vec<Value> = docs
        .iter()
        .enumerate()
        .map(|(i, d)| json!({"id": d.entry.id, "source": d.entry.id == source, "changed": edited.contains_key(&i), "writable": d.writable, "written": written.contains(&d.entry.id)}))
        .collect();
    // what InDesign carries that Folio does not yet: said every time (the reports name what differs). Parent pages are carried
    // only when switched on (InDesign's default is off too).
    let mut not_synchronized: Vec<&str> = Vec::new();
    if !book.sync.iter().any(|k| k == "masterPage") { not_synchronized.insert(0, "masterPage"); }
    Ok(json!({"documents": rows, "kinds": book.sync, "notSynchronized": not_synchronized}))
}

fn options_from(r: &Value, current: &Options) -> Options {
    Options {
        repagination: r["repagination"].as_str().map(str::to_owned).unwrap_or_else(|| current.repagination.clone()),
        insert_blank_page: r["insertBlankPage"].as_bool().unwrap_or(current.insert_blank_page),
        automatic: r["automatic"].as_bool().unwrap_or(current.automatic),
    }
}

pub fn run(r: &Value, _: Option<&crate::Session>, profiles: &crate::Profiles) -> Result<Value, String> {
    let root = Path::new(field(r, "root")?);
    // `new`: the answer of File > New > Book... (folder, fileName, replace): the book is made and then read like any other
    let made = if field(r, "action")? == "new" { Some(book::create_in(root, r["folder"].as_str().unwrap_or(""), field(r, "fileName")?, r["replace"].as_bool().unwrap_or(false), &r["open"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default())?) } else { None };
    let path = match &made { Some(m) => m.path.as_str(), None => field(r, "path")? };
    // Save Book As… (build-c): the book file copied to `to` (project-relative `.foliobook`; its entries are project-relative, so they stay
    // right wherever in the project it goes); the book open in the panel is then the new one (`path` in the reply)
    let saved_as = if field(r, "action")? == "saveAs" {
        let to = field(r, "to")?;
        if !to.to_ascii_lowercase().ends_with(".foliobook") { return Err("a book is saved as a .foliobook".into()); }
        // a folder of the new path that is not there yet is made first (the name is checked before anything is made)
        let rel = folio_project::paths::RelPath::parse(to).map_err(|e| e.to_string())?;
        if let Some(parent) = rel.parent() { std::fs::create_dir_all(parent.to_native(root)).map_err(|e| e.to_string())?; }
        let target = folio_project::paths::checked_project_path(root, to)?;
        if target.exists() && r["replace"] != true { return Err(format!("{to} exists")); }
        let source = folio_project::paths::checked_project_path(root, path)?;
        let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
        folio_project::atomic::write_bytes(&target, &bytes).map_err(|e| e.to_string())?;
        Some(to.to_owned())
    } else { None };
    let path = saved_as.as_deref().unwrap_or(path);
    let mut created = None;
    let mut pagination = None;
    let mut sync = None;
    let mut exported = None;
    let mut checked = None;
    let mut cross_references = None;
    let mut packaged = None;
    let mut renumber = false;
    match field(r, "action")? {
        "create" => {
            book::create(root, path, field(r, "name")?)?;
        }
        "read" | "new" | "saveAs" => {}
        "exportPdf" => exported = Some(export::run(root, path, r, profiles)?),
        // Preflight Book…: the preflight panel's report of each document (the documents are only read)
        "preflight" => checked = Some(check::run(root, path, profiles)?),
        // Update All Cross-References: each document of the book updates its cross-references and is written (all or none)
        "updateCrossReferences" => cross_references = Some(xref::run(root, path, profiles)?),
        // Package Book For Print…: each document is packaged (File ▸ Package) into a folder of its own under `dir`
        "package" => packaged = Some(print_package::run(root, path, r, profiles)?),
        "add" => {
            created = Some(book::add(root, path, field(r, "document")?)?.id);
            renumber = true;
        }
        "move" => {
            book::move_to(root, path, field(r, "entry")?, r["index"].as_u64().ok_or("index required")? as usize)?;
            renumber = true;
        }
        "remove" => {
            book::remove(root, path, field(r, "entry")?)?;
            renumber = true;
        }
        "relink" => book::relink(root, path, field(r, "entry")?, field(r, "replacement")?)?,
        "options" => {
            let current = book::read(root, path)?.options;
            book::set_options(root, path, options_from(r, &current))?;
            renumber = true;
        }
        // Merge Identically Named Layers on Export (build-c2): the book's option, kept in the book file
        "mergeLayers" => book::set_merge_layers(root, path, r["merge"].as_bool().ok_or("merge required")?)?,
        "styleSource" => book::set_style_source(root, path, r["entry"].as_str().filter(|s| !s.is_empty()))?,
        "syncKinds" => {
            let kinds: Vec<String> = serde_json::from_value(r["kinds"].clone()).map_err(|e| e.to_string())?;
            book::set_sync(root, path, &kinds)?;
        }
        "repaginate" => pagination = Some(repaginate(root, path, r["apply"].as_bool().unwrap_or(true))?),
        "planPagination" => pagination = Some(repaginate(root, path, false)?),
        "chapterNumber" => {
            let current = book::read(root, path)?;
            let docs = load(root, &current)?;
            let entry = field(r, "entry")?;
            let i = docs.iter().position(|d| d.entry.id == entry).ok_or("entry not found")?;
            if !docs[i].writable { return Err("chapter settings require a .folio document".into()); }
            let number = u32::try_from(r["number"].as_u64().ok_or("number required")?).map_err(|e| e.to_string())?;
            let mut editor = Editor::new(docs[i].parts.clone());
            editor.apply("chapter", &[RegisteredCommand::SetChapterNumber { number, source: field(r, "source")?.into() }.into()]).map_err(|e| e.to_string())?;
            write_all(root, path, &docs, &BTreeMap::from([(i, editor)]))?;
            renumber = true;
        }
        "synchronize" => sync = Some(synchronize(root, path)?),
        _ => return Err("unknown book action".into()),
    }
    // Automatic Pagination: the numbers follow the change at once (a failure is reported, the membership change stays)
    if renumber && pagination.is_none() {
        let book = book::read(root, path)?;
        if book.options.automatic && !book.documents.is_empty() {
            pagination = Some(match repaginate(root, path, true) {
                Ok(v) => v,
                Err(error) => json!({"applied": false, "error": error}),
            });
        }
    }
    let book = book::read(root, path)?;
    let status: Vec<_> = book
        .documents
        .iter()
        .map(|entry| match book::inspect(root, entry) {
            Ok(pages) => json!({"id":entry.id,"ok":true,"pages":pages}),
            Err(error) => json!({"id":entry.id,"ok":false,"error":error}),
        })
        .collect();
    Ok(json!({"book":book,"status":status,"created":created,"file":made,"path":path,"pagination":pagination,"sync":sync,"exported":exported,"preflight":checked,"crossReferences":cross_references,"bookPackage":packaged,
        "format": {"kind": "folio-book", "indb": false, "note": "Folio format: the book is a .foliobook of project-relative paths and SHA-256; InDesign .indb is not read or written"}}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_project::format::{DocManifest, FORMAT, VERSION, write_folio};
    use folio_project::manifest;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(name)
    }
    fn make_folio(root: &Path, project: &str, name: &str, idml: &str, id: &str) {
        let m = DocManifest {
            format: FORMAT.into(), version: VERSION, id: id.into(), project: project.into(), created: String::new(), saved: String::new(), engine: String::new(),
            source: None, link_uris: "folio-ref".into(), references: vec![], graphics: vec![], pins: vec![], saved_lines: None, import_report: None, extra: Default::default(),
        };
        write_folio(&root.join(name), &fixture(idml), &mut |_, _| Ok(PartAction::Keep), &m, None, None).unwrap();
    }
    /// A document as File > New makes it (`documentNew`): `pages` pages, the first one numbered `start`.
    fn make_new_document(root: &Path, project: &str, name: &str, id: &str, pages: u32, start: u32) {
        let font = folio_project::newdoc::FontChoice { family: "Arial".into(), style: "Regular".into(), postscript: "ArialMT".into(), font_type: "TrueType".into() };
        let mut doc = folio_project::newdoc::NewDocument::letter(font);
        doc.pages = pages;
        doc.start = start;
        let idml = root.parent().unwrap().join(format!("{id}.idml"));
        std::fs::write(&idml, doc.package().unwrap()).unwrap();
        let m = DocManifest {
            format: FORMAT.into(), version: VERSION, id: id.into(), project: project.into(), created: String::new(), saved: String::new(), engine: String::new(),
            source: None, link_uris: "folio-ref".into(), references: vec![], graphics: vec![], pins: vec![], saved_lines: None, import_report: None, extra: Default::default(),
        };
        write_folio(&root.join(name), &idml, &mut |_, _| Ok(PartAction::Keep), &m, None, None).unwrap();
    }
    fn names(root: &Path, doc: &str) -> Vec<String> {
        let parts = crate::edit::package_parts(&root.join(doc)).unwrap();
        Editor::new(parts).numbering().unwrap().pages.into_iter().map(|p| p.name).collect()
    }
    fn temp(tag: &str) -> PathBuf {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/bookx-c1-20261023/tests").join(folio_project::util::short_id()).join(tag);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// Adding documents numbers them at once (Automatic Pagination), the pins follow the rewritten files, and the whole project
    /// can be moved afterwards: the numbers and the pins hold (principle 1).
    #[test]
    fn adding_numbers_documents_rewrites_them_and_survives_a_move() {
        let root = temp("book");
        let project = manifest::create(&root, "Book").unwrap();
        for (i, n) in ["a.folio", "b.folio", "c.folio"].iter().enumerate() {
            make_folio(&root, &project.file.id, n, "pages.idml", &format!("doc{i}"));
        }
        book::create(&root, "book.foliobook", "Book").unwrap();
        for n in ["a.folio", "b.folio", "c.folio"] {
            book::add(&root, "book.foliobook", n).unwrap();
        }
        let after = repaginate(&root, "book.foliobook", false).unwrap();
        assert_eq!(after["documents"][2]["range"], "11-15");
        assert_eq!(names(&root, "a.folio"), ["1", "2", "3", "4", "5"]);
        assert_eq!(repaginate(&root, "book.foliobook", true).unwrap()["documents"][1]["range"], "6-10");
        assert_eq!(names(&root, "b.folio"), ["6", "7", "8", "9", "10"]);
        assert_eq!(names(&root, "c.folio"), ["11", "12", "13", "14", "15"]);
        // the pins were renewed in the same step: every document still matches its entry
        let read = book::read(&root, "book.foliobook").unwrap();
        for e in &read.documents {
            book::inspect(&root, e).unwrap();
        }
        // order change renumbers
        book::move_to(&root, "book.foliobook", &read.documents[2].id, 0).unwrap();
        repaginate(&root, "book.foliobook", true).unwrap();
        assert_eq!(names(&root, "c.folio"), ["1", "2", "3", "4", "5"]);
        assert_eq!(names(&root, "a.folio"), ["6", "7", "8", "9", "10"]);
        // next odd page with blank pages: the document is five pages (1-5), the next starts on 6 -> a blank page makes 1-6, next 7
        book::set_options(&root, "book.foliobook", Options { repagination: "nextOdd".into(), insert_blank_page: true, automatic: true }).unwrap();
        let plan = repaginate(&root, "book.foliobook", true).unwrap();
        assert_eq!(plan["documents"][0]["range"], "1-6");
        assert_eq!(plan["documents"][0]["blanks"], 1);
        assert_eq!(names(&root, "c.folio").len(), 6);
        assert_eq!(plan["documents"][1]["range"], "7-12");
        // running it again changes nothing (nothing is written)
        let again = repaginate(&root, "book.foliobook", true).unwrap();
        assert_eq!(again["documents"][0]["written"], false);
        // the folder is moved whole: numbers and pins hold with no relinking
        let moved = root.parent().unwrap().join("moved elsewhere");
        std::fs::rename(&root, &moved).unwrap();
        let read = book::read(&moved, "book.foliobook").unwrap();
        for e in &read.documents {
            book::inspect(&moved, e).unwrap();
        }
        assert_eq!(names(&moved, "a.folio")[0], "7");
    }

    /// A document File > New made has no page descriptors (InDesign writes them when it saves it): the book numbers it from the
    /// sections all the same. It used to stop with `page 1: no number` (principles check F1, 2026-11-18).
    #[test]
    fn a_document_file_new_made_is_numbered_by_the_book() {
        let root = temp("new-documents");
        let project = manifest::create(&root, "Book").unwrap();
        make_new_document(&root, &project.file.id, "a.folio", "doc0", 3, 1);
        make_new_document(&root, &project.file.id, "b.folio", "doc1", 4, 1);
        // a document that starts at a number of its own keeps it (InDesign case e1)
        make_new_document(&root, &project.file.id, "c.folio", "doc2", 2, 9);
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        for n in ["a.folio", "b.folio", "c.folio"] {
            book::add(&root, "book.foliobook", n).unwrap();
        }
        assert!(!crate::edit::package_parts(&root.join("a.folio")).unwrap().values().any(|t| t.contains("<Descriptor")));
        let plan = repaginate(&root, "book.foliobook", false).unwrap();
        let ranges: Vec<&str> = plan["documents"].as_array().unwrap().iter().map(|d| d["range"].as_str().unwrap()).collect();
        assert_eq!(ranges, ["1-3", "4-7", "9-10"]);
        repaginate(&root, "book.foliobook", true).unwrap();
        assert_eq!(names(&root, "a.folio"), ["1", "2", "3"]);
        assert_eq!(names(&root, "b.folio"), ["4", "5", "6", "7"]);
        assert_eq!(names(&root, "c.folio"), ["9", "10"]);
        // the numbers are the same when the book is moved to the front: b is numbered from 1, a follows it
        let read = book::read(&root, "book.foliobook").unwrap();
        book::move_to(&root, "book.foliobook", &read.documents[1].id, 0).unwrap();
        repaginate(&root, "book.foliobook", true).unwrap();
        assert_eq!(names(&root, "b.folio"), ["1", "2", "3", "4"]);
        assert_eq!(names(&root, "a.folio"), ["5", "6", "7"]);
    }

    /// A document File > New made is the target of a Synchronize whose style source has a spot colour: the colour comes with its Ink, the
    /// fifth ink after the four process inks every new document has. It used to stop with "Graphic.xml has no Ink" (mcp-c6, 2026-11-20),
    /// for the person's Synchronize too. The source is never written, a second run changes nothing, and the book can have the
    /// new document first (it is the style source then, and doc.folio is the target).
    #[test]
    fn a_document_file_new_made_takes_the_spot_colours_of_the_style_source() {
        let root = temp("sync-new-document");
        let project = manifest::create(&root, "Book").unwrap();
        make_folio(&root, &project.file.id, "source.folio", "basic.idml", "src");
        make_new_document(&root, &project.file.id, "new.folio", "doc1", 2, 1);
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        book::add(&root, "book.foliobook", "source.folio").unwrap();
        book::add(&root, "book.foliobook", "new.folio").unwrap();
        let graphic = |doc: &str| crate::edit::package_parts(&root.join(doc)).unwrap().remove("Resources/Graphic.xml").unwrap();
        let ink_lines = |t: &str| t.lines().filter(|l| l.trim_start().starts_with("<Ink ")).map(|l| l.trim().to_owned()).collect::<Vec<_>>();
        assert_eq!(ink_lines(&graphic("new.folio")).len(), 4, "a new document has the four process inks");
        let source_before = std::fs::read(root.join("source.folio")).unwrap();
        let result = synchronize(&root, "book.foliobook").unwrap();
        assert_eq!(result["documents"][1]["written"], true, "{result}");
        let t = graphic("new.folio");
        assert!(t.contains("Self=\"Color/Oracle Spot\""), "the spot colour came");
        // its Ink is the source's, after the four process inks (the source has the same four, so the spot ink has the same TrapOrder 5)
        assert_eq!(ink_lines(&t), ink_lines(&graphic("source.folio")));
        assert_eq!(std::fs::read(root.join("source.folio")).unwrap(), source_before, "the source is never written");
        for e in &book::read(&root, "book.foliobook").unwrap().documents {
            book::inspect(&root, e).unwrap();
        }
        synchronize(&root, "book.foliobook").unwrap();
        assert_eq!(graphic("new.folio"), t, "a second synchronization changes nothing");
        // the other way round: the new document is the style source (it has no spot colour to carry) and basic is the target
        let order = book::read(&root, "book.foliobook").unwrap();
        book::move_to(&root, "book.foliobook", &order.documents[1].id, 0).unwrap();
        let result = synchronize(&root, "book.foliobook").unwrap();
        assert_eq!(result["documents"][0]["source"], true, "{result}");
    }

    /// Preflight Book runs the preflight on every document of the book and reports each one; nothing is written.
    #[test]
    fn preflight_checks_every_document_of_the_book_and_writes_nothing() {
        let root = temp("preflight");
        let project = manifest::create(&root, "Book").unwrap();
        for (i, n) in ["a.folio", "b.folio"].iter().enumerate() {
            make_folio(&root, &project.file.id, n, "pages.idml", &format!("doc{i}"));
        }
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        for n in ["a.folio", "b.folio"] {
            book::add(&root, "book.foliobook", n).unwrap();
        }
        let before = (std::fs::read(root.join("a.folio")).unwrap(), std::fs::read(root.join("b.folio")).unwrap());
        let profiles = crate::Profiles::start();
        let out = run(&json!({"root": root.to_str().unwrap(), "path": "book.foliobook", "action": "preflight"}), None, &profiles).unwrap();
        let rows = out["preflight"]["documents"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (row, name) in rows.iter().zip(["a.folio", "b.folio"]) {
            assert_eq!(row["path"], name);
            assert_eq!(row["ok"], true, "{row}");
            assert!(row["report"]["summary"]["errors"].is_u64() && row["report"]["findings"].is_array());
        }
        assert_eq!(before, (std::fs::read(root.join("a.folio")).unwrap(), std::fs::read(root.join("b.folio")).unwrap()));
        // a document that is gone is a row with its error; the others are still checked
        std::fs::remove_file(root.join("a.folio")).unwrap();
        let out = run(&json!({"root": root.to_str().unwrap(), "path": "book.foliobook", "action": "preflight"}), None, &profiles).unwrap();
        let rows = out["preflight"]["documents"].as_array().unwrap();
        assert_eq!(rows[0]["ok"], false);
        assert_eq!(rows[1]["ok"], true);
    }

    /// Update All Cross-References brings a stale cross-reference of each document back to what its destination says, writes only
    /// the documents that changed, and a second run changes nothing.
    #[test]
    fn update_cross_references_refreshes_every_document_of_the_book() {
        use std::io::{Read, Write};
        let root = temp("xref");
        let project = manifest::create(&root, "Book").unwrap();
        // xref.idml with its cross-reference text made stale (the destination's text is "Target heading")
        let stale = root.parent().unwrap().join("stale.idml");
        {
            let mut zin = zip::ZipArchive::new(std::fs::File::open(fixture("xref.idml")).unwrap()).unwrap();
            let mut zout = zip::ZipWriter::new(std::fs::File::create(&stale).unwrap());
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            let mut made = false;
            for i in 0..zin.len() {
                let mut e = zin.by_index(i).unwrap();
                if e.is_dir() { continue; }
                let name = e.name().unwrap().into_owned();
                let mut bytes = Vec::new();
                e.read_to_end(&mut bytes).unwrap();
                if name.starts_with("Stories/") {
                    let text = String::from_utf8(bytes.clone()).unwrap();
                    let at = text.find("<CrossReferenceSource");
                    if let Some(at) = at {
                        let content = text[at..].find("<Content>Target heading</Content>").expect("the cross-reference source text") + at;
                        bytes = format!("{}<Content>Stale text</Content>{}", &text[..content], &text[content + "<Content>Target heading</Content>".len()..]).into_bytes();
                        made = true;
                    }
                }
                zout.start_file(name, options).unwrap();
                zout.write_all(&bytes).unwrap();
            }
            zout.finish().unwrap();
            assert!(made);
        }
        let manifest_of = |id: &str| DocManifest {
            format: FORMAT.into(), version: VERSION, id: id.into(), project: project.file.id.clone(), created: String::new(), saved: String::new(), engine: String::new(),
            source: None, link_uris: "folio-ref".into(), references: vec![], graphics: vec![], pins: vec![], saved_lines: None, import_report: None, extra: Default::default(),
        };
        write_folio(&root.join("a.folio"), &stale, &mut |_, _| Ok(PartAction::Keep), &manifest_of("doc0"), None, None).unwrap();
        make_folio(&root, &project.file.id, "b.folio", "xref.idml", "doc1");
        let story = |doc: &str| crate::edit::package_parts(&root.join(doc)).unwrap().into_iter().filter(|(p, _)| p.starts_with("Stories/")).map(|(_, t)| t).collect::<String>();
        assert!(story("a.folio").contains("Stale text"));
        assert!(story("b.folio").contains("Target heading"));
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        for n in ["a.folio", "b.folio"] {
            book::add(&root, "book.foliobook", n).unwrap();
        }
        let profiles = crate::Profiles::start();
        let request = json!({"root": root.to_str().unwrap(), "path": "book.foliobook", "action": "updateCrossReferences"});
        let out = run(&request, None, &profiles).unwrap();
        let rows = out["crossReferences"]["documents"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r["error"].is_null()), "{out}");
        assert_eq!((rows[0]["changed"].clone(), rows[0]["written"].clone()), (json!(true), json!(true)));
        assert_eq!((rows[1]["changed"].clone(), rows[1]["written"].clone()), (json!(false), json!(false)));
        assert!(story("a.folio").contains("Target heading") && !story("a.folio").contains("Stale text"));
        // nothing is left to update
        let again = run(&request, None, &profiles).unwrap();
        assert!(again["crossReferences"]["documents"].as_array().unwrap().iter().all(|r| r["written"] == json!(false)));
    }

    /// Package Book For Print makes the package of File ▸ Package for every document, each in a folder of its own under the folder given;
    /// a folder that exists is not written into.
    #[test]
    fn package_makes_a_package_folder_for_every_document_of_the_book() {
        // (one package at a time in this process: File ▸ Package's dialog test makes one too)
        let _slot = crate::package::TEST_SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp("package");
        let project = manifest::create(&root, "Book").unwrap();
        for (i, n) in ["a.folio", "b.folio"].iter().enumerate() {
            make_folio(&root, &project.file.id, n, "pages.idml", &format!("doc{i}"));
        }
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        for n in ["a.folio", "b.folio"] {
            book::add(&root, "book.foliobook", n).unwrap();
        }
        let printer = root.parent().unwrap().join("printer");
        std::fs::create_dir_all(&printer).unwrap();
        let profiles = crate::Profiles::start();
        let request = json!({"root": root.to_str().unwrap(), "path": "book.foliobook", "action": "package", "dir": printer.to_str().unwrap(), "fonts": false, "links": true, "pdf": false});
        let out = run(&request, None, &profiles).unwrap();
        let rows = out["bookPackage"]["documents"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(row["ok"], json!(true), "{row}");
            assert!(Path::new(row["folder"].as_str().unwrap()).is_dir());
            assert!(row["result"]["files"].as_u64().unwrap() >= 1);
        }
        assert!(printer.join("01 a").is_dir() && printer.join("02 b").is_dir());
        // the same folder again: every package folder exists, so each is a row with its error and nothing is written into it
        let again = run(&request, None, &profiles).unwrap();
        assert!(again["bookPackage"]["documents"].as_array().unwrap().iter().all(|r| r["ok"] == json!(false) && r["code"] == json!("exists")), "{again}");
        // a folder that is not there is refused before anything is made
        let missing = json!({"root": root.to_str().unwrap(), "path": "book.foliobook", "action": "package", "dir": printer.join("nothing").to_str().unwrap()});
        assert!(run(&missing, None, &profiles).unwrap_err().starts_with("folder_missing"));
    }

    /// A document someone has open is not written, and nothing else is written either.
    #[test]
    fn a_locked_document_stops_the_whole_write() {
        let root = temp("lock");
        let project = manifest::create(&root, "Book").unwrap();
        for (i, n) in ["a.folio", "b.folio"].iter().enumerate() {
            make_folio(&root, &project.file.id, n, "pages.idml", &format!("doc{i}"));
        }
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        book::add(&root, "book.foliobook", "a.folio").unwrap();
        book::add(&root, "book.foliobook", "b.folio").unwrap();
        let before = std::fs::read(root.join("b.folio")).unwrap();
        let guard = lock::acquire(&root, "doc1", "b.folio").unwrap();
        let error = repaginate(&root, "book.foliobook", true);
        assert!(error.unwrap_err().starts_with("document_open"));
        assert_eq!(std::fs::read(root.join("b.folio")).unwrap(), before);
        assert_eq!(names(&root, "a.folio")[0], "1");
        drop(guard);
    }

    /// Synchronize Book carries the style source's styles into the others and keeps the document's own (oracle probe g1, g3).
    #[test]
    fn synchronize_adds_and_overwrites_but_keeps_the_documents_own_styles() {
        let root = temp("sync");
        let project = manifest::create(&root, "Book").unwrap();
        make_folio(&root, &project.file.id, "source.folio", "loadstyles-other.idml", "src");
        make_folio(&root, &project.file.id, "target.folio", "loadstyles.idml", "dst");
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        book::add(&root, "book.foliobook", "source.folio").unwrap();
        book::add(&root, "book.foliobook", "target.folio").unwrap();
        let styles = |doc: &str| crate::edit::package_parts(&root.join(doc)).unwrap().remove("Resources/Styles.xml").unwrap();
        assert!(!styles("target.folio").contains("New Para"));
        let source_before = std::fs::read(root.join("source.folio")).unwrap();
        let result = synchronize(&root, "book.foliobook").unwrap();
        let t = styles("target.folio");
        assert!(t.contains("New Para") && t.contains("Target Only"), "added from the source, own style kept");
        assert_eq!(result["documents"][1]["written"], true);
        assert_eq!(std::fs::read(root.join("source.folio")).unwrap(), source_before, "the source is never written");
        for e in &book::read(&root, "book.foliobook").unwrap().documents {
            book::inspect(&root, e).unwrap();
        }
        // a second run adds no style and makes no copy of one
        let named = |t: &str| t.split("<ParagraphStyle ").skip(1).filter_map(|c| c.split(" Name=\"").nth(1)?.split('"').next().map(str::to_owned)).collect::<Vec<_>>();
        synchronize(&root, "book.foliobook").unwrap();
        assert_eq!(named(&styles("target.folio")), named(&t));
        assert!(!named(&t).iter().any(|n| n.ends_with(" copy")));
    }

    /// Parent pages are a kind of their own: off by default (as `Book.synchronizeMasterPage` is), reported while off, carried when on.
    #[test]
    fn parent_pages_are_a_switchable_kind_and_reported_while_off() {
        let root = temp("sync-master");
        let project = manifest::create(&root, "Book").unwrap();
        make_folio(&root, &project.file.id, "source.folio", "loadstyles-other.idml", "src");
        make_folio(&root, &project.file.id, "target.folio", "loadstyles.idml", "dst");
        book::create(&root, "book.foliobook", "Book").unwrap();
        book::set_options(&root, "book.foliobook", Options { automatic: false, ..Options::default() }).unwrap();
        book::add(&root, "book.foliobook", "source.folio").unwrap();
        book::add(&root, "book.foliobook", "target.folio").unwrap();
        let reported = |r: &Value| r["notSynchronized"].as_array().unwrap().iter().any(|v| v == "masterPage");
        assert!(!book::read(&root, "book.foliobook").unwrap().sync.iter().any(|k| k == "masterPage"));
        assert!(reported(&synchronize(&root, "book.foliobook").unwrap()));
        book::set_sync(&root, "book.foliobook", &["masterPage".to_string()]).unwrap();
        let on = synchronize(&root, "book.foliobook").unwrap();
        assert!(!reported(&on));
        assert!(book::set_sync(&root, "book.foliobook", &["movie".to_string()]).is_err());
    }

    /// File > New > Book...: the `new` action makes the file named by the Save dialog's answer and then answers like `read`; the refusals are codes.
    #[test]
    fn new_action_makes_the_file_and_answers_like_read() {
        let root = temp("new-action");
        manifest::create(&root, "Book").unwrap();
        let profiles = crate::Profiles::start();
        let ask = |extra: Value| {
            let mut r = json!({"root": root, "action": "new", "folder": ""});
            r.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            run(&r, None, &profiles)
        };
        let made = ask(json!({"fileName": "책 1"})).unwrap();
        assert_eq!((made["file"]["path"].as_str(), made["file"]["name"].as_str(), made["file"]["replaced"].as_bool()), (Some("책 1.foliobook"), Some("책 1.foliobook"), Some(false)));
        assert_eq!(made["path"], "책 1.foliobook");
        assert!(made["book"]["documents"].as_array().unwrap().is_empty());
        assert_eq!(ask(json!({"fileName": "책 1"})).unwrap_err(), "exists: 책 1.foliobook");
        assert_eq!(ask(json!({"fileName": "책 1", "replace": true, "open": ["책 1.foliobook"]})).unwrap_err(), "open: 책 1.foliobook");
        assert_eq!(ask(json!({"fileName": "책 1", "replace": true})).unwrap()["file"]["replaced"], true);
        assert_eq!(ask(json!({"fileName": " "})).unwrap_err(), "name_required");
        assert!(ask(json!({"fileName": "x", "folder": "../밖"})).is_err());
        assert!(ask(json!({"fileName": "x", "folder": "없음"})).unwrap_err().starts_with("folder_missing"));
    }
}
