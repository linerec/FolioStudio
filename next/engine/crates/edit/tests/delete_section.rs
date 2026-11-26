//! Pages ▸ Delete Section (the Pages panel's Section menu). InDesign 2025 (probe artifacts/menuaudit-b-20261023/delete-section-probe.jsx, result
//! oracle/measured/delete-section-2025.json, read below): deleting a section joins its pages to the section before it, which numbers them on; the first section is the
//! document's default one and cannot be deleted ("Cannot delete the default section in a document", error 19462).
use folio_edit::commands::RegisteredCommand;
use folio_edit::{Command, Editor};
use std::collections::BTreeMap;
use std::io::Read;

fn fixture() -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/pages.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(e.name().unwrap().into_owned(), t);
    }
    parts
}

fn names(ed: &Editor) -> Vec<String> {
    ed.numbering().unwrap().pages.into_iter().map(|p| p.name).collect()
}

/// What InDesign 2025 measured (DOM probe): a key of oracle/measured/delete-section-2025.json.
fn measured(key: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/delete-section-2025.json");
    let all: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all[key].as_str().unwrap_or_else(|| panic!("measured key {key}")).to_owned()
}

/// `Self` of the document's pages in reading order (the spreads in designmap order, then the pages in each).
fn page_ids(ed: &Editor) -> Vec<String> {
    let design = ed.part("designmap.xml").unwrap().to_owned();
    let mut ids = Vec::new();
    for chunk in design.split("<idPkg:Spread src=\"").skip(1) {
        let name = chunk.split('"').next().unwrap();
        let spread = ed.part(name).unwrap();
        let mut rest = spread;
        while let Some(at) = rest.find("<Page ") {
            let tag = &rest[at..];
            let self_at = tag.find("Self=\"").unwrap() + 6;
            ids.push(tag[self_at..].split('"').next().unwrap().to_owned());
            rest = &tag[6..];
        }
    }
    ids
}

fn delete(page: &str) -> Command {
    RegisteredCommand::DeleteSection { page: page.into() }.into()
}

fn section(page: &str, start: u32, style: &str, prefix: &str) -> Command {
    Command::SetSection { page: page.into(), start: Some(start), style: style.into(), prefix: prefix.into(), include_prefix: !prefix.is_empty(), marker: String::new() }
}

#[test]
fn a_middle_section_joins_the_section_before_it_and_undoes() {
    let mut ed = Editor::new(fixture());
    let ids = page_ids(&ed);
    assert_eq!(ids.len(), 5);
    assert_eq!(names(&ed), ["1", "2", "3", "4", "5"]);
    // InDesign case a: a roman section from the third page, removed again
    ed.apply("section", &[section(&ids[2], 1, "LowerRoman", "")]).unwrap();
    assert_eq!(names(&ed).join(","), measured("a-added-pages"));
    ed.apply("delete", &[delete(&ids[2])]).unwrap();
    assert_eq!(names(&ed).join(","), measured("a-after-pages"));
    assert!(ed.undo().is_some());
    assert_eq!(names(&ed), ["1", "2", "i", "ii", "iii"]);
    assert!(ed.redo().is_some());
    assert_eq!(names(&ed), ["1", "2", "3", "4", "5"]);
}

#[test]
fn the_last_section_goes_and_the_one_before_it_numbers_on_with_its_prefix() {
    let mut ed = Editor::new(fixture());
    let ids = page_ids(&ed);
    // InDesign cases b and c: sections on the second (A-10…, prefix A-) and the fourth page (upper roman), the last one removed
    ed.apply("sections", &[section(&ids[1], 10, "Arabic", "A-"), section(&ids[3], 1, "UpperRoman", "")]).unwrap();
    assert_eq!(names(&ed).join(","), measured("b-added-pages"));
    ed.apply("delete", &[delete(&ids[3])]).unwrap();
    assert_eq!(names(&ed).join(","), measured("c-after-pages"));
}

#[test]
fn the_first_section_stays_and_a_page_without_a_section_is_refused() {
    let mut ed = Editor::new(fixture());
    let ids = page_ids(&ed);
    ed.apply("sections", &[section(&ids[1], 10, "Arabic", "A-")]).unwrap();
    let before = names(&ed);
    // InDesign: "Cannot delete the default section in a document." (b-remove-first, d-remove-only)
    assert!(measured("b-remove-first").starts_with("ERROR 19462"));
    assert!(ed.apply("delete", &[delete(&ids[0])]).is_err());
    assert!(ed.apply("delete", &[delete(&ids[2])]).is_err());
    assert!(ed.apply("delete", &[delete("nothing")]).is_err());
    assert_eq!(names(&ed), before);
}
