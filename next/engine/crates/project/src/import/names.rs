//! Plans portable file names and resolves name collisions.

use crate::paths::{RelPath, collision_key, fix_name, numbered_name, split_ext};
#[cfg(test)]
use crate::paths;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Names already planned or present in each project folder.
#[derive(Default)]
pub(super) struct Names {
    taken: HashMap<String, HashSet<String>>,
}

impl Names {
    fn folder(&mut self, root: &Path, dir: &RelPath) -> &mut HashSet<String> {
        self.taken.entry(dir.as_str().to_string()).or_insert_with(|| {
            std::fs::read_dir(dir.on_disk(root))
                .map(|rd| rd.filter_map(Result::ok).map(|e| collision_key(&e.file_name().to_string_lossy())).collect())
                .unwrap_or_default()
        })
    }

    /// A free portable name in `dir` for `wanted`; the fixed name when it had to change.
    pub(super) fn unique(&mut self, root: &Path, dir: &RelPath, wanted: &str) -> (String, bool) {
        let fixed = fix_name(wanted);
        let changed = fixed != wanted;
        let taken = self.folder(root, dir);
        if taken.insert(collision_key(&fixed)) {
            return (fixed, changed);
        }
        for n in 2.. {
            let c = numbered_name(&fixed, n);
            if taken.insert(collision_key(&c)) {
                return (c, true);
            }
        }
        unreachable!()
    }
}

pub(super) fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The name the manifest records as a file's `originalName`: normalized to NFC like every other name Folio stores (a Mac writes
/// Korean file names decomposed, NFD; the record is for reading, not for finding the file).
pub(super) fn recorded_name(disk: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    disk.nfc().collect()
}

/// `<stem> 2.folio`, `<stem> 3.folio`, … next to `document`.
pub fn free_document_name(root: &Path, document: &RelPath) -> Option<RelPath> {
    let (stem, ext) = split_ext(document.file_name());
    (2..1000).find_map(|n| {
        let name = format!("{stem} {n}{ext}");
        let rel = match document.parent() {
            Some(d) => d.join(&name).ok()?,
            None => RelPath::parse(&name).ok()?,
        };
        (!rel.on_disk(root).exists()).then_some(rel)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn bugfind_collision_respects_name_byte_limit() {
        let dir = super::RelPath::parse("assets").unwrap();
        let root = std::path::Path::new("unused-test-root");
        for wanted in [format!("{}.png", "a".repeat(251)), format!("{}.png", "한".repeat(83)), format!("a.{}", "x".repeat(253))] {
            let mut names = super::Names::default();
            names.taken.insert("assets".into(), Default::default());
            assert_eq!(names.unique(root, &dir, &wanted), (wanted.clone(), false));
            for n in 2..=12 {
                let (name, changed) = names.unique(root, &dir, &wanted);
                assert!(changed);
                assert!(name.contains(&format!(" ({n})")), "{name}");
                assert!(super::paths::check_name(&name).is_ok(), "{} bytes: {name}", name.len());
                assert!(dir.join(&name).is_ok());
            }
        }
    }
    use super::recorded_name;
    use unicode_normalization::UnicodeNormalization;

    /// The manifest's `originalName` is NFC even when the file on disk (a Mac's) spells its Korean name decomposed (principles check F2).
    #[test]
    fn the_recorded_original_name_is_nfc() {
        let nfc = "표지 그림.jpg";
        let nfd: String = nfc.nfd().collect();
        assert_ne!(nfd, nfc);
        assert_eq!(recorded_name(&nfd), nfc);
        assert_eq!(recorded_name(nfc), nfc);
        assert_eq!(recorded_name("a?.jpg"), "a?.jpg");
    }
}
