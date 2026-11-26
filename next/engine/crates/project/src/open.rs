//! Opening a `.folio`: every reference is resolved relative to the project root on this
//! machine, checked by content hash, and handed to the IDML reader as previews, link
//! locations, saved lines and extracted embedded files.
//!
//! Per reference with a path: the stored spelling (or an NFC-equal one on disk) is used when
//! it exists; its SHA-256 (from the hash index when size and time are unchanged) decides
//! whether it is still the same content (`ok`) or `changed`. A path that no longer exists is
//! looked up by content inside the project (`moved`). Nothing outside the project is
//! consulted: references never depend on search folders or the old reader cache.

use crate::format::{self, DocManifest, RefKind, Reference};
use crate::hash::HashIndex;
use crate::manifest::Project;
use crate::paths::{self, RelPath};
use folio_idml::{LinkOverride, ReaderData};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The state of one reference on this machine.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefState {
    pub id: String,
    pub kind: RefKind,
    pub path: Option<RelPath>,
    /// `ok`, `changed` (same path, other content), `moved` (found by content elsewhere in the
    /// project), `missing` (not in the project), `notInProject` (never was: a missing link,
    /// a font that may not be copied).
    pub state: &'static str,
    /// Where it was found (root-relative), when `moved`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found_at: Option<RelPath>,
    #[serde(skip)]
    pub file: Option<PathBuf>,
    #[serde(skip)]
    pub size: Option<u64>,
    #[serde(skip)]
    pub mtime: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub references: usize,
    pub ok: usize,
    pub changed: usize,
    pub moved: usize,
    pub missing: usize,
    pub not_in_project: usize,
    /// Links that resolve to a project file (of those with a path).
    pub links_in_project: usize,
    pub links_resolved: usize,
    pub hashed_files: usize,
    pub hashed_bytes: u64,
    pub index_hits: usize,
    pub resolve_ms: f64,
    pub pins_changed: usize,
    pub long_paths: usize,
}

/// A pinned shared resource whose current content differs from the pin.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinChange {
    pub reference: String,
    pub path: RelPath,
    pub pinned: String,
    pub current: Option<String>,
}

/// A resolved `.folio`.
#[derive(Clone)]
pub struct Opened {
    pub project: Project,
    pub file: PathBuf,
    pub rel: RelPath,
    pub manifest: DocManifest,
    pub refs: Vec<RefState>,
    pub summary: Summary,
    pub pin_changes: Vec<PinChange>,
    pub warnings: Vec<String>,
    /// For the IDML reader.
    pub reader: ReaderData,
}

impl Opened {
    pub fn state(&self, id: &str) -> Option<&RefState> {
        self.refs.iter().find(|r| r.id == id)
    }
}

