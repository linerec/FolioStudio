//! Data Merge computed fields in the document: the list is kept in the label `folio.datamerge` (key `computed`, other keys stay),
//! one undo step; and the merged document gets their values like any column's (templates of InDesign's own exports,
//! `oracle/fixtures/datamerge-merge`).
use folio_edit::commands::RegisteredCommand;
use folio_edit::{Command, Editor};
use folio_idml::datamerge::parse;
use folio_idml::datamerge_expr::{apply, ComputedField};
use folio_model::datamerge::ComputedField as Field;
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

fn set(fields: &[(&str, &str)]) -> Command {
    RegisteredCommand::SetDataMergeComputedFields { fields: fields.iter().map(|(name, expr)| Field { name: (*name).into(), expr: (*expr).into() }).collect() }.into()
}

const DM: &str = "designmap.xml";

#[test]
fn the_list_is_a_label_of_the_document_and_one_undo_step() {
    let mut ed = Editor::new(package("exactfit"));
    let original = ed.part(DM).unwrap().to_owned();
    assert!(ed.data_merge_computed().is_empty());
    ed.apply("computed", &[set(&[("이름 전체", "{성} & \" \" & {이름}"), ("@사진", "\"images/\" & {코드} & \".jpg\"")])]).unwrap();
    let text = ed.part(DM).unwrap().to_owned();
    assert!(text.contains("<KeyValuePair Key=\"folio.datamerge\" Value=\"{&quot;computed&quot;:["), "{text}");
    // the labels that were there stay
    assert!(text.contains("<KeyValuePair Key=\"kAdobeDPS_Version\" Value=\"2\" />"));
    let got = ed.data_merge_computed();
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].name.as_str(), got[0].expr.as_str()), ("이름 전체", "{성} & \" \" & {이름}"));
    assert_eq!(got[1].name, "@사진");
    // the same list again: nothing changes
    let before = ed.part(DM).unwrap().to_owned();
    ed.apply("same", &[set(&[("이름 전체", "{성} & \" \" & {이름}"), ("@사진", "\"images/\" & {코드} & \".jpg\"")])]).unwrap();
    assert_eq!(ed.part(DM).unwrap(), before);
    // replaced as a whole, then taken away: the label goes with the last field
    ed.apply("one", &[set(&[("a", "1")])]).unwrap();
    assert_eq!(ed.data_merge_computed().len(), 1);
    ed.apply("none", &[set(&[])]).unwrap();
    assert!(ed.data_merge_computed().is_empty());
    assert!(!ed.part(DM).unwrap().contains("folio.datamerge"));
    // undo and redo
    assert!(ed.undo().is_some());
    assert_eq!(ed.data_merge_computed().len(), 1);
    assert!(ed.undo().is_some() && ed.undo().is_some());
    assert_eq!(ed.part(DM).unwrap(), original);
    assert!(ed.redo().is_some());
    assert_eq!(ed.data_merge_computed().len(), 2);
}

#[test]
fn other_keys_of_the_label_are_kept() {
    let mut parts = package("exactfit");
    let seeded = parts[DM].replace("<KeyValuePair Key=\"kAdobeDPS_Version\" Value=\"2\" />", "<KeyValuePair Key=\"kAdobeDPS_Version\" Value=\"2\" /><KeyValuePair Key=\"folio.datamerge\" Value=\"{&quot;query&quot;:{&quot;connection&quot;:&quot;잡지DB&quot;}}\" />");
    assert_ne!(seeded, parts[DM]);
    parts.insert(DM.into(), seeded);
    let mut ed = Editor::new(parts);
    assert!(ed.data_merge_computed().is_empty());
    ed.apply("computed", &[set(&[("x", "1 + 1")])]).unwrap();
    let label = ed.data_merge_label();
    assert_eq!(label["query"]["connection"], "잡지DB");
    assert_eq!(label["computed"][0]["name"], "x");
    // taking the computed fields away leaves the query
    ed.apply("none", &[set(&[])]).unwrap();
    let label = ed.data_merge_label();
    assert_eq!(label["query"]["connection"], "잡지DB");
    assert!(label.get("computed").is_none());
    assert!(ed.part(DM).unwrap().contains("folio.datamerge"));
}

