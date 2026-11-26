//! File-name index of the search folders.
//!
//! A scan lists every folder below the roots once (in parallel, one level at a time) and
//! keeps each folder's listing with the folder's own modification time. A folder's time
//! changes when an entry is added, removed or renamed in it, so a later scan of the same
//! roots re-lists only folders whose time changed and reuses the rest; the validation
//! costs one `stat` per folder. Index entries are kept in memory per root set, and
//! optionally in a cache file (`FileIndex::scan_cached`).
//!
//! Only names are indexed. File contents and times are checked when a link is resolved and
//! probed. Folders that hold NAS / OS metadata (`@eaDir`, `#recycle`, …) are skipped, and
//! symbolic links / junctions to folders are not followed (cycles).

use crate::uri::name_key;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, UNIX_EPOCH};

/// Folder names never descended into.
pub const SKIPPED_FOLDERS: &[&str] = &[
    "@eaDir",
    "#recycle",
    "#snapshot",
    ".SynologyWorkingDirectory",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".Trashes",
    ".Spotlight-V100",
    ".fseventsd",
    ".git",
];

/// Hard bounds: deeper or larger trees are cut off (and counted in `IndexStats::truncated`).
const MAX_DEPTH: usize = 64;
const MAX_DIRS: usize = 2_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Listing {
    path: PathBuf,
    /// Folder modification time, ns since the Unix epoch; `None` when unreadable.
    mtime: Option<u128>,
    files: Vec<String>,
    subdirs: Vec<String>,
}

#[derive(Default)]
struct Data {
    roots: Vec<PathBuf>,
    dirs: Vec<Listing>,
    /// `name_key(dir path with '/')` per dir.
    dir_keys: Vec<String>,
    /// `name_key(file name)` → (dir, file) indices.
    by_name: HashMap<String, Vec<(u32, u32)>>,
    stats: IndexStats,
}

/// What a scan did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexStats {
    pub roots: usize,
    pub dirs: usize,
    pub files: usize,
    /// Folders read this time / reused from the previous scan.
    pub listed_dirs: usize,
    pub reused_dirs: usize,
    /// Folders that could not be read (permission, vanished).
    pub errors: usize,
    /// Depth or size bound reached.
    pub truncated: bool,
    pub elapsed_ms: f64,
    /// "scan" (nothing reusable), "memory" or "file" (previous listing reused).
    pub source: String,
}

/// Index of file names below a set of search roots. Cheap to clone.
#[derive(Clone, Default)]
pub struct FileIndex {
    data: Arc<Data>,
}

impl std::fmt::Debug for FileIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileIndex").field("roots", &self.data.roots).field("stats", &self.data.stats).finish()
    }
}

fn memory() -> &'static Mutex<HashMap<Vec<PathBuf>, Arc<Data>>> {
    static M: OnceLock<Mutex<HashMap<Vec<PathBuf>, Arc<Data>>>> = OnceLock::new();
    M.get_or_init(Mutex::default)
}

fn mtime_of(path: &Path) -> Option<u128> {
    let m = std::fs::metadata(path).ok()?;
    if !m.is_dir() {
        return None;
    }
    Some(m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos())
}

fn dir_key(path: &Path) -> String {
    name_key(&path.to_string_lossy().replace('\\', "/"))
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    roots: Vec<PathBuf>,
    dirs: Vec<Listing>,
}
const CACHE_VERSION: u32 = 1;

impl FileIndex {
    /// An index with no roots (only InDesign's own relink locations are searched).
    pub fn empty() -> FileIndex {
        FileIndex::default()
    }

    /// Indexes file names below `roots`. Repeated calls with the same roots in this process
    /// reuse unchanged folder listings (validated by folder modification time).
    pub fn scan(roots: &[PathBuf]) -> FileIndex {
        Self::scan_inner(roots, None)
    }

    /// Like [`scan`](Self::scan), and also keeps the listings in `cache_file` so that a
    /// new process starts from them. The file is rewritten only when something changed.
    pub fn scan_cached(roots: &[PathBuf], cache_file: &Path) -> FileIndex {
        Self::scan_inner(roots, Some(cache_file))
    }

