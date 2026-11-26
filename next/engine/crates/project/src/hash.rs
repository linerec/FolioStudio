//! Content identity: SHA-256 and size decide whether two files are the same; modification
//! times never do.
//!
//! [`HashIndex`] is a regenerable cache in `.folio/cache/hash-index.json` (root-relative
//! path → size, modification time, SHA-256) so unchanged files are not hashed again: an entry
//! is reused only when both size and time still match, otherwise the file is read. Losing or
//! deleting the cache costs time, never correctness.

use crate::paths::RelPath;
use crate::util::{hex, mtime_ns};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const INDEX_FILE: &str = "hash-index.json";
const INDEX_FORMAT: &str = "folio-hash-index";
const INDEX_VERSION: u32 = 1;
const BUFFER: usize = 1 << 20;

/// SHA-256 (lower-case hex) and size of a file.
pub fn sha256_file(path: &Path) -> std::io::Result<(String, u64)> {
    sha256_file_cancellable(path, &AtomicBool::new(false))
}

pub fn sha256_file_cancellable(path: &Path, cancel: &AtomicBool) -> std::io::Result<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; BUFFER];
    let mut n = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        let k = f.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((hex(&h.finalize()), n))
}

pub fn sha256_bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub size: u64,
    /// Nanoseconds since the epoch, as a string (JSON numbers lose precision).
    pub mtime: String,
    pub sha256: String,
    /// Actual root-relative disk spelling. Prevents two distinct NFC/NFD entries on
    /// NTFS with identical size/mtime from sharing a cached content hash.
    #[serde(default)]
    pub spelling: String,
}

#[derive(Serialize, Deserialize)]
struct IndexFile {
    format: String,
    version: u32,
    entries: BTreeMap<String, Entry>,
}

/// Counters of one session's use of the index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub hits: usize,
    pub hashed: usize,
    pub hashed_bytes: u64,
}

/// The project's hash cache.
pub struct HashIndex {
    root: PathBuf,
    entries: BTreeMap<String, Entry>,
    dirty: bool,
    pub stats: IndexStats,
}

impl HashIndex {
    fn file(root: &Path) -> PathBuf {
        root.join(".folio").join("cache").join(INDEX_FILE)
    }

    /// The index of the project at `root` (empty when there is none or it is unreadable).
    pub fn load(root: &Path) -> HashIndex {
        let raw: BTreeMap<String, Entry> = std::fs::read_to_string(Self::file(root))
            .ok()
            .and_then(|t| serde_json::from_str::<IndexFile>(&t).ok())
            .filter(|f| f.format == INDEX_FORMAT && f.version == INDEX_VERSION)
            .map(|f| f.entries)
            .unwrap_or_default();
        let mut entries = BTreeMap::new();
        let mut collisions = std::collections::BTreeSet::new();
        let mut dirty = false;
        for (key, value) in raw {
            let Ok(rel) = RelPath::parse(&key) else { dirty = true; continue };
            dirty |= key != rel.as_str();
            if entries.insert(rel.to_string(), value).is_some() {
                collisions.insert(rel.to_string());
                dirty = true;
            }
        }
        for key in collisions { entries.remove(&key); }
        HashIndex { root: root.to_path_buf(), entries, dirty, stats: IndexStats::default() }
    }

    /// The cached hash of `rel` when its size and time still match the file at `abs`.
    pub fn cached(&self, rel: &RelPath, abs: &Path, meta: &std::fs::Metadata) -> Option<&Entry> {
        let e = self.entries.get(rel.as_str())?;
        (e.spelling == self.spelling(abs) && e.size == meta.len() && mtime_ns(meta).map(|t| t.to_string()).as_deref() == Some(e.mtime.as_str())).then_some(e)
    }

    /// SHA-256 and size of the project file `rel` (at `abs` on disk), from the cache or read.
    pub fn hash(&mut self, rel: &RelPath, abs: &Path) -> std::io::Result<(String, u64)> {
        let meta = std::fs::metadata(abs)?;
        if let Some((sha, size)) = self.cached(rel, abs, &meta).map(|e| (e.sha256.clone(), e.size)) {
            self.stats.hits += 1;
            return Ok((sha, size));
        }
        let (sha, size) = sha256_file(abs)?;
        self.stats.hashed += 1;
        self.stats.hashed_bytes += size;
        // the time after reading: a file changed while it was read is hashed again next time
        let after = std::fs::metadata(abs)?;
        if after.len() == size {
            self.record(rel, abs, &after, &sha);
        }
        Ok((sha, size))
    }

