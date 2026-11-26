//! Requires FOLIO_INDD_USER_INPUT for the private local real-document oracle.
//! Since fork revision folio.4 the fork writes effects and stroke styles the
//! upstream converter leaves out (third_party/indd-large/FOLIO_FORK.txt), so
//! its IDML is compared between its own buffered and file-backed routes;
//! previews and database objects are still compared with upstream.
#[test]
fn file_backed_model_is_byte_identical_to_buffered_fork_on_real_fixture() {
    let Ok(path) = std::env::var("FOLIO_INDD_USER_INPUT") else {
        eprintln!("private fixture absent; real-file equivalence not executed");
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let before = indd::convert(&bytes, "same-document").unwrap();
    let corrected = indd_large::convert(&bytes, "same-document").unwrap();
    assert_eq!(before.previews.len(), corrected.previews.len());
    for (old, new) in before.previews.iter().zip(&corrected.previews) {
        assert_eq!(old.data, new.data, "preview pixel bytes remain unchanged");
        assert_eq!(
            old.graphics, new.graphics,
            "preview ownership remains unchanged"
        );
    }
    let container = indd::Container::parse(&bytes).unwrap();
    let (db, header, _) =
        indd_large::Database::open_file(std::fs::File::open(&path).unwrap()).unwrap();
    let mut doc = indd_large::model::Reader::new(&db)
        .document(header.version)
        .unwrap();
    if let Some(packet) = container.xmp().unwrap() {
        doc.read_xmp(packet);
    }
    let mut after = Vec::new();
    indd_large::idml::write(&doc, "same-document", &mut after).unwrap();
    assert_eq!(
        corrected.idml, after,
        "file-backed storage must not reinterpret full model"
    );
    let original_db = container.database().unwrap();
    for (uid, class) in db.classes() {
        if *class != 0xffffffff {
            let old = original_db.object(*uid).unwrap();
            let new = db.object(*uid).unwrap();
            assert_eq!(old, new, "object {uid}");
        }
    }
}
