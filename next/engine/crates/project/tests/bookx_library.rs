use folio_model::library::Snapshot;
use folio_project::{library, manifest};

#[test]
fn library_moves_checks_hashes_and_preserves_failed_writes() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/bookx-20261023/tests").join(folio_project::util::short_id());
    let root = base.join("original");
    std::fs::create_dir_all(&base).unwrap();
    manifest::create(&root, "bookx").unwrap();
    let rel = "sample.foliolibrary";
    library::create(&root, rel, "Library").unwrap();
    assert!(library::read(&root, rel).unwrap().assets.is_empty());
    assert!(library::create(&root, rel, "Overwrite").is_err());
    let source = Snapshot { parts: [("Spreads/x.xml".into(), "<Rectangle Self=\"a\"><Unknown flag=\"preserve\"/></Rectangle>".into())].into(), ids: vec!["a".into()], links: Default::default() };
    let asset = library::store(&root, rel, "Item", "Description", &source).unwrap();
    let moved = base.join("moved");
    std::fs::rename(&root, &moved).unwrap();
    assert_eq!(library::snapshot(&moved, &asset).unwrap(), source);
    let path = moved.join(&asset.path);
    let original = std::fs::read(&path).unwrap();
    let saved = std::fs::read(moved.join(rel)).unwrap();
    std::fs::write(&path, b"changed").unwrap();
    assert!(library::snapshot(&moved, &asset).is_err());
    assert!(library::relink(&moved, rel, &asset.id, &asset.path).is_err());
    assert_eq!(std::fs::read(moved.join(rel)).unwrap(), saved);
    std::fs::remove_file(&path).unwrap();
    assert!(library::snapshot(&moved, &asset).is_err());
    std::fs::write(moved.join("replacement.json"), original).unwrap();
    library::relink(&moved, rel, &asset.id, "replacement.json").unwrap();
    let reopened = library::read(&moved, rel).unwrap();
    assert_eq!(reopened.assets[0].description, "Description");
    assert_eq!(library::snapshot(&moved, &reopened.assets[0]).unwrap(), source);
    assert!(folio_project::paths::checked_project_path(&moved, "../escape").is_err());
    library::remove(&moved, rel, &asset.id).unwrap();
    assert!(library::read(&moved, rel).unwrap().assets.is_empty());
    assert!(moved.join("replacement.json").is_file());
}

