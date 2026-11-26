#[path = "support/inventory.rs"]
mod inventory;
use folio_model::interop::{self, Mapping};
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn every_model_declaration_is_registered() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = inventory::inventory(&root.join("src"));
    let entries = interop::entries().unwrap();
    let registered: BTreeSet<_> = entries.iter().map(|e| e.item.to_string()).collect();
    let missing: Vec<_> = expected.difference(&registered).collect();
    let stale: Vec<_> = registered.difference(&expected).collect();
    assert!(missing.is_empty() && stale.is_empty(), "unregistered: {missing:?}; stale: {stale:?}");
    let unknown = entries.iter().filter(|e| e.mapping == Mapping::Unclassified).count();
    println!("interop: {} registered, {} unclassified", entries.len(), unknown);
    // Every model declaration is exact, approximate or Folio-only (interop-c2, 2026-11-19). A new field starts classified, with its evidence.
    let open: Vec<_> = entries.iter().filter(|e| e.mapping == Mapping::Unclassified).map(|e| e.item).collect();
    assert!(open.is_empty(), "unclassified model items (classify each as exact, approximate or folio-only with a case under oracle/goldens): {open:?}");
    for e in entries.iter().filter(|e| e.mapping != Mapping::Unclassified) {
        for evidence in e.evidence.split(';') {
            assert!(root.join("../../../oracle/goldens").join(evidence).is_dir(), "{}: missing evidence {evidence}", e.item);
        }
    }
}

#[test]
fn inventory_detects_optional_tuple_enum_and_inline_fields() {
    let source = r#"
        pub struct S { pub field: Option<String> }
        pub enum E { Empty, Named { hidden: bool }, Tuple(u32) }
        pub struct Tuple(pub u32);
        mod nested { pub struct Added { pub x: f64 } }
    "#;
    let got = inventory::declarations(source, "");
    for key in ["S.field", "E::Empty", "E::Named.hidden", "E::Tuple.0", "Tuple.0", "nested::Added.x"] {
        assert!(got.contains(key), "{key}");
    }
    let mut registered = got.clone();
    registered.remove("S.field");
    assert_eq!(got.difference(&registered).collect::<Vec<_>>(), vec!["S.field"]);
}

#[test]
fn catalog_rejects_duplicates_unknown_status_and_unsupported_claims() {
    assert!(interop::parse("A\texact\tsame\t").is_err());
    assert!(interop::parse("A\tunknown\tunknown\t").is_err());
    assert!(interop::parse("A\tunclassified\tunknown\t\nA\tunclassified\tunknown\t").is_err());
    assert!(interop::parse("A\tunclassified\tunknown\t").is_ok());
    assert!(interop::report(["NoSuchModel.field"]).is_err());
    assert_eq!(interop::lookup("Affine.tx").unwrap().unwrap().mapping, Mapping::Exact);
    let entries = interop::entries().unwrap();
    let non_exact = entries.iter().find(|e| e.mapping != Mapping::Exact).unwrap().item;
    let differences = interop::report(["Affine.tx", non_exact, non_exact]).unwrap();
    assert_eq!(differences.len(), 1);
    assert_eq!(differences[0].item, non_exact);
}
