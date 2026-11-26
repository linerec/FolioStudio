use folio_project::{book, manifest};

#[test]
fn book_membership_moves_reopens_and_never_writes_documents() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/bookx-20261023/tests").join(folio_project::util::short_id());
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join("original");manifest::create(&root, "Book").unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/bookx-library.idml");
    let original = std::fs::read(&source).unwrap();
    std::fs::write(root.join("first.idml"), &original).unwrap();
    std::fs::write(root.join("second.idml"), &original).unwrap();
    let path = "book.foliobook";book::create(&root, path, "Book").unwrap();
    assert!(book::read(&root, path).unwrap().documents.is_empty());
    assert!(book::create(&root, path, "Overwrite").is_err());
    let a = book::add(&root, path, "first.idml").unwrap();
    let b = book::add(&root, path, "second.idml").unwrap();assert_ne!(a.id,b.id);
    assert!(book::add(&root, path, "first.idml").is_err());
    assert!(book::add(&root, path, "missing.idml").is_err());
    book::move_to(&root,path,&b.id,0).unwrap();
    let moved = base.join("moved");std::fs::rename(&root,&moved).unwrap();
    let read = book::read(&moved,path).unwrap();assert_eq!(read.documents, vec![b.clone(),a.clone()]);
    assert_eq!(book::inspect(&moved,&a).unwrap(),1);
    let saved = std::fs::read(moved.join(path)).unwrap();
    assert!(book::move_to(&moved,path,&a.id,2).is_err());
    assert!(book::relink(&moved,path,&a.id,"second.idml").is_err());
    // Case-insensitive volumes resolve both spellings to the same document.
    if moved.join("SECOND.idml").exists() {
        assert!(book::relink(&moved,path,&a.id,"SECOND.idml").is_err());
    }
    assert_eq!(std::fs::read(moved.join(path)).unwrap(),saved);
    std::fs::rename(moved.join("first.idml"),moved.join("renamed.idml")).unwrap();
    assert!(book::inspect(&moved,&a).is_err());
    book::relink(&moved,path,&a.id,"renamed.idml").unwrap();
    assert_eq!(book::inspect(&moved,&book::read(&moved,path).unwrap().documents[1]).unwrap(),1);
    book::remove(&moved,path,&b.id).unwrap();
    assert_eq!(book::read(&moved,path).unwrap().documents.len(),1);
    assert_eq!(std::fs::read(moved.join("renamed.idml")).unwrap(),original);
    assert_eq!(std::fs::read(moved.join("second.idml")).unwrap(),original);
    std::fs::write(moved.join("renamed.idml"),b"corrupt").unwrap();
    let saved = std::fs::read(moved.join(path)).unwrap();
    assert!(book::inspect(&moved,&book::read(&moved,path).unwrap().documents[0]).is_err());
    assert!(book::relink(&moved,path,&a.id,"renamed.idml").is_err());
    assert_eq!(std::fs::read(moved.join(path)).unwrap(),saved);
}
