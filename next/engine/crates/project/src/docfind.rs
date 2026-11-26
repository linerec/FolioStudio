//! Finding the documents of a project by what they are, not by where they were (principle 1: a project moves as a folder, and a
//! document inside it can be moved or renamed too).
//!
//! A `.folio` has an id (`folio/manifest.json`, set once when it is made or imported) that moving or renaming keeps. A link to an
//! object of another document names that document by a reference of the placing document's manifest (root-relative path + the
//! document's id, `ops/contentlink/foreign.rs`); when the path no longer holds that document the project is searched for the id
//! ([`resolve`]). A document is not found by a content hash (it changes whenever it is edited).
//!
//! An INDD that was imported keeps what it was imported from (`source.name`, `source.sha256`): the content links InDesign made to an
//! `.indd` of another computer name only that file's address, so a document imported from a file of that name is its project copy
//! ([`imported_from`]).
//!
//! Manifests are read once per file version (size and time), so asking again is cheap. Nothing is written here.

use crate::format;
use crate::paths::{self, RelPath};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// What a document's manifest says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocEntry {
    pub path: RelPath,
    pub id: String,
    /// Where it was imported from: the file name and the SHA-256 of that INDD / IDML (`None`: made in Folio).
    pub source_name: Option<String>,
    pub source_sha256: Option<String>,
    /// Its name is a sync client's conflict copy (`name_<machine>_Conflict.folio`): found only when nothing else has the id.
    pub conflict: bool,
}

/// The documents of the project that hold an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    One(RelPath),
    /// More than one candidate after leaving out conflict copies: it is not guessed which.
    Several(Vec<RelPath>),
    None,
}

type Cached = (SystemTime, u64, Option<(String, Option<(String, String)>)>);
static MANIFESTS: Mutex<Option<HashMap<PathBuf, Cached>>> = Mutex::new(None);

const MAX_FILES: usize = 20_000;

fn read_entry(file: &Path) -> Option<(String, Option<(String, String)>)> {
    let meta = std::fs::metadata(file).ok()?;
    let (time, len) = (meta.modified().ok()?, meta.len());
    let mut cache = MANIFESTS.lock().unwrap_or_else(|p| p.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((t, l, e)) = cache.get(file)
        && *t == time
        && *l == len
    {
        return e.clone();
    }
    let read = format::read_manifest(file).ok().map(|(m, _)| (m.id, m.source.map(|s| (s.name, s.sha256))));
    cache.insert(file.to_path_buf(), (time, len, read.clone()));
    read
}

fn walk(dir: &Path, names: &mut Vec<String>, out: &mut Vec<(RelPath, PathBuf)>, depth: usize) {
    if depth > 32 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut list: Vec<_> = rd.filter_map(Result::ok).collect();
    list.sort_by_key(|e| e.file_name());
    for e in list {
        let name = e.file_name().to_string_lossy().into_owned();
        // the data folder and the project file at the root are not documents; links and junctions are not followed
        if names.is_empty() && (name == crate::manifest::DATA_DIR || name == crate::manifest::PROJECT_FILE) {
            continue;
        }
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue;
        }
        names.push(name.clone());
        if t.is_dir() {
            walk(&e.path(), names, out, depth + 1);
        } else if name.to_lowercase().ends_with(".folio")
            && let Ok(rel) = RelPath::from_names(names)
        {
            out.push((rel, e.path()));
        }
        names.pop();
    }
}

/// Every `.folio` of the project with its id (a file that cannot be read as a document is left out).
pub fn documents(root: &Path) -> Vec<DocEntry> {
    let mut files = Vec::new();
    walk(root, &mut Vec::new(), &mut files, 0);
    let present: std::collections::HashSet<String> = files.iter().map(|(r, _)| r.as_str().to_owned()).collect();
    files
        .into_iter()
        .filter_map(|(path, file)| {
            let (id, source) = read_entry(&file)?;
            let conflict = crate::conflicts::classify(path.file_name(), &|sibling| {
                let dir = path.parent().map(|p| format!("{}/", p.as_str())).unwrap_or_default();
                present.contains(&format!("{dir}{sibling}"))
            })
            .is_some();
            Some(DocEntry { path, id, source_name: source.as_ref().map(|s| s.0.clone()), source_sha256: source.map(|s| s.1), conflict })
        })
        .collect()
}

/// The document of the project with `id`: [`Found::One`] when exactly one file holds it (a sync client's conflict copy is left out when
/// another file holds it too), [`Found::Several`] when it cannot be told which one.
pub fn find_by_id(root: &Path, id: &str) -> Found {
    let all: Vec<DocEntry> = documents(root).into_iter().filter(|d| d.id == id).collect();
    let mut list: Vec<&DocEntry> = all.iter().filter(|d| !d.conflict).collect();
    if list.is_empty() {
        list = all.iter().collect();
    }
    match list.as_slice() {
        [] => Found::None,
        [one] => Found::One(one.path.clone()),
        many => Found::Several(many.iter().map(|d| d.path.clone()).collect()),
    }
}

