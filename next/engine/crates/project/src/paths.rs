//! Portable paths: the one place where project references are parsed, checked and turned
//! into paths of this machine.
//!
//! A stored reference is a [`RelPath`]: relative to the project root, names separated by
//! `/`, every name NFC and valid on Windows, macOS and Linux ([`check_name`]), no `.` or
//! `..` and nothing absolute. Native paths of any OS (`C:\…`, `\\server\share\…`,
//! `/Volumes/…`, `/home/…`) are parsed lexically by [`NativePath::parse`], whatever OS runs
//! the code, so the rules are testable everywhere. [`relative_to`] turns a native path
//! below a native root into a `RelPath`; [`RelPath::to_native`] and [`resolve_on_disk`] go
//! the other way (the latter also finds names a Mac wrote decomposed, NFD).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// Root-relative paths longer than this (UTF-16 units, as Windows counts) get a warning:
/// Windows' classic `MAX_PATH` is 260 for the whole path, which leaves about 60 for the
/// folder that holds the project.
pub const LONG_PATH_WARN: usize = 200;

/// Resolve an existing parent inside a project, rejecting symlink escapes as well.
pub fn checked_project_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    crate::manifest::load(&root).map_err(|e| e.to_string())?;
    let path = RelPath::parse(relative).map_err(|e| e.to_string())?.to_native(&root);
    let parent = path.parent().ok_or("missing parent")?.canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) || (path.exists() && !path.canonicalize().map_err(|e| e.to_string())?.starts_with(&root)) {
        return Err("path leaves the project".into());
    }
    Ok(path)
}

/// Longest name, in UTF-8 bytes (Linux and most NAS file systems allow 255 bytes).
pub const MAX_NAME_BYTES: usize = 255;

/// Characters Windows does not allow in names (`/` is the separator everywhere).
pub const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '\\', '|', '?', '*'];

/// Why a single name cannot be used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "issue", rename_all = "camelCase")]
pub enum NameIssue {
    Empty,
    /// `.` or `..`.
    DotName,
    ForbiddenChar { ch: char },
    Control { code: u32 },
    /// Windows drops a final dot or space, so the name would change.
    TrailingDotOrSpace,
    /// `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`9`, `LPT1`–`9` (with any extension).
    Reserved { name: String },
    TooLong { bytes: usize },
}

impl fmt::Display for NameIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameIssue::Empty => write!(f, "빈 이름"),
            NameIssue::DotName => write!(f, "'.' 또는 '..'"),
            NameIssue::ForbiddenChar { ch } => write!(f, "쓸 수 없는 문자 '{ch}'"),
            NameIssue::Control { code } => write!(f, "제어 문자 U+{code:04X}"),
            NameIssue::TrailingDotOrSpace => write!(f, "이름 끝의 점이나 공백"),
            NameIssue::Reserved { name } => write!(f, "Windows 예약 이름 {name}"),
            NameIssue::TooLong { bytes } => write!(f, "이름이 너무 김({bytes}바이트)"),
        }
    }
}

/// Why a path cannot be stored as a reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "camelCase")]
pub enum PathError {
    Empty,
    /// A drive, UNC share, leading `/` or URI scheme.
    Absolute { path: String },
    /// `..` (anywhere) or a path outside the project root.
    OutsideRoot { path: String },
    /// `\` used as a separator.
    Backslash,
    /// Not in Unicode normalisation form C.
    NotNfc,
    /// `a//b`, a leading or trailing `/`.
    EmptyName,
    BadName { name: String, issue: NameIssue },
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathError::Empty => write!(f, "빈 경로"),
            PathError::Absolute { path } => write!(f, "절대 경로는 저장하지 않습니다: {path}"),
            PathError::OutsideRoot { path } => write!(f, "프로젝트 폴더 밖 경로: {path}"),
            PathError::Backslash => write!(f, "구분자는 '/'만 씁니다"),
            PathError::NotNfc => write!(f, "NFC가 아닌 이름"),
            PathError::EmptyName => write!(f, "빈 이름이 있는 경로"),
            PathError::BadName { name, issue } => write!(f, "'{name}': {issue}"),
        }
    }
}

impl std::error::Error for PathError {}

