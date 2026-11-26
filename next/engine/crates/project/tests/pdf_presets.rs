use folio_project::{manifest, presets};
use serde_json::{Value, json};

#[test]
fn portable_atomic_presets_reject_unsafe_names_and_follow_project_move() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../artifacts/pdfexport-20261022/tests")
        .join(format!("presets-{}", folio_project::util::short_id()));
    manifest::create(&root, "pdf presets").unwrap();
    let p = json!({"version":1,"name":"인쇄소","standard":"X4"});
    presets::save(&root, "인쇄소", &p).unwrap();
    let listed = presets::list::<Value>(&root).unwrap();
    assert_eq!(listed, vec![("인쇄소".into(), p.clone())]);
    for invalid in ["../escape", "C:\\preset", "/preset", "CON", "name.", "a/b"] {
        assert!(presets::save(&root, invalid, &p).is_err(), "{invalid}");
    }
    presets::save(&root, "Press", &p).unwrap();
    assert!(presets::save(&root, "press", &p).is_err());
    let moved = root.with_extension("moved");
    std::fs::rename(&root, &moved).unwrap();
    assert_eq!(presets::list::<Value>(&moved).unwrap().len(), 2);
    presets::delete(&moved, "Press").unwrap();
    assert_eq!(presets::list::<Value>(&moved).unwrap().len(), 1);
}
