//! Data Merge ▸ Select Data Source against InDesign 2025 (probe `datamerge-c4-a`, templates `oracle/fixtures/datamerge-merge`): the
//! preferences of InDesign's own template exports hold `<DataMerge DataSourceFileType=… DataSourceFile=… />` as the first element,
//! in front of `DataMergeOption`. Folio's `selectDataSource` must write the same element (its `DataSourceFile` is a project
//! reference, never the path of a computer); `removeDataSource` leaves a document that selecting brings back byte for byte.
use folio_edit::commands::RegisteredCommand;
use folio_edit::{Command, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

const PART: &str = "Resources/Preferences.xml";

fn oracle() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

fn package(name: &str) -> BTreeMap<String, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(oracle().join("fixtures/datamerge-merge").join(format!("{name}.idml"))).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() { continue; }
        let name = e.name().unwrap().into_owned();
        if name.ends_with(".xml") || name == "mimetype" {
            let mut text = String::new();
            e.read_to_string(&mut text).unwrap();
            parts.insert(name, text);
        }
    }
    parts
}

fn select(source: Option<&str>, kind: Option<&str>) -> Command {
    RegisteredCommand::SelectDataSource { path: "data/people.csv".into(), source: source.map(str::to_owned), file_type: kind.map(str::to_owned), delimiter: None, encoding: None, platform: None, preserve_spaces: None }.into()
}

fn remove() -> Command {
    RegisteredCommand::RemoveDataSource {}.into()
}

/// The range of the `DataMerge` element InDesign wrote (with the path of the machine it ran on).
fn element(text: &str) -> (usize, usize) {
    let start = text.find("<DataMerge ").expect("DataMerge element");
    (start, start + text[start..].find("/>").unwrap() + 2)
}

#[test]
fn select_data_source_writes_what_indesign_writes() {
    for name in ["exactfit", "blanks", "fitc2f"] {
        let mut ed = Editor::new(package(name));
        let original = ed.part(PART).unwrap().to_owned();
        let (start, end) = element(&original);
        let measured = &original[start..end];
        assert!(measured.starts_with("<DataMerge DataSourceFileType=\"CommaSeparated\" DataSourceFile=\"") && measured.ends_with(".csv\" />"), "{name}: {measured}");
        // a document without the source: removing leaves InDesign's text without the element and its line
        ed.apply("remove", &[remove()]).unwrap();
        let without = ed.part(PART).unwrap().to_owned();
        assert!(!without.contains("<DataMerge ") && without.contains("<DataMergeOption "), "{name}");
        assert_eq!(without.len() + measured.len() + "\n\t".len(), original.len(), "{name}: only the element and its line went");
        let expected = original.replacen(measured, "<DataMerge DataSourceFileType=\"CommaSeparated\" DataSourceFile=\"folio:path-ref/r1\" />", 1);
        // selecting puts the same element back in the same place
        ed.apply("select", &[select(Some("folio:path-ref/r1"), Some("CommaSeparated"))]).unwrap();
        assert_eq!(ed.part(PART).unwrap(), expected, "{name}");
        // one undo step each
        assert!(ed.undo().is_some());
        assert_eq!(ed.part(PART).unwrap(), without, "{name}: undo of select");
        assert!(ed.undo().is_some());
        assert_eq!(ed.part(PART).unwrap(), original, "{name}: undo of remove");
        assert!(ed.redo().is_some() && ed.redo().is_some());
        assert_eq!(ed.part(PART).unwrap(), expected, "{name}: redo");
    }
}

#[test]
fn selecting_again_replaces_the_source_and_keeps_other_attributes() {
    let mut ed = Editor::new(package("exactfit"));
    ed.apply("a", &[select(Some("folio:path-ref/r1"), Some("CommaSeparated"))]).unwrap();
    ed.apply("b", &[select(Some("folio:path-ref/r2"), Some("TabDelimited"))]).unwrap();
    let text = ed.part(PART).unwrap();
    assert_eq!(text.matches("<DataMerge ").count(), 1, "{text}");
    assert!(text.contains("<DataMerge DataSourceFileType=\"TabDelimited\" DataSourceFile=\"folio:path-ref/r2\" />"), "{text}");
    // a UTF-16 file has no type (InDesign writes none)
    ed.apply("c", &[select(Some("folio:path-ref/r3"), Some(""))]).unwrap();
    assert!(ed.part(PART).unwrap().contains("<DataMerge DataSourceFile=\"folio:path-ref/r3\" />"));
    // an attribute Folio does not know stays
    let mut parts = ed.parts().clone();
    let t = parts.get_mut(PART).unwrap();
    *t = t.replacen("<DataMerge DataSourceFile", "<DataMerge Extra=\"1\" DataSourceFile", 1);
    let mut ed = Editor::new(parts);
    ed.apply("d", &[select(Some("folio:path-ref/r4"), Some("CommaSeparated"))]).unwrap();
    assert!(ed.part(PART).unwrap().contains("<DataMerge DataSourceFileType=\"CommaSeparated\" DataSourceFile=\"folio:path-ref/r4\" Extra=\"1\" />"));
    assert_eq!(ed.data_source(), Some(("folio:path-ref/r4".to_owned(), Some("CommaSeparated".to_owned()))));
    ed.apply("e", &[remove()]).unwrap();
    assert_eq!(ed.data_source(), None);
    // removing where there is none changes nothing
    let before = ed.parts().clone();
    ed.apply("f", &[remove()]).unwrap();
    assert_eq!(ed.parts(), &before);
}

#[test]
fn a_document_without_a_data_merge_option_takes_the_element_first() {
    let mut parts = package("exactfit");
    let t = parts.get_mut(PART).unwrap();
    let (start, end) = element(t);
    t.replace_range(start..end, "");
    *t = t.replacen("<DataMergeOption ", "<Gone ", 1);
    let mut ed = Editor::new(parts);
    ed.apply("select", &[select(Some("folio:path-ref/r1"), Some("CommaSeparated"))]).unwrap();
    let text = ed.part(PART).unwrap();
    let first = text.find("<DataMerge DataSourceFileType").expect("inserted");
    assert!(text[..first].matches('<').count() <= 3, "first element: {}", &text[..first]);
    assert_eq!(ed.data_source().map(|s| s.0), Some("folio:path-ref/r1".to_owned()));
}

#[test]
fn the_host_must_give_the_file_reference() {
    let mut ed = Editor::new(package("exactfit"));
    let error = ed.apply("select", &[select(None, None)]).expect_err("no reference");
    assert_eq!(error.code, "invalid", "{}", error.message);
    let error = ed.apply("select", &[select(Some("folio:path-ref/r1"), Some("Semicolon"))]).expect_err("unknown type");
    assert_eq!(error.code, "invalid", "{}", error.message);
}