/// Where the document `id` is now: at `at` when that file is the document (the usual case: one file read), else wherever the project
/// has it. `Err` carries the candidates when it cannot be told (several files with the id).
pub fn resolve(root: &Path, at: Option<&RelPath>, id: &str) -> Result<Option<RelPath>, Vec<RelPath>> {
    if let Some(at) = at
        && let Some(file) = paths::resolve_on_disk(root, at).filter(|f| f.is_file())
        && read_entry(&file).is_some_and(|(found, _)| found == id)
    {
        return Ok(Some(at.clone()));
    }
    match find_by_id(root, id) {
        Found::One(p) => Ok(Some(p)),
        Found::None => Ok(None),
        Found::Several(c) => Err(c),
    }
}

/// The file name of an address of another computer (`C:/Users/kim/My Docs/a.indd`, `/Volumes/x/a.indd`, `file:///…/a.indd`), its
/// separators of either kind and a `%20`-style escape understood.
pub fn file_name_of_address(address: &str) -> String {
    let tail = address.rsplit(['/', '\\']).next().unwrap_or(address);
    let tail = tail.split(['?', '#']).next().unwrap_or(tail);
    let mut bytes = Vec::with_capacity(tail.len());
    let raw = tail.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() && let Some(Ok(b)) = tail.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16)) {
            bytes.push(b);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The documents imported from a file called like `file_name` (the name of the INDD an InDesign link names): same name without
/// regard to case, or the same name with `.indd` / `.idml` / `.indt` exchanged (an IDML made from it is its copy too). A document
/// whose `source.sha256` is `sha256` is a sure match and is the only one returned.
pub fn imported_from(root: &Path, file_name: &str, sha256: Option<&str>) -> Vec<DocEntry> {
    fn stem(n: &str) -> String {
        let (s, e) = paths::split_ext(n);
        paths::collision_key(if matches!(e.to_lowercase().as_str(), ".indd" | ".idml" | ".indt") { s } else { n })
    }
    let want = stem(file_name);
    let all: Vec<DocEntry> = documents(root).into_iter().filter(|d| d.source_name.as_deref().is_some_and(|n| stem(n) == want)).collect();
    if let Some(sha) = sha256 {
        let sure: Vec<DocEntry> = all.iter().filter(|d| d.source_sha256.as_deref() == Some(sha)).cloned().collect();
        if !sure.is_empty() {
            return sure;
        }
    }
    let real: Vec<DocEntry> = all.iter().filter(|d| !d.conflict).cloned().collect();
    if real.is_empty() { all } else { real }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{DocManifest, PartAction, RefKind, Reference, SourceInfo};

    fn doc(root: &Path, rel: &str, id: &str, project: &str, source: Option<(&str, &str)>, references: Vec<Reference>) {
        let idml = root.join("tiny.idml");
        crate::format::tests_support::tiny_idml(&idml);
        let m = DocManifest {
            format: format::FORMAT.into(),
            version: format::VERSION,
            id: id.into(),
            project: project.into(),
            created: "2026-11-13T00:00:00Z".into(),
            saved: "2026-11-13T00:00:00Z".into(),
            engine: String::new(),
            source: source.map(|(name, sha)| SourceInfo { name: name.into(), kind: "indd".into(), sha256: sha.into(), bytes: 1, reader: String::new() }),
            link_uris: "folio-ref".into(),
            references,
            graphics: vec![],
            pins: vec![],
            saved_lines: None,
            import_report: None,
            extra: Default::default(),
        };
        let file = root.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        format::write_folio(&file, &idml, &mut |_, _| Ok(PartAction::Keep), &m, None, None).unwrap();
        let _ = std::fs::remove_file(idml);
    }

    fn source_ref(path: &str, document: &str) -> Reference {
        let mut extra = serde_json::Map::new();
        extra.insert("document".into(), serde_json::json!(document));
        Reference { id: "r1".into(), kind: RefKind::File, path: Some(RelPath::parse(path).unwrap()), sha256: None, bytes: None, hint: None, original_name: None, status: Some("found".into()), font: None, extra }
    }

    #[test]
    fn a_moved_or_renamed_source_document_is_found_by_its_id() {
        let base = std::env::temp_dir().join(format!("folio-docfind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let project = crate::manifest::create(&base.join("p"), "p").unwrap();
        let root = project.root.clone();
        let pid = project.file.id.clone();
        doc(&root, "a.folio", "doc-a", &pid, None, vec![]);
        doc(&root, "issue/b.folio", "doc-b", &pid, None, vec![source_ref("a.folio", "doc-a")]);
        let state = |expect_path: &str| {
            let o = crate::open::resolve(&root.join("issue/b.folio")).unwrap();
            let st = o.state("r1").unwrap();
            (st.state, st.found_at.as_ref().map(|p| p.as_str().to_owned()), st.file.is_some(), expect_path.to_owned())
        };
        assert_eq!(state("").0, "ok");
        // moved into another folder and renamed
        std::fs::create_dir_all(root.join("old/2025")).unwrap();
        std::fs::rename(root.join("a.folio"), root.join("old/2025/Cover (final).folio")).unwrap();
        let (s, at, has_file, _) = state("");
        assert_eq!((s, at.as_deref(), has_file), ("moved", Some("old/2025/Cover (final).folio"), true));
        assert_eq!(resolve(&root, Some(&RelPath::parse("a.folio").unwrap()), "doc-a"), Ok(Some(RelPath::parse("old/2025/Cover (final).folio").unwrap())));
        // another document now at the old path is not the source
        doc(&root, "a.folio", "someone-else", &pid, None, vec![]);
        let (s, at, _, _) = state("");
        assert_eq!((s, at.as_deref()), ("moved", Some("old/2025/Cover (final).folio")));
        // a sync client's conflict copy next to it does not make it ambiguous
        std::fs::copy(root.join("old/2025/Cover (final).folio"), root.join("old/2025/Cover (final)_PC_2026-11-13_Conflict.folio")).unwrap();
        assert_eq!(find_by_id(&root, "doc-a"), Found::One(RelPath::parse("old/2025/Cover (final).folio").unwrap()));
        // two real copies: not guessed
        std::fs::copy(root.join("old/2025/Cover (final).folio"), root.join("Cover copy.folio")).unwrap();
        assert!(matches!(find_by_id(&root, "doc-a"), Found::Several(c) if c.len() == 2));
        let (s, at, has_file, _) = state("");
        assert_eq!((s, at, has_file), ("missing", None, false));
        // gone altogether
        std::fs::remove_file(root.join("Cover copy.folio")).unwrap();
        std::fs::remove_file(root.join("old/2025/Cover (final).folio")).unwrap();
        std::fs::remove_file(root.join("old/2025/Cover (final)_PC_2026-11-13_Conflict.folio")).unwrap();
        assert_eq!(find_by_id(&root, "doc-a"), Found::None);
        // the whole project moved elsewhere: the root-relative path still holds
        doc(&root, "a.folio", "doc-a", &pid, None, vec![]);
        let moved = base.join("deeper").join("other name");
        std::fs::create_dir_all(&moved).unwrap();
        crate::open::tests::copy_dir(&root, &moved);
        let o = crate::open::resolve(&moved.join("issue/b.folio")).unwrap();
        assert_eq!(o.state("r1").unwrap().state, "ok");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_document_imported_from_a_file_is_found_by_that_files_name() {
        let base = std::env::temp_dir().join(format!("folio-docfind2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let project = crate::manifest::create(&base.join("p"), "p").unwrap();
        let root = project.root.clone();
        let pid = project.file.id.clone();
        doc(&root, "cover.folio", "doc-cover", &pid, Some(("Cover.INDD", "aa")), vec![]);
        doc(&root, "other/cover.folio", "doc-cover2", &pid, Some(("cover.indd", "bb")), vec![]);
        doc(&root, "inside.folio", "doc-in", &pid, Some(("inside.idml", "cc")), vec![]);
        doc(&root, "mine.folio", "doc-mine", &pid, None, vec![]);
        let names = |v: Vec<DocEntry>| v.into_iter().map(|d| d.path.as_str().to_owned()).collect::<Vec<_>>();
        assert_eq!(names(imported_from(&root, "inside.indd", None)), vec!["inside.folio"]);
        assert_eq!(names(imported_from(&root, "COVER.indd", None)), vec!["cover.folio", "other/cover.folio"]);
        assert_eq!(names(imported_from(&root, "cover.indd", Some("bb"))), vec!["other/cover.folio"]);
        assert!(imported_from(&root, "mine.indd", None).is_empty());
        assert_eq!(file_name_of_address("C:/Users/kim/My Docs/a%20b.indd"), "a b.indd");
        assert_eq!(file_name_of_address(r"D:\Work\잡지\표지.indd"), "표지.indd");
        assert_eq!(file_name_of_address("file:///Volumes/x/a.indd?18"), "a.indd");
        let _ = std::fs::remove_dir_all(&base);
    }
}
