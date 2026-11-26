use folio_model::library::Snapshot;
use folio_project::{library, manifest};

#[test]
fn library_copies_links_by_content_survives_move_and_rejects_changed_bytes() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/bookx-c2-20261023/tests").join(folio_project::util::short_id());
    let root = base.join("original");
    std::fs::create_dir_all(&base).unwrap();
    manifest::create(&root, "links").unwrap();
    library::create(&root, "test.foliolibrary", "Links").unwrap();
    let mut assets = Vec::new();
    for (i, bytes) in [b"first".as_slice(), b"second".as_slice()].iter().enumerate() {
        let folder = base.join(format!("source-{i}"));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("same.png");
        std::fs::write(&file, bytes).unwrap();
        let uri = folio_project::paths::file_uri(&file);
        let mut snapshot = Snapshot { parts: [("Spreads/test.xml".into(), format!("<Rectangle Self=\"r\"><Image Self=\"i\"><Link Self=\"l\" LinkResourceURI=\"{uri}\"/></Image></Rectangle>"))].into(), ids: vec!["r".into()], links: Default::default() };
        library::links::capture(&root, &mut snapshot, |u| (u == uri).then(|| file.clone())).unwrap();
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("file:") && !serialized.contains("source-"));
        assets.push(library::store(&root, "test.foliolibrary", "Same", "", &snapshot).unwrap());
        std::fs::remove_file(file).unwrap();
    }
    let moved = base.join("moved");
    std::fs::rename(&root, &moved).unwrap();
    let a = library::snapshot(&moved, &assets[0]).unwrap();
    let b = library::snapshot(&moved, &assets[1]).unwrap();
    let a = a.links.values().next().unwrap();
    let b = b.links.values().next().unwrap();
    assert_ne!(a.path, b.path);
    assert_eq!(std::fs::read(library::links::resolve(&moved, a).unwrap()).unwrap(), b"first");
    assert_eq!(std::fs::read(library::links::resolve(&moved, b).unwrap()).unwrap(), b"second");
    let container = std::fs::read(moved.join("test.foliolibrary")).unwrap();
    std::fs::write(moved.join(&a.path), b"changed").unwrap();
    assert!(library::snapshot(&moved, &assets[0]).is_err());
    assert!(library::relink(&moved, "test.foliolibrary", &assets[0].id, &assets[0].path).is_err());
    assert_eq!(std::fs::read(moved.join("test.foliolibrary")).unwrap(), container);
    assert!(library::snapshot(&moved, &assets[1]).is_ok());
}
