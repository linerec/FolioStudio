//! Data Merge ▸ Update Content in Data Fields and its log (build-c2, Folio's own: InDesign's merged document keeps no field
//! links, datamerge-c3). A merged document made with a link remembers its field texts; an update replaces only those still as
//! the merge left them, keeps what a person typed elsewhere and what a person changed in a field, and logs each one.
use folio_edit::{Command, Editor};
use serde_json::{Value, json};
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

fn records(rows: &[[&str; 2]]) -> Vec<Vec<String>> {
    rows.iter().map(|r| r.iter().map(|v| v.to_string()).collect()).collect()
}

fn merged(rows: &[[&str; 2]]) -> Editor {
    let mut ed = Editor::new(package("single"));
    let link = json!({"template": {"path": "single.folio", "sha256": "t"}, "source": {"path": "people.csv", "sha256": "a"}});
    let command: Command = folio_edit::commands::RegisteredCommand::MergeRecords {
        fields: vec!["Name".into(), "City".into()], records: records(rows), range: String::new(), multiple: false, arrange: "rows".into(),
        row_spacing: 0.0, column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false, fitting: "proportional".into(), center_image: false,
        graphics: BTreeMap::new(), embed: None, link: Some(link.to_string()),
    }.into();
    ed.apply("merge", &[command]).unwrap();
    ed
}

fn text(ed: &Editor, story: &str) -> String {
    let part = format!("Stories/Story_{story}.xml");
    folio_edit::text::contents(ed.part(&part).unwrap(), story).unwrap()
}

fn update(ed: &mut Editor, rows: &[[&str; 2]], at: &str) {
    let command: Command = folio_edit::commands::RegisteredCommand::UpdateMergedFields {
        fields: vec!["Name".into(), "City".into()], records: records(rows), sha256: "b".into(), at: at.into(),
    }.into();
    ed.apply("update fields", &[command]).unwrap();
}

#[test]
fn update_replaces_the_fields_the_merge_left_and_keeps_what_a_person_changed() {
    let rows = [["Alice", "Seoul"], ["Bob", "Busan"]];
    let mut ed = merged(&rows);
    let m = ed.merged_document().expect("the merged document remembers its fields");
    assert_eq!(m["template"]["path"], "single.folio");
    assert_eq!(m["source"]["path"], "people.csv");
    let stories: Vec<Value> = m["stories"].as_array().unwrap().clone();
    assert!(stories.len() >= 2, "{stories:?}");
    let first = stories.iter().find(|s| s["record"] == 0).unwrap()["story"].as_str().unwrap().to_owned();
    let second = stories.iter().find(|s| s["record"] == 1).unwrap()["story"].as_str().unwrap().to_owned();
    let (a, b) = (text(&ed, &first), text(&ed, &second));
    assert!(a.contains("Alice") && a.contains("Seoul") && b.contains("Bob"), "{a:?} {b:?}");
    // a person types in front of the first record's text and changes Bob to Robert
    ed.apply("type", &[Command::InsertText { story: first.clone(), at: 0, text: "Dear ".into() }]).unwrap();
    let at = text(&ed, &second).find("Bob").unwrap();
    let at = text(&ed, &second)[..at].chars().count();
    ed.apply("edit", &[Command::DeleteText { story: second.clone(), start: at, end: at + 3 }]).unwrap();
    ed.apply("edit", &[Command::InsertText { story: second.clone(), at, text: "Robert".into() }]).unwrap();
    let before = ed.parts().clone();
    // the source changes: Seoul → Incheon, Bob → Bobby
    update(&mut ed, &[["Alice", "Incheon"], ["Bobby", "Busan"]], "2026-11-21T18:00:00Z");
    let (a, b) = (text(&ed, &first), text(&ed, &second));
    assert!(a.starts_with("Dear ") && a.contains("Alice") && a.contains("Incheon") && !a.contains("Seoul"), "{a:?}");
    assert!(b.contains("Robert") && !b.contains("Bobby"), "a field a person changed stays: {b:?}");
    let log = ed.merged_document().unwrap()["log"].as_array().unwrap().clone();
    let results: Vec<(String, String, String)> = log.iter().filter(|e| e["result"] != "summary")
        .map(|e| (e["field"].as_str().unwrap().into(), e["to"].as_str().unwrap().into(), e["result"].as_str().unwrap().into())).collect();
    assert!(results.contains(&("City".into(), "Incheon".into(), "updated".into())), "{results:?}");
    assert!(results.contains(&("Name".into(), "Bobby".into(), "edited".into())), "{results:?}");
    let summary = log.last().unwrap();
    assert_eq!((summary["result"].as_str(), summary["updated"].as_u64(), summary["left"].as_u64()), (Some("summary"), Some(1), Some(1)));
    assert_eq!(ed.merged_document().unwrap()["source"]["sha256"], "b");
    // a second update with Incheon → Daejeon finds the text the last update put there
    update(&mut ed, &[["Alice", "Daejeon"], ["Bobby", "Busan"]], "2026-11-21T18:05:00Z");
    assert!(text(&ed, &first).contains("Daejeon"));
    // one undo step back to before the update
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
}

#[test]
fn a_document_not_merged_by_folio_has_nothing_to_update() {
    let mut ed = Editor::new(package("single"));
    let command: Command = folio_edit::commands::RegisteredCommand::UpdateMergedFields { fields: vec![], records: vec![], sha256: String::new(), at: String::new() }.into();
    assert_eq!(ed.apply("update", &[command]).unwrap_err().code, "notMerged");
    // a merge without a link (an oracle case, an MCP call) writes nothing extra
    let mut plain = Editor::new(package("single"));
    let command: Command = folio_edit::commands::RegisteredCommand::MergeRecords {
        fields: vec!["Name".into(), "City".into()], records: records(&[["A", "B"]]), range: String::new(), multiple: false, arrange: "rows".into(),
        row_spacing: 0.0, column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false, fitting: "proportional".into(), center_image: false,
        graphics: BTreeMap::new(), embed: None, link: None,
    }.into();
    plain.apply("merge", &[command]).unwrap();
    assert!(plain.merged_document().is_none());
}
