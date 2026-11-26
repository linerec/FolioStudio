//! RTF files: the expected values are what InDesign 2025 made of the same files (artifacts/placetext-c2-20261023/measurement rtf*,
//! next/oracle/goldens/text/placetext-rtf-*).
use super::*;

fn rtf(name: &str) -> TextSource {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(name)).unwrap();
    read_bytes(&bytes, &Options::default()).unwrap()
}

#[test]
fn an_rtf_style_sheet_brings_its_styles_and_only_the_differences() {
    // measured: docxtext3-r01 / artifacts/docxtext-c4-20261023/measurement/r01.idml
    let s = rtf("docxtext3-r01.rtf");
    assert_eq!(s.paragraph_styles.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Normal", "heading 1", "Quote"]);
    let kv = |attrs: &[(String, String)]| attrs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
    assert_eq!(s.paragraphs[0].style.as_deref(), Some("heading 1"));
    assert_eq!(kv(&s.paragraphs[0].attrs), ["SpaceBefore=0", "SpaceAfter=0", "KeepWithNext=0"]);
    assert_eq!(kv(&s.paragraphs[0].runs[0].attrs), ["FontStyle=Regular", "PointSize=12"]);
    // plain, bold, 16 pt says what the style says
    assert!(s.paragraphs[1].runs[0].attrs.is_empty());
    assert_eq!(kv(&s.paragraph_styles[1].attrs), ["FontStyle=Bold", "PointSize=16", "SpaceBefore=12", "SpaceAfter=6", "KeepWithNext=1"]);
    assert_eq!(s.paragraph_styles[1].based_on.as_deref(), Some("Normal"));
    assert_eq!(s.paragraph_styles[1].next.as_deref(), Some("Normal"));
    // a file without a style sheet in use keeps its paragraphs without styles
    assert!(read_bytes(br"{\rtf1\ansi\pard Body\par}", &Options::default()).unwrap().paragraph_styles.is_empty());
}

#[test]
fn rtf_text_keeps_its_fonts_formatting_and_paragraph_settings() {
    let s = rtf("placetext-format.rtf");
    assert!(s.paragraph_styles.is_empty() && s.colors.iter().map(|c| c.name.as_str()).eq(["Red", "Blue"]));
    let attrs = |p: usize, r: usize| s.paragraphs[p].runs[r].attrs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
    assert_eq!(attrs(0, 1), ["FontStyle=Bold"]);
    assert_eq!(attrs(0, 3), ["FontStyle=Bold Italic"]);
    assert_eq!(s.font_families(), ["Arial", "Times New Roman"]);
    // the return takes the formatting of the point it stands at (underline was switched off before it)
    assert!(s.paragraphs[0].end.as_ref().is_some_and(|e| e.attrs.is_empty()));
    let p = rtf("placetext-para.rtf");
    let has = |i: usize, kv: (&str, &str)| p.paragraphs[i].attrs.contains(&(kv.0.to_owned(), kv.1.to_owned()));
    assert!(has(0, ("Justification", "CenterAlign")) && has(1, ("Justification", "RightAlign")) && has(2, ("Justification", "LeftJustified")));
    assert!(has(3, ("LeftIndent", "36")) && has(3, ("FirstLineIndent", "18")) && has(4, ("SpaceBefore", "12")) && has(4, ("SpaceAfter", "6")));
    assert_eq!(p.paragraphs[5].tabs, [("LeftAlign".to_owned(), 144.0)]);
}

#[test]
fn rtf_last_par_ends_the_file_and_one_more_leaves_an_empty_paragraph() {
    let plain = rtf("placetext-plain.rtf");
    assert_eq!(plain.story_text(), "First paragraph.\rSecond paragraph.\r\rFourth after blank.");
    let trail = rtf("placetext-rtftrail.rtf");
    assert_eq!((trail.story_text().as_str(), trail.ends_with_return), ("A\rB\r", true));
    assert_eq!(rtf("placetext-rtfnopar.rtf").paragraphs.len(), 1);
    assert_eq!(rtf("placetext-empty.rtf").story_text(), "");
    assert_eq!(rtf("placetext-unicode.rtf").story_text(), "caf\u{e9} \u{d55c}\u{ae00} uni");
}

#[test]
fn rtf_colours_are_named_the_way_indesign_names_them() {
    let s = rtf("placetext-rtfcolors.rtf");
    let names: Vec<_> = s.colors.iter().map(|c| c.name.as_str()).collect();
    // black is no colour; Yellow's own name is not what InDesign gives it
    assert_eq!(names, ["Bright Green", "Gray-50%", "RTF r18 g52 b86", "R=255 G=255 B=0", "Pink", "Turquoise", "Red"]);
}

/// `FOLIO_DUMP_RTF=<file> cargo test -p folio-textimport dump_rtf -- --ignored --nocapture`: what the reader makes of a file (a debugging aid for the oracle).
#[test]
#[ignore]
fn dump_rtf() {
    let Ok(path) = std::env::var("FOLIO_DUMP_RTF") else { return };
    let s = read(std::path::Path::new(&path), &Options::default()).unwrap();
    println!("{}", serde_json::to_string(&s.paragraph_styles).unwrap());
    println!("{}", serde_json::to_string(&s.character_styles).unwrap());
    for p in &s.paragraphs {
        println!("P style={:?} attrs={:?} tabs={:?} props={:?}", p.style, p.attrs, p.tabs, p.properties);
        for r in p.runs.iter().chain(p.end.iter()) {
            println!("   R cs={:?} {:?} attrs={:?} props={:?}", r.char_style, r.text, r.attrs, r.props);
        }
    }
}

fn table_at(name: &str) -> Table {
    table_of(&rtf(name).paragraphs[0]).clone()
}

fn table_of(p: &Paragraph) -> &Table {
    match p.runs.iter().find_map(|r| r.inline.as_ref()) {
        Some(Inline::Table(t)) => t,
        other => panic!("not a table: {other:?}"),
    }
}

#[test]
fn an_rtf_table_is_a_paragraph_of_its_own_with_the_grid_of_all_its_rows() {
    // measured: artifacts/docxtext-c5-20261023/measurement/w01, w09, w10, w17 (docxtext-c5-rtf-w*)
    let s = rtf("docxtext5-w01.rtf");
    assert_eq!(s.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>(), ["before", "\u{16}", "after"]);
    let t = table_of(&s.paragraphs[1]);
    assert_eq!((t.rows, t.columns.clone(), t.header_rows, t.zero_insets), (2, vec![100.0, 100.0], 0, true));
    assert_eq!(t.cells.iter().map(|c| c.paragraphs[0].text()).collect::<Vec<_>>(), ["A", "B", "C", "D"]);
    // rows of different grids: the union of the edges, cells that span columns
    let t = table_at("docxtext5-w09.rtf");
    assert_eq!(t.columns, [100.0, 50.0, 50.0, 100.0]);
    assert_eq!(t.cells.iter().map(|c| (c.row, c.column, c.column_span)).collect::<Vec<_>>(), [(0, 0, 1), (0, 1, 2), (0, 3, 1), (1, 0, 2), (1, 2, 2)]);
    // the row starts at \trleft; a minimum row height
    assert_eq!(table_at("docxtext5-w17.rtf").columns, [105.4, 100.0]);
    assert_eq!(table_at("docxtext5-w10.rtf").row_heights, [20.0]);
}

#[test]
fn an_rtf_table_merges_rows_not_columns_and_counts_its_header_rows() {
    // measured: w06 (horizontal merge: nothing), w07, w21 (vertical merge), w23 (header rows are counted)
    let t = table_at("docxtext5-w06.rtf");
    assert!(t.cells.iter().all(|c| c.column_span == 1 && c.row_span == 1));
    let t = table_at("docxtext5-w21.rtf");
    assert_eq!(t.cells.iter().map(|c| (c.row, c.column, c.row_span)).collect::<Vec<_>>(), [(0, 0, 2), (0, 1, 3), (0, 2, 1), (1, 2, 1), (2, 0, 1), (2, 2, 1)]);
    assert_eq!(table_at("docxtext5-w23.rtf").header_rows, 3);
}

#[test]
fn an_rtf_table_fills_and_aligns_its_cells() {
    // measured: w04, w15, w25
    let t = table_at("docxtext5-w04.rtf");
    let fill = |i: usize| t.cells[i].attrs.iter().filter(|(k, _)| k.starts_with("Fill")).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
    assert_eq!(fill(0), ["FillColor=Color/Red", "FillTint=100"]);
    assert_eq!(fill(2), ["FillTint=100"]);
    let t = table_at("docxtext5-w25.rtf");
    assert!(t.cells[0].attrs.contains(&("FillTint".to_owned(), "50".to_owned())) && t.cells[1].attrs.contains(&("FillTint".to_owned(), "25".to_owned())));
    let t = table_at("docxtext5-w15.rtf");
    assert_eq!(t.cells.iter().map(|c| c.attrs.iter().find(|(k, _)| k == "VerticalJustification").unwrap().1.clone()).collect::<Vec<_>>(), ["CenterAlign", "BottomAlign", "TopAlign"]);
}

#[test]
fn an_rtf_list_is_the_same_list_as_a_word_one_with_the_paragraphs_own_indents() {
    // measured: l01 (bullets), l02 (numbers, a plain paragraph between), l03 (two levels), l11, l20 (a start), l07 (no indent)
    let s = rtf("docxtext5-l01.rtf");
    let kv = |p: &Paragraph| p.attrs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
    assert!(kv(&s.paragraphs[0]).contains(&"BulletsAndNumberingListType=BulletList".to_owned()) && kv(&s.paragraphs[0]).contains(&"LeftIndent=36".to_owned()));
    assert!(s.paragraphs[0].properties.contains("BulletCharacterValue=\"183\""));
    assert_eq!(s.paragraphs[0].text(), "one");
    assert_eq!(s.character_styles.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Styles for Word/RTF Imported Lists:Word Imported List Style1"]);
    let s = rtf("docxtext5-l02.rtf");
    assert!(kv(&s.paragraphs[0]).contains(&"NumberingContinue=false".to_owned()) && !kv(&s.paragraphs[3]).iter().any(|a| a.starts_with("NumberingContinue")));
    assert_eq!(s.numbering_lists, ["Word Imported List 1"]);
    // after a second-level paragraph every paragraph carries the restart policy, a plain one too
    let s = rtf("docxtext5-l03.rtf");
    assert!(!s.paragraphs[0].properties.contains("NumberingRestartPolicies") && s.paragraphs[3].properties.contains("NumberingRestartPolicies") && s.paragraphs[4].properties.contains("NumberingRestartPolicies"));
    // a start other than 1 stays on the first unbroken run of the list
    let has_start = |s: &TextSource, p: usize| s.paragraphs[p].attrs.iter().any(|(k, v)| k == "NumberingStartAt" && v == "3");
    let s = rtf("docxtext5-l20.rtf");
    assert!(has_start(&s, 0) && has_start(&s, 1) && has_start(&s, 2));
    let s = rtf("docxtext5-l11.rtf");
    assert!(has_start(&s, 0) && !has_start(&s, 2));
    // the old paragraph numbering is no list; the numbering text goes
    let s = rtf("docxtext5-l05.rtf");
    assert_eq!(s.paragraphs[0].text(), "old one");
    assert!(!kv(&s.paragraphs[0]).iter().any(|a| a.starts_with("Bullets")));
}

#[test]
fn rtf_paragraph_borders_become_the_paragraph_border() {
    // build-a: the sides, weights (twips), colour of the colour table, line kind and the space to the text
    let s = read_bytes(br"{\rtf1\ansi{\colortbl;\red255\green0\blue0;}\pard\brdrt\brdrs\brdrw20\brsp40\brdrcf1\brdrb\brdrdb\brdrw10 Boxed\par\pard Plain\par}", &Options::default()).unwrap();
    let get = |p: usize, k: &str| s.paragraphs[p].attrs.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    assert_eq!(get(0, "ParagraphBorderOn").as_deref(), Some("true"));
    assert_eq!(get(0, "ParagraphBorderTopLineWeight").as_deref(), Some("1"));
    assert_eq!(get(0, "ParagraphBorderTopOffset").as_deref(), Some("2"));
    assert_eq!(get(0, "ParagraphBorderBottomLineWeight").as_deref(), Some("0.5"));
    assert_eq!(get(0, "ParagraphBorderLeftLineWeight").as_deref(), Some("0"));
    assert_eq!(get(0, "ParagraphBorderColor").as_deref(), Some("Color/Red"));
    assert_eq!(get(0, "ParagraphBorderType").as_deref(), Some("StrokeStyle/$ID/Solid"));
    assert_eq!(get(1, "ParagraphBorderOn"), None);
    assert!(s.colors.iter().any(|c| c.name == "Red") && s.warnings.is_empty(), "{:?}", s.warnings);
    // \box: the four sides; \brdrnone takes a side away
    let s = read_bytes(br"{\rtf1\ansi\pard\box\brdrdot\brdrw15 A\par}", &Options::default()).unwrap();
    let a = &s.paragraphs[0].attrs;
    for side in ["Top", "Left", "Bottom", "Right"] {
        assert!(a.contains(&(format!("ParagraphBorder{side}LineWeight"), "0.75".to_owned())), "{a:?}");
    }
    assert!(a.contains(&("ParagraphBorderType".to_owned(), "StrokeStyle/$ID/Canned Dotted".to_owned())));
    let s = read_bytes(br"{\rtf1\ansi\pard\brdrb\brdrnone A\par}", &Options::default()).unwrap();
    assert!(!s.paragraphs[0].attrs.iter().any(|(k, _)| k == "ParagraphBorderOn"));
    // without formatting: no border
    let plain = read_bytes(br"{\rtf1\ansi\pard\box\brdrs A\par}", &Options { remove_formatting: true, ..Options::default() }).unwrap();
    assert!(!plain.paragraphs[0].attrs.iter().any(|(k, _)| k.starts_with("ParagraphBorder")));
}

#[test]
fn rtf_table_borders_go_to_the_sides_a_cell_does_not_say() {
    // build-a: \trbrdr… outer sides on the table's edge, h and v between rows and columns; a cell's own side wins
    let row = |cells: &str| format!(r"\trowd\trbrdrt\brdrs\brdrw20\trbrdrl\brdrs\brdrw20\trbrdrb\brdrs\brdrw20\trbrdrr\brdrs\brdrw20\trbrdrh\brdrs\brdrw10\trbrdrv\brdrs\brdrw10{cells}\cellx2000\cellx4000\pard\intbl A\cell B\cell\row");
    let file = format!(r"{{\rtf1\ansi{}{}\pard after\par}}", row(""), row(r"\clbrdrt\brdrs\brdrw40"));
    let s = read_bytes(file.as_bytes(), &Options::default()).unwrap();
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    let t = s.paragraphs.iter().flat_map(|p| &p.runs).find_map(|r| match &r.inline { Some(Inline::Table(t)) => Some(t), _ => None }).unwrap();
    let edge = |row: usize, column: usize, key: &str| {
        let cell = t.cells.iter().find(|c| c.row == row && c.column == column).unwrap();
        cell.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    };
    assert_eq!(edge(0, 0, "TopEdgeStrokeWeight").as_deref(), Some("1"));
    assert_eq!(edge(0, 0, "LeftEdgeStrokeWeight").as_deref(), Some("1"));
    assert_eq!(edge(0, 1, "LeftEdgeStrokeWeight").as_deref(), Some("0.5"), "{:?}", t.cells[1].attrs);
    assert_eq!(edge(1, 1, "BottomEdgeStrokeWeight").as_deref(), Some("1"));
    assert_eq!(edge(1, 1, "RightEdgeStrokeWeight").as_deref(), Some("1"));
}
