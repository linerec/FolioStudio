//! Stages and places import files, hashes their contents and rolls back failed imports.

use super::names::Names;
use super::{Error, Plan, Src, io};
use crate::hash::{self, HashIndex};
use crate::paths::RelPath;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

/// File extension for data by its first bytes (names of extracted files).
pub fn sniff_ext(head: &[u8]) -> &'static str {
    if head.starts_with(&[0xFF, 0xD8]) {
        ".jpg"
    } else if head.starts_with(b"\x89PNG") {
        ".png"
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        ".tif"
    } else if head.starts_with(b"8BPS") {
        ".psd"
    } else if head.starts_with(b"%PDF") {
        ".pdf"
    } else if head.starts_with(b"%!PS") || head.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        ".eps"
    } else if head.starts_with(b"GIF8") {
        ".gif"
    } else if head.starts_with(b"BM") {
        ".bmp"
    } else if head.starts_with(b"<?xml") || head.starts_with(b"<svg") {
        ".svg"
    } else {
        ".raw"
    }
}

fn head_of(p: &Path) -> Vec<u8> {
    let mut b = [0u8; 16];
    let n = std::fs::File::open(p).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
    b[..n].to_vec()
}

/// Copies `src` to `dst` (new file) hashing on the way, keeping the modification time (so
/// InDesign's link stamp still matches after export). Returns SHA-256 and size.
fn copy_hashed(src: &Path, dst: &Path, cancel: &AtomicBool, done: &dyn Fn(u64)) -> std::io::Result<(String, u64)> {
    use sha2::{Digest, Sha256};
    let mut input = std::fs::File::open(src)?;
    let meta = input.metadata()?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
    let r = (|| -> std::io::Result<(String, u64)> {
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 4 << 20];
        let mut n = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
            }
            let k = input.read(&mut buf)?;
            if k == 0 {
                break;
            }
            h.update(&buf[..k]);
            out.write_all(&buf[..k])?;
            n += k as u64;
            done(k as u64);
        }
        out.sync_all()?;
        if let Ok(t) = meta.modified() {
            let _ = out.set_modified(t);
        }
        Ok((crate::util::hex(&h.finalize()), n))
    })();
    if r.is_err() {
        drop(out);
        let _ = std::fs::remove_file(dst);
    }
    r
}

/// Moves (renames) when possible, else copies; returns SHA-256 and size of the result.
fn move_hashed(src: &Path, dst: &Path, cancel: &AtomicBool, done: &dyn Fn(u64)) -> std::io::Result<(String, u64)> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::symlink_metadata(dst).is_err() && std::fs::rename(src, dst).is_ok() {
        let r = hash::sha256_file_cancellable(dst, cancel)?;
        done(r.1);
        return Ok(r);
    }
    copy_hashed(src, dst, cancel, done)
}

/// Every regular file of the project by size (for finding content already there).
fn project_sizes(root: &Path) -> HashMap<u64, Vec<RelPath>> {
    let mut out: HashMap<u64, Vec<RelPath>> = HashMap::new();
    hash::walk_files(root, &mut |rel, m| out.entry(m.len()).or_default().push(rel));
    out
}

/// Undoes a failed or cancelled import: placed files, then new folders (when empty),
/// then the staging folder.
pub(super) struct Cleanup {
    pub(super) staging: PathBuf,
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    pub(super) done: bool,
}

impl Cleanup {
    pub(super) fn new(staging: PathBuf) -> Self {
        Self { staging, files: Vec::new(), dirs: Vec::new(), done: false }
    }

