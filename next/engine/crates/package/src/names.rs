//! File names inside one package folder.
//!
//! Two names collide when they are equal after NFC normalisation and lower-casing: NTFS
//! compares names case-insensitively, and macOS (where many print houses open packages)
//! treats NFC and NFD spellings of a Korean name as the same file. The first name is kept
//! exactly as it was spelled on disk (NFD stays NFD); later ones get " (2)", " (3)", …
//! before the extension.

use folio_links::uri::name_key;
use std::collections::HashSet;

/// Names already used in one folder of the package.
#[derive(Clone, Debug, Default)]
pub struct NameTable {
    taken: HashSet<String>,
}

impl NameTable {
    pub fn new() -> NameTable {
        NameTable::default()
    }

    /// Whether `name` (or a spelling that collides with it) is taken.
    pub fn contains(&self, name: &str) -> bool {
        self.taken.contains(&name_key(name))
    }

    /// Takes `name` as it is; false when it collides with a name already taken.
    pub fn reserve(&mut self, name: &str) -> bool {
        self.taken.insert(name_key(name))
    }

    /// `name` when it is free, else the first free "stem (n).ext" (n ≥ 2). The result is
    /// taken.
    pub fn unique(&mut self, name: &str) -> String {
        if self.reserve(name) {
            return name.to_string();
        }
        let (stem, ext) = split_ext(name);
        for n in 2.. {
            let candidate = format!("{stem} ({n}){ext}");
            if self.reserve(&candidate) {
                return candidate;
            }
        }
        unreachable!("unbounded counter")
    }
}

/// Splits "photo.final.jpg" into ("photo.final", ".jpg"). A leading dot is part of the stem
/// (".hidden" has no extension), and so is everything when there is no dot.
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_name_kept_rest_suffixed() {
        let mut t = NameTable::new();
        assert_eq!(t.unique("photo.jpg"), "photo.jpg");
        assert_eq!(t.unique("photo.jpg"), "photo (2).jpg");
        assert_eq!(t.unique("photo.jpg"), "photo (3).jpg");
        // NTFS: case-insensitive
        assert_eq!(t.unique("PHOTO.JPG"), "PHOTO (4).JPG");
        // a real file already called "photo (5).jpg" keeps its name; the counter skips it
        assert_eq!(t.unique("photo (5).jpg"), "photo (5).jpg");
        assert_eq!(t.unique("photo.jpg"), "photo (6).jpg");
    }

    #[test]
    fn nfc_and_nfd_collide_but_keep_their_spelling() {
        let nfc = "복김_2024.pdf";
        let nfd = folio_links::uri::nfd(nfc);
        assert_ne!(nfc, nfd);
        let mut t = NameTable::new();
        // the first spelling is kept exactly (NFD stays NFD)
        assert_eq!(t.unique(&nfd), nfd);
        assert!(t.contains(nfc));
        let second = t.unique(nfc);
        assert_eq!(second, "복김_2024 (2).pdf");
    }

    #[test]
    fn extensions() {
        assert_eq!(split_ext("a.tar.gz"), ("a.tar", ".gz"));
        assert_eq!(split_ext(".hidden"), (".hidden", ""));
        assert_eq!(split_ext("README"), ("README", ""));
        assert_eq!(split_ext("dot."), ("dot.", ""));
        let mut t = NameTable::new();
        assert_eq!(t.unique("README"), "README");
        assert_eq!(t.unique("readme"), "readme (2)");
        assert_eq!(t.unique(".hidden"), ".hidden");
        assert_eq!(t.unique(".hidden"), ".hidden (2)");
    }

    #[test]
    fn reserved_root_names_push_documents_aside() {
        let mut t = NameTable::new();
        for n in ["Links", "Document fonts", "Instructions.txt", "package-report.json"] {
            assert!(t.reserve(n));
        }
        assert_eq!(t.unique("instructions.TXT"), "instructions (2).TXT");
        assert!(!t.reserve("links"));
    }
}
