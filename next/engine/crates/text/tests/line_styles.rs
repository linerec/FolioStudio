//! Nested line styles on the synthetic face (every glyph 0.5 em, word spaces 0.25 em): the paragraph's first lines get the
//! character style's attributes, and the lines are set again until the styled lines break where they are styled
//! (next/oracle/rules/dropcap.ko.md; the InDesign comparison is the oracle's `nlines-*` scenarios).

use folio_model::layout::LaidLine;
use folio_model::nested::NestedLineStyle;
use folio_model::{
    CharacterAttrs, Document, ItemKind, Page, PageItem, Paragraph, ParagraphAttrs, PathPoint, Point, Rect, Spread, Story, SubPath, TextFrame,
    TextRun,
};
use folio_text::{FontStore, layout_document};

fn frame(w: f64, h: f64) -> PageItem {
    let p = |x, y| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    let tf = TextFrame { story: "s".into(), columns: 1, gutter: 12.0, ..TextFrame::default() };
    PageItem {
        id: "f".into(),
        kind: ItemKind::TextFrame(tf),
        paths: vec![SubPath { points: vec![p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)], open: false }],
        ..PageItem::default()
    }
}

fn attrs(size: f64) -> CharacterAttrs {
    CharacterAttrs { point_size: size, language: "Korean".into(), ..CharacterAttrs::default() }
}

/// One paragraph of `n` four-letter words in 10 pt, with nested line styles `items`; the style `big` sets 20 pt, `bold` 15 pt.
fn paragraph(n: usize, items: Vec<NestedLineStyle>) -> Paragraph {
    paragraph_of(&(0..n).map(|_| "abcd").collect::<Vec<_>>().join(" "), items)
}

fn paragraph_of(text: &str, items: Vec<NestedLineStyle>) -> Paragraph {
    let variants = items.iter().map(|i| attrs(if i.style.ends_with("big") { 20.0 } else { 15.0 })).collect();
    Paragraph {
        style: "Body".into(),
        attrs: ParagraphAttrs { hyphenation: false, line_styles: items, ..ParagraphAttrs::default() },
        runs: vec![TextRun { text: text.into(), attrs: attrs(10.0), line_attrs: variants, ..TextRun::default() }],
    }
}

fn item(style: &str, lines: u32, repeat: bool) -> NestedLineStyle {
    NestedLineStyle { style: format!("CharacterStyle/{style}"), lines, repeat, source: None }
}

fn lines_of(p: Paragraph) -> Vec<LaidLine> {
    let doc = Document {
        spreads: vec![Spread {
            id: "s1".into(),
            pages: vec![Page { id: "p1".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 600.0, bottom: 800.0 }, ..Page::default() }],
            items: vec![frame(100.0, 400.0)],
            ..Spread::default()
        }],
        stories: [("s".to_string(), Story { id: "s".into(), paragraphs: vec![p], ..Story::default() })].into(),
        ..Document::default()
    };
    layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone()
}

/// Glyph size of each line (the first glyph's), and the number of characters each line holds.
fn shape(lines: &[LaidLine]) -> (Vec<f64>, Vec<usize>) {
    (lines.iter().map(|l| l.glyphs.first().map_or(0.0, |g| g.size)).collect(), lines.iter().map(|l| l.end - l.start).collect())
}

#[test]
fn a_line_without_styles_is_as_before() {
    let (sizes, counts) = shape(&lines_of(paragraph(10, vec![])));
    assert_eq!(sizes, vec![10.0, 10.0, 10.0]);
    assert_eq!(counts, vec![20, 20, 9], "4 words of 87.5 pt fit a 100 pt line");
}

#[test]
fn the_styled_line_breaks_with_its_own_widths() {
    // 20 pt glyphs: a word is 40 pt, a space 5 pt, so two words fit; the third line onward is 10 pt again and holds four
    let (sizes, counts) = shape(&lines_of(paragraph(12, vec![item("big", 1, false)])));
    assert_eq!(sizes[0], 20.0);
    assert!(sizes[1..].iter().all(|&s| s == 10.0), "{sizes:?}");
    assert_eq!(counts[0], 10, "two words and the space after them");
    assert_eq!(counts[1], 20);
}

#[test]
fn two_lines_and_a_repeat() {
    let (sizes, counts) = shape(&lines_of(paragraph(14, vec![item("big", 2, false)])));
    assert_eq!(&sizes[..3], &[20.0, 20.0, 10.0], "{sizes:?} {counts:?}");
    let (sizes, _) = shape(&lines_of(paragraph(14, vec![item("big", 1, false), item("mid", 1, false), item("x", 2, true)])));
    assert_eq!(&sizes[..4], &[20.0, 15.0, 20.0, 15.0], "{sizes:?}");
}

#[test]
fn a_word_that_fits_only_in_the_next_lines_style_stays_on_the_next_line() {
    // 20 pt: "abcd abcd " is 90 pt of 100, "ab" would need 20 pt more and does not fit; in 10 pt it would (10 pt). Laying the
    // paragraph again never settles (styled, "ab" fits and is styled; then it does not fit), so InDesign's answer is
    // the one with each line broken in its own style: line 1 ends before "ab".
    let (sizes, counts) = shape(&lines_of(paragraph_of("abcd abcd ab abcd abcd abcd abcd abcd", vec![item("big", 1, false)])));
    assert_eq!(sizes[0], 20.0);
    assert_eq!(counts[0], 10, "{counts:?}");
    assert!(sizes[1..].iter().all(|&s| s == 10.0), "{sizes:?}");
}

#[test]
fn a_repeat_settles_line_by_line() {
    let (sizes, counts) = shape(&lines_of(paragraph_of("abcd abcd ab abcd abcd abcd abcd abcd abcd abcd abcd", vec![item("big", 1, false), item("mid", 1, false), item("x", 2, true)])));
    assert_eq!(&sizes[..3], &[20.0, 15.0, 20.0], "{sizes:?} {counts:?}");
}
