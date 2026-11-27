//! Word files: the expected values are what InDesign 2025 made of the same files (artifacts/placetext-c2-20261023/measurement,
//! next/oracle/goldens/text/placetext-docx-*).
use super::*;
use std::io::{Cursor, Write};

pub(super) fn word_file(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn word_quotes_keep_run_boundaries_in_body_and_formatted_footnotes() {
    let body = r#"<document><body>
        <p><r><t>한🙂</t></r><r><rPr><b/></rPr><t>"'</t></r></p>
        <p><r><t>"B"</t></r><r><footnoteReference id="1"/></r></p>
        </body></document>"#;
    let notes = r#"<footnotes><footnote id="1"><p>
        <r><t>"A</t></r><r><rPr><b/></rPr><t>"</t></r>
        </p></footnote></footnotes>"#;
    let bytes = word_file(&[("word/document.xml", body), ("word/footnotes.xml", notes)]);
    let source = read_bytes(&bytes, &Options::default()).unwrap();
    assert_eq!(source.paragraphs[0].runs[0].text, "한🙂");
    assert_eq!(source.paragraphs[0].runs[1].text, "”’");
    assert_eq!(source.paragraphs[1].runs[0].text, "“B”");
    let marker = &source.paragraphs[1].runs[1];
    assert!(matches!(&marker.inline, Some(Inline::Footnote(text)) if text == "“A”"));
    assert_eq!(marker.note.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["“A", "“"]);
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(name)).unwrap()
}

fn docx(name: &str) -> TextSource {
    read_bytes(&fixture(name), &Options::default()).unwrap()
}

#[test]
fn a_broken_word_file_is_an_error() {
    let word = read_bytes(b"PK\x03\x04rest of a zip", &Options::default()).unwrap_err();
    assert!(word.contains("Word"), "{word}");
}

#[test]
fn word_styles_come_with_their_settings_in_the_order_of_the_file() {
    let s = docx("placetext-styles.docx");
    let names: Vec<_> = s.paragraph_styles.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["Normal", "Heading 1", "Quote"]);
    let heading = &s.paragraph_styles[1];
    assert_eq!(heading.based_on.as_deref(), Some("Normal"));
    for want in [("FontStyle", "Bold"), ("PointSize", "16"), ("SpaceBefore", "12"), ("SpaceAfter", "3"), ("KeepWithNext", "1")] {
        assert!(heading.attrs.contains(&(want.0.to_owned(), want.1.to_owned())), "{want:?} in {:?}", heading.attrs);
    }
    assert_eq!(s.paragraph_styles[0].attrs, [("PointSize".to_owned(), "11".to_owned())]);
    assert_eq!(s.character_styles.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Strong"]);
    assert_eq!(s.paragraphs.iter().map(|p| p.style.as_deref()).collect::<Vec<_>>(), [Some("Heading 1"), Some("Normal"), Some("Quote"), Some("Normal")]);
    assert_eq!(s.paragraphs[3].runs[1].char_style.as_deref(), Some("Strong"));
}

#[test]
fn word_local_formatting_is_written_only_where_it_differs_from_the_style() {
    let s = docx("placetext-overlap.docx");
    let attrs = |p: usize, r: usize| s.paragraphs[p].runs[r].attrs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
    // the runs of one style that look alike are one run to the story writer; here every run says what it changes
    assert!(attrs(0, 0).is_empty());
    assert_eq!(attrs(0, 1), ["FontStyle=Bold Italic"], "italic on a bold style adds up");
    assert!(attrs(0, 2).is_empty(), "bold off on a bold style leaves nothing");
    assert_eq!(attrs(0, 3), ["PointSize=20"]);
    assert!(attrs(0, 4).is_empty(), "the style's own size is no override");
    assert_eq!(attrs(1, 4), ["Tracking=181"]);
}

#[test]
fn word_colours_are_swatches_named_by_their_values() {
    let s = docx("placetext-colors.docx");
    let names: Vec<_> = s.colors.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Word_R255_G0_B0", "Word_R0_G255_B0", "Word_R0_G0_B0", "Word_R31_G56_B100", "Word_R68_G114_B196"]);
    assert_eq!(s.colors[3].rgb, [31, 56, 100]);
    // "auto" is no colour
    assert!(s.paragraphs[0].runs[3].attrs.is_empty());
}

#[test]
fn word_breaks_follow_the_page_break_option() {
    let split = |o: PageBreaks| {
        let s = read_bytes(&fixture("placetext-breaks.docx"), &Options { convert_page_breaks: o, ..Options::default() }).unwrap();
        (s.paragraphs.iter().map(|p| p.break_type.clone()).collect::<Vec<_>>(), s.story_text())
    };
    let (kinds, text) = split(PageBreaks::PageBreak);
    assert_eq!(kinds, [None, Some("NextPage".into()), None, None, Some("NextColumn".into()), None]);
    assert!(text.starts_with("Before line break\u{2028}after line break\rBefore page break\rafter page\rA\tB\rcol\rbreak"), "{text:?}");
    assert!(split(PageBreaks::ColumnBreak).0.iter().all(|k| k.as_deref() != Some("NextPage")));
    let (kinds, text) = split(PageBreaks::None);
    assert!(kinds.iter().all(Option::is_none));
    assert!(text.contains("Before page breakafter page") && text.ends_with("colbreak"), "{text:?}");
}

#[test]
fn word_without_paragraphs_is_one_empty_paragraph_and_brings_no_style() {
    let s = docx("placetext-empty.docx");
    assert_eq!(s.story_text(), "");
    assert!(s.paragraph_styles.is_empty() && s.colors.is_empty());
}

#[test]
fn word_quotes_become_curly_and_remove_formatting_keeps_the_text_only() {
    assert_eq!(docx("placetext-wquotes.docx").story_text(), "He said \u{201c}hi\u{201d} and \u{2018}bye\u{2019} don\u{2019}t. \u{201c}curly\u{201d} \u{2018}single\u{2019} 5\u{2019}10\u{201d}");
    let plain = Options { remove_formatting: true, ..Options::default() };
    let s = read_bytes(&fixture("placetext-styles.docx"), &plain).unwrap();
    assert!(s.paragraph_styles.is_empty() && s.character_styles.is_empty());
    assert!(s.paragraphs.iter().all(|p| p.style.is_none() && p.runs.iter().all(|r| r.attrs.is_empty() && r.char_style.is_none())));
}

#[test]
fn word_lists_are_marked_as_lists_not_dropped() {
    let s = docx("placetext-lists.docx");
    let kinds: Vec<_> = s.paragraphs.iter().map(|p| p.attrs.iter().find(|(k, _)| k == "BulletsAndNumberingListType").map(|(_, v)| v.as_str())).collect();
    assert_eq!(kinds, [Some("BulletList"), Some("BulletList"), Some("NumberedList"), Some("NumberedList"), None]);
    assert_eq!(s.numbering_lists, ["Word Imported List 1"]);
}
