//! Book numbering of one document (oracle probes bookx-c1-repaginate, -repaginate2, -sync; measurements in
//! artifacts/bookx-c1-20261023/measurement): the book gives the first page a number and the rest follow.
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

#[test]
fn book_start_numbers_pages_from_the_given_number_and_undoes() {
    let mut ed = Editor::new(fixture());
    let before = ed.numbering().unwrap();
    assert!(before.first_continues);
    assert_eq!(before.first_start, None);
    assert_eq!(names(&ed), ["1", "2", "3", "4", "5"]);
    let start = |n: u32| -> Command { RegisteredCommand::SetBookPageStart { start: n }.into() };
    ed.apply("book", &[start(7)]).unwrap();
    let after = ed.numbering().unwrap();
    assert_eq!(names(&ed), ["7", "8", "9", "10", "11"]);
    assert_eq!(after.pages.last().unwrap().number, 11);
    assert_eq!(after.first_start, Some(7));
    // as InDesign's `d1`: the next document of the book starts after the last number
    ed.apply("book", &[start(1)]).unwrap();
    assert_eq!(names(&ed), ["1", "2", "3", "4", "5"]);
    // a bad number is refused and changes nothing
    assert!(ed.apply("book", &[start(0)]).is_err());
    assert_eq!(names(&ed), ["1", "2", "3", "4", "5"]);
    ed.apply("book", &[start(40)]).unwrap();
    assert!(ed.undo().is_some());
    assert_eq!(names(&ed), ["1", "2", "3", "4", "5"]);
}

#[test]
fn a_first_section_with_its_own_number_keeps_it() {
    let mut ed = Editor::new(fixture());
    let page = ed.numbering().unwrap();
    assert_eq!(page.pages.len(), 5);
    let first = {
        let design = ed.part("designmap.xml").unwrap();
        let at = design.find("PageStart=\"").unwrap() + 11;
        design[at..].split('"').next().unwrap().to_owned()
    };
    // InDesign case e1: the document with a number of its own is not renumbered by the book
    ed.apply("section", &[Command::SetSection { page: first, start: Some(10), style: "Arabic".into(), prefix: String::new(), include_prefix: false, marker: String::new() }])
        .unwrap();
    assert_eq!(names(&ed), ["10", "11", "12", "13", "14"]);
    ed.apply("book", &[RegisteredCommand::SetBookPageStart { start: 3 }.into()]).unwrap();
    assert_eq!(names(&ed), ["10", "11", "12", "13", "14"]);
    let n = ed.numbering().unwrap();
    assert!(!n.first_continues);
    assert_eq!(n.first_start, Some(10));
}

fn idml_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.is_dir() {
            idml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "idml") {
            out.push(path);
        }
    }
}

