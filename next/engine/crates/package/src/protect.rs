//! Where a package may not be written.
//!
//! Product rules only — no folder of any particular machine is built in:
//!
//! 1. never into or over the source document's own file;
//! 2. never inside a protected root the user configured (none by default; the app passes
//!    them, and `FOLIO_PROTECTED_ROOTS` adds some for tests);
//! 3. a destination inside the source document's folder or a link search folder that lies in
//!    a Synology Drive sync folder (an ancestor holding `.SynologyWorkingDirectory`) is
//!    allowed with a warning: hundreds of MB written next to the originals there sync back to
//!    the NAS and mix copies with the originals. Users do keep their work on NAS sync
//!    folders, so this is not refused.
//!
//! Paths are compared after resolving junctions and symbolic links (canonical form), and
//! case-insensitively on Windows.

use std::path::{Component, Path, PathBuf};

/// Marker folder Synology Drive Client keeps in each sync root.
pub const SYNOLOGY_MARKER: &str = ".SynologyWorkingDirectory";

/// Roots protected without configuration: none. Which folders to protect is the user's
/// choice (app settings), never a product default.
pub fn default_protected_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn strip_verbatim(s: &str) -> String {
    if let Some(r) = s.strip_prefix(r"\\?\UNC\") {
        // a verbatim UNC path back to its plain UNC spelling (two leading separators)
        [r"\\", r].concat()
    } else if let Some(r) = s.strip_prefix(r"\\?\") {
        r.to_string()
    } else {
        s.to_string()
    }
}

/// Comparison form of a path: canonical (deepest existing ancestor resolved, the rest
/// appended), verbatim prefix removed, lower-cased on Windows.
pub fn normalize(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = abs.as_path();
    let base = loop {
        if let Ok(c) = std::fs::canonicalize(cur) {
            break c;
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                cur = parent;
            }
            _ => break cur.to_path_buf(),
        }
    };
    let mut full = base;
    for r in rest.into_iter().rev() {
        full.push(r);
    }
    let s = strip_verbatim(&full.to_string_lossy());
    PathBuf::from(if cfg!(windows) { s.to_lowercase() } else { s })
}

/// Whether `inner` is `outer` or inside it (both already normalised).
fn within(inner: &Path, outer: &Path) -> bool {
    let a: Vec<Component> = inner.components().collect();
    let b: Vec<Component> = outer.components().collect();
    a.len() >= b.len() && a[..b.len()] == b[..]
}

/// The nearest folder at or above `dir` that is a Synology Drive sync root.
pub fn sync_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find(|a| a.join(SYNOLOGY_MARKER).is_dir()).map(Path::to_path_buf)
}

/// Why a destination is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The destination is the source document's own file (or inside it).
    SourceDocument(PathBuf),
    /// The destination is inside a protected root the user configured.
    Protected { root: PathBuf },
}

/// A destination that is allowed but deserves a word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caution {
    /// The source folder or link search folder the destination is in.
    pub guarded: PathBuf,
    /// The sync root that folder lies in.
    pub sync_root: PathBuf,
}

/// Refuses `dest` when it is the source document itself or inside a configured protected
/// root; otherwise the cautions (destination next to originals in a sync folder).
pub fn check(dest: &Path, source_doc: &Path, link_roots: &[PathBuf], protected: &[PathBuf]) -> Result<Vec<Caution>, Refusal> {
    let dest_n = normalize(dest);
    if within(&dest_n, &normalize(source_doc)) {
        return Err(Refusal::SourceDocument(source_doc.to_path_buf()));
    }
    if let Some(root) = protected.iter().find(|p| within(&dest_n, &normalize(p))) {
        return Err(Refusal::Protected { root: root.clone() });
    }
    let mut guards: Vec<PathBuf> = source_doc.parent().map(Path::to_path_buf).into_iter().collect();
    guards.extend(link_roots.iter().cloned());
    let mut cautions = Vec::new();
    for g in guards {
        if within(&dest_n, &normalize(&g))
            && let Some(sync) = sync_root(&g)
            && !cautions.iter().any(|c: &Caution| c.guarded == g)
        {
            cautions.push(Caution { guarded: g, sync_root: sync });
        }
    }
    Ok(cautions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_machine_defaults() {
        assert!(default_protected_roots().is_empty());
    }

    #[test]
    fn refusals_and_cautions() {
        let base = std::env::temp_dir().join(format!("folio-package-protect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let nas = base.join("NAS");
        let doc_dir = nas.join("magazine").join("잡지원본");
        let photos = nas.join("magazine").join("photos");
        std::fs::create_dir_all(&doc_dir).unwrap();
        std::fs::create_dir_all(&photos).unwrap();
        let doc = doc_dir.join("#347.indd");
        std::fs::write(&doc, b"x").unwrap();
        let elsewhere = base.join("out");
        std::fs::create_dir_all(&elsewhere).unwrap();

        // the document's own file is never a destination
        assert_eq!(check(&doc, &doc, &[], &[]), Err(Refusal::SourceDocument(doc.clone())));
        if cfg!(windows) {
            let upper = PathBuf::from(doc.to_string_lossy().to_uppercase());
            assert!(check(&upper, &doc, &[], &[]).is_err());
        }
        // nothing is protected by default: next to the document is fine
        assert_eq!(check(&doc_dir.join("pkg"), &doc, &[], &[]), Ok(Vec::new()));
        // a configured root refuses anything inside it, at any depth
        let protected = vec![nas.clone()];
        assert_eq!(check(&doc_dir.join("a").join("pkg"), &doc, &[], &protected), Err(Refusal::Protected { root: nas.clone() }));
        assert!(check(&nas.join("other").join("pkg"), &doc, &[], &protected).is_err());
        assert_eq!(check(&elsewhere.join("pkg"), &doc, std::slice::from_ref(&photos), &protected), Ok(Vec::new()));

        // a Synology Drive sync folder: allowed, with a caution, next to the originals only
        let synced = base.join("Drive");
        std::fs::create_dir_all(synced.join(SYNOLOGY_MARKER)).unwrap();
        std::fs::create_dir_all(synced.join("issue")).unwrap();
        std::fs::create_dir_all(synced.join("links")).unwrap();
        let sdoc = synced.join("issue").join("b.indd");
        std::fs::write(&sdoc, b"x").unwrap();
        let c = check(&synced.join("issue").join("pkg"), &sdoc, &[], &[]).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(normalize(&c[0].sync_root), normalize(&synced));
        let c = check(&synced.join("links").join("pkg"), &sdoc, &[synced.join("links")], &[]).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(check(&synced.join("elsewhere"), &sdoc, &[], &[]), Ok(Vec::new()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn junctions_do_not_get_around_it() {
        let base = std::env::temp_dir().join(format!("folio-package-junction-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let nas = base.join("NAS");
        let doc_dir = nas.join("docs");
        std::fs::create_dir_all(&doc_dir).unwrap();
        let doc = doc_dir.join("a.indd");
        std::fs::write(&doc, b"x").unwrap();
        let link = base.join("shortcut");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&doc_dir)
            .output()
            .is_ok_and(|o| o.status.success());
        if made {
            assert!(check(&link.join("pkg"), &doc, &[], std::slice::from_ref(&nas)).is_err());
            let _ = std::fs::remove_dir(&link);
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
