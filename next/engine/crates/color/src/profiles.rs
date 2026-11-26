//! ICC profiles installed on this machine, found by their description.
//!
//! InDesign names a document's profiles by ICC description ('desc' text, or each record of
//! a v4 'mluc'), not by file name. The index reads only each file's header, tag table and
//! description tag. Search order (first match wins):
//!
//! 1. `%WINDIR%\System32\spool\drivers\color`
//! 2. `%ProgramFiles(x86)%\Common Files\Adobe\Color\Profiles` (with `Recommended`)
//! 3. `%ProgramFiles%\Common Files\Adobe\Color\Profiles`
//! 4. `%APPDATA%\Adobe\Color\Profiles`
//! 5. the document's own folder (see [`ProfileIndex::find_for_document`])
//!
//! Folders are searched up to [`DEPTH`] levels deep; only `.icc` / `.icm` files with the
//! ICC signature count.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Folder depth searched below each root (the root itself is depth 0).
pub const DEPTH: usize = 3;
/// Largest profile file read whole (the format allows more; real profiles are < 10 MB).
pub const MAX_PROFILE_BYTES: u64 = 32 * 1024 * 1024;
/// Largest description tag read.
const MAX_DESC_BYTES: u32 = 64 * 1024;

/// One profile file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEntry {
    pub path: PathBuf,
    /// Every description the file carries: 'desc' ASCII and Unicode text, or each 'mluc'
    /// record. The first is the primary one.
    pub names: Vec<String>,
    /// Data colour space signature, e.g. `"CMYK"`, `"RGB "`.
    pub space: String,
    /// Profile class signature, e.g. `"prtr"`, `"mntr"`.
    pub class: String,
    pub bytes: u64,
}

/// Profiles by description.
#[derive(Clone, Debug, Default)]
pub struct ProfileIndex {
    entries: Vec<ProfileEntry>,
    by_name: HashMap<String, usize>,
    by_folded: HashMap<String, usize>,
    roots: Vec<PathBuf>,
    scan_ms: f64,
}

impl ProfileIndex {
    /// The installed profile folders, in search order (existing or not).
    pub fn installed_roots() -> Vec<PathBuf> {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let mut roots = Vec::new();
        if let Some(w) = env("WINDIR").or_else(|| env("SystemRoot")) {
            roots.push(w.join("System32").join("spool").join("drivers").join("color"));
        }
        let adobe = |base: PathBuf| base.join("Common Files").join("Adobe").join("Color").join("Profiles");
        if let Some(p) = env("ProgramFiles(x86)") {
            roots.push(adobe(p));
        }
        if let Some(p) = env("ProgramFiles") {
            roots.push(adobe(p));
        }
        if let Some(p) = env("APPDATA") {
            roots.push(p.join("Adobe").join("Color").join("Profiles"));
        }
        #[cfg(target_os = "macos")]
        {
            roots.push(PathBuf::from("/System/Library/ColorSync/Profiles"));
            roots.push(PathBuf::from("/Library/ColorSync/Profiles"));
            if let Some(home) = env("HOME") {
                roots.push(home.join("Library/ColorSync/Profiles"));
            }
        }
        #[cfg(target_os = "linux")]
        {
            roots.push(PathBuf::from("/usr/share/color/icc"));
            roots.push(PathBuf::from("/usr/local/share/color/icc"));
            if let Some(home) = env("HOME") {
                roots.push(home.join(".local/share/color/icc"));
            }
        }
        let mut seen = std::collections::HashSet::new();
        roots.retain(|r| seen.insert(r.to_string_lossy().to_lowercase()));
        roots
    }

    /// Indexes the installed profile folders.
    pub fn scan_installed() -> ProfileIndex {
        ProfileIndex::scan(&ProfileIndex::installed_roots())
    }

    /// Indexes the given folders (first root wins for a description found twice).
    pub fn scan(roots: &[PathBuf]) -> ProfileIndex {
        let started = Instant::now();
        let mut index = ProfileIndex { roots: roots.to_vec(), ..ProfileIndex::default() };
        for root in roots {
            let mut files = Vec::new();
            profile_files(root, 0, &mut files);
            files.sort();
            for f in files {
                if let Some(e) = describe(&f) {
                    index.push(e);
                }
            }
        }
        index.scan_ms = started.elapsed().as_secs_f64() * 1000.0;
        index
    }

    fn push(&mut self, e: ProfileEntry) {
        let i = self.entries.len();
        for n in &e.names {
            self.by_name.entry(n.clone()).or_insert(i);
            self.by_folded.entry(fold(n)).or_insert(i);
        }
        self.entries.push(e);
    }

    /// The installed profile whose description is `description` (exact, else ignoring case
    /// and surrounding white space).
    pub fn find(&self, description: &str) -> Option<PathBuf> {
        self.find_entry(description).map(|e| e.path.clone())
    }

    pub fn find_entry(&self, description: &str) -> Option<&ProfileEntry> {
        let i = self.by_name.get(description.trim()).or_else(|| self.by_folded.get(&fold(description)))?;
        self.entries.get(*i)
    }

    /// [`find`](Self::find), then the profiles in the document's folder (not indexed:
    /// read on each call).
    pub fn find_for_document(&self, description: &str, document_dir: Option<&Path>) -> Option<PathBuf> {
        if let Some(p) = self.find(description) {
            return Some(p);
        }
        let dir = document_dir?;
        let mut files = Vec::new();
        profile_files(dir, DEPTH, &mut files);
        files.sort();
        let folded = fold(description);
        files.into_iter().find(|f| describe(f).is_some_and(|e| e.names.iter().any(|n| n == description.trim() || fold(n) == folded)))
    }