fn is_reserved(name: &str) -> bool {
    // The device name is the part before the first dot ("CON.txt", "nul.tar.gz").
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    // COM1–9, LPT1–9; Windows also reserves the superscript digits ¹ ² ³.
    let mut chars = stem.chars();
    let head: String = chars.by_ref().take(3).collect::<String>().to_ascii_uppercase();
    let rest: Vec<char> = chars.collect();
    (head == "COM" || head == "LPT") && rest.len() == 1 && matches!(rest[0], '1'..='9' | '¹' | '²' | '³')
}

/// Whether `name` is usable as a file or folder name on Windows, macOS and Linux.
pub fn check_name(name: &str) -> Result<(), NameIssue> {
    if name.is_empty() {
        return Err(NameIssue::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameIssue::DotName);
    }
    for c in name.chars() {
        if (c as u32) < 0x20 || c == '\u{7F}' {
            return Err(NameIssue::Control { code: c as u32 });
        }
        if FORBIDDEN_CHARS.contains(&c) || c == '/' {
            return Err(NameIssue::ForbiddenChar { ch: c });
        }
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(NameIssue::TrailingDotOrSpace);
    }
    if is_reserved(name) {
        return Err(NameIssue::Reserved { name: name.split('.').next().unwrap_or(name).to_string() });
    }
    let nfc: String = name.nfc().collect();
    if nfc.len() > MAX_NAME_BYTES {
        return Err(NameIssue::TooLong { bytes: nfc.len() });
    }
    Ok(())
}

/// A usable spelling of `name`: NFC, forbidden and control characters replaced by `_`,
/// final dots and spaces removed, reserved device names prefixed with `_`, shortened to
/// [`MAX_NAME_BYTES`] keeping the extension. Usable names come back unchanged (as NFC).
pub fn fix_name(name: &str) -> String {
    let mut s: String = name
        .nfc()
        .map(|c| if (c as u32) < 0x20 || c == '\u{7F}' || FORBIDDEN_CHARS.contains(&c) || c == '/' { '_' } else { c })
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() || s == "." || s == ".." {
        s = "_".to_string();
    }
    if is_reserved(&s) {
        s.insert(0, '_');
    }
    if s.len() > MAX_NAME_BYTES {
        s = fit_name(&s, "");
    }
    s
}

/// Add a collision number to an already portable name, reserving its bytes before shortening.
pub(crate) fn numbered_name(name: &str, number: usize) -> String {
    fit_name(name, &format!(" ({number})"))
}

fn fit_name(s: &str, suffix: &str) -> String {
    let (stem, ext) = split_ext(s);
    if s.len() + suffix.len() <= MAX_NAME_BYTES {
        return format!("{stem}{suffix}{ext}");
    }
    // An extension can itself exceed the limit. Preserve it only when there is
    // room for at least one complete stem character; otherwise shorten the name.
    let (stem, ext) = if ext.len() + suffix.len() + stem.chars().next().map_or(1, char::len_utf8) > MAX_NAME_BYTES {
        (s.to_string(), String::new())
    } else {
        (stem.to_string(), ext.to_string())
    };
    let keep = MAX_NAME_BYTES - ext.len() - suffix.len();
    let mut cut = String::new();
    for c in stem.chars() {
        if cut.len() + c.len_utf8() > keep {
            break;
        }
        cut.push(c);
    }
    format!("{}{suffix}{ext}", cut.trim_end_matches(['.', ' ']))
}

/// "photo.final.jpg" → ("photo.final", ".jpg"); a leading dot belongs to the stem.
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// Comparison key of a name: NFC, lower-cased. Two names with the same key may not live in
/// the same folder (Windows and default macOS volumes ignore case; macOS stores Korean names
/// decomposed, so NFC and NFD spellings are the same file there).
pub fn collision_key(name: &str) -> String {
    let lowered: String = name.nfc().flat_map(char::to_lowercase).collect();
    lowered.nfc().collect()
}

/// A validated reference: root-relative, `/`-separated, NFC, portable names.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelPath(String);

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelPath({:?})", self.0)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for RelPath {
    type Error = PathError;
    fn try_from(s: String) -> Result<RelPath, PathError> {
        RelPath::parse(&s)
    }
}

impl From<RelPath> for String {
    fn from(p: RelPath) -> String {
        p.0
    }
}

fn looks_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/')
        || s.starts_with('\\')
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        || s.contains("://")
}

