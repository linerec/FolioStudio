//! The project explorer: documents, assets and other files of a project, with what each
//! document refers to and whether it is there, sync conflict copies and name problems.

use crate::conflicts;
use crate::format;
use crate::lock;
use crate::manifest::Project;
use crate::paths::{self, RelPath};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: RelPath,
    pub name: String,
    /// `dir`, `document`, `file`.
    pub kind: &'static str,
    pub bytes: u64,
    pub children: Vec<Entry>,
    /// Set for conflict copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<conflicts::ConflictKind>,
    /// What a file is for the explorer (its icon and what opening it does): `book`, `library`, `image`, `font`, `data`
    /// (a data merge source or snapshot), `indesign` (INDD, INDT, IDML to import), `pdf`; absent for other files and folders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<&'static str>,
}

/// The explorer's kind of a file, by its extension (the folder it is in does not matter: a project is free in how it is arranged).
pub fn resource_kind(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
    Some(match ext {
        "foliobook" => "book",
        "foliolibrary" => "library",
        "jpg" | "jpeg" | "png" | "tif" | "tiff" | "psd" | "gif" | "bmp" | "webp" | "eps" | "ai" | "svg" | "heic" => "image",
        "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2" | "pfb" | "pfm" | "dfont" => "font",
        "csv" | "tsv" | "txt" => "data",
        "indd" | "indt" | "idml" => "indesign",
        "pdf" => "pdf",
        _ => return None,
    })
}

/// Per document: references and how many are present (by path; content is checked when
/// the document opens), and who holds its lock.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub path: RelPath,
    pub id: Option<String>,
    pub links: usize,
    pub links_present: usize,
    /// Links that were not in the project at import (hint only).
    pub links_missing: usize,
    pub files: usize,
    pub files_present: usize,
    pub locked_by: Option<Value>,
    pub error: Option<String>,
}

const MAX_ENTRIES: usize = 20_000;

fn build(dir: &Path, names: &mut Vec<String>, count: &mut usize, conflicts: &BTreeMap<String, conflicts::ConflictKind>) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    let mut list: Vec<_> = rd.filter_map(Result::ok).collect();
    list.sort_by_key(|e| e.file_name());
    for e in list {
        if *count >= MAX_ENTRIES {
            break;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if names.is_empty() && (name == crate::manifest::DATA_DIR || name == crate::manifest::PROJECT_FILE) {
            continue;
        }
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue;
        }
        names.push(name.clone());
        let Ok(rel) = RelPath::from_names(names) else {
            names.pop();
            continue;
        };
        *count += 1;
        if t.is_dir() {
            let children = build(&e.path(), names, count, conflicts);
            let bytes = children.iter().map(|c| c.bytes).sum();
            out.push(Entry { path: rel, name, kind: "dir", bytes, children, conflict: None, resource: None });
        } else {
            let bytes = e.metadata().map_or(0, |m| m.len());
            let kind = if name.to_lowercase().ends_with(".folio") { "document" } else { "file" };
            let conflict = conflicts.get(rel.as_str()).copied();
            let resource = if kind == "file" { resource_kind(&name) } else { None };
            out.push(Entry { path: rel, name, kind, bytes, children: Vec::new(), conflict, resource });
        }
        names.pop();
    }
    // folders first
    out.sort_by_key(|e| (e.kind != "dir", e.name.to_lowercase()));
    out
}

fn document_info(project: &Project, rel: &RelPath) -> DocumentInfo {
    let mut info = DocumentInfo { path: rel.clone(), id: None, links: 0, links_present: 0, links_missing: 0, files: 0, files_present: 0, locked_by: None, error: None };
    match format::read_manifest(&rel.on_disk(&project.root)) {
        Ok((m, _)) => {
            info.id = Some(m.id.clone());
            for r in &m.references {
                let present = r.path.as_ref().is_some_and(|p| paths::resolve_on_disk(&project.root, p).is_some());
                if r.kind == format::RefKind::Link {
                    info.links += 1;
                    if present {
                        info.links_present += 1;
                    } else if r.path.is_none() {
                        info.links_missing += 1;
                    }
                }
                if r.path.is_some() {
                    info.files += 1;
                    info.files_present += usize::from(present);
                }
            }
            if let Some(h) = lock::holder(&project.root, &m.id) {
                // this engine's own lock (the document open in this window) is not "in use"
                let mine = h.pid == std::process::id() && h.machine.eq_ignore_ascii_case(&lock::machine_name());
                info.locked_by = Some(json!({"user": h.user, "machine": h.machine, "time": h.time, "stale": lock::is_stale(&h), "mine": mine}));
            }
        }
        Err(e) => info.error = Some(e.to_string()),
    }
    info
}

/// The explorer's view of the project.
pub fn list(project: &Project) -> Value {
    let started = std::time::Instant::now();
    let found = conflicts::scan(&project.root);
    let by_path: BTreeMap<String, conflicts::ConflictKind> = found.iter().map(|c| (c.path.to_string(), c.kind)).collect();
    let mut count = 0;
    let entries = build(&project.root, &mut Vec::new(), &mut count, &by_path);
    let mut documents = Vec::new();
    fn docs(e: &[Entry], out: &mut Vec<RelPath>) {
        for x in e {
            if x.kind == "document" {
                out.push(x.path.clone());
            }
            docs(&x.children, out);
        }
    }
    let mut doc_paths = Vec::new();
    docs(&entries, &mut doc_paths);
    for d in &doc_paths {
        documents.push(document_info(project, d));
    }
    let issues = name_issues(&project.root);
    json!({
        "root": project.root.display().to_string(),
        "id": project.file.id,
        "name": project.file.name,
        "entries": entries,
        "documents": documents,
        "conflicts": found,
        "issues": issues,
        "truncated": count >= MAX_ENTRIES,
        "ms": started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Names that are not portable, collisions and links anywhere in the project.
pub fn name_issues(root: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    fn go(root: &Path, dir: &Path, out: &mut Vec<Value>, depth: usize) {
        if depth > 32 || out.len() > 200 {
            return;
        }
        for i in paths::check_listing(dir) {
            let rel = dir.strip_prefix(root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            out.push(json!({"folder": rel, "name": i.name, "kind": i.kind, "other": i.other, "detail": i.detail}));
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.filter_map(Result::ok) {
            if depth == 0 && e.file_name() == crate::manifest::DATA_DIR {
                continue;
            }
            if e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()) {
                go(root, &e.path(), out, depth + 1);
            }
        }
    }
    go(root, root, &mut out, 0);
    out
}

#[cfg(test)]
mod resource_tests {
    #[test]
    fn files_get_the_explorer_kind_of_their_extension() {
        let k = super::resource_kind;
        assert_eq!((k("Issue 3.foliobook"), k("로고.FolioLibrary"), k("cover.TIF"), k("Pretendard.otf"), k("merge.csv")), (Some("book"), Some("library"), Some("image"), Some("font"), Some("data")));
        assert_eq!((k("원본.indd"), k("proof.pdf"), k("notes"), k("archive.zip")), (Some("indesign"), Some("pdf"), None, None));
    }
}
