//! IDML `LinkResourceURI` → the path the saving machine meant.
//!
//! InDesign writes `file:` URIs without an authority: `file:/Users/…` on macOS,
//! `file:C:/Users/…` on Windows; older and third-party files use `file:///…` or
//! `file://server/share/…`. Components are percent-decoded UTF-8 and normalised to NFC
//! (macOS stores Korean names decomposed, NFD).

use unicode_normalization::UnicodeNormalization;
use std::path::{Path, PathBuf};

/// Resolve a native path without changing disk names. Sync clients may decompose just
/// one component, so normalising the whole path to NFC or NFD is insufficient.
/// Exact spelling wins; a non-exact ambiguous spelling is never guessed.
pub fn resolve_native(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return Some(PathBuf::from("."));
    }
    if std::fs::symlink_metadata(path).is_ok() {
        return Some(path.to_path_buf());
    }
    let parent = resolve_native(path.parent().filter(|p| *p != path)?)?;
    let name = path.file_name()?.to_string_lossy();
    let key: String = name.nfc().collect();
    let mut matches = std::fs::read_dir(parent).ok()?.filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().nfc().eq(key.chars()));
    let found = matches.next()?.path();
    matches.next().is_none().then_some(found)
}

/// How the saving machine wrote the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PathStyle {
    /// `/Users/…` (macOS, Unix).
    Posix,
    /// `C:/…`.
    WindowsDrive,
    /// `//server/share/…`.
    Unc,
    /// No root (relative or unrecognised).
    Relative,
}

/// A parsed link location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkPath {
    /// Decoded path with `/` separators, NFC (`C:/x/y.jpg`, `/Users/x/y.jpg`, `//srv/s/y.jpg`).
    pub path: String,
    pub style: PathStyle,
    /// Drive (`C:`) or UNC host + share (`srv/share`) when present.
    pub root: Option<String>,
    /// Folders then the file name, NFC, below the root.
    pub components: Vec<String>,
}

impl LinkPath {
    pub fn file_name(&self) -> Option<&str> {
        self.components.last().map(String::as_str)
    }

    /// Folders above the file, nearest last.
    pub fn folders(&self) -> &[String] {
        let n = self.components.len();
        &self.components[..n.saturating_sub(1)]
    }

    /// The path on this machine when it is directly addressable here: drive and UNC paths
    /// on Windows, POSIX paths elsewhere. A macOS `/Users/…` path is not looked up on
    /// Windows (it would silently mean `\Users\…` on the current drive).
    pub fn local_path(&self) -> Option<std::path::PathBuf> {
        if self.components.is_empty() {
            return None;
        }
        if cfg!(windows) {
            match self.style {
                PathStyle::WindowsDrive => {
                    let root = self.root.as_deref()?;
                    Some(std::path::PathBuf::from(format!("{root}\\{}", self.components.join("\\"))))
                }
                PathStyle::Unc => {
                    let root = self.root.as_deref()?.replace('/', "\\");
                    Some(std::path::PathBuf::from(format!("\\\\{root}\\{}", self.components.join("\\"))))
                }
                _ => None,
            }
        } else {
            match self.style {
                PathStyle::Posix => Some(std::path::PathBuf::from(format!("/{}", self.components.join("/")))),
                _ => None,
            }
        }
    }
}