impl RelPath {
    /// Validates at the reference boundary and stores NFC. Sync clients and older
    /// manifests may supply NFD; disk spellings are resolved separately.
    pub fn parse(s: &str) -> Result<RelPath, PathError> {
        if s.is_empty() {
            return Err(PathError::Empty);
        }
        if looks_absolute(s) {
            return Err(PathError::Absolute { path: s.to_string() });
        }
        if s.contains('\\') {
            return Err(PathError::Backslash);
        }
        for name in s.split('/') {
            if name.is_empty() {
                return Err(PathError::EmptyName);
            }
            if name == ".." {
                return Err(PathError::OutsideRoot { path: s.to_string() });
            }
            check_name(name).map_err(|issue| PathError::BadName { name: name.to_string(), issue })?;
        }
        Ok(RelPath(s.nfc().collect()))
    }

    /// Joins checked names (`["assets", "a.jpg"]`).
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Result<RelPath, PathError> {
        let joined = names.iter().map(|n| n.as_ref().nfc().collect::<String>()).collect::<Vec<_>>().join("/");
        RelPath::parse(&joined)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    pub fn parent(&self) -> Option<RelPath> {
        self.0.rfind('/').map(|i| RelPath(self.0[..i].to_string()))
    }

    /// This path with one more name (checked).
    pub fn join(&self, name: &str) -> Result<RelPath, PathError> {
        RelPath::parse(&format!("{}/{}", self.0, name.nfc().collect::<String>()))
    }

    /// The path on this machine below `root` (names as stored, NFC).
    pub fn to_native(&self, root: &Path) -> PathBuf {
        let mut p = root.to_path_buf();
        for n in self.names() {
            p.push(n);
        }
        p
    }

    /// Existing spelling, including existing NFD parents of a new output file.
    /// This never renames a file and is also suitable for a create-new destination.
    pub fn on_disk(&self, root: &Path) -> PathBuf {
        let mut path = root.to_path_buf();
        for name in self.names() {
            let next = path.join(name);
            path = folio_links::uri::resolve_native(&next).unwrap_or(next);
        }
        path
    }

    /// Key for "same entry" comparisons of whole paths (see [`collision_key`]).
    pub fn collision_key(&self) -> String {
        collision_key(&self.0)
    }

    /// Length as Windows counts it (UTF-16 units).
    pub fn utf16_len(&self) -> usize {
        self.0.encode_utf16().count()
    }

    /// Long enough to risk Windows' `MAX_PATH` ([`LONG_PATH_WARN`]).
    pub fn is_long(&self) -> bool {
        self.utf16_len() > LONG_PATH_WARN
    }
}

/// How a native path is rooted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathStyle {
    /// `C:\…` or `C:/…` (also `\\?\C:\…`).
    WindowsDrive,
    /// `\\server\share\…` (also `\\?\UNC\…` and `//server/share/…`).
    Unc,
    /// `/…` (macOS, Linux).
    Posix,
    /// No root.
    Relative,
}

/// A native path of any OS, parsed lexically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePath {
    pub style: PathStyle,
    /// `C:` (upper case) or `server/share`.
    pub root: Option<String>,
    /// Names below the root, NFC; `.` removed, `..` kept as given.
    pub names: Vec<String>,
}

impl NativePath {
    /// Parses `s` as the OS that wrote it would: drive letters, UNC and verbatim (`\\?\`)
    /// prefixes are Windows paths (both separators); anything else starting with `/` is a
    /// POSIX path (only `/` separates; a `\` stays part of the name, which [`check_name`]
    /// then refuses).
    pub fn parse(s: &str) -> NativePath {
        let s: String = s.nfc().collect();
        Self::parse_spelling(&s)
    }

