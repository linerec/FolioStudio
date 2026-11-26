//! Word lists: the expected values are what InDesign 2025 made of the same files
//! (artifacts/docxtext-c2-20261023/measurement/lists2.idml, lists3.idml; next/oracle/goldens/docxtext2/docxtext-c2-lists*).
use super::*;

fn docx(name: &str) -> TextSource {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(name)).unwrap();
    read_bytes(&bytes, &Options::default()).unwrap()
}

fn attr<'a>(p: &'a Paragraph, key: &str) -> Option<&'a str> {
    p.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn paragraph<'a>(s: &'a TextSource, text: &str) -> &'a Paragraph {
    s.paragraphs.iter().find(|p| p.text() == text).unwrap_or_else(|| panic!("no paragraph {text}"))
}

#[test]
fn a_numbered_list_has_its_expression_list_and_character_style() {
    let s = docx("docxtext-lists.docx");
    let one = paragraph(&s, "Number one");
    assert_eq!(attr(one, "BulletsAndNumberingListType"), Some("NumberedList"));
    assert_eq!((attr(one, "LeftIndent"), attr(one, "FirstLineIndent")), (Some("36"), Some("-18")));
    assert_eq!((attr(one, "NumberingContinue"), attr(one, "NumberingApplyRestartPolicy"), attr(one, "NumberingExpression")), (Some("false"), Some("false"), Some("^1.^t")));
    assert!(one.properties.contains("NumberingList/Word Imported List 1") && one.properties.contains("Word Imported List Style3"), "{}", one.properties);
    // the second paragraph of the list does not restart it; a deeper level restarts after the level above it
    let two = paragraph(&s, "Number two");
    assert_eq!(attr(two, "NumberingContinue"), None);
    let sub = paragraph(&s, "Sub letter a");
    assert_eq!((attr(sub, "NumberingLevel"), attr(sub, "NumberingContinue")), (Some("2"), Some("false")));
    assert!(sub.properties.contains("<NumberingFormat type=\"string\">a, b, c, d...</NumberingFormat>"));
    assert!(sub.properties.contains("RestartPolicy=\"AfterSpecificLevel\" LowerLevel=\"1\" UpperLevel=\"1\""));
    let roman = paragraph(&s, "Sub roman i");
    assert!(roman.properties.contains("RestartPolicy=\"RangeOfLevels\" LowerLevel=\"1\" UpperLevel=\"2\""));
    // a start value other than 1 stands on the first paragraph only
    assert_eq!(attr(paragraph(&s, "Start five"), "NumberingStartAt"), Some("5"));
    assert_eq!(attr(paragraph(&s, "Start six"), "NumberingStartAt"), None);
    assert_eq!(s.numbering_lists.len(), 4);
}

#[test]
fn a_bullet_names_its_character_unless_it_is_the_dot() {
    let s = docx("docxtext-lists.docx");
    assert!(!paragraph(&s, "Bullet one").properties.contains("BulletChar"));
    assert!(paragraph(&s, "Sub bullet").properties.contains("BulletCharacterValue=\"111\""));
    let s = docx("docxtext2-lists2.docx");
    assert!(paragraph(&s, "Item 13 of num 13").properties.contains("BulletCharacterValue=\"8211\""));
    // Word's Symbol and Wingdings bullets: the low byte of the private-area character
    assert!(paragraph(&s, "Item 15 of num 15").properties.contains("BulletCharacterValue=\"183\""));
    assert!(paragraph(&s, "Item 16 of num 16").properties.contains("BulletCharacterValue=\"167\""));
}

#[test]
fn a_suffix_and_a_number_format_change_the_expression_and_the_format() {
    let s = docx("docxtext2-lists2.docx");
    assert_eq!(attr(paragraph(&s, "Item 18 of num 18"), "NumberingExpression"), Some("^1. "));
    assert_eq!(attr(paragraph(&s, "Item 19 of num 19"), "NumberingExpression"), Some("^1."));
    assert_eq!(attr(paragraph(&s, "Item 6 of num 6"), "NumberingExpression"), Some("Chapter ^1:^t"));
    for (text, format) in [("Item 1 of num 1", Some("A, B, C, D...")), ("Item 3 of num 3", Some("01,02,03...")), ("Item 4 of num 4", None), ("Item 10 of num 10", Some("$ID/(kanji) 1,2,3,4...")), ("Item 11 of num 11", None)] {
        let p = paragraph(&s, text);
        assert_eq!(p.properties.contains("NumberingFormat"), format.is_some(), "{text}");
        if let Some(format) = format {
            assert!(p.properties.contains(format), "{text}: {}", p.properties);
        }
    }
}

#[test]
fn a_list_carried_by_a_paragraph_style_and_a_list_continued_after_a_plain_paragraph() {
    let s = docx("docxtext2-lists3.docx");
    // the numbering survives a plain paragraph between two list paragraphs
    assert_eq!(attr(paragraph(&s, "Continue three"), "NumberingContinue"), None);
    // a direct indent beats the list level's
    assert_eq!((attr(paragraph(&s, "Direct indent"), "LeftIndent"), attr(paragraph(&s, "Direct indent"), "FirstLineIndent")), (Some("72"), Some("-36")));
    // the style carries the list; a numId 0 takes it away
    assert_eq!(attr(paragraph(&s, "Style carries the list"), "BulletsAndNumberingListType"), Some("BulletList"));
    assert_eq!(attr(paragraph(&s, "Style but no list"), "BulletsAndNumberingListType"), None);
}