#[derive(Debug)]
pub enum Error {
    NotInProject(PathBuf),
    Project(crate::manifest::Error),
    Format(format::Error),
    Path(paths::PathError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotInProject(p) => write!(f, "{}은(는) Folio 프로젝트(folio.project가 있는 폴더) 안에 있지 않습니다", p.display()),
            Error::Project(e) => write!(f, "{e}"),
            Error::Format(e) => write!(f, "{e}"),
            Error::Path(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

fn mtime_secs(m: &std::fs::Metadata) -> Option<f64> {
    m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs_f64())
}

/// Hashes the files of `jobs` (reference index, path, file) on a few threads, through the
/// index; returns (index, sha, size) for those that could be read.
type HashedFile = (usize, RelPath, PathBuf, std::io::Result<(String, u64)>);

fn hash_all(index: &mut HashIndex, jobs: Vec<(usize, RelPath, PathBuf)>) -> Vec<(usize, String, u64)> {
    let mut out = Vec::new();
    let mut misses = Vec::new();
    for (i, rel, file) in jobs {
        if let Ok(m) = std::fs::metadata(&file) {
            match index.cached(&rel, &file, &m) {
                Some(e) => {
                    out.push((i, e.sha256.clone(), e.size));
                    index.stats.hits += 1;
                }
                None => misses.push((i, rel, file)),
            }
        }
    }
    if misses.is_empty() {
        return out;
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 4);
    let chunk = misses.len().div_ceil(threads).max(1);
    let hashed: Vec<HashedFile> = std::thread::scope(|s| {
        let hs: Vec<_> = misses
            .chunks(chunk)
            .map(|c| s.spawn(move || c.iter().map(|(i, r, f)| (*i, r.clone(), f.clone(), crate::hash::sha256_file(f))).collect::<Vec<_>>()))
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    for (i, rel, file, r) in hashed {
        if let Ok((sha, size)) = r {
            index.stats.hashed += 1;
            index.stats.hashed_bytes += size;
            if let Ok(m) = std::fs::metadata(&file)
                && m.len() == size
            {
                index.record(&rel, &file, &m, &sha);
            }
            out.push((i, sha, size));
        }
    }
    out
}

/// Resolves every reference of the `.folio` at `file`.
pub fn resolve(file: &Path) -> Result<Opened, Error> {
    let started = Instant::now();
    let file = std::path::absolute(file).unwrap_or_else(|_| file.to_path_buf());
    let file = folio_links::uri::resolve_native(&file).unwrap_or(file);
    let project = crate::manifest::open(&file).map_err(|_| Error::NotInProject(file.clone()))?;
    let rel = paths::relative_to_root(&project.root, &file).map_err(Error::Path)?;
    let (manifest, _) = format::read_manifest(&file).map_err(Error::Format)?;
    let mut warnings = Vec::new();
    if manifest.project != project.file.id {
        warnings.push(format!(
            "이 문서는 다른 프로젝트({})에서 만들어졌습니다. 참조는 지금 프로젝트 폴더 기준으로 찾습니다.",
            manifest.project
        ));
    }
    let mut index = HashIndex::load(&project.root);
    let mut refs: Vec<RefState> = manifest
        .references
        .iter()
        .map(|r| RefState { id: r.id.clone(), kind: r.kind, path: r.path.clone(), state: "notInProject", found_at: None, file: None, size: None, mtime: None })
        .collect();
    // 1. present at the stored path (or an NFC-equal spelling)
    let mut jobs = Vec::new();
    for (i, r) in manifest.references.iter().enumerate() {
        let Some(p) = &r.path else { continue };
        if p.is_long() {
            warnings.push(format!("경로가 깁니다(Windows MAX_PATH 위험): {p}"));
        }
        match paths::resolve_on_disk(&project.root, p).filter(|f| f.is_file()) {
            Some(f) => {
                if let Some(l) = paths::link_inside(&project.root, &f) {
                    warnings.push(format!("프로젝트 안의 심볼릭 링크/정션은 쓰지 않습니다: {}", l.display()));
                }
                refs[i].file = Some(f.clone());
                // a link's source document is a document, known by its id (step 3b), not by a hash of a file that changes when it is edited
                if r.extra.contains_key("document") {
                    refs[i].state = "ok";
                    continue;
                }
                jobs.push((i, p.clone(), f));
            }
            None => refs[i].state = "missing",
        }
    }
    // 2. content check
    let hashed = hash_all(&mut index, jobs);
    let mut digest: HashMap<usize, (String, u64)> = HashMap::new();
    for (i, sha, size) in hashed {
        digest.insert(i, (sha, size));
    }
    for (i, r) in manifest.references.iter().enumerate() {
        if refs[i].file.is_none() || r.extra.contains_key("document") {
            continue;
        }
        match digest.get(&i) {
            Some((sha, _)) if r.sha256.as_deref().is_none_or(|s| s == sha) => refs[i].state = "ok",
            Some(_) => refs[i].state = "changed",
            None => {
                refs[i].state = "missing";
                refs[i].file = None;
            }
        }
    }
    // 3. missing paths: the same content elsewhere in the project
    for (i, r) in manifest.references.iter().enumerate() {
        if refs[i].state != "missing" {
            continue;
        }
        if let (Some(sha), Some(bytes)) = (&r.sha256, r.bytes)
            && let Some(found) = index.find_by_hash(sha, bytes)
        {
            refs[i].file = Some(found.on_disk(&project.root));
            refs[i].found_at = Some(found);
            refs[i].state = "moved";
        }
    }
    // 3b. the source document of a content link is the document with that id: where the path says when it is there, else wherever the
    // project has it now (it was moved or renamed). Another document at the path, or several with the id, is not it.
    for (i, r) in manifest.references.iter().enumerate() {
        let Some(id) = r.extra.get("document").and_then(|v| v.as_str()) else { continue };
        match crate::docfind::resolve(&project.root, r.path.as_ref(), id) {
            Ok(Some(found)) if r.path.as_ref() == Some(&found) => {
                refs[i].file = Some(found.on_disk(&project.root));
                refs[i].state = "ok";
            }
            Ok(Some(found)) => {
                refs[i].file = Some(found.on_disk(&project.root));
                refs[i].found_at = Some(found);
                refs[i].state = "moved";
            }
            Ok(None) | Err(_) => {
                refs[i].file = None;
                if r.path.is_some() {
                    refs[i].state = "missing";
                }
            }
        }
    }
    for st in &mut refs {
        if let Some(f) = &st.file
            && let Ok(m) = std::fs::metadata(f)
        {
            st.size = Some(m.len());
            st.mtime = mtime_secs(&m);
        }
    }
    let _ = index.save();

    let mut summary = Summary { references: refs.len(), hashed_files: index.stats.hashed, hashed_bytes: index.stats.hashed_bytes, index_hits: index.stats.hits, ..Default::default() };
    for (st, r) in refs.iter().zip(&manifest.references) {
        match st.state {
            "ok" => summary.ok += 1,
            "changed" => summary.changed += 1,
            "moved" => summary.moved += 1,
            "missing" => summary.missing += 1,
            _ => summary.not_in_project += 1,
        }
        if r.kind == RefKind::Link && r.path.is_some() {
            summary.links_in_project += 1;
            if st.file.is_some() {
                summary.links_resolved += 1;
            }
        }
        if r.path.as_ref().is_some_and(RelPath::is_long) {
            summary.long_paths += 1;
        }
    }

    // pins: shared resources whose content changed since the document was saved
    let mut pin_changes = Vec::new();
    let position: HashMap<&str, usize> = manifest.references.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
    for p in &manifest.pins {
        let current = position.get(p.reference.as_str()).and_then(|i| digest.get(i)).map(|d| d.0.clone());
        if current.as_deref() != Some(p.sha256.as_str()) {
            pin_changes.push(PinChange { reference: p.reference.clone(), path: p.path.clone(), pinned: p.sha256.clone(), current });
        }
    }
    summary.pins_changed = pin_changes.len();

    let reader = reader_data(&file, &manifest, &refs, &mut warnings);
    summary.resolve_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(Opened { project, file, rel, manifest, refs, summary, pin_changes, warnings, reader })
}

/// What the IDML reader needs: previews, link locations, saved lines, embedded files.
fn reader_data(file: &Path, m: &DocManifest, refs: &[RefState], warnings: &mut Vec<String>) -> ReaderData {
    let by_id: HashMap<&str, (&Reference, &RefState)> = m.references.iter().zip(refs).map(|(r, s)| (r.id.as_str(), (r, s))).collect();
    let mut data = ReaderData { parser: m.source.as_ref().map(|s| s.reader.clone()).filter(|s| !s.is_empty()), ..Default::default() };
    for g in &m.graphics {
        if let Some((_, st)) = g.preview.as_deref().and_then(|id| by_id.get(id))
            && let Some(f) = &st.file
        {
            data.previews.insert(g.self_id.clone(), f.to_string_lossy().into_owned());
        }
        if let Some((_, st)) = g.embedded.as_deref().and_then(|id| by_id.get(id))
            && let Some(f) = &st.file
        {
            data.embedded.insert(g.self_id.clone(), f.clone());
        }
        let (Some(link_self), Some((r, st))) = (&g.link_self, g.link.as_deref().and_then(|id| by_id.get(id))) else { continue };
        let modified_at_import = r.status.as_deref() == Some("modified");
        let o = match (&st.file, st.state) {
            // a file that was not the one InDesign placed: keep InDesign's record, so the
            // panel (and InDesign after export) shows it as modified
            (Some(f), "ok" | "moved") if modified_at_import => LinkOverride { uri: paths::file_uri(f), stamp: None },
            // the same content: the panel shows it as found and unchanged
            (Some(f), "ok" | "moved") => LinkOverride { uri: paths::file_uri(f), stamp: Some((st.size, st.mtime)) },
            // other content at the path: shown as modified
            (Some(f), _) => LinkOverride { uri: paths::file_uri(f), stamp: Some((r.bytes, Some(0.0))) },
            // not in the project: the original location, as a hint only
            (None, _) => LinkOverride { uri: r.hint.clone().unwrap_or_default(), stamp: None },
        };
        data.links.insert(link_self.clone(), o);
    }
    if let Some(part) = &m.saved_lines {
        match format::read_part(file, &part.part) {
            Ok(Some(bytes)) => match serde_json::from_slice(&bytes) {
                Ok(v) => data.saved_lines = Some(v),
                Err(e) => warnings.push(format!("saved lines: {e}")),
            },
            Ok(None) => warnings.push(format!("saved lines: part {} missing", part.part)),
            Err(e) => warnings.push(format!("saved lines: {e}")),
        }
    }
    data
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::format::{GraphicRefs, PartAction, SourceInfo};

    #[test]
    fn resolves_by_path_then_by_content() {
        let base = std::env::temp_dir().join(format!("folio-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let project = crate::manifest::create(&base.join("p"), "p").unwrap();
        let root = project.root.clone();
        std::fs::create_dir_all(root.join("assets/doc/previews")).unwrap();
        std::fs::write(root.join("assets/doc/a.jpg"), b"aaaa").unwrap();
        std::fs::write(root.join("assets/doc/b.jpg"), b"bbbb").unwrap();
        std::fs::write(root.join("assets/doc/previews/p.tif"), b"pppp").unwrap();
        let sha = |b: &[u8]| crate::hash::sha256_bytes(b);
        let r = |id: &str, path: Option<&str>, data: &[u8], hint: Option<&str>| Reference {
            id: id.into(),
            kind: if id.starts_with('p') { RefKind::Preview } else { RefKind::Link },
            path: path.map(|p| RelPath::parse(p).unwrap()),
            sha256: path.map(|_| sha(data)),
            bytes: path.map(|_| data.len() as u64),
            hint: hint.map(str::to_string),
            original_name: None,
            status: None,
            font: None,
            extra: Default::default(),
        };
        let m = DocManifest {
            format: format::FORMAT.into(),
            version: format::VERSION,
            id: "d1".into(),
            project: project.file.id.clone(),
            created: String::new(),
            saved: String::new(),
            engine: String::new(),
            source: Some(SourceInfo::default()),
            link_uris: "folio-ref".into(),
            references: vec![
                r("r1", Some("assets/doc/a.jpg"), b"aaaa", None),
                r("r2", Some("assets/doc/b.jpg"), b"BBBB", None),
                r("r3", Some("assets/old/c.jpg"), b"aaaa", None),
                r("r4", None, b"", Some("file:/Volumes/x/missing.pdf")),
                r("p1", Some("assets/doc/previews/p.tif"), b"pppp", None),
            ],
            graphics: vec![
                GraphicRefs { self_id: "g1".into(), folio_id: "f1".into(), frame: None, link_self: Some("l1".into()), link: Some("r1".into()), preview: Some("p1".into()), embedded: None },
                GraphicRefs { self_id: "g2".into(), folio_id: "f2".into(), frame: None, link_self: Some("l2".into()), link: Some("r2".into()), preview: None, embedded: None },
                GraphicRefs { self_id: "g3".into(), folio_id: "f3".into(), frame: None, link_self: Some("l3".into()), link: Some("r3".into()), preview: None, embedded: None },
                GraphicRefs { self_id: "g4".into(), folio_id: "f4".into(), frame: None, link_self: Some("l4".into()), link: Some("r4".into()), preview: None, embedded: None },
            ],
            pins: vec![],
            saved_lines: None,
            import_report: None,
            extra: Default::default(),
        };
        let idml = base.join("x.idml");
        crate::format::tests_support::tiny_idml(&idml);
        let doc = root.join("doc.folio");
        format::write_folio(&doc, &idml, &mut |_, _| Ok(PartAction::Keep), &m, None, None).unwrap();

        let o = resolve(&doc).unwrap();
        let state = |id: &str| o.state(id).unwrap().state;
        assert_eq!(state("r1"), "ok");
        assert_eq!(state("r2"), "changed");
        assert_eq!(state("r3"), "moved");
        assert_eq!(o.state("r3").unwrap().found_at.as_ref().unwrap().as_str(), "assets/doc/a.jpg");
        assert_eq!(state("r4"), "notInProject");
        assert_eq!(state("p1"), "ok");
        assert_eq!(o.rel.as_str(), "doc.folio");
        assert!(o.reader.previews["g1"].ends_with("p.tif"));
        assert!(o.reader.links["l1"].uri.starts_with("file:"));
        assert_eq!(o.reader.links["l2"].stamp, Some((Some(4), Some(0.0))));
        assert_eq!(o.reader.links["l4"].uri, "file:/Volumes/x/missing.pdf");
        assert_eq!(o.summary.links_in_project, 3);
        assert_eq!(o.summary.links_resolved, 3);
        // second open: hashes come from the index
        let again = resolve(&doc).unwrap();
        assert_eq!(again.summary.hashed_files, 0, "{:?}", again.summary);

        // the whole project copied elsewhere (another depth and name) resolves the same
        let moved = base.join("deeper").join("other name");
        std::fs::create_dir_all(&moved).unwrap();
        copy_dir(&root, &moved);
        let o2 = resolve(&moved.join("doc.folio")).unwrap();
        assert_eq!(o2.state("r1").unwrap().state, "ok");
        assert!(o2.reader.previews["g1"].starts_with(&*moved.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&base);
    }

    pub(crate) fn copy_dir(from: &Path, to: &Path) {
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let t = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                std::fs::create_dir_all(&t).unwrap();
                copy_dir(&e.path(), &t);
            } else {
                std::fs::copy(e.path(), t).unwrap();
            }
        }
    }
}