    // URI export must retain the actual disk spelling, unlike comparison/reference keys.
    fn parse_spelling(s: &str) -> NativePath {
        let mut rest: &str = s;
        // Verbatim and device prefixes.
        for prefix in [r"\\?\UNC\", r"\\.\UNC\"] {
            if let Some(r) = rest.strip_prefix(prefix) {
                return Self::unc(r);
            }
        }
        for prefix in [r"\\?\", r"\\.\"] {
            if let Some(r) = rest.strip_prefix(prefix) {
                rest = r;
            }
        }
        let b = rest.as_bytes();
        if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\') {
            let drive = rest[..2].to_ascii_uppercase();
            return NativePath { style: PathStyle::WindowsDrive, root: Some(drive), names: split(&rest[2..], true) };
        }
        if rest.starts_with(r"\\") || (rest.starts_with("//") && !rest.starts_with("///")) {
            return Self::unc(&rest[2..]);
        }
        if rest.starts_with('/') {
            return NativePath { style: PathStyle::Posix, root: None, names: split(rest, false) };
        }
        // A relative path: Windows-style when it uses backslashes.
        NativePath { style: PathStyle::Relative, root: None, names: split(rest, rest.contains('\\')) }
    }

    fn unc(r: &str) -> NativePath {
        let mut parts = split(r, true).into_iter();
        let host = parts.next().unwrap_or_default();
        let share = parts.next().unwrap_or_default();
        NativePath { style: PathStyle::Unc, root: Some(format!("{host}/{share}")), names: parts.collect() }
    }

    /// Whether names compare case-insensitively on the file system that wrote the path
    /// (Windows always; macOS volumes normally; Linux not). POSIX paths below `/Volumes/`
    /// or `/Users/` are taken as macOS.
    pub fn case_insensitive(&self) -> bool {
        match self.style {
            PathStyle::WindowsDrive | PathStyle::Unc => true,
            PathStyle::Posix => matches!(self.names.first().map(String::as_str), Some("Volumes" | "Users" | "Applications" | "System")),
            PathStyle::Relative => false,
        }
    }

    pub fn is_absolute(&self) -> bool {
        self.style != PathStyle::Relative
    }
}

fn split(s: &str, windows: bool) -> Vec<String> {
    let parts: Box<dyn Iterator<Item = &str>> = if windows { Box::new(s.split(['/', '\\'])) } else { Box::new(s.split('/')) };
    parts.filter(|p| !p.is_empty() && *p != ".").map(str::to_string).collect()
}

/// `path` as a reference below `root` (both native paths of the same machine, any OS form).
/// Names compare NFC-equal, and case-insensitively where the root's file system ignores
/// case. The stored names are those of `path`, NFC.
pub fn relative_to(root: &str, path: &str) -> Result<RelPath, PathError> {
    let r = NativePath::parse(root);
    let p = NativePath::parse(path);
    if !r.is_absolute() || !p.is_absolute() {
        return Err(PathError::OutsideRoot { path: path.to_string() });
    }
    if p.names.iter().any(|n| n == "..") || r.names.iter().any(|n| n == "..") {
        return Err(PathError::OutsideRoot { path: path.to_string() });
    }
    let fold = r.case_insensitive();
    let same = |a: &str, b: &str| if fold { collision_key(a) == collision_key(b) } else { a == b };
    let roots_match = r.style == p.style
        && match (&r.root, &p.root) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => true,
            _ => false,
        };
    if !roots_match || p.names.len() <= r.names.len() || !r.names.iter().zip(&p.names).all(|(a, b)| same(a, b)) {
        return Err(PathError::OutsideRoot { path: path.to_string() });
    }
    RelPath::from_names(&p.names[r.names.len()..])
}

/// [`relative_to`] for paths of this machine.
pub fn relative_to_root(root: &Path, path: &Path) -> Result<RelPath, PathError> {
    let original = relative_to(&root.to_string_lossy(), &path.to_string_lossy());
    // Unix absolute paths may still contain `..`. Ask the filesystem to resolve
    // them so a parent after a symbolic link uses the link target's parent.
    if original.is_ok() || !root.is_absolute() || !path.is_absolute()
        || !root.components().chain(path.components()).any(|part| part == Component::ParentDir)
    {
        return original;
    }
    let outside = || PathError::OutsideRoot { path: path.to_string_lossy().into_owned() };
    let real_root = root.canonicalize().map_err(|_| outside())?;
    let real_path = path.canonicalize().map_err(|_| outside())?;
    let relative = real_path.strip_prefix(&real_root).map_err(|_| outside())?;
    if relative.as_os_str().is_empty() {
        return Err(outside());
    }
    RelPath::parse(&relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
}

/// The file or folder a reference names under `root`, as it exists on disk: the stored
/// spelling first, then, name by name, an entry whose NFC form is the same (names a Mac or a
/// sync client wrote decomposed). `None` when nothing matches.
pub fn resolve_on_disk(root: &Path, rel: &RelPath) -> Option<PathBuf> {
    folio_links::uri::resolve_native(&rel.to_native(root))
}

/// A symbolic link or junction at `path` or in a folder between `root` and it (projects
/// may not contain any: they would not survive a copy to another machine or OS).
pub fn link_inside(root: &Path, path: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(root).ok()?;
    let mut cur = root.to_path_buf();
    for c in rel.components() {
        cur.push(c);
        if std::fs::symlink_metadata(&cur).is_ok_and(|m| m.file_type().is_symlink()) {
            return Some(cur);
        }
    }
    None
}

/// A problem with one name of a folder listing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingIssue {
    pub name: String,
    /// `invalid`, `collision` (same key as `other`), `notNfc` or `link`.
    pub kind: &'static str,
    pub other: Option<String>,
    pub detail: Option<String>,
}

