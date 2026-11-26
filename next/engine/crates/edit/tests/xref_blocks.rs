//! Cross-reference formats InDesign 2025 computes that Folio used to refuse (build-c2, measured by
//! `oracle/indesign/buildc2-xref-probe.jsx`; its document is `oracle/fixtures/buildc2/xref-probe.idml`, results in
//! `artifacts/build-c2-20261023/xref/probe-result.json`): Chapter Number and File Name blocks, a partial paragraph whose delimiter is
//! not in the paragraph (the whole paragraph), references inside the destination's own story, a compound format, and the
//! typographer's quotes that follow the character before the reference.
use folio_edit::{Command, Editor};
use folio_model::toc::TocSource;
use std::{collections::BTreeMap, io::Read, path::Path};

fn parts() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/buildc2/xref-probe.idml");
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

/// Every paragraph of the destination story is on page 1.
fn locations() -> Vec<TocSource> {
    (0..3).map(|paragraph| TocSource { story: "u128".into(), paragraph, page: "1".into(), page_index: 0, hidden: false, number: String::new(), book: None }).collect()
}

#[test]
fn folio_computes_what_indesign_wrote_for_every_reference() {
    let mut ed = Editor::new(parts());
    ed.set_file_name("xref-probe.indd");
    let before = ed.parts().clone();
    // Update Cross-References changes a source only when what it shows is not what the format computes
    ed.apply("update", &[folio_edit::commands::RegisteredCommand::UpdateCrossReference { id: String::new(), locations: locations() }.into()]).unwrap();
    for (name, text) in ed.parts() {
        if name.starts_with("Stories/") { assert_eq!(text, &before[name], "{name}: Folio's text differs from InDesign's"); }
    }
}

#[test]
fn new_references_with_the_chapter_number_file_name_and_in_the_same_story() {
    let mut ed = Editor::new(parts());
    ed.set_file_name("xref-probe.indd");
    let story = |ed: &Editor, id: &str| folio_edit::text::contents(ed.part(&format!("Stories/Story_{id}.xml")).unwrap(), id).unwrap();
    let insert = |format: &str, name: &str, at: usize, target: &str| -> Command {
        folio_edit::commands::RegisteredCommand::InsertCrossReference {
            story: target.into(), at, target_story: String::new(), target_at: 0, destination: "HyperlinkTextDestination/d0".into(),
            format: format.into(), name: name.into(), locations: locations(),
        }.into()
    };
    // after "Refs", in plain text
    ed.apply("chapter", &[insert("F chapter", "c2", 4, "u13f")]).unwrap();
    let text = story(&ed, "u13f");
    assert!(text.starts_with("RefsChapter \u{18}\r"), "{text:?}");
    assert!(ed.part("Stories/Story_u13f.xml").unwrap().matches("TV XRefChapterNumber\" ResultText=\"7\"").count() >= 2);
    ed.apply("file", &[insert("F file", "f2", 0, "u13f")]).unwrap();
    assert!(story(&ed, "u13f").starts_with("in xref-probe.inddRefs"), "{:?}", story(&ed, "u13f"));
    // in the destination's own story: right after a letter a closing quote comes first (as InDesign's `same.text`), at a
    // paragraph's start an opening one
    let a = story(&ed, "u128");
    let end = a.find("colon").map(|b| a[..b].chars().count() + 5).unwrap();
    ed.apply("same", &[insert("F text", "s2", end, "u128")]).unwrap();
    assert!(story(&ed, "u128").contains("colon\u{201d}Heading one: detail\u{201d}\r"), "{:?}", story(&ed, "u128"));
    let a = story(&ed, "u128");
    let third = a.find("Third").map(|b| a[..b].chars().count()).unwrap();
    ed.apply("start", &[insert("F text", "s3", third, "u128")]).unwrap();
    assert!(story(&ed, "u128").contains("\r\u{201c}Heading one: detail\u{201d}Third"), "{:?}", story(&ed, "u128"));
}
