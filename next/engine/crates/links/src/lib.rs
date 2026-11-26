//! Linked files of a document: where they are now, and what they are.
//!
//! ```no_run
//! use folio_links::{FileIndex, LinkSearch, LinkStatus, resolve, probe};
//! let index = FileIndex::scan(&["project/assets".into()]);
//! let search = LinkSearch { document_dir: Some("project".into()), roots: index.roots().to_vec() };
//! match resolve("file:/Users/Shared/magazine/AD_PDF_2022/a.pdf", &search, &index) {
//!     LinkStatus::Found { path, how } => println!("{} ({how:?}) {:?}", path.display(), probe(&path)),
//!     LinkStatus::Missing => println!("missing"),
//!     LinkStatus::Ambiguous(c) => println!("{} candidates", c.len()),
//! }
//! ```
//!
//! [`resolve`] follows InDesign's own relink order first — the original path, the same path
//! relative to the document folder, the document's `Links` folder — then the file-name
//! [`FileIndex`] of the search roots. An index match is chosen by the longest run of folder
//! names it shares with the original path (nearest folder first); equal best matches stay
//! [`LinkStatus::Ambiguous`] — nothing is guessed. Names compare NFC- and case-insensitively.
//!
//! [`LinkReport::build`] resolves every link of a `Document` once (one owner for "which
//! file is this graphic"), compares each found file with the size and time IDML recorded,
//! and [`probe`]s the files' headers. Consumers (preflight, PDF export) should read the
//! report rather than resolve again.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

mod index;
pub mod probe;
mod report;
pub mod uri;

pub use index::{FileIndex, IndexStats, SKIPPED_FOLDERS};
pub use probe::{ColorSpace, ImageFormat, ImageInfo, PdfPage, PlacedFont, ProbeError, VectorColors, probe};
pub mod pdfplace;
pub mod pdfraster;
pub mod pdfcomments;
pub mod psdlayers;
pub use report::{LinkEntry, LinkReport, LinkSummary, Modified, owner_frames};
pub use uri::{LinkPath, PathStyle, parse_uri};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where to look for links besides their original location.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkSearch {
    /// Folder of the InDesign document (not of a converted copy), when known.
    pub document_dir: Option<PathBuf>,
    /// Search folders (the roots of the [`FileIndex`] passed to [`resolve`]).
    pub roots: Vec<PathBuf>,
}

/// How a found file was located.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum How {
    /// At the path IDML recorded.
    Original,
    /// Same trailing folders + name relative to the document folder (or a folder above it).
    DocumentRelative,
    /// In the `Links` folder next to the document (InDesign Package layout).
    LinksFolder,
    /// File-name index; `score` = trailing folder names shared with the original path.
    Search { score: usize },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LinkStatus {
    Found { path: PathBuf, how: How },
    Missing,
    /// Several files match equally well (sorted); none is chosen.
    Ambiguous(Vec<PathBuf>),
}

impl LinkStatus {
    pub fn path(&self) -> Option<&Path> {
        match self {
            LinkStatus::Found { path, .. } => Some(path),
            _ => None,
        }
    }
}

/// Folders above the document folder tried for document-relative paths.
const RELATIVE_ANCESTORS: usize = 2;
/// Longest trailing path (folders + name) tried relative to the document.
const RELATIVE_DEPTH: usize = 6;

/// Whether a file exists at `path` (as given, or with its name spelled NFD as macOS
/// writes it); the index answers for paths below its roots without touching the disk.
fn existing(path: &Path, index: &FileIndex) -> Option<PathBuf> {
    if index.covers(path) {
        return index.lookup(path);
    }
    uri::resolve_native(path).filter(|p| p.is_file())
}

fn join(base: &Path, parts: &[String]) -> PathBuf {
    let mut p = base.to_path_buf();
    for c in parts {
        p.push(c);
    }
    p
}