    pub fn entries(&self) -> &[ProfileEntry] {
        &self.entries
    }

    /// The folders scanned, in search order.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Time the scan took (ms).
    pub fn scan_ms(&self) -> f64 {
        self.scan_ms
    }
}

fn fold(s: &str) -> String {
    s.trim().to_lowercase()
}

fn is_profile_name(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("icc") || e.eq_ignore_ascii_case("icm"))
}

/// Profile files below `dir` down to `DEPTH` (from `depth`).
fn profile_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let p = e.path();
        if t.is_dir() {
            subdirs.push(p);
        } else if t.is_file() && is_profile_name(&p) {
            out.push(p);
        }
    }
    if depth < DEPTH {
        subdirs.sort();
        for d in subdirs {
            profile_files(&d, depth + 1, out);
        }
    }
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn sig(b: &[u8]) -> String {
    b.iter().map(|&c| if c.is_ascii_graphic() || c == b' ' { c as char } else { '?' }).collect()
}

/// Header and descriptions of a profile file; `None` when it is not an ICC profile or has
/// no description. Reads the header, the tag table and the description tag only.
pub fn describe(path: &Path) -> Option<ProfileEntry> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut head = [0u8; 132];
    f.read_exact(&mut head).ok()?;
    if &head[36..40] != b"acsp" {
        return None;
    }
    let count = be32(&head[128..132]).min(1024) as usize;
    let mut table = vec![0u8; count * 12];
    f.read_exact(&mut table).ok()?;
    let (offset, size) = table.chunks_exact(12).find(|t| &t[0..4] == b"desc").map(|t| (be32(&t[4..8]), be32(&t[8..12])))?;
    if !(12..=MAX_DESC_BYTES).contains(&size) || offset as u64 + size as u64 > len {
        return None;
    }
    f.seek(SeekFrom::Start(offset as u64)).ok()?;
    let mut tag = vec![0u8; size as usize];
    f.read_exact(&mut tag).ok()?;
    let names = description_names(&tag);
    if names.is_empty() {
        return None;
    }
    Some(ProfileEntry { path: path.to_path_buf(), names, space: sig(&head[16..20]), class: sig(&head[12..16]), bytes: len })
}

/// The strings of a 'desc' tag (textDescriptionType, multiLocalizedUnicodeType or
/// textType), primary first, duplicates removed.
pub fn description_names(tag: &[u8]) -> Vec<String> {
    let utf16 = |b: &[u8]| String::from_utf16_lossy(&b.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>());
    // Latin-1 text up to its terminating NUL.
    let ascii = |b: &[u8]| b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect::<String>();
    let mut raw: Vec<String> = Vec::new();
    if tag.len() >= 12 {
        match &tag[0..4] {
            b"desc" => {
                let n = be32(&tag[8..12]) as usize;
                if let Some(a) = tag.get(12..12 + n) {
                    raw.push(ascii(a));
                    let u = 12 + n;
                    if let Some(h) = tag.get(u..u + 8) {
                        let chars = be32(&h[4..8]) as usize;
                        if let Some(b) = tag.get(u + 8..u + 8 + chars * 2) {
                            raw.push(utf16(b));
                        }
                    }
                }
            }
            b"mluc" if tag.len() >= 16 => {
                let (records, size) = (be32(&tag[8..12]) as usize, (be32(&tag[12..16]) as usize).max(12));
                for r in 0..records.min(256) {
                    let at = 16 + r * size;
                    let Some(rec) = tag.get(at..at + 12) else { break };
                    let (len, off) = (be32(&rec[4..8]) as usize, be32(&rec[8..12]) as usize);
                    if let Some(b) = tag.get(off..off + len) {
                        raw.push(utf16(b));
                    }
                }
            }
            b"text" => raw.push(ascii(&tag[8..])),
            _ => {}
        }
    }
    let mut out: Vec<String> = Vec::new();
    for s in raw {
        // NUL padding, byte order marks some profiles put in their Unicode text, white space
        let s = s.trim_matches(|c: char| c == '\0' || c == '\u{FEFF}' || c.is_whitespace()).to_string();
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// The whole profile file, checked for the ICC signature and a sane size.
pub fn read_profile(path: &Path) -> std::io::Result<Vec<u8>> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_PROFILE_BYTES {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("profile larger than {MAX_PROFILE_BYTES} bytes")));
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() < 132 || &bytes[36..40] != b"acsp" {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not an ICC profile"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_desc_and_mluc() {
        // textDescriptionType: "SWOP" + Unicode "SWOP2"
        let mut t = b"desc\0\0\0\0".to_vec();
        t.extend_from_slice(&5u32.to_be_bytes());
        t.extend_from_slice(b"SWOP\0");
        t.extend_from_slice(&0u32.to_be_bytes());
        t.extend_from_slice(&5u32.to_be_bytes());
        for c in "SWOP2".encode_utf16() {
            t.extend_from_slice(&c.to_be_bytes());
        }
        assert_eq!(description_names(&t), ["SWOP", "SWOP2"]);
        // mluc with two records, the second a duplicate.
        let mut m = b"mluc\0\0\0\0".to_vec();
        m.extend_from_slice(&2u32.to_be_bytes());
        m.extend_from_slice(&12u32.to_be_bytes());
        let s: Vec<u8> = "Japan Color".encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
        for _ in 0..2 {
            m.extend_from_slice(b"enUS");
            m.extend_from_slice(&(s.len() as u32).to_be_bytes());
            m.extend_from_slice(&40u32.to_be_bytes());
        }
        m.extend_from_slice(&s);
        assert_eq!(description_names(&m), ["Japan Color"]);
    }
}