    fn scan_inner(roots: &[PathBuf], cache_file: Option<&Path>) -> FileIndex {
        let mut roots: Vec<PathBuf> = roots.iter().map(|r| {
            let absolute = std::path::absolute(r).unwrap_or_else(|_| r.clone());
            crate::uri::resolve_native(&absolute).unwrap_or(absolute)
        }).collect();
        let mut seen = std::collections::HashSet::new();
        roots.retain(|r| seen.insert(std::fs::canonicalize(r).unwrap_or_else(|_| r.clone())));
        let started = Instant::now();
        let mut previous = memory().lock().ok().and_then(|m| m.get(&roots).cloned());
        let mut source = if previous.is_some() { "memory" } else { "scan" };
        if previous.is_none()
            && let Some(file) = cache_file
            && let Ok(bytes) = std::fs::read(file)
            && let Ok(c) = serde_json::from_slice::<CacheFile>(&bytes)
            && c.version == CACHE_VERSION
            && c.roots == roots
        {
            previous = Some(Arc::new(Data { roots: c.roots, dirs: c.dirs, ..Data::default() }));
            source = "file";
        }
        let mut data = build(&roots, previous.as_deref());
        let changed = data.stats.listed_dirs > 0 || previous.as_ref().is_none_or(|p| p.dirs.len() != data.dirs.len());
        // Nothing changed: keep the previous name tables (same listings, same order).
        if !changed
            && let Some(p) = previous.as_ref()
            && !p.by_name.is_empty()
        {
            data.dir_keys = p.dir_keys.clone();
            data.by_name = p.by_name.clone();
        } else {
            index_names(&mut data);
        }
        data.stats.source = source.into();
        data.stats.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        if let Some(file) = cache_file
            && (changed || source != "file" && !file.exists())
        {
            let c = CacheFile { version: CACHE_VERSION, roots: roots.clone(), dirs: data.dirs.clone() };
            if let Ok(bytes) = serde_json::to_vec(&c) {
                let tmp = file.with_extension(format!("tmp{}", std::process::id()));
                if let Some(parent) = file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, file).is_err() {
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
        let data = Arc::new(data);
        if let Ok(mut m) = memory().lock() {
            m.insert(roots, data.clone());
        }
        FileIndex { data }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.data.roots
    }

    pub fn stats(&self) -> &IndexStats {
        &self.data.stats
    }

    pub fn len(&self) -> usize {
        self.data.stats.files
    }

    pub fn is_empty(&self) -> bool {
        self.data.stats.files == 0
    }

    /// Files named `name` (NFC/NFD- and case-insensitive), in index order.
    pub fn candidates(&self, name: &str) -> Vec<PathBuf> {
        let d = &self.data;
        d.by_name
            .get(&name_key(name))
            .map(|v| v.iter().map(|&(di, fi)| d.dirs[di as usize].path.join(&d.dirs[di as usize].files[fi as usize])).collect())
            .unwrap_or_default()
    }

    /// Whether `path` lies below one of the roots (so the index knows whether it exists).
    pub fn covers(&self, path: &Path) -> bool {
        let k = dir_key(path);
        self.data.roots.iter().any(|r| {
            let rk = dir_key(r);
            k.len() > rk.len() && k.starts_with(&rk) && k[rk.len()..].starts_with('/')
        })
    }

    /// The indexed file at `path` (matched NFC/NFD- and case-insensitively), as spelled on
    /// disk. Only meaningful when [`covers`](Self::covers) is true.
    pub fn lookup(&self, path: &Path) -> Option<PathBuf> {
        let name = path.file_name()?.to_string_lossy();
        let parent = dir_key(path.parent()?);
        let d = &self.data;
        let matches: Vec<_> = d.by_name.get(&name_key(&name))?.iter().filter(|&&(di, _)| d.dir_keys[di as usize] == parent).map(|&(di, fi)| {
            let l = &d.dirs[di as usize];
            l.path.join(&l.files[fi as usize])
        }).collect();
        matches.iter().find(|p| p.as_path() == path).cloned()
            .or_else(|| (matches.len() == 1).then(|| matches[0].clone()))
    }
}

/// Lists `path` (or reuses `prev` when the folder time is unchanged).
fn visit(path: &Path, prev: Option<&Listing>) -> (Listing, bool, bool) {
    let mtime = mtime_of(path);
    if let (Some(p), Some(t)) = (prev, mtime)
        && p.mtime == Some(t)
    {
        return (p.clone(), true, false);
    }
    let mut l = Listing { path: path.to_path_buf(), mtime, files: Vec::new(), subdirs: Vec::new() };
    let Ok(rd) = std::fs::read_dir(path) else {
        return (l, false, true);
    };
    for e in rd.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if t.is_dir() {
            if !SKIPPED_FOLDERS.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                l.subdirs.push(name);
            }
        } else if t.is_file() || t.is_symlink() {
            // A symbolic link may point at a file; it is indexed by its own name.
            l.files.push(name);
        }
    }
    (l, false, false)
}