/// Number of trailing folder names `candidate`'s folders share with `original` folders.
fn shared_suffix(candidate: &Path, original: &[String]) -> usize {
    let folders: Vec<String> = candidate
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(uri::name_key(&s.to_string_lossy())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    folders.iter().rev().zip(original.iter().rev()).take_while(|(a, b)| **a == uri::name_key(b)).count()
}

/// Finds the file a `LinkResourceURI` refers to (see the crate documentation).
pub fn resolve(uri: &str, search: &LinkSearch, index: &FileIndex) -> LinkStatus {
    let Some(lp) = parse_uri(uri) else { return LinkStatus::Missing };
    resolve_path(&lp, search, index)
}

/// [`resolve`] for an already parsed link path.
pub fn resolve_path(lp: &LinkPath, search: &LinkSearch, index: &FileIndex) -> LinkStatus {
    let Some(name) = lp.file_name() else { return LinkStatus::Missing };
    // 1. the original path
    if let Some(p) = lp.local_path().and_then(|p| existing(&p, index)) {
        return LinkStatus::Found { path: p, how: How::Original };
    }
    if let Some(doc) = &search.document_dir {
        // 2. relative to the document: longest trailing path first; in folders above the
        //    document only with at least one folder name in common.
        let n = lp.components.len().min(RELATIVE_DEPTH);
        let bases: Vec<&Path> = doc.ancestors().take(RELATIVE_ANCESTORS + 1).collect();
        for k in (1..=n).rev() {
            let tail = &lp.components[lp.components.len() - k..];
            for (level, base) in bases.iter().enumerate() {
                if level > 0 && k < 2 {
                    continue;
                }
                if let Some(p) = existing(&join(base, tail), index) {
                    return LinkStatus::Found { path: p, how: How::DocumentRelative };
                }
            }
        }
        // 3. the document's Links folder
        if let Some(p) = existing(&doc.join("Links").join(name), index) {
            return LinkStatus::Found { path: p, how: How::LinksFolder };
        }
    }
    // 4. the file-name index
    let candidates = index.candidates(name);
    if candidates.is_empty() {
        return LinkStatus::Missing;
    }
    let folders = lp.folders();
    let scored: Vec<(usize, PathBuf)> = candidates.into_iter().map(|c| (shared_suffix(&c, folders), c)).collect();
    let best = scored.iter().map(|s| s.0).max().unwrap_or(0);
    let mut top: Vec<PathBuf> = scored.into_iter().filter(|s| s.0 == best).map(|s| s.1).collect();
    if top.len() == 1 {
        return LinkStatus::Found { path: top.pop().unwrap_or_default(), how: How::Search { score: best } };
    }
    top.sort();
    LinkStatus::Ambiguous(top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_unicode_components_without_search_index() {
        use unicode_normalization::UnicodeNormalization;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).unwrap().join("artifacts/mac-i18n-20261020/unit-tests").join(format!("folio-link-mixed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let actual = root.join("자료".nfd().collect::<String>()).join("사진.png");
        std::fs::create_dir_all(actual.parent().unwrap()).unwrap();
        std::fs::write(&actual, b"image").unwrap();
        for requested in [root.join("자료/사진.png"), root.join("자료/사진.png".nfd().collect::<String>())] {
            assert_eq!(existing(&requested, &FileIndex::empty()), Some(actual.clone()));
        }
        let index = FileIndex::scan(std::slice::from_ref(&root));
        assert_eq!(index.candidates("사진.png"), index.candidates(&"사진.png".nfd().collect::<String>()));
        std::fs::remove_dir_all(root).unwrap();
    }

    fn tree(name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("folio-resolve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        root
    }

    #[test]
    fn indesign_order_then_index() {
        let root = tree(
            "order",
            &[
                "issue/doc/Links/packaged.jpg",
                "issue/doc/sub/rel.jpg",
                "issue/AD_PDF_2022/ad.pdf",
                "archive/AD_PDF_2022/ad.pdf",
                "archive/2023/AD_PDF_2022/ad.pdf",
                "x/one.tif",
                "y/two.tif",
                "z/two.tif",
                "w/photos/two.tif",
            ],
        );
        let doc = root.join("issue/doc");
        let search = LinkSearch { document_dir: Some(doc.clone()), roots: vec![root.clone()] };
        let index = FileIndex::scan(std::slice::from_ref(&root));
        // relative to the document: same trailing folder + name
        let s = resolve("file:/Users/a/Shared/doc/sub/rel.jpg", &search, &index);
        assert_eq!(s, LinkStatus::Found { path: doc.join("sub").join("rel.jpg"), how: How::DocumentRelative });
        // a folder above the document, sharing the folder name
        let s = resolve("file:/Users/Shared/magazine/AD_PDF_2022/ad.pdf", &search, &index);
        assert_eq!(s, LinkStatus::Found { path: root.join("issue").join("AD_PDF_2022").join("ad.pdf"), how: How::DocumentRelative });
        // Links folder
        let s = resolve("file:/Volumes/Old/whatever/packaged.jpg", &search, &index);
        assert!(matches!(s, LinkStatus::Found { how: How::LinksFolder, .. }), "{s:?}");
        // unique name anywhere
        let s = resolve("file:/Users/q/one.tif", &search, &index);
        assert_eq!(s, LinkStatus::Found { path: root.join("x").join("one.tif"), how: How::Search { score: 0 } });
        // same name, folder decides
        let s = resolve("file:/Users/q/photos/two.tif", &search, &index);
        assert_eq!(s, LinkStatus::Found { path: root.join("w").join("photos").join("two.tif"), how: How::Search { score: 1 } });
        // tie stays ambiguous
        match resolve("file:/Users/q/other/two.tif", &search, &index) {
            LinkStatus::Ambiguous(c) => assert_eq!(c.len(), 3),
            other => panic!("{other:?}"),
        }
        // without the document folder, the index's longest suffix decides
        let bare = LinkSearch { document_dir: None, roots: vec![root.clone()] };
        let s = resolve("file:/Users/q/2023/AD_PDF_2022/ad.pdf", &bare, &index);
        assert_eq!(s, LinkStatus::Found { path: root.join("archive/2023/AD_PDF_2022/ad.pdf"), how: How::Search { score: 2 } });
        match resolve("file:/Users/q/AD_PDF_2022/ad.pdf", &bare, &index) {
            LinkStatus::Ambiguous(c) => assert_eq!(c.len(), 3),
            other => panic!("{other:?}"),
        }
        assert_eq!(resolve("file:/Users/q/none.png", &bare, &index), LinkStatus::Missing);
        // original path
        let uri = format!("file:{}", root.join("x/one.tif").to_string_lossy().replace('\\', "/"));
        assert!(matches!(resolve(&uri, &bare, &FileIndex::empty()), LinkStatus::Found { how: How::Original, .. }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nfd_names_on_disk() {
        let decomposed = uri::nfd("복김_2024.pdf");
        let root = tree("nfd", &[&format!("AD/{decomposed}")]);
        let index = FileIndex::scan(std::slice::from_ref(&root));
        let search = LinkSearch { document_dir: None, roots: vec![root.clone()] };
        // composed in the URI, decomposed on disk (and the reverse via percent-encoded NFD)
        let s = resolve("file:/Users/x/AD/복김_2024.pdf", &search, &index);
        assert!(matches!(s, LinkStatus::Found { how: How::Search { score: 1 }, .. }), "{s:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
