//! File > New > Book... / Library... as InDesign 2025 does them (`next/oracle/rules/booklib-c1.ko.md`), kept inside the project.
use folio_project::{book, library, manifest};
use std::path::{Path, PathBuf};

fn project(tag: &str) -> PathBuf {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/booklib-c1-20261023/tests").join(folio_project::util::short_id());
    let root = base.join(tag);
    std::fs::create_dir_all(&base).unwrap();
    manifest::create(&root, "booklib").unwrap();
    root
}

#[test]
fn a_new_book_is_empty_named_by_its_file_and_replaced_only_when_asked() {
    let root = project("book");
    // no extension typed: it is added; the book is named like its file (InDesign: `bl-new.indb`)
    let made = book::create_in(&root, "", "Book 1", false, &[]).unwrap();
    assert_eq!((made.path.as_str(), made.name.as_str(), made.replaced), ("Book 1.foliobook", "Book 1.foliobook", false));
    let b = book::read(&root, &made.path).unwrap();
    assert_eq!(b.name, "Book 1.foliobook");
    assert!(b.documents.is_empty());
    // the same name again: the engine says so and writes nothing (InDesign asks "already exists, replace it?"); Yes replaces it with an empty book
    assert_eq!(book::create_in(&root, "", "Book 1", false, &[]), Err("exists: Book 1.foliobook".into()));
    let mut b = book::read(&root, &made.path).unwrap();
    b.name = "kept".into();
    std::fs::write(root.join(&made.path), serde_json::to_vec(&b).unwrap()).unwrap();
    assert_eq!(book::read(&root, &made.path).unwrap().name, "kept");
    assert_eq!(book::create_in(&root, "", "Book 1", false, &[]), Err("exists: Book 1.foliobook".into()));
    assert_eq!(book::read(&root, &made.path).unwrap().name, "kept", "a refused name leaves the file alone");
    // the book is open in a window: the replace question comes first, then the refusal, and the file is as it was
    let open = vec!["Book 1.foliobook".to_string()];
    assert_eq!(book::create_in(&root, "", "Book 1", false, &open), Err("exists: Book 1.foliobook".into()));
    assert_eq!(book::create_in(&root, "", "Book 1", true, &open), Err("open: Book 1.foliobook".into()));
    assert_eq!(book::read(&root, &made.path).unwrap().name, "kept");
    let again = book::create_in(&root, "", "Book 1.foliobook", true, &[]).unwrap();
    assert!(again.replaced);
    assert_eq!(book::read(&root, &again.path).unwrap().name, "Book 1.foliobook");
    // in a folder of the project, with a Korean name; a folder that is not there or outside the project is refused
    std::fs::create_dir(root.join("책")).unwrap();
    let in_folder = book::create_in(&root, "책", "첫 책", false, &[]).unwrap();
    assert_eq!(in_folder.path, "책/첫 책.foliobook");
    assert_eq!(book::create_in(&root, "없음", "x", false, &[]), Err("folder_missing: 없음".into()));
    assert!(book::create_in(&root, "../밖", "x", false, &[]).is_err());
    assert!(!root.join("..").join("밖").exists());
    // the whole project moves and the book is still there
    let moved = root.parent().unwrap().join("moved");
    std::fs::rename(&root, &moved).unwrap();
    assert_eq!(book::read(&moved, "책/첫 책.foliobook").unwrap().name, "첫 책.foliobook");
}

#[test]
fn a_new_library_is_empty_named_by_its_file_and_replaced_only_when_asked() {
    let root = project("library");
    let made = library::create_in(&root, "", "Library", false, &[]).unwrap();
    assert_eq!((made.path.as_str(), made.name.as_str()), ("Library.foliolibrary", "Library.foliolibrary"));
    let lib = library::read(&root, &made.path).unwrap();
    assert_eq!(lib.name, "Library.foliolibrary");
    assert!(lib.assets.is_empty());
    assert_eq!(library::create_in(&root, "", "library", false, &[]), Err("name_collision: Library.foliolibrary".into()));
    assert_eq!(library::create_in(&root, "", "Library", false, &[]), Err("exists: Library.foliolibrary".into()));
    assert_eq!(library::create_in(&root, "", "Library", true, &["Library.foliolibrary".into()]), Err("open: Library.foliolibrary".into()));
    assert!(library::create_in(&root, "", "Library", true, &[]).unwrap().replaced);
    assert_eq!(library::create_in(&root, "", "  ", false, &[]), Err("name_required".into()));
    // a book and a library of the same stem do not meet: the extensions differ
    assert!(book::create_in(&root, "", "Library", false, &[]).is_ok());
}