/// Every IDML InDesign wrote for the oracle: with the descriptors taken out, the numbers the sections give are the numbers
/// the descriptors held (the proof that the computed numbers are InDesign's, not a guess). Documents with several sections
/// and with a section that starts its own numbering are among them.
#[test]
fn numbers_from_the_sections_equal_the_descriptors_of_every_oracle_document() {
    let mut names = Vec::new();
    idml_files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle"), &mut names);
    names.sort();
    let (mut checked, mut several, mut restarts) = (0, 0, 0);
    let mut different = Vec::new();
    for path in names {
        let Ok(mut z) = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()) else { continue };
        let mut parts = BTreeMap::new();
        for i in 0..z.len() {
            let mut e = z.by_index(i).unwrap();
            if e.is_dir() {
                continue;
            }
            let mut bytes = Vec::new();
            e.read_to_end(&mut bytes).unwrap();
            if let Ok(t) = String::from_utf8(bytes) {
                parts.insert(e.name().unwrap().into_owned(), t);
            }
        }
        let Ok(original) = Editor::new(parts.clone()).numbering() else { continue };
        let mut had = false;
        for (name, text) in parts.iter_mut() {
            if !name.starts_with("Spreads/") {
                continue;
            }
            while let Some(at) = text.find("<Descriptor") {
                let end = text[at..].find("</Descriptor>").unwrap() + at + "</Descriptor>".len();
                text.replace_range(at..end, "");
                had = true;
            }
        }
        if !had {
            continue;
        }
        let design = &parts["designmap.xml"];
        let sections = design.matches("<Section ").count();
        several += usize::from(sections > 1);
        restarts += usize::from(design.matches("ContinueNumbering=\"false\"").count() > 0);
        let computed = Editor::new(parts).numbering().unwrap();
        checked += 1;
        let a: Vec<(&str, i64)> = original.pages.iter().map(|p| (p.name.as_str(), p.number)).collect();
        let b: Vec<(&str, i64)> = computed.pages.iter().map(|p| (p.name.as_str(), p.number)).collect();
        if a != b {
            different.push(format!("{}: descriptors {a:?} computed {b:?}", path.file_name().unwrap().to_string_lossy()));
        }
    }
    println!("checked {checked} documents ({several} with several sections, {restarts} with a section that restarts)");
    assert!(checked > 100 && several >= 3 && restarts >= 3, "too few documents to prove it: {checked}/{several}/{restarts}");
    assert!(different.is_empty(), "{}", different.join("\n"));
}

fn page_ids(ed: &Editor) -> Vec<String> {
    let design = ed.part("designmap.xml").unwrap().to_owned();
    let mut ids = Vec::new();
    for src in design.split("<idPkg:Spread src=\"").skip(1) {
        let text = ed.part(src.split('"').next().unwrap()).unwrap();
        ids.extend(text.split("<Page Self=\"").skip(1).map(|p| p.split('"').next().unwrap().to_owned()));
    }
    ids
}

/// A document with no page descriptors (File > New) goes through the section and book edits like one that has them: the
/// pages get the same names and numbers (sections that restart, continue, take a prefix, a book's first number).
#[test]
fn a_document_without_descriptors_numbers_like_one_with_them() {
    let with = fixture();
    let mut without = with.clone();
    for (name, text) in without.iter_mut() {
        if name.starts_with("Spreads/") {
            while let Some(at) = text.find("<Descriptor") {
                let end = text[at..].find("</Descriptor>").unwrap() + at + "</Descriptor>".len();
                text.replace_range(at..end, "");
            }
        }
    }
    let (mut a, mut b) = (Editor::new(with), Editor::new(without));
    let rows = |ed: &Editor| ed.numbering().unwrap().pages.into_iter().map(|p| (p.name, p.number)).collect::<Vec<_>>();
    assert_eq!(rows(&a), rows(&b));
    let ids = page_ids(&a);
    assert_eq!(ids.len(), 5);
    let section = |page: &str, start: Option<u32>, style: &str, prefix: &str| Command::SetSection { page: page.into(), start, style: style.into(), prefix: prefix.into(), include_prefix: !prefix.is_empty(), marker: String::new() };
    let steps: Vec<(&str, Vec<Command>)> = vec![
        ("a section that restarts on page 3", vec![section(&ids[2], Some(1), "LowerRoman", "")]),
        ("a section with a prefix on page 5", vec![section(&ids[4], Some(7), "Arabic", "A-")]),
        ("a book's first number", vec![RegisteredCommand::SetBookPageStart { start: 4 }.into()]),
    ];
    for (what, commands) in steps {
        a.apply("section", &commands).unwrap();
        b.apply("section", &commands).unwrap();
        assert_eq!(rows(&a), rows(&b), "{what}");
    }
    // the last state, spelled out: roman section on pages 3-4 and the prefixed one on page 5
    assert_eq!(rows(&b).iter().map(|r| r.1).collect::<Vec<_>>(), [4, 5, 1, 2, 7]);
    assert!(b.numbering().unwrap().pages.iter().all(|p| !p.name.is_empty()));
}
