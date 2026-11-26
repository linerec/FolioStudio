//! Data Merge ▸ Preview (build-c): `fillMergeRecord` puts one record's values into the template's own placeholders. Inside a
//! preview session the document shows them and is given back byte for byte, with nothing in the undo history; sent as an edit
//! of its own it is one undo step. The templates are InDesign's own exports (fixtures/datamerge-merge).
use folio_edit::commands::RegisteredCommand;
use folio_edit::{Command, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

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

fn stories(ed: &Editor) -> String {
    ed.parts().iter().filter(|(n, _)| n.starts_with("Stories/")).map(|(_, t)| t.as_str()).collect()
}

fn fill(fields: &[String], record: &[String]) -> Command {
    RegisteredCommand::FillMergeRecord { fields: fields.to_vec(), record: record.to_vec(), remove_blank_lines: false }.into()
}

#[test]
fn preview_shows_a_record_and_gives_the_template_back() {
    let bytes = std::fs::read(oracle().join("fixtures/datamerge-merge/people.csv")).unwrap();
    let source = folio_idml::datamerge::parse(&bytes).unwrap();
    let fields: Vec<String> = source.fields.iter().map(|f| f.name.clone()).collect();
    let mut ed = Editor::new(package("single"));
    let original = ed.parts().clone();
    assert!(stories(&ed).contains("&lt;&lt;Name&gt;&gt;"), "the template has the Name placeholder");
    ed.apply("", &[RegisteredCommand::PreviewStart { label: "Data Merge Preview".into() }.into()]).unwrap();
    for record in [1usize, 0] {
        ed.apply("", &[RegisteredCommand::PreviewApply { commands: vec![fill(&fields, &source.records[record])] }.into()]).unwrap();
        let shown = stories(&ed);
        let name = folio_edit::xml::escape(&source.records[record][fields.iter().position(|f| f == "Name").unwrap()]);
        assert!(shown.contains(&name), "record {record}: {name} shown");
        assert!(!shown.contains("&lt;&lt;Name&gt;&gt;"), "record {record}: placeholder replaced");
        // the data source and the field links stay while the preview shows a record
        assert!(ed.parts()["designmap.xml"].contains("<Hyperlink "), "field links kept");
    }
    ed.apply("", &[RegisteredCommand::PreviewCancel {}.into()]).unwrap();
    assert_eq!(ed.parts(), &original, "the template comes back byte for byte");
    assert!(ed.undo().is_none(), "nothing in the undo history");
}

#[test]
fn filling_a_record_alone_is_one_undo_step() {
    let bytes = std::fs::read(oracle().join("fixtures/datamerge-merge/people.csv")).unwrap();
    let source = folio_idml::datamerge::parse(&bytes).unwrap();
    let fields: Vec<String> = source.fields.iter().map(|f| f.name.clone()).collect();
    let mut ed = Editor::new(package("single"));
    let original = ed.parts().clone();
    ed.apply("fill", &[fill(&fields, &source.records[1])]).unwrap();
    assert_ne!(ed.parts(), &original);
    assert!(ed.undo().is_some());
    assert_eq!(ed.parts(), &original);
    // a placeholder of a field the source does not have is refused, as the merge refuses it
    let error = ed.apply("fill", &[fill(&["Other".to_owned()], &["x".to_owned()])]).expect_err("missing field");
    assert!(error.message.contains("placeholder"), "{}", error.message);
    assert_eq!(ed.parts(), &original);
}
