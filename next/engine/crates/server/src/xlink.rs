//! Content links between two documents of one project (Edit ▸ Place and Link / the Content Placer's Create Link into another document;
//! oracle `conveyor-c4`, InDesign 2025 probes b3/b4/b5, edit crate `ops/contentlink/foreign.rs`).
//!
//! Principle 1 (a project moves as a folder, no address of this computer is stored): the source document is a **reference of the
//! placing document's manifest** (`kind: file`, root-relative `path`, no hash since a document changes when it is edited, and the
//! source document's id in `document`), written in the `Link` as `idexternalSharedArticle:folio:path-ref/<id>?<n>`; an IDML export gives
//! InDesign's spelling back (the path of this computer, `project/export.rs`). The source must be a `.folio` of the same project.
//!
//! Making the link writes the link numbers into the source document (a `LinkResourceId` on the object and its story, as InDesign marks
//! the source document): it is read from disk, the numbers are given by its own editor (`markLinkSources`), and it is written back
//! atomically. A source someone has open is not written under them (as the book's documents are not), the user closes it first. The
//! numbers are idempotent: linking the same object again finds its number, so a placing that is refused afterwards leaves nothing to
//! undo in the source.
//!
//! The state of a link is read from the source *file* (InDesign does too: with the source closed a changed object makes the link out of
//! date, a moved file makes it missing): [`content_links`] reads the documents the links name, once per change of the file.
use crate::{Session, clip::Held, edit::EditState, place::Prepared};
use folio_edit::{Command, ContentLink, Editor, Foreign, ForeignMark, ForeignSource, commands::RegisteredCommand};
use folio_project::format::{self, PartAction, RefKind, Reference};
use folio_project::{lock, paths};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Before an edit that links what it places (`placeAndLink`, or a paste followed by `linkPlaced`) with objects of another document:
/// the source document gets its link numbers, the placing document the reference to it, and the editor the numbers.
pub(crate) fn prepare(h: &Held, cur: &Session, es: &EditState, prepared: &mut Prepared) -> Result<(), String> {
    if h.from() == es.path {
        return Ok(());
    }
    let folio = cur.folio.as_ref().ok_or("다른 문서의 개체와 링크하려면 두 문서가 같은 프로젝트에 있어야 합니다. 프로젝트를 열어서 하세요")?;
    let root = &folio.opened.project.root;
    let from = h.from();
    let rel = paths::relative_to_root(root, from)
        .map_err(|_| "원본 문서가 이 프로젝트 안에 있지 않아 링크를 만들 수 없습니다. 프로젝트 안의 .folio 문서만 링크 원본이 될 수 있습니다".to_owned())?;
    if !rel.as_str().ends_with(".folio") {
        return Err("INDD·IDML 문서는 링크 원본이 될 수 없습니다. 프로젝트로 가져온 .folio 문서만 됩니다".into());
    }
    let (manifest, _) = format::read_manifest(from).map_err(|e| format!("원본 문서를 읽지 못했습니다: {e}"))?;
    // someone has the source open: it is not written under them
    if lock::holder(root, &manifest.id).is_some_and(|holder| !lock::is_stale(&holder)) {
        return Err(format!("원본 문서 {}이(가) 열려 있습니다. 저장하고 닫은 뒤 다시 하세요 (링크 번호를 원본에 써야 합니다)", rel.as_str()));
    }
    let ids = h.clip.ids();
    let parts = crate::edit::package_parts(from)?;
    let mut source = Editor::new(parts.clone());
    source.apply("link", &[Command::Registered(RegisteredCommand::MarkLinkSources { ids: ids.clone() })]).map_err(|e| format!("원본 문서: {}", e.message))?;
    let disk = folio_edit::link_marks(source.parts(), &ids).map_err(|e| e.message)?;
    // the copy was made from what was copied: its state is what the link compares the source with
    let snapshot = h.clip.marks();
    let marks: Vec<ForeignMark> = disk.iter().map(|d| {
        let was = snapshot.iter().find(|s| s.id == d.id);
        ForeignMark {
            id: d.id.clone(),
            resource: d.resource,
            stamp: was.map_or(d.stamp, |s| s.stamp),
            story: d.story.map(|(n, st)| (n, was.and_then(|s| s.story).map_or(st, |s| s.1))),
            members: d.members.clone(),
        }
    }).collect();
    let touched: BTreeMap<String, Option<String>> = source.touched().iter().map(|p| (p.clone(), source.part(p).map(str::to_owned))).collect();
    if !touched.is_empty() {
        write_marked(from, &touched, &parts)?;
    }
    // the reference of the placing document: one per source document
    let document = manifest.id.clone();
    let existing = es.graphics.iter().flat_map(|g| &g.references).chain(&prepared.refs)
        .find(|r| r.kind == RefKind::File && r.path.as_ref() == Some(&rel) && r.extra.get("document").and_then(|v| v.as_str()) == Some(document.as_str()));
    let id = match existing {
        Some(r) => r.id.clone(),
        None => {
            let mut extra = serde_json::Map::new();
            extra.insert("document".into(), serde_json::json!(document));
            let r = Reference {
                id: format!("r{}", folio_project::util::short_id()),
                kind: RefKind::File,
                path: Some(rel),
                sha256: None,
                bytes: None,
                hint: None,
                original_name: None,
                status: Some("found".into()),
                font: None,
                extra,
            };
            let id = r.id.clone();
            prepared.refs.push(r);
            id
        }
    };
    prepared.foreign = Some(ForeignSource { document: format!("{}{id}", format::PATH_REF_SCHEME), marks });
    Ok(())
}