    /// Creates `dir` and the folders above it inside `root`, remembering new ones (outermost
    /// first) for rollback.
    pub(super) fn create_dirs(&mut self, root: &Path, dir: &Path) -> Result<(), Error> {
        let mut missing = Vec::new();
        let mut cur = dir;
        while cur.starts_with(root) && cur != root && std::fs::symlink_metadata(cur).is_err() {
            missing.push(cur.to_path_buf());
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        std::fs::create_dir_all(dir).map_err(io)?;
        self.dirs.extend(missing.into_iter().rev());
        Ok(())
    }

    pub(super) fn place(&mut self, root: &Path, placed: &[Placed], index: &mut HashIndex) -> Result<(), Error> {
        for pl in placed {
            let (Some(staged), Some(rel)) = (&pl.staged, &pl.path) else { continue };
            let dest = rel.on_disk(root);
            if let Some(parent) = dest.parent() {
                self.create_dirs(root, parent)?;
            }
            if std::fs::symlink_metadata(&dest).is_ok() {
                return Err(Error::new("io", format!("프로젝트에 같은 이름의 파일이 이미 생겼습니다: {rel}")));
            }
            std::fs::rename(staged, &dest).map_err(io)?;
            self.files.push(dest.clone());
            if let (Ok(m), Some(sha)) = (std::fs::metadata(&dest), &pl.sha) {
                index.record(rel, &dest, &m, sha);
            }
        }
        Ok(())
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if !self.done {
            for f in self.files.iter().rev() {
                let _ = std::fs::remove_file(f);
            }
            for d in self.dirs.iter().rev() {
                let _ = std::fs::remove_dir(d);
            }
        }
        let _ = std::fs::remove_dir_all(&self.staging);
    }
}

pub(super) struct Placed {
    pub(super) sha: Option<String>,
    pub(super) bytes: Option<u64>,
    pub(super) path: Option<RelPath>,
    pub(super) staged: Option<PathBuf>,
}

pub(super) struct Staged {
    pub(super) placed: Vec<Placed>,
    pub(super) index: HashIndex,
    pub(super) dedup: usize,
    pub(super) copy_ms: f64,
}

pub(super) fn stage(
    root: &Path,
    staging: &Path,
    plan: &Plan,
    cancel: &AtomicBool,
    add: &dyn Fn(u64),
    files: &AtomicUsize,
) -> Result<Staged, Error> {
    let mut index = HashIndex::load(root);
    let sizes = project_sizes(root);
    // (sha, bytes) → final reference path
    let mut by_content: HashMap<(String, u64), RelPath> = HashMap::new();
    let mut placed: Vec<Placed> = Vec::with_capacity(plan.items.len());
    let mut dedup = 0usize;
    let mut names = Names::default();
    // reader files already moved into staging by an earlier item (a preview that is also
    // a graphic's embedded file): later items share what the first one placed
    let mut moved: HashMap<PathBuf, usize> = HashMap::new();
    let t_copy = Instant::now();
    for it in &plan.items {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::new("cancelled", "가져오기를 취소했습니다"));
        }
        let mut pl = Placed { sha: None, bytes: None, path: None, staged: None };
        match &it.src {
            Src::None => {
                if let Some(path) = &it.hash_only
                    && let Ok((sha, n)) = hash::sha256_file_cancellable(path, cancel)
                {
                    pl.sha = Some(sha);
                    pl.bytes = Some(n);
                    add(n);
                    files.fetch_add(1, Ordering::Relaxed);
                }
            }
            Src::Keep(rel) => {
                let (sha, n) = index.hash(rel, &rel.on_disk(root)).map_err(io)?;
                add(n);
                files.fetch_add(1, Ordering::Relaxed);
                by_content.entry((sha.clone(), n)).or_insert_with(|| rel.clone());
                pl = Placed { sha: Some(sha), bytes: Some(n), path: Some(rel.clone()), staged: None };
            }
            Src::Move(src) if moved.contains_key(src) => {
                let first = &placed[moved[src]];
                dedup += 1;
                files.fetch_add(1, Ordering::Relaxed);
                pl = Placed { sha: first.sha.clone(), bytes: first.bytes, path: first.path.clone(), staged: None };
            }
            Src::Copy(src) | Src::Move(src) => {
                if matches!(it.src, Src::Move(_)) {
                    moved.insert(src.clone(), placed.len());
                }
                let tmp = staging.join(format!("f{}", placed.len()));
                let (sha, n) = match &it.src {
                    Src::Copy(_) => copy_hashed(src, &tmp, cancel, add),
                    _ => move_hashed(src, &tmp, cancel, add),
                }
                .map_err(|e| {
                    let mut err = io(e);
                    if err.code == "io" {
                        err.message = format!("{}: {}", src.display(), err.message);
                    }
                    err
                })?;
                files.fetch_add(1, Ordering::Relaxed);
                // the same content already in this import, or already in the project
                let existing = by_content.get(&(sha.clone(), n)).cloned().or_else(|| {
                    sizes.get(&n).and_then(|cands| cands.iter().find(|c| index.hash(c, &c.on_disk(root)).is_ok_and(|(s, _)| s == sha)).cloned())
                });
                if let Some(rel) = existing {
                    dedup += 1;
                    let _ = std::fs::remove_file(&tmp);
                    by_content.insert((sha.clone(), n), rel.clone());
                    pl = Placed { sha: Some(sha), bytes: Some(n), path: Some(rel), staged: None };
                } else {
                    let dest = if it.hash_named {
                        let dir = it.dest.clone().unwrap_or_else(|| plan.assets.clone());
                        let name = format!("{}{}", &sha[..16], sniff_ext(&head_of(&tmp)));
                        let (n2, _) = names.unique(root, &dir, &name);
                        dir.join(&n2).unwrap()
                    } else {
                        it.dest.clone().unwrap_or_else(|| plan.assets.join(&sha[..16]).expect("SHA-256 prefix is a valid ASCII child name"))
                    };
                    by_content.insert((sha.clone(), n), dest.clone());
                    pl = Placed { sha: Some(sha), bytes: Some(n), path: Some(dest), staged: Some(tmp) };
                }
            }
        }
        placed.push(pl);
    }
    let copy_ms = t_copy.elapsed().as_secs_f64() * 1000.0;
    Ok(Staged { placed, index, dedup, copy_ms })
}