/// Item information, the kind of an item, several objects in one item, search and order (InDesign probe bookx-c1-library a1: the
/// kind is geometry for a frame and for several objects, text for a text frame, image for a frame with a graphic; the newest
/// item is listed first; an item may keep a name another has; the name and the description can be changed later).
#[test]
fn library_items_have_kind_label_search_order_and_information() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/bookx-c1-20261023/tests").join(folio_project::util::short_id()).join("lib");
    std::fs::create_dir_all(&root).unwrap();
    manifest::create(&root, "bookx").unwrap();
    let rel = "kinds.foliolibrary";
    library::create(&root, rel, "Kinds").unwrap();
    let snap = |xml: &str, ids: &[&str]| Snapshot { parts: [("Spreads/x.xml".into(), xml.to_owned())].into(), ids: ids.iter().map(|s| s.to_string()).collect(), links: Default::default() };
    let rect = snap("<Rectangle Self=\"r\"/>", &["r"]);
    let text = snap("<TextFrame Self=\"t\" ParentStory=\"s\"/>", &["t"]);
    let pic = snap("<Rectangle Self=\"p\"><Image Self=\"i\"/></Rectangle>", &["p"]);
    let two = snap("<Rectangle Self=\"r\"/><Rectangle Self=\"q\"/>", &["r", "q"]);
    let a = library::store_with(&root, rel, "rect", "", "", &rect).unwrap();
    assert_eq!(a.kind, "geometry");
    let b = library::store_with(&root, rel, "Text item", "has words", "mylabel", &text).unwrap();
    assert_eq!(b.kind, "text");
    let c = library::store_with(&root, rel, "pic", "", "", &pic).unwrap();
    assert_eq!(c.kind, "image");
    let d = library::store_with(&root, rel, "rect", "again", "", &two).unwrap();
    assert_eq!((d.kind.as_str(), d.name.as_str()), ("geometry", "rect"));
    assert!(!d.created.is_empty());
    let lib = library::read(&root, rel).unwrap();
    // newest first, as InDesign lists them; the same name twice is allowed
    assert_eq!(library::view(&lib, "", "newest"), [d.id.clone(), c.id.clone(), b.id.clone(), a.id.clone()]);
    assert_eq!(library::view(&lib, "", "oldest"), [a.id.clone(), b.id.clone(), c.id.clone(), d.id.clone()]);
    assert_eq!(library::view(&lib, "", "name"), [c.id.clone(), d.id.clone(), a.id.clone(), b.id.clone()]);
    assert_eq!(library::view(&lib, "", "type"), [d.id.clone(), a.id.clone(), c.id.clone(), b.id.clone()]);
    assert_eq!(library::view(&lib, "WORDS", "newest"), [b.id.clone()]);
    assert_eq!(library::view(&lib, "mylabel", "newest"), [b.id.clone()]);
    assert_eq!(library::view(&lib, "image", "newest"), [c.id.clone()]);
    assert!(library::view(&lib, "nothing", "newest").is_empty());
    // Item Information
    library::update(&root, rel, &a.id, Some("renamed"), Some("d2"), Some("lab")).unwrap();
    let again = library::read(&root, rel).unwrap();
    let a2 = again.assets.iter().find(|x| x.id == a.id).unwrap();
    assert_eq!((a2.name.as_str(), a2.description.as_str(), a2.label.as_str()), ("renamed", "d2", "lab"));
    // a name is kept as given: empty, spaces only or padded, as InDesign does (probe bookx-c4-names)
    for name in ["", "  ", " y ", "a b"] {
        library::update(&root, rel, &a.id, Some(name), None, None).unwrap();
        assert_eq!(library::read(&root, rel).unwrap().assets.iter().find(|x| x.id == a.id).unwrap().name, name);
    }
    assert_eq!(library::store_with(&root, rel, "", "", "", &rect).unwrap().name, "");
    assert!(library::update(&root, rel, "missing", Some("x"), None, None).is_err());
    // an older container (no kind, label or time) still opens
    let old = String::from_utf8(std::fs::read(root.join(rel)).unwrap()).unwrap().replace("\"kind\"", "\"kindx\"");
    std::fs::write(root.join(rel), old).unwrap();
    assert_eq!(library::read(&root, rel).unwrap().assets.len(), 5);
}

