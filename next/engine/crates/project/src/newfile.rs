//! Where File > New > Book... and Library... put their file: the answer of InDesign's "New Book" / "New Library" Save dialog,
//! kept inside the project (principle 1: a book or a library file lives in the project folder and is named by a project-relative path).
//!
//! What InDesign 2025 does (measured through its dialog, `next/oracle/rules/booklib-c1.ko.md`): the dialog proposes a name
//! (`Book 1.indb`, `Library.indl`) in a folder, the typed name gets the extension when it has none, a name that exists asks
//! "<name> already exists. Do you want to replace it?" (Yes replaces the file with a new empty book or library, No goes back to the dialog),
//! and a file that is already open as a book or library is not saved over ("Cannot save to <path> because it is already open").
//! Folio differences: the folder is a project folder (anything outside the project is refused), a name with another extension gets the
//! Folio extension appended too (the container is recognised by it), and "already open" is a fact of the window that has the file open, so
//! the screen checks it (the engine keeps no list of open books).
use crate::paths::{NameIssue, RelPath, check_name, collision_key};
use std::path::Path;

/// A file chosen in a New dialog, resolved against the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Project-relative path (`/` separators, NFC).
    pub path: String,
    /// The file name with its extension (what InDesign calls the name of the book or library).
    pub file_name: String,
    /// A file of that name is already there.
    pub exists: bool,
}

/// `folder` is a project-relative folder (`""` is the project folder), `file_name` what the person typed, `extension` the Folio extension with its dot.
/// Errors are stable codes the screen and a script can tell apart: `name_required`, `bad_name: <why>`, `folder_missing: <folder>`,
/// `outside_project`, `name_collision: <existing name>` (a name that differs from an existing one only by case or Unicode form).
pub fn resolve(root: &Path, folder: &str, file_name: &str, extension: &str) -> Result<Target, String> {
    let typed = file_name.trim();
    let mut name = typed.to_string();
    // the extension is added when it is not there (InDesign: no extension typed -> added; Folio also for another extension) and written in its one spelling
    let cut = name.len().saturating_sub(extension.len());
    if name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(extension) {
        name.truncate(cut);
    }
    if name.is_empty() {
        return Err("name_required".into());
    }
    name.push_str(extension);
    check_name(&name).map_err(|issue: NameIssue| format!("bad_name: {issue}"))?;
    // normalised the way the project stores names
    let name: String = RelPath::parse(&name).map_err(|e| format!("bad_name: {e}"))?.as_str().to_string();
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    crate::manifest::load(&root).map_err(|e| e.to_string())?;
    let folder = folder.trim().trim_matches('/');
    let dir = if folder.is_empty() {
        root.clone()
    } else {
        let rel = RelPath::parse(folder).map_err(|e| format!("bad_name: {e}"))?;
        let dir = rel.to_native(&root);
        if !dir.is_dir() {
            return Err(format!("folder_missing: {folder}"));
        }
        // a folder reached through a link out of the project is outside
        if !dir.canonicalize().map_err(|e| e.to_string())?.starts_with(&root) {
            return Err("outside_project".into());
        }
        dir
    };
    let key = collision_key(&name);
    let mut exists = false;
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.filter_map(Result::ok) {
        let existing = entry.file_name().to_string_lossy().to_string();
        if collision_key(&existing) != key {
            continue;
        }
        if existing == name && entry.path().is_file() {
            exists = true;
        } else {
            return Err(format!("name_collision: {existing}"));
        }
    }
    let path = if folder.is_empty() { name.clone() } else { format!("{}/{name}", RelPath::parse(folder).map_err(|e| format!("bad_name: {e}"))?.as_str()) };
    Ok(Target { path, file_name: name, exists })
}

/// The explorer's New Folder (uix-c1): an empty folder `name` in `folder` (project-relative, `""` the project folder), with the
/// name rules and error codes of `resolve` (`name_required`, `bad_name: …`, `folder_missing: …`, `outside_project`); any file or
/// folder of that name, also one differing only by case or Unicode form, is `name_collision: <existing>`. Answers the new folder's
/// project-relative path. Nothing is replaced and nothing outside the project is made.
pub fn make_folder(root: &Path, folder: &str, name: &str) -> Result<String, String> {
    let target = resolve(root, folder, name, "")?;
    if target.exists {
        return Err(format!("name_collision: {}", target.file_name));
    }
    std::fs::create_dir(target_dir(root, &target.path)?).map_err(|e| e.to_string())?;
    Ok(target.path)
}