fn read_part(file: &Path, name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(file).ok()?)).ok()?;
    let mut e = z.by_name(name).ok()?;
    let mut out = Vec::new();
    e.read_to_end(&mut out).ok()?;
    Some(out)
}

/// The source document written again with the parts the marking touched (atomically; the saved lines are made again when it opens).
fn write_marked(file: &Path, touched: &BTreeMap<String, Option<String>>, old: &BTreeMap<String, String>) -> Result<(), String> {
    let (mut manifest, _) = format::read_manifest(file).map_err(|e| e.to_string())?;
    manifest.saved = folio_project::util::now_rfc3339();
    let report = read_part(file, format::REPORT_PART);
    let added: Vec<(&str, &str)> = touched.iter().filter(|(n, t)| t.is_some() && !old.contains_key(*n)).map(|(n, t)| (n.as_str(), t.as_deref().unwrap_or(""))).collect();
    let mut action = |name: &str, _: &mut dyn FnMut() -> std::io::Result<String>| -> std::io::Result<PartAction> {
        Ok(match touched.get(name) {
            Some(Some(text)) => PartAction::Replace(text.clone()),
            Some(None) => PartAction::Drop,
            None => PartAction::Keep,
        })
    };
    format::write_folio_with_parts(file, file, &mut action, &manifest, None, report.as_deref(), &added).map(|_| ()).map_err(|e| format!("원본 문서를 쓰지 못했습니다: {e}"))
}

type Cached = (SystemTime, u64, Arc<BTreeMap<String, String>>);
static DOCUMENTS: Mutex<Option<HashMap<PathBuf, Cached>>> = Mutex::new(None);

/// The parts of a source document, read again only when its file changed.
fn read_document(file: &Path) -> Option<Arc<BTreeMap<String, String>>> {
    let meta = std::fs::metadata(file).ok()?;
    let (time, len) = (meta.modified().ok()?, meta.len());
    let mut cache = DOCUMENTS.lock().unwrap_or_else(|p| p.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((t, l, parts)) = cache.get(file) && *t == time && *l == len {
        return Some(parts.clone());
    }
    let parts = Arc::new(crate::edit::package_parts(file).ok()?);
    cache.insert(file.to_path_buf(), (time, len, parts.clone()));
    Some(parts)
}

/// The document a link names (`folio:path-ref/<id>`): a project document by its reference, read from its file.
fn resolve(root: Option<&Path>, references: &[Reference], uri: &str) -> Foreign {
    let (Some(root), Some(id)) = (root, uri.strip_prefix(format::PATH_REF_SCHEME)) else { return Foreign::Unknown };
    let Some(r) = references.iter().find(|r| r.id == id) else { return Foreign::Unknown };
    // an address from an IDML import (a hint, no file of the project) is not a document we can read
    let Some(doc) = r.extra.get("document").and_then(|v| v.as_str()) else { return Foreign::Unknown };
    // the document with that id: at the stored path when it is there, else where the project has it now (it was moved or renamed:
    // principle 1, a project's documents are known by what they are); several candidates or none is a missing source
    let Ok(Some(found)) = paths_of(root, r.path.as_ref(), doc) else { return Foreign::Missing };
    let file = found.to_native(root);
    match read_document(&file) {
        Some(parts) => Foreign::Doc { name: file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), parts },
        None => Foreign::Missing,
    }
}