fn build(roots: &[PathBuf], previous: Option<&Data>) -> Data {
    let prev: HashMap<&Path, &Listing> = previous.map(|p| p.dirs.iter().map(|l| (l.path.as_path(), l)).collect()).unwrap_or_default();
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(1, 8)).unwrap_or(4);
    let mut stats = IndexStats { roots: roots.len(), ..IndexStats::default() };
    let mut dirs: Vec<Listing> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut level: Vec<PathBuf> = roots.iter().filter(|r| seen.insert((*r).clone())).cloned().collect();
    let mut depth = 0;
    while !level.is_empty() {
        if depth > MAX_DEPTH || dirs.len() + level.len() > MAX_DIRS {
            stats.truncated = true;
            break;
        }
        let chunk = level.len().div_ceil(threads).max(1);
        let results: Vec<Vec<(Listing, bool, bool)>> = std::thread::scope(|s| {
            let handles: Vec<_> = level
                .chunks(chunk)
                .map(|c| {
                    let prev = &prev;
                    s.spawn(move || c.iter().map(|p| visit(p, prev.get(p.as_path()).copied())).collect::<Vec<_>>())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
        });
        let mut next = Vec::new();
        for (l, reused, error) in results.into_iter().flatten() {
            if reused {
                stats.reused_dirs += 1;
            } else if error {
                stats.errors += 1;
            } else {
                stats.listed_dirs += 1;
            }
            for s in &l.subdirs {
                let child = l.path.join(s);
                if seen.insert(child.clone()) {
                    next.push(child);
                }
            }
            dirs.push(l);
        }
        level = next;
        depth += 1;
    }
    stats.dirs = dirs.len();
    stats.files = dirs.iter().map(|d| d.files.len()).sum();
    Data { roots: roots.to_vec(), dirs, dir_keys: Vec::new(), by_name: HashMap::new(), stats }
}

/// Name tables of `data.dirs`.
fn index_names(data: &mut Data) {
    data.dir_keys = data.dirs.iter().map(|l| dir_key(&l.path)).collect();
    let mut by_name: HashMap<String, Vec<(u32, u32)>> = HashMap::with_capacity(data.stats.files);
    for (di, l) in data.dirs.iter().enumerate() {
        for (fi, f) in l.files.iter().enumerate() {
            by_name.entry(name_key(f)).or_default().push((di as u32, fi as u32));
        }
    }
    data.by_name = by_name;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("folio-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rescans_only_changed_folders() {
        let root = tmp("index");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join("@eaDir")).unwrap();
        std::fs::write(root.join("a/x.jpg"), b"1").unwrap();
        std::fs::write(root.join("a/b/X.JPG"), b"2").unwrap();
        std::fs::write(root.join("@eaDir/x.jpg"), b"3").unwrap();
        let i = FileIndex::scan(std::slice::from_ref(&root));
        assert_eq!(i.stats().files, 2);
        assert_eq!(i.candidates("x.jpg").len(), 2);
        assert!(i.covers(&root.join("a/x.jpg")));
        assert!(i.lookup(&root.join("A/X.jpg")).is_some());
        let again = FileIndex::scan(std::slice::from_ref(&root));
        assert_eq!((again.stats().listed_dirs, again.stats().source.as_str()), (0, "memory"));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("a/b/new.png"), b"4").unwrap();
        let after = FileIndex::scan(std::slice::from_ref(&root));
        assert_eq!(after.stats().listed_dirs, 1, "{:?}", after.stats());
        assert_eq!(after.candidates("NEW.png").len(), 1);
        // a cache file serves a new process (simulated by a different root spelling key)
        let file = root.join("index.json");
        let first = FileIndex::scan_cached(std::slice::from_ref(&root), &file);
        assert!(file.exists());
        assert_eq!(first.stats().files, 3);
        let _ = std::fs::remove_dir_all(&root);
    }
}