#[test]
fn a_wrong_list_is_refused_and_nothing_changes() {
    let mut ed = Editor::new(package("exactfit"));
    let original = ed.part(DM).unwrap().to_owned();
    for (list, why) in [
        (vec![("a", "{x} &")], "syntax"),
        (vec![("", "1")], "empty name"),
        (vec![("@", "1")], "empty name after @"),
        (vec![("a{b}", "1")], "braces in a name"),
        (vec![("a", "1"), ("a", "2")], "twice"),
        (vec![("a", "FOO(1)")], "unknown function"),
        (vec![("a", "")], "empty expression"),
    ] {
        let error = ed.apply("bad", &[set(&list)]).unwrap_err();
        assert_eq!(error.code, "invalid", "{why}");
        assert_eq!(ed.part(DM).unwrap(), original, "{why}");
    }
    // the same name as an image field and as a text field are two fields
    ed.apply("ok", &[set(&[("a", "1"), ("@a", "2"), ("#a", "3")])]).unwrap();
    assert_eq!(ed.data_merge_computed().len(), 3);
    let error = ed.apply("bad", &[set(&[("a", "1"); 65])]).unwrap_err();
    assert!(error.message.contains("at most 64") || error.message.contains("another computed field"), "{}", error.message);
    // the message says which field and where
    let error = ed.apply("bad", &[set(&[("가격표", "TEXT({가격},")])]).unwrap_err();
    assert!(error.message.starts_with("가격표: ") && error.message.contains("character"), "{}", error.message);
}

fn merge(ed: &mut Editor, source: &folio_idml::datamerge::Source) {
    let command: Command = RegisteredCommand::MergeRecords {
        fields: source.fields.iter().map(|f| f.name.clone()).collect(), records: source.records.clone(), range: String::new(),
        multiple: false, arrange: "rows".into(), row_spacing: 0.0, column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false,
        fitting: "proportional".into(), center_image: false, graphics: BTreeMap::new(), embed: None, link: None,
    }.into();
    ed.apply("merge", &[command]).unwrap();
}

#[test]
fn the_merged_document_gets_the_values_of_a_computed_field() {
    // the template of InDesign's export holds the placeholder `<<Name>>`; the data source has First and Last, `Name` is computed
    let source = parse(b"First,Last,Price\r\nAda,Lovelace,15000\r\nAlan,Turing,8000\r\n").unwrap();
    let computed = [ComputedField { name: "Name".into(), expr: "{First} & \" \" & UPPER({Last}) & \" / \" & TEXT({Price}, \"#,##0\") & \"won\"".into() }];
    let applied = apply(&source, &computed);
    assert!(applied.definition_errors.is_empty() && applied.record_errors.is_empty(), "{applied:?}");
    let mut ed = Editor::new(package("exactfit"));
    merge(&mut ed, &applied.source);
    let all: String = ed.parts().iter().filter(|(n, _)| n.starts_with("Stories/") || n.starts_with("Spreads/")).map(|(_, t)| t.as_str()).collect();
    assert!(all.contains("Ada LOVELACE / 15,000won"), "first record");
    assert!(all.contains("Alan TURING / 8,000won"), "second record");
    assert!(!all.contains("&lt;&lt;Name&gt;&gt;") && !all.contains("<<Name>>"), "no placeholder is left");
    // without the computed field the template's placeholder has no field: the merge says so
    let mut ed = Editor::new(package("exactfit"));
    let command: Command = RegisteredCommand::MergeRecords {
        fields: source.fields.iter().map(|f| f.name.clone()).collect(), records: source.records.clone(), range: String::new(),
        multiple: false, arrange: "rows".into(), row_spacing: 0.0, column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false,
        fitting: "proportional".into(), center_image: false, graphics: BTreeMap::new(), embed: None, link: None,
    }.into();
    assert!(ed.apply("merge", &[command]).is_err());
}