/// Checks the names of one folder: validity, NFC, case / normalisation collisions and links.
pub fn check_listing(dir: &Path) -> Vec<ListingIssue> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Err(issue) = check_name(&name) {
            out.push(ListingIssue { name: name.clone(), kind: "invalid", other: None, detail: Some(issue.to_string()) });
        } else if name.nfc().collect::<String>() != name {
            out.push(ListingIssue { name: name.clone(), kind: "notNfc", other: None, detail: None });
        }
        if e.file_type().is_ok_and(|t| t.is_symlink()) {
            out.push(ListingIssue { name: name.clone(), kind: "link", other: None, detail: None });
        }
        if let Some(prev) = seen.insert(collision_key(&name), name.clone()) {
            out.push(ListingIssue { name, kind: "collision", other: Some(prev), detail: None });
        }
    }
    out
}

/// Percent-encodes a reference for a URI path (RFC 3986 unreserved characters and `/` stay).
pub fn uri_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The `file:` URI InDesign writes for a file of this machine: `file:C:/dir/name.jpg` on
/// Windows (`file://server/share/…` for UNC), `file:/Users/…` elsewhere; names
/// percent-encoded as UTF-8, separators `/`.
pub fn file_uri(path: &Path) -> String {
    let n = NativePath::parse_spelling(&path.to_string_lossy());
    let names = n.names.iter().map(|s| uri_escape(s)).collect::<Vec<_>>().join("/");
    match n.style {
        PathStyle::WindowsDrive => format!("file:{}/{names}", n.root.unwrap_or_default()),
        PathStyle::Unc => format!("file://{}/{names}", n.root.map(|r| uri_escape(&r)).unwrap_or_default()),
        _ => format!("file:/{names}"),
    }
}

