//! Word pictures: the expected values are what InDesign 2025 made of the same files
//! (artifacts/docxtext-c3-20261023/measurement/g1.json and g1 IDML; next/oracle/goldens/docxtext2/docxtext-c3-pic*).
use super::*;

fn docx(name: &str) -> TextSource {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(name)).unwrap();
    read_bytes(&bytes, &Options::default()).unwrap()
}

fn pictures(s: &TextSource) -> Vec<&Picture> {
    s.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| match &r.inline {
        Some(Inline::Picture(p)) => Some(p),
        _ => None,
    }).collect()
}

#[test]
fn an_inline_picture_is_one_object_character_the_size_of_its_extent() {
    let s = docx("docxtext3-g01.docx");
    let pics = pictures(&s);
    assert_eq!(pics.len(), 1);
    assert_eq!((pics[0].width, pics[0].height, pics[0].format.as_str()), (144.0, 72.0, "PNG"));
    assert_eq!(decode_base64(&pics[0].data).unwrap().len(), 151);
    // text, the picture's character, text
    let runs: Vec<&str> = s.paragraphs[0].runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(runs, ["Before ", "\u{fffc}", " after"]);
    assert!(pics[0].alt.is_none());
}

#[test]
fn a_floating_picture_and_a_crop_come_in_inline_and_stretched_with_notes() {
    let s = docx("docxtext3-g05.docx");
    assert_eq!(pictures(&s).len(), 1);
    assert!(s.warnings.iter().any(|w| w.contains("떠 있는 그림")), "{:?}", s.warnings);
    let s = docx("docxtext3-g06.docx");
    assert_eq!(pictures(&s).len(), 1);
    assert!(s.warnings.iter().any(|w| w.contains("잘라내기")), "{:?}", s.warnings);
}

#[test]
fn the_description_is_the_alternate_text_and_the_same_bytes_come_in_twice() {
    assert_eq!(pictures(&docx("docxtext3-g07.docx"))[0].alt.as_deref(), Some("A red and blue picture"));
    let s = docx("docxtext3-g04.docx");
    let pics = pictures(&s);
    assert_eq!(pics.len(), 2);
    assert_eq!(pics[0].data, pics[1].data);
}

fn first_table(s: &TextSource) -> &Table {
    s.paragraphs.iter().flat_map(|p| p.runs.iter()).find_map(|r| match &r.inline {
        Some(Inline::Table(t)) => Some(t),
        _ => None,
    }).expect("a table")
}

fn edge<'a>(t: &'a Table, row: usize, column: usize, key: &str) -> Option<&'a str> {
    t.cells.iter().find(|c| c.row == row && c.column == column).and_then(|c| c.attrs.iter().find(|(k, _)| k == key)).map(|(_, v)| v.as_str())
}

#[test]
fn a_cell_border_is_written_on_both_cells_of_the_edge_and_the_bottom_goes_down_the_column() {
    // docxtext3-e04 (InDesign: artifacts/docxtext-c3-20261023/measurement/e04.idml): the bottom of the first cell of a 3x3 table
    let s = docx("docxtext3-e04.docx");
    let t = first_table(&s);
    assert_eq!(edge(t, 0, 0, "BottomEdgeStrokePriority"), Some("2"));
    assert_eq!(edge(t, 1, 0, "TopEdgeStrokePriority"), Some("2"));
    assert_eq!(edge(t, 1, 0, "BottomEdgeStrokePriority"), Some("2"));
    assert_eq!(edge(t, 2, 0, "TopEdgeStrokePriority"), Some("2"));
    assert_eq!(edge(t, 2, 0, "BottomEdgeStrokePriority"), None);
    assert_eq!(edge(t, 0, 0, "BottomEdgeStrokeColor"), Some("Color/Word_R255_G0_B0"));
    assert!(s.colors.iter().any(|c| c.name == "Word_R255_G0_B0"));
}

#[test]
fn the_kind_of_line_names_the_stroke_style() {
    let t = docx("docxtext2-b5.docx");
    assert_eq!(edge(first_table(&t), 1, 1, "BottomEdgeStrokeType"), Some("StrokeStyle/$ID/ThinThin"));
    assert!(!t.warnings.iter().any(|w| w.contains("칸의 테두리")));
}

fn first_footnote(s: &TextSource) -> &Run {
    s.paragraphs.iter().flat_map(|p| p.runs.iter()).find(|r| matches!(r.inline, Some(Inline::Footnote(_)))).expect("a footnote")
}

#[test]
fn a_footnote_keeps_the_formatting_of_its_runs_and_its_paragraphs() {
    // docxtext3-q01 (bold and italic), q02 (two paragraphs), q03 (colour and size)
    let s = docx("docxtext3-q01.docx");
    let note = first_footnote(&s);
    let runs: Vec<(&str, Vec<(&str, &str)>)> = note.note.iter().map(|r| (r.text.as_str(), r.attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect())).collect();
    assert_eq!(runs[1], ("bold", vec![("FontStyle", "Bold")]));
    assert_eq!(runs[3], ("italic", vec![("FontStyle", "Italic")]));
    assert!(!s.warnings.iter().any(|w| w.contains("글자 서식")));
    let s = docx("docxtext3-q02.docx");
    let note = first_footnote(&s);
    assert_eq!(note.inline, Some(Inline::Footnote(" First paragraph.\rSecond paragraph.".into())));
    assert!(note.note.is_empty());
    let s = docx("docxtext3-q03.docx");
    let attrs: Vec<&(String, String)> = first_footnote(&s).note.iter().flat_map(|r| r.attrs.iter()).collect();
    assert!(attrs.iter().any(|(k, v)| k == "FillColor" && v == "Color/Word_R255_G0_B0") && attrs.iter().any(|(k, v)| k == "PointSize" && v == "20"));
}

#[test]
fn a_table_border_of_another_kind_names_the_stroke_style() {
    // docxtext3-s01 (double), s02 (dashed), s12 (dashed between rows only), s13 (dotted between columns only)
    let key = |t: &Table, k: &str| t.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let s = docx("docxtext3-s01.docx");
    let t = first_table(&s);
    assert_eq!(key(t, "TopBorderStrokeType").as_deref(), Some("StrokeStyle/$ID/ThinThin"));
    assert_eq!((key(t, "StartRowStrokeType"), key(t, "EndRowStrokeType")), (Some("StrokeStyle/$ID/ThinThin".into()), Some("StrokeStyle/$ID/ThinThin".into())));
    assert_eq!(key(t, "EndColumnLineStyle").as_deref(), Some("StrokeStyle/$ID/ThinThin"));
    assert!(!s.warnings.iter().any(|w| w.contains("표 테두리 모양")));
    let s = docx("docxtext3-s13.docx");
    assert_eq!(key(first_table(&s), "StartColumnStrokeType").as_deref(), Some("StrokeStyle/$ID/Canned Dotted"));
    assert_eq!(key(first_table(&docx("docxtext3-s08.docx")), "TopBorderStrokeType"), None);
}