/// Percent-decodes into bytes, then UTF-8 (invalid sequences become U+FFFD).
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Parses a `LinkResourceURI` (or a plain path). `None` for empty input and for other
/// schemes (`http:`, `asset:` …), which are not files.
pub fn parse_uri(uri: &str) -> Option<LinkPath> {
    let uri = uri.trim();
    if uri.is_empty() {
        return None;
    }
    let rest = if uri.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:")) {
        &uri[5..]
    } else {
        // Another scheme (two or more letters before ':'), not a drive letter.
        if let Some(colon) = uri.find(':')
            && colon >= 2
            && uri[..colon].bytes().all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.')
        {
            return None;
        }
        uri
    };
    let decoded: String = percent_decode(rest).replace('\\', "/").nfc().collect();
    let mut s = decoded.as_str();
    // `//authority/…`: an empty or `localhost` authority is a local path, any other host a
    // UNC share.
    let mut unc_host = None;
    if let Some(after) = s.strip_prefix("//") {
        if after.starts_with('/') {
            s = after;
        } else if after.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("localhost/")) {
            s = &after[9..];
        } else {
            unc_host = Some(after);
        }
    }
    if let Some(unc) = unc_host {
        let mut parts = unc.split('/').filter(|p| !p.is_empty());
        let host = parts.next()?;
        let share = parts.next()?;
        let components: Vec<String> = parts.map(str::to_owned).collect();
        return Some(LinkPath {
            path: format!("//{host}/{share}/{}", components.join("/")),
            style: PathStyle::Unc,
            root: Some(format!("{host}/{share}")),
            components,
        });
    }
    // `/C:/…` → `C:/…`
    if let Some(t) = s.strip_prefix('/')
        && t.split('/').next().is_some_and(is_drive)
    {
        s = t;
    }
    let first = s.split('/').next().unwrap_or("");
    if is_drive(first) {
        let drive = first.to_ascii_uppercase();
        let components: Vec<String> = s[2..].split('/').filter(|p| !p.is_empty()).map(str::to_owned).collect();
        return Some(LinkPath {
            path: format!("{drive}/{}", components.join("/")),
            style: PathStyle::WindowsDrive,
            root: Some(drive),
            components,
        });
    }
    let posix = s.starts_with('/');
    let components: Vec<String> = s.split('/').filter(|p| !p.is_empty() && *p != ".").map(str::to_owned).collect();
    if components.is_empty() {
        return None;
    }
    Some(LinkPath {
        path: if posix { format!("/{}", components.join("/")) } else { components.join("/") },
        style: if posix { PathStyle::Posix } else { PathStyle::Relative },
        root: None,
        components,
    })
}

/// Comparison key for a file or folder name: NFC, case-folded (Windows and default macOS
/// volumes ignore case).
pub fn name_key(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let lowered: String = s.nfc().flat_map(char::to_lowercase).collect();
    lowered.nfc().collect()
}

/// The NFD spelling of `s` (for looking up names synced from macOS unchanged).
pub fn nfd(s: &str) -> String {
    s.nfd().collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bugfind_korean_relative_path() {
        assert!(super::parse_uri("한글.png").is_some());
    }
    use super::*;

    #[test]
    fn mac_uri_nfd_korean() {
        let u = "file:/Users/Shared/magazine/AD_PDF_2022/%E1%84%87%E1%85%A9%E1%86%A8%E1%84%80%E1%85%B5%E1%86%B7_2024.pdf";
        let p = parse_uri(u).unwrap();
        assert_eq!(p.style, PathStyle::Posix);
        assert_eq!(p.file_name(), Some("복김_2024.pdf"));
        assert_eq!(p.folders(), ["Users", "Shared", "magazine", "AD_PDF_2022"]);
        assert_eq!(p.path, "/Users/Shared/magazine/AD_PDF_2022/복김_2024.pdf");
        if cfg!(windows) {
            assert_eq!(p.local_path(), None);
        }
    }

    #[test]
    fn windows_and_spaces() {
        let p = parse_uri("file:C:/Users/editor/My%20Docs/a.png").unwrap();
        assert_eq!(p.style, PathStyle::WindowsDrive);
        assert_eq!(p.root.as_deref(), Some("C:"));
        assert_eq!(p.components, ["Users", "editor", "My Docs", "a.png"]);
        if cfg!(windows) {
            assert_eq!(p.local_path().unwrap(), std::path::PathBuf::from(r"C:\Users\editor\My Docs\a.png"));
        }
        let q = parse_uri("file:///c:/x/b.jpg").unwrap();
        assert_eq!(q.path, "C:/x/b.jpg");
        let r = parse_uri("file:///Users/x/c.tif").unwrap();
        assert_eq!((r.style, r.path.as_str()), (PathStyle::Posix, "/Users/x/c.tif"));
        let s = parse_uri("file://server/share/dir/d.psd").unwrap();
        assert_eq!((s.style, s.root.as_deref(), s.components.len()), (PathStyle::Unc, Some("server/share"), 2));
        let t = parse_uri("file://localhost/Users/e.eps").unwrap();
        assert_eq!(t.path, "/Users/e.eps");
        assert_eq!(parse_uri("http://example.com/a.jpg"), None);
        assert_eq!(parse_uri(""), None);
        // a literal percent that is not an escape survives
        assert_eq!(parse_uri("file:/a/100%.jpg").unwrap().file_name(), Some("100%.jpg"));
    }

    #[test]
    fn keys_match_across_forms() {
        let composed = "복김_2024.PDF";
        let decomposed = nfd("복김_2024.pdf");
        assert_ne!(composed, decomposed);
        assert_eq!(name_key(composed), name_key(&decomposed));
        assert_eq!(name_key("Café.JPG"), name_key(&nfd("café.jpg")));
    }
}
