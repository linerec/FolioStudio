//! Data Merge ▸ database source ▸ refresh mode (`setDataMergeRefreshMode`): only the `refresh` of the remembered query changes, in the
//! document label `folio.datamerge`, no query runs, the other keys stay, one undo step.
use folio_edit::commands::RegisteredCommand;
use folio_edit::{Command, Editor};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

fn package(name: &str) -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/datamerge-merge").join(format!("{name}.idml"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
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

fn mode(m: &str) -> Command { RegisteredCommand::SetDataMergeRefreshMode { mode: m.into() }.into() }

/// What the host gives an edit that selects a database source (it ran the query and wrote the snapshot).
fn select(refresh: &str) -> Command {
    RegisteredCommand::DataMergeSelectDatabase {
        query: json!({"connection": "잡지DB", "table": {"name": "ads"}, "refresh": refresh}), param_values: None, today: None,
        source: Some("folio:path-ref/r1".into()), file_type: Some(String::new()),
        state: Some(json!({"query": {"connection": "잡지DB", "table": {"name": "ads"}, "refresh": refresh}, "paramValues": {"월": "2026-11"}, "fetchedAt": "2026-11-19T10:00:00Z", "rows": 3, "sha256": "ab"})),
    }.into()
}

#[test]
fn the_mode_is_changed_alone_and_one_undo_step_goes_back() {
    let mut ed = Editor::new(package("exactfit"));
    // a document that has no database source refuses it (nothing to change)
    assert!(ed.apply("mode", &[mode("askOnOpen")]).is_err());
    ed.apply("select", &[select("manual")]).unwrap();
    assert_eq!(ed.data_merge_database().unwrap()["query"]["refresh"], "manual");
    let before = ed.data_merge_database().unwrap();
    ed.apply("mode", &[mode("askOnOpen")]).unwrap();
    let after = ed.data_merge_database().unwrap();
    assert_eq!(after["query"]["refresh"], "askOnOpen");
    // everything else of the state stays as it was
    for key in ["paramValues", "fetchedAt", "rows", "sha256"] { assert_eq!(after[key], before[key], "{key}"); }
    assert_eq!(after["query"]["connection"], "잡지DB");
    ed.apply("auto", &[mode("autoOnOpen")]).unwrap();
    assert_eq!(ed.data_merge_database().unwrap()["query"]["refresh"], "autoOnOpen");
    // a mode that is not one is refused and nothing changes
    let text = ed.part("designmap.xml").unwrap().to_owned();
    assert!(ed.apply("bad", &[mode("sometimes")]).is_err());
    assert_eq!(ed.part("designmap.xml").unwrap(), text);
    // undo goes back one mode at a time
    assert!(ed.undo().is_some());
    assert_eq!(ed.data_merge_database().unwrap()["query"]["refresh"], "askOnOpen");
    assert!(ed.undo().is_some());
    assert_eq!(ed.data_merge_database().unwrap()["query"]["refresh"], "manual");
    assert!(ed.redo().is_some());
    assert_eq!(ed.data_merge_database().unwrap()["query"]["refresh"], "askOnOpen");
}
