//! Print Blank Pages off: the pages InDesign leaves out, as measured by
//! `next/oracle/indesign/printing-blank-probe.jsx` (`artifacts/printx-c1-20261023/measurement/printing-blank.json`).
use folio_model::{Affine, Document, ItemKind, Layer, Page, PageItem, Paint, Paragraph, Rect, Spread, Story, SubPath, PathPoint, Point, TextFrame, TextRun};
use folio_pdf::printing::is_blank;

fn rect_item(id: &str, x: f64, layer: &str) -> PageItem {
    let corner = |x: f64, y: f64| PathPoint { anchor: Point { x, y }, left: Point { x, y }, right: Point { x, y } };
    PageItem {
        id: id.into(),
        kind: ItemKind::Rectangle { content: None },
        paths: vec![SubPath { points: vec![corner(x + 20.0, 20.0), corner(x + 80.0, 20.0), corner(x + 80.0, 70.0), corner(x + 20.0, 70.0)], open: false }],
        fill: Paint::Swatch { id: "Black".into(), tint: 100.0 },
        layer: Some(layer.into()),
        ..Default::default()
    }
}

fn story(id: &str, text: &str) -> Story {
    Story { id: id.into(), paragraphs: vec![Paragraph { runs: vec![TextRun { text: text.into(), ..Default::default() }], ..Default::default() }], ..Default::default() }
}

fn frame(id: &str, x: f64, story: &str) -> PageItem {
    PageItem { kind: ItemKind::TextFrame(TextFrame { story: story.into(), ..Default::default() }), fill: Paint::None, ..rect_item(id, x, "base") }
}

#[test]
fn blank_pages_follow_the_measured_rules() {
    let page = |i: usize| Page {
        id: format!("p{i}"),
        name: (i + 1).to_string(),
        index: i,
        bounds: Rect { left: 0.0, top: 0.0, right: 240.0, bottom: 300.0 },
        transform: Affine::translate(300.0 * i as f64, 0.0),
        master: (i == 2).then(|| "m".to_string()),
        ..Default::default()
    };
    let layer = |id: &str, visible: bool, printable: bool, order: usize| Layer { id: id.into(), name: id.into(), visible, printable, order, ..Default::default() };
    let mut doc = Document {
        layers: vec![layer("base", true, true, 0), layer("hid", false, true, 1), layer("nop", true, false, 2)],
        ..Document::default()
    };
    doc.stories.insert("empty".into(), story("empty", ""));
    doc.stories.insert("space".into(), story("space", " "));
    let x = |i: usize| 300.0 * i as f64;
    let invisible = PageItem { fill: Paint::None, ..rect_item("invisible", x(3), "base") };
    let nonprinting = PageItem { print: folio_model::ItemPrint { nonprinting: true, ..Default::default() }, ..rect_item("np", x(9), "base") };
    let group = PageItem { kind: ItemKind::Group { children: vec![rect_item("child", 0.0, "base")] }, paths: vec![], transform: Affine::translate(x(10), 0.0), ..Default::default() };
    let spread = Spread {
        id: "s".into(),
        pages: (0..11).map(page).collect(),
        items: vec![
            rect_item("filled", x(1), "base"), invisible, frame("emptyFrame", x(4), "empty"), frame("spaceFrame", x(5), "space"),
            rect_item("hidden", x(6), "hid"), rect_item("noprint", x(7), "nop"), rect_item("pasteboard", -500.0, "base"), nonprinting, group,
        ],
        ..Default::default()
    };
    let master = Spread { id: "m".into(), pages: vec![page(0)], items: vec![rect_item("masterRect", 0.0, "base")], ..Default::default() };
    doc.spreads = vec![spread.clone()];
    doc.master_spreads = vec![master];
    let blank: Vec<bool> = spread.pages.iter().map(|p| is_blank(&doc, &spread, p)).collect();
    // 1 empty, 2 filled, 3 parent item only, 4 no fill/stroke, 5 empty frame, 6 one space, 7 hidden layer, 8 non-printing layer, 9 pasteboard, 10 Nonprinting, 11 group of a filled rectangle
    assert_eq!(blank, [true, false, false, true, true, false, true, true, true, true, false]);
    // Show Parent Items off: the parent's rectangle no longer counts.
    let bare = Spread { hide_master_items: true, ..spread.clone() };
    assert!(is_blank(&doc, &bare, &bare.pages[2]));
}
