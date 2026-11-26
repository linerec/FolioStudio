use folio_edit::{Editor, commands::RegisteredCommand as Toc};
use folio_model::toc::{TocEntry, TocSource, TocStyle};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> BTreeMap<String, String> {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/text.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for n in 0..zip.len() {
        let mut p = zip.by_index(n).unwrap();
        if p.is_dir() {
            continue;
        }
        let mut t = String::new();
        p.read_to_string(&mut t).unwrap();
        out.insert(p.name().unwrap().into_owned(), t);
    }
    out
}
fn setup() -> (Editor, TocStyle, TocSource) {
    let ed = Editor::new(fixture());
    let doc =
        folio_idml::read_parts(ed.parts(), Path::new("text.idml"), &Default::default()).unwrap();
    let story = doc
        .stories
        .values()
        .find(|s| {
            s.paragraphs
                .first()
                .is_some_and(|p| p.runs.iter().any(|r| !r.text.is_empty()))
        })
        .unwrap();
    let style = TocStyle {
        name: "Interop flags".into(),
        title: "Contents".into(),
        entries: vec![TocEntry {
            name: story.paragraphs[0].style.clone(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let source = TocSource {
        story: story.id.clone(),
        paragraph: 0,
        page: "1".into(),
        page_index: 0,
        hidden: false,
        number: String::new(),
        book: None,
    };
    (ed, style, source)
}

fn save(ed: &mut Editor, style: TocStyle) -> String {
    ed.apply("style", &[Toc::SaveTOCStyle { style }.into()])
        .unwrap();
    ed.toc_styles()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Interop flags")
        .unwrap()
        .id
}

fn toc_text(ed: &Editor) -> Vec<String> {
    let doc = folio_idml::read_parts(ed.parts(), Path::new("text.idml"), &Default::default()).unwrap();
    let id = doc.toc_stories.keys().next().unwrap();
    doc.stories[id].paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect()).collect()
}

/// Include Book Documents is saved (IDML IncludeBookDocuments="true"); the paragraphs of the book's other documents come in
/// the sources (the server reads them) and are listed in their order with their own page names; run-in joins the entries of a
/// level that follow each other with a semicolon (InDesign probe artifacts/build-b2-20261023/booktoc).
#[test]
fn book_documents_and_run_in_are_generated() {
    let (mut ed, mut style, source) = setup();
    style.include_book_documents = true;
    style.title = String::new();
    let entry = style.entries[0].name.clone();
    let id = save(&mut ed, style);
    assert!(ed.parts().values().any(|t| t.contains("IncludeBookDocuments=\"true\"")));
    let other = |text: &str, page: &str| TocSource { story: String::new(), paragraph: 0, page: page.into(), page_index: 0, hidden: false, number: String::new(),
        book: Some(folio_model::toc::BookParagraph { document: "b.folio".into(), style: entry.clone(), text: text.into() }) };
    let sources = vec![source.clone(), other("Beta one", "6"), other("Beta two", "7")];
    ed.apply("book", &[Toc::CreateTOC { style: id.clone(), frame: String::new(), replace: false, sources: sources.clone() }.into()]).unwrap();
    let rows = toc_text(&ed);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(rows[1].starts_with("Beta one") && rows[1].ends_with('6') && rows[2].ends_with('7'), "{rows:?}");
    // the same paragraphs, run in: one paragraph
    let (mut ed, mut style, _) = setup();
    style.include_book_documents = true;
    style.run_in = true;
    style.title = String::new();
    let id = save(&mut ed, style);
    ed.apply("run in", &[Toc::CreateTOC { style: id, frame: String::new(), replace: false, sources }.into()]).unwrap();
    let rows = toc_text(&ed);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].matches(';').count(), 2, "{rows:?}");
    // a style without Include Book Documents leaves the other documents out
    let (mut ed, style, source) = setup();
    let id = save(&mut ed, style);
    ed.apply("one", &[Toc::CreateTOC { style: id, frame: String::new(), replace: false, sources: vec![source, other("Beta one", "6")] }.into()]).unwrap();
    assert!(!toc_text(&ed).iter().any(|r| r.contains("Beta")));
}

#[test]
fn bookmark_and_anchor_flags_do_not_control_current_generated_links() {
    for (bookmarks, anchors) in [(false, false), (false, true), (true, false), (true, true)] {
        let (mut ed, mut style, source) = setup();
        style.create_bookmarks = bookmarks;
        style.make_anchor = anchors;
        let id = save(&mut ed, style);
        let before = &ed.parts()["designmap.xml"];
        let count = |xml: &str, tag: &str| xml.matches(&format!("<{tag} ")).count();
        let bookmarks = count(before, "Bookmark");
        let anchors = count(before, "HyperlinkTextDestination");
        let destinations = count(before, "HyperlinkPageDestination");
        ed.apply(
            "create",
            &[Toc::CreateTOC {
                style: id,
                frame: String::new(),
                replace: false,
                sources: vec![source],
            }
            .into()],
        )
        .unwrap();
        let after = &ed.parts()["designmap.xml"];
        assert_eq!(count(after, "Bookmark"), bookmarks + 1);
        assert_eq!(count(after, "HyperlinkTextDestination"), anchors);
        assert_eq!(count(after, "HyperlinkPageDestination"), destinations + 1);
    }
}