fn paths_of(root: &Path, at: Option<&folio_project::RelPath>, id: &str) -> Result<Option<folio_project::RelPath>, Vec<folio_project::RelPath>> {
    folio_project::docfind::resolve(root, at, id)
}

/// The content links of the open document, the links to other documents of the project with the state of their sources.
pub fn content_links(cur: &Session, es: &EditState) -> Vec<ContentLink> {
    let root = cur.folio.as_ref().map(|f| f.opened.project.root.clone());
    match &es.graphics {
        Some(g) => es.editor.content_links_with(&|uri| resolve(root.as_deref(), &g.references, uri)),
        None => es.editor.content_links(),
    }
}

/// Before an update of a content link: when the link is to another document, that document's parts are read for the editor (it takes
/// the source object from them). A document that cannot be read is left out: the editor then says the source is not available.
pub(crate) fn prepare_update(cur: &Session, es: &EditState, cmd: &serde_json::Value, prepared: &mut Prepared) {
    let (Some(link), Some(g)) = (cmd["link"].as_str(), &es.graphics) else { return };
    let root = cur.folio.as_ref().map(|f| f.opened.project.root.clone());
    for (_, doc) in es.editor.link_documents(&[link.to_owned()]) {
        if let Foreign::Doc { parts, .. } = resolve(root.as_deref(), &g.references, &doc) {
            prepared.foreign_docs.insert(doc, parts);
        }
    }
}

/// Before Links panel ▸ Relink of content links to another document that InDesign made (`reconnectContentLinks`, conveyor-c5): an import kept
/// the address of the `.indd` they name as the hint of a reference and no document of the project. The project's document that was
/// imported from a file of that name (its `source.name`, or the one the command's `target` names) becomes the reference's document: the
/// reference keeps its hint (the export gives InDesign's address back), gets the root-relative path and the document's id, and the
/// editor gets the document's parts (it joins the links to it). No address of this computer is stored.
pub(crate) fn prepare_reconnect(cur: &Session, es: &EditState, cmd: &mut serde_json::Value, prepared: &mut Prepared) -> Result<(), String> {
    let folio = cur.folio.as_ref().ok_or("InDesign 링크를 문서에 다시 이으려면 프로젝트를 열어서 하세요")?;
    let graphics = es.graphics.as_ref().ok_or("이 문서는 프로젝트 문서가 아닙니다")?;
    let root = &folio.opened.project.root;
    let asked = cmd["document"].as_str().unwrap_or_default().to_owned();
    let id = asked.strip_prefix(format::PATH_REF_SCHEME).unwrap_or(&asked).to_owned();
    let reference = graphics.references.iter().chain(&prepared.refs).find(|r| r.id == id).ok_or_else(|| format!("참조 {id}이(가) 이 문서에 없습니다"))?;
    if reference.extra.contains_key("document") {
        return Err("이 링크는 이미 프로젝트의 문서에 이어져 있습니다".into());
    }
    let hint = reference.hint.clone().filter(|h| !h.is_empty()).ok_or("이 참조에는 InDesign 이 쓴 원래 주소가 없습니다")?;
    let own = folio.opened.manifest.id.clone();
    let chosen = match cmd["target"].as_str().filter(|t| !t.is_empty()) {
        Some(t) => {
            let rel = folio_project::RelPath::parse(t).map_err(|e| format!("{t}: {e}"))?;
            let file = rel.to_native(root);
            if !rel.as_str().to_lowercase().ends_with(".folio") || !file.is_file() {
                return Err(format!("{t}은(는) 이 프로젝트의 .folio 문서가 아닙니다"));
            }
            let (m, _) = format::read_manifest(&file).map_err(|e| format!("{t}을(를) 읽지 못했습니다: {e}"))?;
            if m.id == own {
                return Err("문서 자신에게는 이을 수 없습니다".into());
            }
            (rel, m.id)
        }
        None => {
            let name = folio_project::docfind::file_name_of_address(&hint);
            let found: Vec<_> = folio_project::docfind::imported_from(root, &name, None).into_iter().filter(|d| d.id != own).collect();
            match found.as_slice() {
                [] => return Err(format!("이 프로젝트에 {name}에서 가져온 문서가 없습니다. 그 INDD 를 프로젝트로 가져온 뒤 다시 하세요")),
                [one] => (one.path.clone(), one.id.clone()),
                many => {
                    let list: Vec<&str> = many.iter().map(|d| d.path.as_str()).collect();
                    return Err(format!("{name}에서 가져온 문서가 여러 개입니다 ({}). 어느 것인지 target 으로 고르세요", list.join(", ")));
                }
            }
        }
    };
    let file = chosen.0.to_native(root);
    let parts = read_document(&file).ok_or_else(|| format!("{}을(를) 읽지 못했습니다", chosen.0))?;
    // the links name a reference of the project document from now on (so undo gives them the address back): it keeps the .indd address as its hint
    let mut extra = serde_json::Map::new();
    extra.insert("document".into(), serde_json::json!(chosen.1));
    let joined = Reference {
        id: format!("r{}", folio_project::util::short_id()),
        kind: RefKind::File,
        path: Some(chosen.0),
        sha256: None,
        bytes: None,
        hint: Some(hint),
        original_name: reference.original_name.clone(),
        status: Some("found".into()),
        font: None,
        extra,
    };
    let uri = format!("{}{id}", format::PATH_REF_SCHEME);
    prepared.foreign_docs.insert(uri.clone(), parts);
    cmd["document"] = serde_json::json!(uri);
    cmd["to"] = serde_json::json!(format!("{}{}", format::PATH_REF_SCHEME, joined.id));
    prepared.refs.push(joined);
    Ok(())
}