fn target_dir(root: &Path, rel: &str) -> Result<std::path::PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    Ok(RelPath::parse(rel).map_err(|e| format!("bad_name: {e}"))?.to_native(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_folder_follows_the_name_rules_and_never_replaces() {
        let root = project("folder");
        assert_eq!(make_folder(&root, "", "그림").unwrap(), "그림");
        assert!(root.join("그림").is_dir());
        assert_eq!(make_folder(&root, "그림", "표지").unwrap(), "그림/표지");
        assert_eq!(make_folder(&root, "", "그림"), Err("name_collision: 그림".into()));
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        assert_eq!(make_folder(&root, "", "A.TXT"), Err("name_collision: a.txt".into()));
        assert_eq!(make_folder(&root, "", "a.txt"), Err("name_collision: a.txt".into()));
        assert!(make_folder(&root, "", "a:b").unwrap_err().starts_with("bad_name"));
        assert_eq!(make_folder(&root, "", "  "), Err("name_required".into()));
        assert_eq!(make_folder(&root, "없음", "x"), Err("folder_missing: 없음".into()));
        assert!(make_folder(&root, "../밖", "x").is_err());
    }

    fn project(tag: &str) -> std::path::PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/booklib-c1-20261023/tests").join(crate::util::short_id()).join(tag);
        std::fs::create_dir_all(&root).unwrap();
        crate::manifest::create(&root, "시험").unwrap();
        root
    }

    #[test]
    fn extension_is_added_and_the_project_stays_the_limit() {
        let root = project("resolve");
        std::fs::create_dir(root.join("책")).unwrap();
        let t = resolve(&root, "", "Book 1", ".foliobook").unwrap();
        assert_eq!((t.path.as_str(), t.file_name.as_str(), t.exists), ("Book 1.foliobook", "Book 1.foliobook", false));
        // typed with the extension (any case), spaces around the name are not part of it
        assert_eq!(resolve(&root, "", "  Book 1.FOLIOBOOK ", ".foliobook").unwrap().file_name, "Book 1.foliobook");
        // another extension: the Folio one is appended (InDesign keeps the typed name; Folio recognises the container by its extension)
        assert_eq!(resolve(&root, "책", "x.txt", ".foliobook").unwrap().path, "책/x.txt.foliobook");
        assert_eq!(resolve(&root, "", "", ".foliobook"), Err("name_required".into()));
        assert_eq!(resolve(&root, "", "  ", ".foliobook"), Err("name_required".into()));
        assert_eq!(resolve(&root, "", ".foliobook", ".foliobook"), Err("name_required".into()));
        assert!(resolve(&root, "", "a/b", ".foliobook").unwrap_err().starts_with("bad_name"));
        assert!(resolve(&root, "", "a:b", ".foliobook").unwrap_err().starts_with("bad_name"));
        assert!(resolve(&root, "", "CON", ".foliobook").unwrap_err().starts_with("bad_name"));
        assert_eq!(resolve(&root, "없는 폴더", "x", ".foliobook"), Err("folder_missing: 없는 폴더".into()));
        assert!(resolve(&root, "../밖", "x", ".foliobook").unwrap_err().starts_with("bad_name"));
        assert!(resolve(&root, "C:/밖", "x", ".foliobook").is_err());
    }

    #[test]
    fn an_existing_name_is_found_and_a_case_twin_is_refused() {
        let root = project("exists");
        std::fs::write(root.join("Book 1.foliobook"), b"x").unwrap();
        assert!(resolve(&root, "", "Book 1", ".foliobook").unwrap().exists);
        assert_eq!(resolve(&root, "", "book 1", ".foliobook"), Err("name_collision: Book 1.foliobook".into()));
        std::fs::create_dir(root.join("폴더.foliobook")).unwrap();
        assert_eq!(resolve(&root, "", "폴더", ".foliobook"), Err("name_collision: 폴더.foliobook".into()));
    }
}
