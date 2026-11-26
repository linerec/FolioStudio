//! Copies that sync clients make when two machines change the same file: Synology Drive
//! (`name_<machine>_<date>_Conflict.ext`, `name_CaseConflict.ext`), Dropbox
//! (`name (conflicted copy).ext`, `name (Kim's conflicted copy 2026-10-19).ext`, localised
//! "충돌" variants), Nextcloud (`name (conflicted copy 2026-10-19 101112).ext`) and OneDrive
//! (`name-<MACHINE>.ext` next to `name.ext`). They are reported, never changed.

use crate::paths::{RelPath, collision_key, split_ext};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    Synology,
    Dropbox,
    OneDrive,
    /// Another client's "conflict" naming.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictCopy {
    pub path: RelPath,
    pub kind: ConflictKind,
    /// The file it is a conflicting copy of, when that file is next to it.
    pub original: Option<RelPath>,
}

fn is_machine_token(t: &str) -> bool {
    t.len() >= 3
        && t.len() <= 63
        && t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        && t.chars().any(|c| c.is_ascii_uppercase())
        && !t.starts_with('-')
}

/// What kind of conflict copy a file name is (by name alone; OneDrive needs `has_sibling`).
pub fn classify(name: &str, has_sibling: &dyn Fn(&str) -> bool) -> Option<ConflictKind> {
    let (stem, ext) = split_ext(name);
    let lower = stem.to_lowercase();
    if lower.contains("_conflict") || lower.contains("_caseconflict") {
        return Some(ConflictKind::Synology);
    }
    if let Some(open) = lower.rfind('(')
        && lower.ends_with(')')
    {
        let inner = &lower[open..];
        if inner.contains("conflicted copy") || inner.contains("충돌") || inner.contains("競合") {
            return Some(ConflictKind::Dropbox);
        }
        if inner.contains("conflict") {
            return Some(ConflictKind::Other);
        }
    }
    // OneDrive: "<stem>-<MACHINE>[-<n>]" beside "<stem>.ext"
    let mut base = stem;
    if let Some((b, n)) = base.rsplit_once('-')
        && !n.is_empty()
        && n.chars().all(|c| c.is_ascii_digit())
        && n.len() <= 3
    {
        base = b;
    }
    // machine names may hold hyphens: try every split "<b>-<token>"
    for (i, _) in base.match_indices('-').collect::<Vec<_>>().into_iter().rev() {
        let (b, token) = (&base[..i], &base[i + 1..]);
        if !b.is_empty() && is_machine_token(token) && has_sibling(&format!("{b}{ext}")) {
            return Some(ConflictKind::OneDrive);
        }
    }
    None
}

/// The likely original of a conflict copy among its siblings: the longest sibling stem with
/// the same extension that starts the copy's stem.
fn original_of(name: &str, siblings: &[String]) -> Option<String> {
    let (stem, ext) = split_ext(name);
    siblings
        .iter()
        .filter(|s| s.as_str() != name)
        .filter(|s| {
            let (s_stem, s_ext) = split_ext(s);
            collision_key(s_ext) == collision_key(ext) && stem.len() > s_stem.len() && stem.starts_with(s_stem) && stem[s_stem.len()..].starts_with(['_', ' ', '-', '('])
        })
        .max_by_key(|s| s.len())
        .cloned()
}

/// Every conflict copy in the project (outside `.folio/`).
pub fn scan(root: &Path) -> Vec<ConflictCopy> {
    let mut out = Vec::new();
    let mut by_dir: std::collections::BTreeMap<Option<RelPath>, Vec<String>> = Default::default();
    crate::hash::walk_files(root, &mut |rel, _| {
        by_dir.entry(rel.parent()).or_default().push(rel.file_name().to_string());
    });
    for (dir, names) in by_dir {
        let keys: std::collections::HashSet<String> = names.iter().map(|n| collision_key(n)).collect();
        for name in &names {
            let Some(kind) = classify(name, &|s| keys.contains(&collision_key(s))) else { continue };
            let rel = |n: &str| match &dir {
                Some(d) => d.join(n).ok(),
                None => RelPath::parse(n).ok(),
            };
            let Some(path) = rel(name) else { continue };
            out.push(ConflictCopy { path, kind, original: original_of(name, &names).and_then(|o| rel(&o)) });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_each_client() {
        let siblings = ["report.folio", "cover.folio", "page.folio"];
        let has = |s: &str| siblings.contains(&s);
        let k = |n: &str| classify(n, &has);
        assert_eq!(k("report_DESKTOP-1A2B_Oct-19-2026_Conflict.folio"), Some(ConflictKind::Synology));
        assert_eq!(k("report_CaseConflict.folio"), Some(ConflictKind::Synology));
        assert_eq!(k("report (conflicted copy).folio"), Some(ConflictKind::Dropbox));
        assert_eq!(k("report (Kim's conflicted copy 2026-10-19).folio"), Some(ConflictKind::Dropbox));
        assert_eq!(k("report (김의 충돌된 사본 2026-10-19).folio"), Some(ConflictKind::Dropbox));
        assert_eq!(k("report (conflicted copy 2026-10-19 101112).folio"), Some(ConflictKind::Dropbox));
        assert_eq!(k("report-DESKTOP-7K2L.folio"), Some(ConflictKind::OneDrive));
        assert_eq!(k("report-LAPTOP-AB12-2.folio"), Some(ConflictKind::OneDrive));
        // not conflicts
        assert_eq!(k("report.folio"), None);
        assert_eq!(k("page-2.folio"), None);
        assert_eq!(k("cover-final.folio"), None);
        assert_eq!(k("other-DESKTOP-7K2L.folio"), None, "no sibling 'other.folio'");
        assert_eq!(k("conflict-resolution.md"), None);
    }

    #[test]
    fn scan_reports_copies_with_their_originals() {
        let root = std::env::temp_dir().join(format!("folio-conflicts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("assets/347")).unwrap();
        for f in ["347.folio", "347_NAS_Oct-19-2026_Conflict.folio", "assets/347/a.jpg", "assets/347/a (conflicted copy).jpg", "assets/347/b-PC01.jpg"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        std::fs::create_dir_all(root.join(".folio/cache")).unwrap();
        std::fs::write(root.join(".folio/cache/x_Conflict.json"), b"x").unwrap();
        let mut found = scan(&root);
        found.sort_by(|a, b| a.path.cmp(&b.path));
        let names: Vec<(&str, Option<&str>)> = found.iter().map(|c| (c.path.as_str(), c.original.as_ref().map(RelPath::as_str))).collect();
        assert_eq!(names, vec![("347_NAS_Oct-19-2026_Conflict.folio", Some("347.folio")), ("assets/347/a (conflicted copy).jpg", Some("assets/347/a.jpg"))]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