/// Query `xlinkSources`: the other documents the content links of the open document name, and which of them are not joined to a document of the
/// project yet (a content link made by InDesign names the `.indd` of another computer): for each reference the file name, the links that name
/// it and the project's documents that were imported from a file of that name (`candidates`). `reconnectContentLinks` joins one.
pub fn sources(s: Option<&Session>) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let s = s.ok_or("no document open")?;
    let shared = s.edit.as_ref().ok_or("no document open")?;
    let es = shared.lock().unwrap_or_else(|p| p.into_inner());
    let (Some(folio), Some(graphics)) = (s.folio.as_ref(), es.graphics.as_ref()) else { return Ok(json!({"sources": []})) };
    let root = &folio.opened.project.root;
    let own = folio.opened.manifest.id.clone();
    let links = content_links(s, &es);
    let mut rows = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for l in links.iter().filter(|l| l.external) {
        let Some(id) = folio_edit::external_document_of(&l.uri).and_then(|d| d.strip_prefix(format::PATH_REF_SCHEME)) else { continue };
        if seen.iter().any(|x| x == id) {
            continue;
        }
        seen.push(id.to_owned());
        let Some(r) = graphics.references.iter().find(|r| r.id == id) else { continue };
        let mine: Vec<&ContentLink> = links.iter().filter(|c| c.external && folio_edit::external_document_of(&c.uri).and_then(|d| d.strip_prefix(format::PATH_REF_SCHEME)) == Some(id)).collect();
        let document = r.extra.get("document").and_then(|v| v.as_str());
        let name = r.hint.as_deref().map(folio_project::docfind::file_name_of_address).or_else(|| r.path.as_ref().map(|p| p.file_name().to_owned())).unwrap_or_default();
        let candidates: Vec<serde_json::Value> = match (document, r.hint.as_deref()) {
            (None, Some(_)) => folio_project::docfind::imported_from(root, &name, None).into_iter().filter(|d| d.id != own)
                .map(|d| json!({"path": d.path.as_str(), "id": d.id})).collect(),
            _ => Vec::new(),
        };
        rows.push(json!({"reference": r.id, "file": name, "connected": document.is_some(), "document": document, "path": r.path.as_ref().map(|p| p.as_str()),
            "links": mine.iter().map(|c| &c.link).collect::<Vec<_>>(), "candidates": candidates}));
    }
    Ok(json!({"sources": rows}))
}