/// [`file_uri`] in the `file:///…` form XMP uses.
pub fn file_url(path: &Path) -> String {
    let uri = file_uri(path);
    match uri.strip_prefix("file:") {
        Some(rest) if rest.starts_with("//") => uri,
        Some(rest) => format!("file:///{}", rest.trim_start_matches('/')),
        None => uri,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for ok in ["a.jpg", "사진 1.psd", ".hidden", "a..b", "CONSOLE.txt", "COM10", "LPT", "x-y_z (2).tif", "ünïcode"] {
            assert_eq!(check_name(ok), Ok(()), "{ok}");
        }
        assert_eq!(check_name(""), Err(NameIssue::Empty));
        assert_eq!(check_name(".."), Err(NameIssue::DotName));
        for (bad, ch) in [("a<b", '<'), ("a>b", '>'), ("a:b", ':'), ("a\"b", '"'), ("a\\b", '\\'), ("a|b", '|'), ("a?b", '?'), ("a*b", '*'), ("a/b", '/')] {
            assert_eq!(check_name(bad), Err(NameIssue::ForbiddenChar { ch }), "{bad}");
        }
        assert_eq!(check_name("a\u{1}b"), Err(NameIssue::Control { code: 1 }));
        assert_eq!(check_name("a\tb"), Err(NameIssue::Control { code: 9 }));
        assert_eq!(check_name("name."), Err(NameIssue::TrailingDotOrSpace));
        assert_eq!(check_name("name "), Err(NameIssue::TrailingDotOrSpace));
        for r in ["CON", "con", "Prn.txt", "aux.tar.gz", "NUL", "COM1", "com9.log", "LPT1", "lpt9", "COM¹", "CON .txt"] {
            assert!(matches!(check_name(r), Err(NameIssue::Reserved { .. })), "{r}");
        }
        assert!(matches!(check_name(&"가".repeat(90)), Err(NameIssue::TooLong { bytes: 270 })));
    }

    #[test]
    fn fixing_names() {
        assert_eq!(fix_name("a:b?.jpg"), "a_b_.jpg");
        assert_eq!(fix_name("report. . "), "report");
        assert_eq!(fix_name("CON.txt"), "_CON.txt");
        assert_eq!(fix_name("..."), "_");
        assert_eq!(fix_name("ok name.png"), "ok name.png");
        let long = format!("{}.jpg", "가".repeat(100));
        let fixed = fix_name(&long);
        assert!(fixed.len() <= MAX_NAME_BYTES && fixed.ends_with(".jpg"), "{fixed}");
        assert_eq!(check_name(&fixed), Ok(()));
        // NFD in, NFC out
        let nfd: String = "복김.pdf".nfd().collect();
        assert_eq!(fix_name(&nfd), "복김.pdf");
        for weird in ["a\u{0}b", "x|y*z", "AUX", "  ", "LPT3.c"] {
            assert_eq!(check_name(&fix_name(weird)), Ok(()), "{weird}");
        }
    }

    #[test]
    fn rel_paths() {
        let p = RelPath::parse("assets/347호/사진 1.jpg").unwrap();
        assert_eq!(p.file_name(), "사진 1.jpg");
        assert_eq!(p.parent().unwrap().as_str(), "assets/347호");
        assert_eq!(p.names().count(), 3);
        assert_eq!(serde_json::to_string(&p).unwrap(), "\"assets/347호/사진 1.jpg\"");
        assert!(serde_json::from_str::<RelPath>("\"../x\"").is_err());
        for (bad, kind) in [
            ("", "empty"),
            ("/abs/x", "absolute"),
            ("C:/x", "absolute"),
            ("c:x", "absolute"),
            ("\\\\srv\\s\\x", "absolute"),
            ("file:///x", "absolute"),
            ("a\\b", "backslash"),
            ("a/../b", "outside"),
            ("..", "outside"),
            ("a//b", "emptyName"),
            ("a/", "emptyName"),
            ("a/CON/b", "badName"),
            ("a/b.", "badName"),
        ] {
            let e = RelPath::parse(bad).unwrap_err();
            let got = match e {
                PathError::Empty => "empty",
                PathError::Absolute { .. } => "absolute",
                PathError::Backslash => "backslash",
                PathError::OutsideRoot { .. } => "outside",
                PathError::EmptyName => "emptyName",
                PathError::BadName { .. } => "badName",
                PathError::NotNfc => "nfc",
            };
            assert_eq!(got, kind, "{bad}");
        }
        let nfd: String = "assets/복김.pdf".nfd().collect();
        assert_eq!(RelPath::parse(&nfd).unwrap().as_str(), "assets/복김.pdf");
        let decoded: RelPath = serde_json::from_str(&serde_json::to_string(&nfd).unwrap()).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), "\"assets/복김.pdf\"");
        assert_eq!(RelPath::from_names(&["assets", &"복김.pdf".nfd().collect::<String>()]).unwrap().as_str(), "assets/복김.pdf");
        let long = RelPath::parse(&format!("assets/{}/x.jpg", "a".repeat(200))).unwrap();
        assert!(long.is_long());
        assert!(!p.is_long());
        let root = Path::new("root");
        assert_eq!(p.to_native(root), Path::new("root").join("assets").join("347호").join("사진 1.jpg"));
    }

    #[test]
    fn native_forms_of_every_os() {
        let w = NativePath::parse(r"c:\Users\editor\Proj\a.jpg");
        assert_eq!((w.style, w.root.as_deref(), w.names.len()), (PathStyle::WindowsDrive, Some("C:"), 4));
        let w2 = NativePath::parse("D:/Archive/magazine");
        assert_eq!((w2.style, w2.names.clone()), (PathStyle::WindowsDrive, vec!["Archive".to_string(), "magazine".to_string()]));
        let v = NativePath::parse(r"\\?\C:\long\path");
        assert_eq!((v.style, v.root.as_deref()), (PathStyle::WindowsDrive, Some("C:")));
        let u = NativePath::parse(r"\\nas\share\issues\347");
        assert_eq!((u.style, u.root.as_deref(), u.names.clone()), (PathStyle::Unc, Some("nas/share"), vec!["issues".to_string(), "347".to_string()]));
        let vu = NativePath::parse(r"\\?\UNC\nas\share\x");
        assert_eq!((vu.style, vu.root.as_deref()), (PathStyle::Unc, Some("nas/share")));
        let fu = NativePath::parse("//nas/share/x");
        assert_eq!((fu.style, fu.root.as_deref()), (PathStyle::Unc, Some("nas/share")));
        let mac = NativePath::parse("/Volumes/chussd/magazine/a.pdf");
        assert_eq!((mac.style, mac.names[0].as_str()), (PathStyle::Posix, "Volumes"));
        assert!(mac.case_insensitive());
        let linux = NativePath::parse("/home/kim/projects/x");
        assert!(!linux.case_insensitive());
        // a backslash in a POSIX name stays in the name
        assert_eq!(NativePath::parse("/home/a\\b").names, vec!["home".to_string(), "a\\b".to_string()]);
        assert_eq!(NativePath::parse("rel\\x").style, PathStyle::Relative);
    }

    #[test]
    fn relative_to_root_on_every_os() {
        // Windows: case-insensitive, either separator, NFC
        assert_eq!(relative_to(r"C:\Proj\Issue", r"c:\proj\issue\assets\a.jpg").unwrap().as_str(), "assets/a.jpg");
        assert_eq!(relative_to("D:/X", r"D:\X\사진\b.png").unwrap().as_str(), "사진/b.png");
        let nfd: String = "D:/X/복김/a.pdf".nfd().collect();
        assert_eq!(relative_to("D:/X", &nfd).unwrap().as_str(), "복김/a.pdf");
        assert!(relative_to(r"C:\Proj", r"D:\Proj\a").is_err());
        assert!(relative_to(r"C:\Proj", r"C:\Project\a").is_err());
        assert!(relative_to(r"C:\Proj", r"C:\Proj").is_err());
        assert!(relative_to(r"C:\Proj", r"C:\Proj\..\x").is_err());
        // UNC
        assert_eq!(relative_to(r"\\NAS\Share\p", r"\\nas\share\P\fonts\x.otf").unwrap().as_str(), "fonts/x.otf");
        assert!(relative_to(r"\\nas\share\p", r"\\nas\other\p\x").is_err());
        // macOS: case-insensitive volumes
        assert_eq!(relative_to("/Volumes/Work/Issue", "/Volumes/work/issue/assets/a.jpg").unwrap().as_str(), "assets/a.jpg");
        assert_eq!(relative_to("/Users/kim/p", "/Users/kim/p/x/y.tif").unwrap().as_str(), "x/y.tif");
        // Linux: case-sensitive
        assert_eq!(relative_to("/home/kim/p", "/home/kim/p/a/b.jpg").unwrap().as_str(), "a/b.jpg");
        assert!(relative_to("/home/kim/p", "/home/kim/P/a").is_err());
        // mixed styles never match
        assert!(relative_to("/home/kim/p", r"C:\home\kim\p\a").is_err());
        // names that are not portable are refused, not silently fixed
        assert!(matches!(relative_to("/home/p", "/home/p/x/a:b.jpg"), Err(PathError::BadName { .. })));
        assert!(relative_to("/home/p", "/home/p/a:b.jpg").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn native_parent_path_uses_the_filesystem() {
        let base = std::env::temp_dir().join(format!("folio-native-parent-{}", std::process::id()));
        let root = base.join("project");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::create_dir_all(base.join("outside/nested")).unwrap();
        std::fs::write(root.join("drawing.idml"), b"").unwrap();
        std::fs::write(base.join("outside/drawing.idml"), b"").unwrap();
        std::os::unix::fs::symlink(base.join("outside/nested"), root.join("linked")).unwrap();
        assert_eq!(relative_to_root(&root, &root.join("nested/../drawing.idml")).unwrap().as_str(), "drawing.idml");
        assert!(matches!(relative_to_root(&root, &root.join("linked/../drawing.idml")), Err(PathError::OutsideRoot { .. })));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn uris() {
        let p = Path::new(r"C:\Proj\assets\사진 1.jpg");
        if cfg!(windows) {
            assert_eq!(file_uri(p), "file:C:/Proj/assets/%EC%82%AC%EC%A7%84%201.jpg");
            assert_eq!(file_url(p), "file:///C:/Proj/assets/%EC%82%AC%EC%A7%84%201.jpg");
            assert_eq!(file_uri(Path::new(r"\\nas\share\a b.png")), "file://nas/share/a%20b.png");
        }
        assert_eq!(file_uri(Path::new("/Users/kim/a.png")), "file:/Users/kim/a.png");
        assert_eq!(file_url(Path::new("/Users/kim/a.png")), "file:///Users/kim/a.png");
        // round trip through the link parser of folio-links
        let back = folio_links::parse_uri(&file_uri(Path::new("/Volumes/x/복김 a.pdf"))).unwrap();
        assert_eq!(back.path, "/Volumes/x/복김 a.pdf");
    }

    #[test]
    fn disk_resolution_finds_nfd_names() {
        let root = std::env::temp_dir().join(format!("folio-paths-nfd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir_nfd: String = "자료".nfd().collect();
        let file_nfd: String = "복김.pdf".nfd().collect();
        std::fs::create_dir_all(root.join(&dir_nfd)).unwrap();
        std::fs::write(root.join(&dir_nfd).join(&file_nfd), b"x").unwrap();
        let rel = RelPath::parse("자료/복김.pdf").unwrap();
        let found = resolve_on_disk(&root, &rel).unwrap();
        assert_eq!(std::fs::read(&found).unwrap(), b"x");
        assert!(resolve_on_disk(&root, &RelPath::parse("자료/없음.pdf").unwrap()).is_none());
        // the listing check reports the decomposed names and a case collision
        std::fs::write(root.join("A.txt"), b"1").unwrap();
        std::fs::write(root.join("a.TXT"), b"2").unwrap();
        let issues = check_listing(&root);
        let case_insensitive_fs = std::fs::read(root.join("a.txt")).is_ok() && std::fs::read_dir(&root).unwrap().count() == 2;
        assert!(issues.iter().any(|i| i.kind == "notNfc"), "{issues:?}");
        if !case_insensitive_fs {
            assert!(issues.iter().any(|i| i.kind == "collision"), "{issues:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collisions() {
        assert_eq!(collision_key("Photo.JPG"), collision_key("photo.jpg"));
        assert_eq!(collision_key("복김.pdf"), collision_key(&"복김.PDF".nfd().collect::<String>()));
        assert_ne!(collision_key("a.jpg"), collision_key("b.jpg"));
    }

    #[test]
    fn project_reference_survives_windows_mac_nas_and_linux_moves() {
        // The same persisted reference must be produced by imports on every platform.
        let relative = "assets/347호/사진 1.otf";
        for (root, file) in [
            (r"C:\Work\Issue", format!(r"C:\Work\Issue\{}", relative.replace('/', "\\"))),
            (r"\\NAS\Work\Issue", format!(r"\\NAS\Work\Issue\{}", relative.replace('/', "\\"))),
            ("/Volumes/Work/Issue", format!("/Volumes/Work/Issue/{relative}").nfd().collect()),
            ("/Users/designer/Issue", format!("/Users/designer/Issue/{relative}")),
            ("/home/designer/Issue", format!("/home/designer/Issue/{relative}")),
        ] {
            let imported = relative_to(root, &file).unwrap();
            let json = serde_json::to_string(&imported).unwrap();
            assert_eq!(json, format!("\"{relative}\""));
            let restored: RelPath = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.as_str(), relative);
        }

        // Simulate a Mac-created NFD file copied to a deeper Windows/NAS location.
        let base = std::env::temp_dir().join("folio-portability");
        let root = base.join(format!("moved-{}", std::process::id())).join("깊은 폴더");
        let rel = RelPath::parse(relative).unwrap();
        let nfd: String = "사진 1.otf".nfd().collect();
        let dir = root.join("assets/347호");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(nfd), b"portable-font-placeholder").unwrap();
        let resolved = resolve_on_disk(&root, &rel).unwrap();
        assert_eq!(std::fs::read(&resolved).unwrap(), b"portable-font-placeholder");
        let uri = file_uri(&std::path::absolute(&resolved).unwrap());
        let parsed = folio_links::parse_uri(&uri).unwrap();
        assert!(!parsed.path.contains('\\'), "{uri}");
        assert_eq!(crate::hash::sha256_bytes(&std::fs::read(resolved).unwrap()),
                   crate::hash::sha256_bytes(b"portable-font-placeholder"));
    }
}