#[test]
fn symbol_bullets_share_one_character_style_with_the_language() {
    let s = docx("docxtext2-lists2.docx");
    let shared: Vec<_> = s.character_styles.iter().filter(|c| c.attrs.iter().any(|(k, _)| k == "AppliedLanguage")).collect();
    assert_eq!(shared.len(), 1);
    assert!(shared[0].name.ends_with("Word Imported List Style15"));
}

#[test]
fn an_endnote_is_one_reference_with_the_text_of_its_note_and_the_file_ends_with_a_return() {
    let s = docx("docxtext2-en2.docx");
    let notes: Vec<_> = s.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| match &r.inline {
        Some(Inline::Endnote(t)) => Some(t.as_str()),
        _ => None,
    }).collect();
    // the text after the number: Word's note text keeps its leading space
    assert_eq!(notes, [" First end note.", " Second end note.", " Third end note."]);
    assert!(s.ends_with_return, "a Word file with endnotes comes in with a return after its last paragraph");
    assert!(!s.warnings.iter().any(|w| w.contains("미주")), "{:?}", s.warnings);
    assert!(s.paragraph_styles.iter().any(|p| p.name == "Endnote text") && s.character_styles.iter().any(|c| c.name == "Endnote reference"));
    assert!(!docx("docxtext2-fn3.docx").ends_with_return);
}

fn table_of(s: &TextSource) -> &Table {
    s.paragraphs.iter().flat_map(|p| p.runs.iter()).find_map(|r| match &r.inline {
        Some(Inline::Table(t)) => Some(t),
        _ => None,
    }).expect("a table")
}

fn has(attrs: &[(String, String)], key: &str, value: &str) -> bool {
    attrs.iter().any(|(k, v)| k == key && v == value)
}

#[test]
fn a_shaded_cell_has_its_colour_and_the_cells_below_it_the_tint() {
    let s = docx("docxtext2-b1.docx");
    let t = table_of(&s);
    let at = |row: usize, column: usize| t.cells.iter().find(|c| c.row == row && c.column == column).unwrap();
    assert!(has(&at(1, 1).attrs, "FillColor", "Color/Word_R255_G255_B0") && has(&at(1, 1).attrs, "FillTint", "100"));
    // InDesign (measured) gives the cell under a shaded cell the tint with no colour; nothing to the other cells
    assert_eq!(at(2, 1).attrs, [("FillTint".to_owned(), "100".to_owned())]);
    assert!(at(0, 1).attrs.is_empty() && at(1, 0).attrs.is_empty() && at(2, 0).attrs.is_empty() && at(2, 2).attrs.is_empty());
    // a percentage pattern keeps its colour and takes the percentage as the tint
    let s = docx("docxtext2-b13.docx");
    let t = table_of(&s);
    let pct = t.cells.iter().find(|c| c.row == 2 && c.column == 0).unwrap();
    assert!(has(&pct.attrs, "FillColor", "Color/Word_R0_G0_B0") && has(&pct.attrs, "FillTint", "25"));
    // an automatic fill is a fill of its own (no colour), and the cell under a percentage cell takes its percentage
    let at = |row: usize, column: usize| t.cells.iter().find(|c| c.row == row && c.column == column).unwrap();
    assert!(has(&at(0, 1).attrs, "FillColor", "Swatch/None") && has(&at(0, 1).attrs, "FillTint", "100"));
    assert_eq!(at(1, 0).attrs, [("FillTint".to_owned(), "50".to_owned())]);
    assert!(at(0, 2).attrs.is_empty());
}

#[test]
fn table_borders_are_the_outer_sides_and_the_lines_between_rows_and_columns() {
    // only the top and the bottom said: the others stay InDesign's
    let s = docx("docxtext2-b3.docx");
    let t = table_of(&s);
    assert!(has(&t.attrs, "TopBorderStrokeWeight", "3") && has(&t.attrs, "BottomBorderStrokeColor", "Color/Word_R0_G0_B255"));
    assert!(!t.attrs.iter().any(|(k, _)| k.starts_with("Left") || k.starts_with("Right") || k.contains("Row")));
    // all six, thin and automatic: the weights, and the inner lines name black for their start
    let s = docx("docxtext2-b4.docx");
    let t = table_of(&s);
    assert!(has(&t.attrs, "LeftBorderStrokeWeight", "0.5") && has(&t.attrs, "StartRowStrokeColor", "Color/Black") && has(&t.attrs, "EndColumnStrokeWeight", "0.5"));
    assert!(!t.attrs.iter().any(|(k, _)| k == "EndRowStrokeColor" || k.ends_with("BorderStrokeColor")));
    // none: weight 0 and the type n (the column strokes' end type has InDesign's own name for it)
    let s = docx("docxtext2-b6.docx");
    let t = table_of(&s);
    assert!(has(&t.attrs, "TopBorderStrokeType", "n") && has(&t.attrs, "EndColumnLineStyle", "n") && has(&t.attrs, "StartRowStrokeWeight", "0"));
    // a table style gives no border; the border of a single cell is carried over since docxtext-c3
    assert!(table_of(&docx("docxtext2-b14.docx")).attrs.is_empty());
    assert!(!docx("docxtext2-b5.docx").warnings.iter().any(|w| w.contains("칸의 테두리")));
}

#[test]
fn no_lists_with_remove_formatting() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links/docxtext-lists.docx")).unwrap();
    let s = read_bytes(&bytes, &Options { remove_formatting: true, ..Options::default() }).unwrap();
    assert!(s.numbering_lists.is_empty() && s.paragraphs.iter().all(|p| p.attrs.is_empty() && p.properties.is_empty()));
}