    /// Records a hash computed elsewhere (a copy that hashed on the way).
    pub fn record(&mut self, rel: &RelPath, abs: &Path, meta: &std::fs::Metadata, sha256: &str) {
        let Some(t) = mtime_ns(meta) else { return };
        self.entries.insert(rel.as_str().to_string(), Entry { size: meta.len(), mtime: t.to_string(), sha256: sha256.to_string(), spelling: self.spelling(abs) });
        self.dirty = true;
    }

    fn spelling(&self, abs: &Path) -> String {
        abs.strip_prefix(&self.root).unwrap_or(abs).to_string_lossy().replace('\\', "/")
    }

    pub fn forget(&mut self, rel: &RelPath) {
        if self.entries.remove(rel.as_str()).is_some() {
            self.dirty = true;
        }
    }

    /// A project file with this content (`size` bytes, `sha256`), outside `.folio/`: cached
    /// entries first, then every file of that size not in the cache is hashed.
    pub fn find_by_hash(&mut self, sha256: &str, size: u64) -> Option<RelPath> {
        let mut known: Vec<String> = self.entries.iter().filter(|(_, e)| e.sha256 == sha256 && e.size == size).map(|(k, _)| k.clone()).collect();
        known.sort();
        for k in known {
            if let Ok(rel) = RelPath::parse(&k) {
                let abs = rel.on_disk(&self.root);
                if let Ok(m) = std::fs::metadata(&abs)
                    && self.cached(&rel, &abs, &m).is_some()
                {
                    return Some(rel);
                }
            }
        }
        let mut candidates = Vec::new();
        walk_files(&self.root, &mut |rel, meta| {
            if meta.len() == size {
                candidates.push(rel);
            }
        });
        candidates.sort();
        for rel in candidates {
            let abs = rel.on_disk(&self.root);
            if let Ok((sha, _)) = self.hash(&rel, &abs)
                && sha == sha256
            {
                return Some(rel);
            }
        }
        None
    }