/// The asset types InDesign gives a stored frame (probe bookx-c3-lib2): a placed PDF or Illustrator file is a PDF, an EPS an EPS; a
/// kind can also be named, as `Library.store` takes `assetType` (probe bookx-c3-lib), and an unknown one is refused.
#[test]
fn pdf_and_eps_items_and_named_kinds() {
    let root = std::env::temp_dir().join(format!("folio-library-kinds-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    manifest::create(&root, "bookx").unwrap();
    let rel = "types.foliolibrary";
    library::create(&root, rel, "Types").unwrap();
    let snap = |xml: &str, ids: &[&str]| Snapshot { parts: [("Spreads/x.xml".into(), xml.to_owned())].into(), ids: ids.iter().map(|s| s.to_string()).collect(), links: Default::default() };
    let pdf = snap("<Rectangle Self=\"p\"><PDF Self=\"i\"/></Rectangle>", &["p"]);
    let eps = snap("<Rectangle Self=\"p\"><EPS Self=\"i\"/></Rectangle>", &["p"]);
    let rect = snap("<Rectangle Self=\"r\"/>", &["r"]);
    assert_eq!(library::store_with(&root, rel, "pdf", "", "", &pdf).unwrap().kind, "pdf");
    assert_eq!(library::store_with(&root, rel, "eps", "", "", &eps).unwrap().kind, "eps");
    for kind in ["geometry", "text", "image", "pdf", "eps", "page", "structure", "indesignFile"] {
        assert_eq!(library::store_as(&root, rel, kind, "", "", Some(kind), &rect).unwrap().kind, kind);
    }
    assert!(library::store_as(&root, rel, "bad", "", "", Some("movie"), &rect).unwrap_err().contains("unknown asset kind"));
    assert_eq!(library::read(&root, rel).unwrap().assets.len(), 10);
    let _ = std::fs::remove_dir_all(&root);
}

/// Keyed script labels of an item (InDesign `insertLabel` / `extractLabel`, probe bookx-c4-labels): apart from `label`, per item, kept across
/// reopening the library and renaming the item, overwritten by the same key; an empty key, line breaks and long values are allowed and an empty
/// value is the same as none. An older container (no `labels`) still opens and one without labels writes none.
#[test]
fn keyed_labels_are_kept_apart_from_the_label_property() {
    let root = std::env::temp_dir().join(format!("folio-library-labels-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    manifest::create(&root, "bookx").unwrap();
    let rel = "labels.foliolibrary";
    library::create(&root, rel, "Labels").unwrap();
    let snap = Snapshot { parts: [("Spreads/x.xml".into(), "<Rectangle Self=\"r\"/>".to_owned())].into(), ids: vec!["r".into()], links: Default::default() };
    let a = library::store_with(&root, rel, "rect", "", "", &snap).unwrap();
    let b = library::store_with(&root, rel, "other", "", "", &snap).unwrap();
    // a container without keyed labels writes none (an older reader sees nothing new)
    assert!(!String::from_utf8(std::fs::read(root.join(rel)).unwrap()).unwrap().contains("\"labels\""));
    let get = |id: &str, key: &str| { let lib = library::read(&root, rel).unwrap(); library::extract_label(lib.assets.iter().find(|x| x.id == id).unwrap(), key) };
    assert_eq!(get(&a.id, "k1"), "");
    library::insert_label(&root, rel, &a.id, "k1", "v1").unwrap();
    library::insert_label(&root, rel, &a.id, "k2", "v2").unwrap();
    assert_eq!((get(&a.id, "k1").as_str(), get(&a.id, "k2").as_str(), get(&a.id, "nope").as_str()), ("v1", "v2", ""));
    // the label property does not touch them and they do not touch it
    library::update(&root, rel, &a.id, None, None, Some("plain")).unwrap();
    let lib = library::read(&root, rel).unwrap();
    assert_eq!(lib.assets[0].label, "plain");
    assert_eq!(get(&a.id, "k1"), "v1");
    // the same key takes the new value; an empty key, line breaks, unicode and a long value are kept as they are
    library::insert_label(&root, rel, &a.id, "k1", "v1b").unwrap();
    library::insert_label(&root, rel, &a.id, "", "ev").unwrap();
    library::insert_label(&root, rel, &a.id, "nl", "a\rb\nc").unwrap();
    library::insert_label(&root, rel, &a.id, "한글", "값 ü").unwrap();
    let long = "x".repeat(5000);
    library::insert_label(&root, rel, &a.id, "long", &long).unwrap();
    assert_eq!((get(&a.id, "k1").as_str(), get(&a.id, "").as_str(), get(&a.id, "nl").as_str(), get(&a.id, "한글").as_str()), ("v1b", "ev", "a\rb\nc", "값 ü"));
    assert_eq!(get(&a.id, "long"), long);
    // an empty value is the same as none
    library::insert_label(&root, rel, &a.id, "k2", "").unwrap();
    assert_eq!(get(&a.id, "k2"), "");
    assert!(!library::read(&root, rel).unwrap().assets[0].labels.contains_key("k2"));
    // per item, kept when the item is renamed
    assert_eq!(get(&b.id, "k1"), "");
    library::update(&root, rel, &a.id, Some("renamed"), None, None).unwrap();
    assert_eq!(get(&a.id, "k1"), "v1b");
    assert!(library::insert_label(&root, rel, "missing", "k", "v").is_err());
    let _ = std::fs::remove_dir_all(&root);
}
