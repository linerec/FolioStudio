//! Word files with tables, footnotes and languages: the expected values are what InDesign 2025 made of the same files
//! (artifacts/docxtext-c1-20261023/measurement, next/oracle/goldens/text/docxtext-*).
use super::*;

fn docx(name: &str) -> TextSource {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(name)).unwrap();
    read_bytes(&bytes, &Options::default()).unwrap()
}

fn table_of(p: &Paragraph, at: usize) -> &Table {
    match &p.runs[at].inline {
        Some(Inline::Table(t)) => t,
        other => panic!("not a table: {other:?}"),
    }
}

#[test]
fn a_table_goes_in_front_of_the_paragraph_that_follows_it() {
    let s = docx("docxtext-table.docx");
    assert_eq!(s.paragraphs.len(), 2);
    assert_eq!(s.story_text(), "Before table\r\u{16}After table");
    let t = table_of(&s.paragraphs[1], 0);
    assert_eq!((t.rows, t.header_rows, t.columns.clone()), (2, 0, vec![100.0, 100.0]));
    let texts: Vec<_> = t.cells.iter().map(|c| (c.row, c.column, c.paragraphs[0].text())).collect();
    assert_eq!(texts, [(0, 0, "A1".to_owned()), (0, 1, "B1".to_owned()), (1, 0, "A2".to_owned()), (1, 1, "B2".to_owned())]);
    assert!(s.warnings.iter().all(|w| !w.contains("표")), "{:?}", s.warnings);
}

#[test]
fn a_table_that_ends_the_file_is_a_paragraph_of_its_own() {
    let s = docx("docxtext-table-edge.docx");
    assert_eq!(s.story_text(), "\u{16}");
    assert!(s.paragraphs[0].style.is_none());
}

#[test]
fn merged_cells_and_header_rows_follow_the_grid() {
    let s = docx("docxtext-table-merge.docx");
    let t = table_of(&s.paragraphs[1], 0);
    let spans: Vec<_> = t.cells.iter().map(|c| (c.row, c.column, c.row_span, c.column_span)).collect();
    // the covered positions have no cell: row 0 holds one cell over both columns, the vertical merge starts in row 1
    assert_eq!(spans, [(0, 0, 1, 2), (1, 0, 2, 1), (1, 1, 1, 1), (2, 1, 1, 1)]);
    let s = docx("docxtext-table-header.docx");
    let t = table_of(&s.paragraphs[1], 0);
    assert_eq!((t.rows, t.header_rows), (3, 1));
}

#[test]
fn an_empty_cell_takes_the_default_paragraph_style_and_a_cell_keeps_its_paragraphs() {
    let s = docx("docxtext-table-rich.docx");
    let t = table_of(&s.paragraphs[1], 0);
    assert_eq!(t.cells[0].paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>(), ["Two ", "paragraphs"]);
    assert_eq!(t.cells[0].paragraphs[0].style.as_deref(), Some("Normal"));
    assert!(t.cells[2].paragraphs[0].runs.is_empty() && t.cells[2].paragraphs[0].style.is_none());
}

#[test]
fn table_borders_and_shading_are_carried_over_and_cell_borders_are_named_as_not() {
    // docxtext-c2 carries the table borders and the cell fill (see tests_docx_c2); the border of a single cell is still named
    let s = docx("docxtext-table-border.docx");
    assert!(!s.warnings.iter().any(|w| w.contains("테두리") || w.contains("배경색")), "{:?}", s.warnings);
    // (docxtext-c3 carries the border of a single cell too: see tests_docx_c3)
    assert!(!docx("docxtext2-b5.docx").warnings.iter().any(|w| w.contains("칸의 테두리")));
}

#[test]
fn a_footnote_is_one_marker_with_the_text_of_its_note() {
    let s = docx("docxtext-fn.docx");
    assert_eq!(s.story_text(), "Text with a note\u{4} and more text\u{4}\rSecond paragraph.");
    let notes: Vec<_> = s.paragraphs[0].runs.iter().filter_map(|r| match &r.inline {
        Some(Inline::Footnote(t)) => Some(t.as_str()),
        _ => None,
    }).collect();
    assert_eq!(notes, [" First note text.", " Second note."]);
    // the style of the reference run goes on to the paragraph's return; "footnote text" is the style the note used, brought in too
    assert_eq!(s.paragraphs[0].runs.last().unwrap().char_style.as_deref(), Some("Footnote reference"));
    assert!(s.paragraph_styles.iter().any(|p| p.name == "Footnote text"));
    assert!(s.character_styles.iter().any(|p| p.name == "Footnote reference" && p.attrs.contains(&("FontStyle".to_owned(), "Nothing".to_owned()))));
}

#[test]
fn languages_follow_the_measured_table() {
    let s = docx("docxtext-lang3.docx");
    let lang = |p: usize, r: usize| s.paragraphs[p].runs[r].attrs.iter().find(|(k, _)| k == "AppliedLanguage").map(|(_, v)| v.as_str());
    // Latin and Hangul in one run: w:val for the whole run; a run of Hangul alone takes the East Asian slot (none given: no language)
    assert_eq!(lang(0, 0), Some("$ID/de_DE_2006"));
    assert_eq!(lang(0, 1), Some("$ID/de_DE_2006"));
    assert_eq!(lang(0, 2), None);
    // a paragraph style's language is not imported, a character style's is (it is on the run)
    assert_eq!(lang(1, 0), None);
    assert_eq!(lang(2, 0), Some("$ID/French"));
    assert_eq!(lang(2, 1), None);
    let s = docx("docxtext-lang2.docx");
    let name = |i: usize| s.paragraphs[i].runs[0].attrs.iter().find(|(k, _)| k == "AppliedLanguage").map(|(_, v)| v.as_str());
    // en-GB, es-ES, nl-NL, sv-SE and sk-SK are not mapped; Slovenian comes in as Slovak
    assert_eq!([name(0), name(1), name(2), name(3), name(4)], [None, None, Some("$ID/Portuguese"), None, None]);
    assert_eq!(name(10), None);
    assert_eq!(name(28), Some("$ID/Slovak"));
}

#[test]
fn removing_formatting_removes_languages_and_notes() {
    let plain = Options { remove_formatting: true, ..Options::default() };
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links/docxtext-lang3.docx")).unwrap();
    let s = read_bytes(&bytes, &plain).unwrap();
    assert!(s.paragraphs.iter().flat_map(|p| &p.runs).all(|r| r.attrs.is_empty()));
}
