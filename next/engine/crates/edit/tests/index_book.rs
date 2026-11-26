//! Generate Index ▸ Include Book Documents (build-b3): the other book documents' entries (the server reads them) merge with
//! this document's by topic path, pages in the book's order; the option is kept in the index options; one undo step.
//! End to end against InDesign 2025: artifacts/build-b3-20261023/tools/bookindex-engine-check.py (same text as InDesign).
use folio_edit::commands::RegisteredCommand as R;
use folio_edit::Editor;
use folio_model::indexes::{BookIndexCross, BookIndexDocument, BookIndexEntry, BookIndexPage};
use std::collections::BTreeMap;
use std::path::Path;

fn fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/index-c3.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    out
}

fn entry(path: &[&str], pages: &[(&str, usize)], cross: Option<(&str, &str)>) -> BookIndexEntry {
    BookIndexEntry {
        path: path.iter().map(|s| s.to_string()).collect(),
        key: path.last().unwrap().to_string(),
        printed: path.last().unwrap().to_string(),
        pages: pages.iter().map(|&(n, i)| BookIndexPage { first: n.into(), last: n.into(), index: i, end: i }).collect(),
        cross: cross.map(|(w, t)| BookIndexCross { lead: String::new(), words: w.into(), tail: format!("\u{a0}{t}") }).into_iter().collect(),
    }
}

fn generate(ed: &mut Editor, include: Option<bool>, book: Option<Vec<BookIndexDocument>>) -> Result<String, String> {
    let doc = folio_idml::read_parts(ed.parts(), Path::new("index.idml"), &folio_idml::ReaderData::default()).unwrap();
    let layout = folio_text::layout_document(&doc, &folio_text::FontStore::discover(&[]));
    let locations = folio_text::layout::toc::locations(&doc, &layout);
    let make = R::GenerateIndex { page: String::new(), frame: String::new(), replace: true, title: "Index".into(), headings: true, font: "Malgun Gothic".into(), locations, include_book_documents: include, book_documents: book };
    let result = ed.apply("generate", &[make.into()]).map_err(|e| e.to_string())?;
    let story = result.created.last().cloned().ok_or("no story")?;
    let doc = folio_idml::read_parts(ed.parts(), Path::new("index.idml"), &folio_idml::ReaderData::default()).unwrap();
    Ok(doc.stories[&story].paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\r"))
}

#[test]
fn the_book_documents_topics_merge_in_book_order_and_one_undo_takes_it_back() {
    let mut ed = Editor::new(fixture());
    let before = ed.parts().clone();
    let a = BookIndexDocument { document: "a.folio".into(), own: false, entries: vec![
        entry(&["Apple"], &[("2", 1)], None), entry(&["Cherry"], &[], None), entry(&["Cherry", "Tart"], &[("2", 1)], None),
        entry(&["Fig"], &[("3", 2)], Some(("See also", "Banana"))), entry(&["Banana"], &[("3", 2)], None),
    ]};
    let own = BookIndexDocument { document: "b.folio".into(), own: true, entries: Vec::new() };
    let c = BookIndexDocument { document: "c.folio".into(), own: false, entries: vec![
        entry(&["Apple"], &[("7", 0), ("8", 1)], None), entry(&["Cherry"], &[], None), entry(&["Cherry", "Pie"], &[("8", 1)], None),
        entry(&["Fig"], &[("7", 0)], Some(("See also", "Banana"))),
    ]};
    let text = generate(&mut ed, Some(true), Some(vec![a, own, c])).unwrap();
    let lines: Vec<_> = text.split('\r').filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, [
        "Index", "A", "Apple  2, 7, 8", "B", "Banana  3", "C", "Cherry", "Pie  8", "Tart  2", "F", "Fig  3, 7. See also\u{a0}Banana",
    ]);
    assert!(ed.parts()["Resources/Preferences.xml"].contains("IncludeBookDocuments=\"true\""));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
}

#[test]
fn without_the_option_or_a_book_only_the_documents_own_topics_print() {
    let mut ed = Editor::new(fixture());
    let other = BookIndexDocument { document: "a.folio".into(), own: false, entries: vec![entry(&["Apple"], &[("2", 1)], None)] };
    // off: the other documents are not merged (and nothing of this document's index to print: only the title)
    let off = generate(&mut ed, Some(false), Some(vec![other.clone()]));
    assert!(off.as_deref().is_ok_and(|t| !t.contains("Apple")), "{off:?}");
    assert!(!ed.parts()["Resources/Preferences.xml"].contains("IncludeBookDocuments=\"true\""));
}

/// Index panel ▸ Import Topics: the other document's topics (names, sort orders, subtopics) join this index; a topic of the same
/// name is kept, not doubled; one undo step takes them out.
#[test]
fn imported_topics_join_the_index_and_one_undo_takes_them_out() {
    use folio_model::indexes::Topic;
    let mut ed = Editor::new(fixture());
    ed.apply("own", &[R::AddIndexTopic { parent: String::new(), name: "Apple".into(), sort_order: String::new() }.into()]).unwrap();
    let before = ed.parts().clone();
    let t = |name: &str, sort: &str, topics: Vec<Topic>| Topic { name: name.into(), sort_order: sort.into(), topics, ..Topic::default() };
    let topics = vec![t("Apple", "", vec![t("Pie", "", vec![])]), t("Zebra", "zeb", vec![])];
    ed.apply("import", &[R::ImportIndexTopics { document: "other.folio".into(), topics }.into()]).unwrap();
    let doc = folio_idml::read_parts(ed.parts(), Path::new("index.idml"), &folio_idml::ReaderData::default()).unwrap();
    let top: Vec<_> = doc.indexes[0].topics.iter().map(|x| (x.name.as_str(), x.sort_order.as_str(), x.topics.len())).collect();
    assert_eq!(top, [("Apple", "", 1), ("Zebra", "zeb", 0)]);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
    // nothing to import is refused
    assert!(ed.apply("none", &[R::ImportIndexTopics { document: "other.folio".into(), topics: vec![] }.into()]).is_err());
}