    /// Writes the cache when it changed (atomically; a failure only costs time later).
    pub fn save(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        // drop entries of files that are gone
        let root = self.root.clone();
        self.entries.retain(|k, _| RelPath::parse(k).is_ok_and(|r| r.on_disk(&root).is_file()));
        let file = Self::file(&self.root);
        std::fs::create_dir_all(file.parent().unwrap_or(&self.root))?;
        let body = IndexFile { format: INDEX_FORMAT.into(), version: INDEX_VERSION, entries: self.entries.clone() };
        crate::atomic::write_bytes(&file, &serde_json::to_vec(&body).map_err(std::io::Error::other)?)?;
        self.dirty = false;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Every regular file of the project below `root` with its reference, skipping `.folio/`,
/// names that are not portable and symbolic links / junctions (not followed).
pub fn walk_files(root: &Path, f: &mut dyn FnMut(RelPath, &std::fs::Metadata)) {
    fn go(dir: &Path, names: &mut Vec<String>, f: &mut dyn FnMut(RelPath, &std::fs::Metadata), depth: usize) {
        if depth > 64 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if names.is_empty() && name == crate::manifest::DATA_DIR {
                continue;
            }
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() {
                continue;
            }
            names.push(name);
            if t.is_dir() {
                go(&e.path(), names, f, depth + 1);
            } else if t.is_file()
                && let Ok(rel) = RelPath::from_names(names)
                && let Ok(m) = e.metadata()
            {
                f(rel, &m);
            }
            names.pop();
        }
    }
    go(root, &mut Vec::new(), f, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposed_paths_cache_move_and_distinct_files() {
        use unicode_normalization::UnicodeNormalization;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).unwrap().join("artifacts/mac-i18n-20261020/unit-tests").join(format!("folio-hash-nfd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let disk = root.join("그림".nfd().collect::<String>()).join("사진.png".nfd().collect::<String>());
        std::fs::create_dir_all(disk.parent().unwrap()).unwrap();
        std::fs::write(&disk, b"photo").unwrap();
        let rel = RelPath::parse("그림/사진.png").unwrap();
        let sha = sha256_bytes(b"photo");
        let mut index = HashIndex::load(&root);
        assert_eq!(index.hash(&rel, &disk).unwrap().0, sha);
        index.save().unwrap();
        let mut again = HashIndex::load(&root);
        assert_eq!(again.len(), 1, "NFD files must survive cache cleanup");
        assert_eq!(again.find_by_hash(&sha, 5), Some(rel.clone()));
        again.hash(&rel, &disk).unwrap();
        assert_eq!(again.stats.hits, 1);
        // A synced cache using NFD keys loads into the same canonical namespace.
        let cache = HashIndex::file(&root);
        let raw = std::fs::read_to_string(&cache).unwrap().replace(rel.as_str(), &rel.as_str().nfd().collect::<String>());
        std::fs::write(&cache, raw).unwrap();
        let mut migrated = HashIndex::load(&root);
        assert_eq!(migrated.find_by_hash(&sha, 5), Some(rel.clone()));
        migrated.save().unwrap();
        assert!(std::fs::read_to_string(cache).unwrap().contains(rel.as_str()));
        // NTFS and case-sensitive Unix can hold distinct normalisation-colliding files.
        // Never substitute one cached hash for another, even with identical size/time.
        let other = root.join("그림").join("사진.png");
        if !other.exists() {
            std::fs::create_dir_all(other.parent().unwrap()).unwrap();
            std::fs::write(&other, b"other").unwrap();
            std::fs::File::options().write(true).open(&other).unwrap()
                .set_modified(std::fs::metadata(&disk).unwrap().modified().unwrap()).unwrap();
            assert_eq!(migrated.hash(&rel, &other).unwrap().0, sha256_bytes(b"other"));
        }
        // A cache miss still discovers a decomposed name by its content.
        let mut empty = HashIndex { root: root.clone(), entries: BTreeMap::new(), dirty: false, stats: IndexStats::default() };
        let wanted = if other.exists() { sha256_bytes(&std::fs::read(&other).unwrap()) } else { sha };
        assert_eq!(empty.find_by_hash(&wanted, 5), Some(rel));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_reuses_unchanged_files_and_finds_moved_ones() {
        let root = std::env::temp_dir().join(format!("folio-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("assets/a")).unwrap();
        std::fs::write(root.join("assets/a/x.jpg"), b"hello").unwrap();
        std::fs::write(root.join("assets/a/y.jpg"), b"world").unwrap();
        let (sha, n) = sha256_file(&root.join("assets/a/x.jpg")).unwrap();
        assert_eq!(n, 5);
        assert_eq!(sha, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(sha256_bytes(b"hello"), sha);

        let x = RelPath::parse("assets/a/x.jpg").unwrap();
        let mut idx = HashIndex::load(&root);
        assert_eq!(idx.hash(&x, &x.to_native(&root)).unwrap().0, sha);
        assert_eq!(idx.stats.hashed, 1);
        assert_eq!(idx.hash(&x, &x.to_native(&root)).unwrap().0, sha);
        assert_eq!(idx.stats.hits, 1);
        idx.save().unwrap();
        assert!(root.join(".folio/cache").join(INDEX_FILE).is_file());

        // a fresh process reuses the saved entries
        let mut again = HashIndex::load(&root);
        again.hash(&x, &x.to_native(&root)).unwrap();
        assert_eq!((again.stats.hits, again.stats.hashed), (1, 0));

        // the same size but other content is read again, never trusted by time
        std::fs::write(root.join("assets/a/x.jpg"), b"HELLO").unwrap();
        let changed = again.hash(&x, &x.to_native(&root)).unwrap().0;
        assert_ne!(changed, sha);

        // a moved file is found by its content
        std::fs::create_dir_all(root.join("elsewhere")).unwrap();
        std::fs::write(root.join("elsewhere/moved.jpg"), b"hello").unwrap();
        assert_eq!(again.find_by_hash(&sha, 5).unwrap().as_str(), "elsewhere/moved.jpg");
        assert_eq!(again.find_by_hash(&sha256_bytes(b"nothing"), 7), None);
        // files under .folio/ are never candidates
        std::fs::write(root.join(".folio/cache/dup.jpg"), b"world").unwrap();
        assert_eq!(again.find_by_hash(&sha256_bytes(b"world"), 5).unwrap().as_str(), "assets/a/y.jpg");
        let _ = std::fs::remove_dir_all(&root);
    }
}
