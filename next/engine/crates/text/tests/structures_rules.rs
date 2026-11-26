//! Tables, footnotes, lists, paragraph rules, shading and borders on small synthetic
//! documents. `FontStore::empty()` gives the synthetic face (every glyph 0.5 em, ascent
//! 0.8 em, descent 0.2 em, auto leading 120 %), so at 10 pt a line's ascent is 8, its descent
//! 2 and its leading 12. Expected geometry follows InDesign 2025 measurements (see
//! `layout/tables.rs` and `layout/structures.rs`).

use folio_model::layout::{DocumentLayout, LaidLine, TextShape};
use folio_model::{
    Affine, BottomOrigin, CellStroke, CharacterAttrs, Document, ItemKind, Justification, ListKind, ListStyle, Note,
    Page, PageItem, Paint, Paragraph, ParagraphAttrs, ParagraphBorder, ParagraphRule, ParagraphShading, PathPoint,
    Point, Rect, RuleWidth, Spread, Story, SubPath, Table, TableCell, TableRow, TextFrame, TextRun, TextVariable,
    TopOrigin, VerticalJustification,
};
use folio_text::{FontStore, format_number, layout_document};

#[path = "support/cleanroom_rules.rs"]
mod cleanroom_rules;

const EPS: f64 = 1e-6;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn rect_path(w: f64, h: f64) -> Vec<SubPath> {
    let p = |x, y| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)], open: false }]
}

fn text_frame(id: &str, story: &str, w: f64, h: f64, f: impl FnOnce(&mut TextFrame)) -> PageItem {
    let mut tf = TextFrame { story: story.into(), columns: 1, gutter: 10.0, ..TextFrame::default() };
    f(&mut tf);
    PageItem { id: id.into(), kind: ItemKind::TextFrame(tf), paths: rect_path(w, h), ..PageItem::default() }
}

fn chars(size: f64) -> CharacterAttrs {
    CharacterAttrs { point_size: size, language: "Korean".into(), ..CharacterAttrs::default() }
}

fn run(text: &str) -> TextRun {
    TextRun { text: text.into(), attrs: chars(10.0), ..TextRun::default() }
}

fn para(text: &str) -> Paragraph {
    Paragraph {
        style: "Body".into(),
        attrs: ParagraphAttrs { hyphenation: false, ..ParagraphAttrs::default() },
        runs: vec![run(text)],
    }
}

fn document(items: Vec<PageItem>, stories: Vec<Story>) -> Document {
    Document {
        spreads: vec![Spread {
            id: "s1".into(),
            pages: vec![Page { id: "p1".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 600.0, bottom: 800.0 }, ..Page::default() }],
            items,
            ..Spread::default()
        }],
        stories: stories.into_iter().map(|s| (s.id.clone(), s)).collect(),
        ..Document::default()
    }
}

fn story(id: &str, paragraphs: Vec<Paragraph>) -> Story {
    Story { id: id.into(), paragraphs, ..Story::default() }
}

fn black() -> Paint {
    Paint::Swatch { id: "K".into(), tint: 100.0 }
}

fn stroke(w: f64) -> CellStroke {
    CellStroke { weight: w, paint: black(), ..CellStroke::default() }
}

fn cell(row: usize, column: usize, text: &str) -> TableCell {
    TableCell {
        row,
        column,
        row_span: 1,
        column_span: 1,
        paragraphs: vec![para(text)],
        insets: [4.0; 4],
        edges: [stroke(1.0), stroke(1.0), stroke(1.0), stroke(1.0)],
        ..TableCell::default()
    }
}

/// `rows` × `cols` table of 50 pt columns, cells "ab", 1 pt black strokes, 4 pt insets.
fn table(rows: usize, cols: usize) -> Table {
    Table {
        id: "t".into(),
        rows: vec![TableRow::default(); rows],
        columns: vec![50.0; cols],
        cells: (0..rows).flat_map(|r| (0..cols).map(move |c| cell(r, c, "ab"))).collect(),
        space_before: 4.0,
        space_after: -4.0,
        border_weights: [1.0; 4],
        ..Table::default()
    }
}

fn table_para(t: Table) -> Paragraph {
    let mut p = para("\u{16}");
    p.runs[0].tables.push(t);
    p
}

fn lines<'a>(layout: &'a DocumentLayout, frame: &str) -> &'a [LaidLine] {
    &layout.frames[frame].lines
}

fn bounds(s: &TextShape) -> Rect {
    s.bounds().unwrap()
}

fn is_rect(s: &TextShape, l: f64, t: f64, r: f64, b: f64) -> bool {
    let x = bounds(s);
    close(x.left, l) && close(x.top, t) && close(x.right, r) && close(x.bottom, b)
}

#[test]
fn table_rows_strokes_and_cell_text() {
    let doc = document(vec![text_frame("f", "s", 300.0, 400.0, |_| {})], vec![story("s", vec![table_para(table(2, 2))])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    // The table's line, then the four cells' lines.
    assert_eq!(l.len(), 5);
    // Rows: inset 4 + depth to the baseline 8 + inset 4 = 16. At the frame top the outer
    // edge (grid + half the border) is the top; the line's baseline is the outer bottom.
    assert!(close(l[0].baseline, 0.5 + 32.0 + 0.5), "{}", l[0].baseline);
    assert!(l[0].glyphs.is_empty());
    let cell_line = |x: f64| l[1..].iter().find(|c| close(c.glyphs[0].x, x) && close(c.baseline, 28.5)).is_some();
    assert!(close(l[1].baseline, 0.5 + 4.0 + 8.0) && close(l[1].glyphs[0].x, 0.5 + 4.0));
    assert!(cell_line(50.5 + 4.0), "second row, second column");
    // Strokes centred on the grid lines; horizontal ones over the full width, extended by
    // half the vertical weight; vertical ones between the outer grid lines. No fills.
    let s = &l[0].shapes;
    assert_eq!(s.len(), 6);
    assert!(s.iter().any(|x| is_rect(x, 0.0, 0.0, 101.0, 1.0)), "top border");
    assert!(s.iter().any(|x| is_rect(x, 0.0, 16.0, 101.0, 17.0)), "row line");
    assert!(s.iter().any(|x| is_rect(x, 0.0, 0.5, 1.0, 32.5)), "left border");
    assert!(s.iter().any(|x| is_rect(x, 50.0, 0.5, 51.0, 32.5)), "column line");
    assert!(layout.overset.is_empty());
}

#[test]
fn table_between_lines_of_text() {
    let doc = document(
        vec![text_frame("f", "s", 300.0, 400.0, |_| {})],
        vec![story("s", vec![para("ab"), table_para(table(1, 2)), para("cd")])],
    );
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert!(close(l[0].baseline, 8.0));
    // Outer top = previous baseline + space before 4; one 16 pt row.
    let t = &l[1];
    assert!(close(t.baseline, 8.0 + 4.0 + 0.5 + 16.0 + 0.5), "{}", t.baseline);
    assert!(t.shapes.iter().any(|x| is_rect(x, 0.0, 12.0, 101.0, 13.0)), "top border at the outer top");
    // The text after: outer bottom + space after (−4) + leading 12.
    let after = l.iter().rev().find(|x| !x.glyphs.is_empty()).unwrap();
    assert!(close(after.baseline, t.baseline - 4.0 + 12.0), "{}", after.baseline);
    // Story indexes: the table is character 3 (after "ab" and its return).
    assert_eq!((t.start, t.end), (3, 4));
    assert_eq!(after.start, 5);
}

#[test]
fn table_rows_move_to_the_next_frame_with_header_and_footer() {
    let mut t = table(7, 1);
    t.header_rows = 1;
    t.footer_rows = 1;
    let mut f1 = text_frame("f1", "s", 100.0, 60.0, |tf| tf.next = Some("f2".into()));
    f1.transform = Affine::IDENTITY;
    let mut f2 = text_frame("f2", "s", 100.0, 200.0, |tf| tf.previous = Some("f1".into()));
    f2.transform = Affine::translate(0.0, 300.0);
    let doc = document(vec![f1, f2], vec![story("s", vec![table_para(t)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    // Frame 1: header, one body row, footer (a second body row and the footer need 65 > 60).
    let a = lines(&layout, "f1");
    assert!(close(a[0].baseline, 0.5 + 3.0 * 16.0 + 0.5), "{}", a[0].baseline);
    assert_eq!(a.len(), 1 + 3);
    // Frame 2: the header again, four body rows, the footer.
    let b = lines(&layout, "f2");
    assert!(close(b[0].baseline, 0.5 + 6.0 * 16.0 + 0.5), "{}", b[0].baseline);
    assert_eq!(b.len(), 1 + 6);
    assert!(close(b[1].baseline, 0.5 + 4.0 + 8.0), "repeated header at the top");
    assert!(layout.overset.is_empty());
}

#[test]
fn table_alignment_spans_and_vertical_justification() {
    let mut t = table(2, 2);
    // Cell (0,0) spans two rows with three lines: depth 8 + 2 × 12 = 32, needs 40; the last
    // row it spans grows by 40 − 32.
    t.cells.retain(|c| !(c.row == 1 && c.column == 0));
    t.cells[0].row_span = 2;
    t.cells[0].paragraphs = vec![para("a"), para("b"), para("c")];
    let mut p = table_para(t);
    p.attrs.justification = Justification::CenterAlign;
    p.attrs.left_indent = 10.0;
    let doc = document(vec![text_frame("f", "s", 300.0, 400.0, |_| {})], vec![story("s", vec![p])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    // Centred in the room after the indent: (300 − 10 − 101) / 2.
    assert!(close(l[0].left, 10.0 + 94.5), "{}", l[0].left);
    assert!(close(l[0].baseline, 0.5 + 16.0 + 24.0 + 0.5), "{}", l[0].baseline);
    // No horizontal line through the merged cell.
    let x0 = l[0].left + 0.5;
    assert!(!l[0].shapes.iter().any(|s| { let b = bounds(s); close(b.top, 16.0) && b.left < x0 + 25.0 }));

    // Exactly 40 pt rows: top, centred and bottom text over a depth of 8 in a 32 pt box.
    let mut t = table(1, 3);
    t.rows[0] = TableRow { height: 40.0, auto_grow: false, ..TableRow::default() };
    t.cells[1].vertical_justification = VerticalJustification::Center;
    t.cells[2].vertical_justification = VerticalJustification::Bottom;
    let doc = document(vec![text_frame("f", "s", 300.0, 400.0, |_| {})], vec![story("s", vec![table_para(t)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let b: Vec<f64> = lines(&layout, "f")[1..].iter().map(|l| l.baseline).collect();
    assert!(close(b[0], 12.5) && close(b[1], 12.5 + 12.0) && close(b[2], 12.5 + 24.0), "{b:?}");
}

#[test]
fn table_fills_priorities_and_diagonals() {
    let mut t = table(2, 2);
    t.cells[0].fill = Paint::Swatch { id: "C".into(), tint: 20.0 };
    // A heavier stroke with a higher priority wins the shared edge, drawn last.
    t.cells[1].edges[2] = CellStroke { weight: 3.0, paint: Paint::Swatch { id: "T".into(), tint: 100.0 }, priority: 2, ..CellStroke::default() };
    t.cells[3].diagonal_down = true;
    t.cells[3].diagonal = stroke(1.0);
    let doc = document(vec![text_frame("f", "s", 300.0, 400.0, |_| {})], vec![story("s", vec![table_para(t)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let s = &lines(&layout, "f")[0].shapes;
    assert!(is_rect(&s[0], 0.5, 0.5, 50.5, 16.5), "fills first, the grid box");
    let heavy = s.iter().position(|x| x.paint == Paint::Swatch { id: "T".into(), tint: 100.0 }).unwrap();
    assert!(is_rect(&s[heavy], 50.0, 15.0, 101.0, 18.0), "{:?}", bounds(&s[heavy]));
    assert_eq!(heavy, s.len() - 1, "heaviest stroke last");
    // The diagonal: a slanted quadrilateral over the cell's grid box, behind the strokes.
    let slanted = |x: &TextShape| {
        let p = &x.paths[0].points;
        !(close(p[0].anchor.y, p[1].anchor.y) || close(p[0].anchor.x, p[1].anchor.x))
    };
    let d = s.iter().position(slanted).unwrap();
    assert!(d < heavy && bounds(&s[d]).width() > 49.0 && bounds(&s[d]).height() > 15.0);
}

fn footnote(text: &str) -> TextRun {
    let mut note_para = para("");
    note_para.runs = vec![
        TextRun { text: "#".into(), attrs: chars(10.0), variable: Some(TextVariable::FootnoteNumber), ..TextRun::default() },
        run(text),
    ];
    TextRun {
        text: "#".into(),
        attrs: chars(10.0),
        variable: Some(TextVariable::FootnoteNumber),
        note: Some(Box::new(Note { paragraphs: vec![note_para], ..Note::default() })),
        ..TextRun::default()
    }
}

#[test]
fn footnotes_at_the_column_bottom() {
    let mut p = para("ab");
    p.runs.push(footnote("\tnote"));
    p.runs.push(run("cd"));
    let mut doc = document(vec![text_frame("f", "s", 200.0, 200.0, |_| {})], vec![story("s", vec![p])]);
    doc.footnotes.rule = Some(folio_model::FootnoteRule { paint: black(), ..folio_model::FootnoteRule::default() });
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert_eq!(l.len(), 2);
    // The reference shows its number (one glyph) in the text.
    assert_eq!(l[0].glyphs.len(), 5);
    assert_eq!((l[0].start, l[0].end), (0, 5), "the reference is one character of the story");
    // The note: first baseline its leading below its top, last baseline on the column bottom;
    // the separator rule centred on the notes' top, 72 pt long.
    let n = &l[1];
    assert!(close(n.baseline, 200.0), "{}", n.baseline);
    assert!(n.shapes.iter().any(|s| is_rect(s, 0.0, 187.5, 72.0, 188.5)), "{:?}", n.shapes.iter().map(bounds).collect::<Vec<_>>());
    assert_eq!(n.glyphs.len(), 5, "number, tab, \"note\"");
}

#[test]
fn footnotes_take_room_from_the_text_and_number_through() {
    // Two-line notes (depth 8 + 12 = 20) with 3 pt between and 5 pt kept above them.
    let mut p = para("ab");
    p.runs.push(footnote("\tlong note text that wraps"));
    p.runs.push(run(" cd ef gh ij kl mn op qr st uv"));
    let mut doc = document(
        vec![
            text_frame("f1", "s", 60.0, 60.0, |tf| tf.next = Some("f2".into())),
            {
                let mut f = text_frame("f2", "s", 60.0, 200.0, |tf| tf.previous = Some("f1".into()));
                f.transform = Affine::translate(0.0, 300.0);
                f
            },
        ],
        vec![story("s", vec![p])],
    );
    doc.footnotes.space_between = 3.0;
    doc.footnotes.spacer = 5.0;
    doc.footnotes.start_at = 4;
    doc.footnotes.numbering = "LowerRoman".into();
    doc.footnotes.first_baseline = folio_model::FirstBaseline::Ascent;
    let layout = layout_document(&doc, &FontStore::empty());
    let a = lines(&layout, "f1");
    let note_lines: Vec<&LaidLine> = a.iter().filter(|l| l.baseline > 30.0).collect();
    assert!(!note_lines.is_empty());
    let last = note_lines.last().unwrap();
    assert!(close(last.baseline, 60.0));
    let depth = 60.0 - (note_lines[0].baseline - 8.0);
    // Text lines stop above the notes and the 5 pt space.
    let text_bottom = a.iter().filter(|l| l.baseline < 60.0 - depth).map(|l| l.baseline).fold(0.0, f64::max);
    assert!(text_bottom <= 60.0 - depth - 5.0 + EPS, "{text_bottom} {depth}");
    assert!(!lines(&layout, "f2").is_empty(), "the rest of the text continues");
    // Numbered from 4 in lower roman: the note starts "iv", then the tab, then "long".
    assert_eq!(note_lines[0].glyphs.len(), 2 + 4);
}

#[test]
fn footnotes_span_the_columns_of_their_frame() {
    // Seven one-line rows in a two-column frame 60 pt deep; the reference is on the seventh,
    // in the second column. Its note spans both columns at the frame bottom, so the first
    // column holds four lines instead of five (laid out again with the room kept).
    let mut p = para("a\nb\nc\nd\ne\nf\ng");
    p.runs.push(footnote("\tabcdefghijklmnopqrst"));
    let mut doc = document(
        vec![text_frame("f", "s", 210.0, 60.0, |tf| {
            tf.columns = 2;
            tf.gutter = 10.0;
        })],
        vec![story("s", vec![p])],
    );
    doc.footnotes.straddle = true;
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    let first_column = l.iter().filter(|x| x.left < 50.0 && x.baseline < 50.0).count();
    assert_eq!(first_column, 4);
    // One note line across the frame's width (36 + 100 pt would not fit a 100 pt column).
    let note = l.iter().find(|x| close(x.baseline, 60.0)).unwrap();
    assert!(note.glyphs.last().unwrap().x > 120.0, "{}", note.glyphs.last().unwrap().x);
    assert!(layout.overset.is_empty());
}

fn list(kind: ListKind, format: &str) -> ListStyle {
    ListStyle {
        kind,
        bullet: "\u{2022}".into(),
        bullet_text_after: "\t".into(),
        expression: "^#.\t".into(),
        format: format.into(),
        start_at: 1,
        level: 1,
        restart_after_higher_level: true,
        marker: chars(10.0),
        ..ListStyle::default()
    }
}

#[test]
fn bullets_numbers_and_hanging_indents() {
    let mut b = para("ab");
    b.attrs.list = Some(list(ListKind::Bullet, ""));
    b.attrs.left_indent = 18.0;
    b.attrs.first_line_indent = -18.0;
    let mut n1 = para("cd");
    n1.attrs.list = Some(list(ListKind::Numbered, "1, 2, 3, 4..."));
    let mut n2 = n1.clone();
    n2.runs[0].text = "ef".into();
    let mut n9 = para("gh");
    n9.attrs.list = Some(ListStyle { restart_at: Some(9), ..list(ListKind::Numbered, "I, II, III, IV...") });
    let mut n10 = para("ij");
    n10.attrs.list = Some(list(ListKind::Numbered, "a, b, c, d..."));
    let doc = document(vec![text_frame("f", "s", 200.0, 400.0, |_| {})], vec![story("s", vec![b, n1, para("x"), n2, n9, n10])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    // Bullet at the first-line position; the text at the left indent (InDesign's implicit tab
    // stop of a hanging indent).
    assert!(close(l[0].glyphs[0].x, 0.0) && close(l[0].glyphs[1].x, 18.0), "{:?}", l[0].glyphs.iter().map(|g| g.x).collect::<Vec<_>>());
    assert_eq!((l[0].start, l[0].end), (0, 3), "the bullet is not story text");
    // "1." then the text at the default tab stop.
    assert_eq!(l[1].glyphs.len(), 4);
    assert!(close(l[1].glyphs[2].x, 36.0));
    // Numbering continues past an unnumbered paragraph: "2."; restarts at IX; then "j.".
    assert_eq!(l[3].glyphs.len(), 4);
    assert_eq!(l[4].glyphs.len(), 3 + 2, "IX.");
    assert_eq!(l[5].glyphs.len(), 2 + 2, "j. (10th letter)");
}

#[test]
fn number_formats() {
    assert_eq!(format_number(4, "1, 2, 3, 4..."), "4");
    assert_eq!(format_number(4, "01, 02, 03, 04..."), "04");
    assert_eq!(format_number(14, "I, II, III, IV..."), "XIV");
    assert_eq!(format_number(9, "i, ii, iii, iv..."), "ix");
    // letters count like spreadsheet columns (InDesign 2025, oracle/measured/listletters-2025.json): not "AA, BB, CC"
    assert_eq!(format_number(28, "A, B, C, D..."), "AB");
    assert_eq!(format_number(11, "a, b, c, d..."), "k");
    assert_eq!(format_number(3, "Asterisks"), "***");
    assert_eq!(format_number(3, "가, 나, 다, 라..."), "다");
    assert_eq!(format_number(2, "ㄱ, ㄴ, ㄷ, ㄹ..."), "ㄴ");
    assert_eq!(format_number(12, "①, ②, ③, ④..."), "⑫");
    assert_eq!(format_number(23, "一, 二, 三, 四..."), "二十三");
    assert_eq!(format_number(7, "１, ２, ３, ４..."), "７");
}

/// Every letter label InDesign 2025 gave in the probe (`next/oracle/measured/listletters-2025.json`: 6 paragraphs in a row from each start
/// number, both cases, up to four letters) is what `format_number` gives, for the format names of paragraphs and of footnotes alike.
#[test]
fn letter_numbers_are_spreadsheet_columns() {
    let measured: serde_json::Value = serde_json::from_str(include_str!("../../../../oracle/measured/listletters-2025.json")).unwrap();
    let mut checked = 0;
    for case in measured["cases"].as_array().unwrap() {
        let start = case["start"].as_u64().unwrap() as u32;
        for (key, formats) in [("upper", ["A, B, C, D...", "UpperLetters"]), ("lower", ["a, b, c, d...", "LowerLetters"])] {
            for (i, label) in case[key].as_array().unwrap().iter().enumerate() {
                for format in formats {
                    assert_eq!(format_number(start + i as u32, format), label.as_str().unwrap(), "{format} {}", start + i as u32);
                    checked += 1;
                }
            }
        }
    }
    assert!(checked >= 200, "{checked}");
    // the ends of the two-, three- and four-letter ranges
    for (n, upper) in [(26, "Z"), (27, "AA"), (52, "AZ"), (53, "BA"), (78, "BZ"), (702, "ZZ"), (703, "AAA"), (18278, "ZZZ"), (18279, "AAAA")] {
        assert_eq!(format_number(n, "A, B, C, D..."), upper);
        assert_eq!(format_number(n, "a, b, c, d..."), upper.to_lowercase());
    }
    assert_eq!(format_number(0, "A, B, C, D..."), "");
}

fn rule(weight: f64, offset: f64, width: RuleWidth, indents: (f64, f64)) -> ParagraphRule {
    ParagraphRule { weight, offset, width, left_indent: indents.0, right_indent: indents.1, paint: Some(black()), ..ParagraphRule::default() }
}

#[test]
fn paragraph_rules() {
    let mut p = para("abcd");
    p.attrs.left_indent = 10.0;
    p.attrs.rule_above = Some(rule(2.0, 3.0, RuleWidth::Column, (5.0, 7.0)));
    p.attrs.rule_below = Some(rule(3.0, 4.0, RuleWidth::Text, (1.0, 2.0)));
    let mut q = para("ef");
    q.attrs.rule_above = Some(ParagraphRule { paint: None, ..rule(1.0, 0.0, RuleWidth::Column, (0.0, 0.0)) });
    q.runs[0].attrs.fill = Paint::Swatch { id: "C".into(), tint: 100.0 };
    let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![p, q])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert!(close(l[0].baseline, 8.0) && close(l[1].baseline, 20.0), "rules take no room");
    // Above: column edges less the rule's indents (paragraph indents do not apply); the
    // rule's bottom `offset` above the baseline.
    assert!(l[0].shapes.iter().any(|s| is_rect(s, 5.0, 3.0, 93.0, 5.0)));
    // Below, text width: the text from 10 to 30 less the rule's indents; its top `offset`
    // below the baseline.
    assert!(l[0].shapes.iter().any(|s| is_rect(s, 11.0, 12.0, 28.0, 15.0)));
    // "Text Color" rules take the text's fill.
    assert!(l[1].shapes.iter().any(|s| s.paint == Paint::Swatch { id: "C".into(), tint: 100.0 }));
}

#[test]
fn shading_and_border_boxes() {
    let border = ParagraphBorder {
        paint: black(),
        weights: [1.0, 2.0, 3.0, 4.0],
        offsets: [1.0, 2.0, 0.0, 0.0],
        merge: true,
        ..ParagraphBorder::default()
    };
    let shading = ParagraphShading { paint: Paint::Swatch { id: "C".into(), tint: 20.0 }, ..ParagraphShading::default() };
    let mut p1 = para("ab");
    p1.attrs.border = Some(border.clone());
    p1.attrs.shading = Some(shading.clone());
    let p2 = p1.clone();
    let mut p3 = para("cd");
    p3.attrs.space_before = 10.0;
    p3.attrs.shading = Some(ParagraphShading { top: TopOrigin::Leading, bottom: BottomOrigin::Baseline, offsets: [0.0, 4.0, 3.0, 5.0], ..shading.clone() });
    let mut p4 = para("ef");
    p4.attrs.left_indent = 15.0;
    p4.attrs.shading = Some(ParagraphShading { width: RuleWidth::Text, ..shading });
    let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![p1, p2, p3, p4])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    // One box for both bordered paragraphs: ascent of the first line to the descent of the
    // last, plus offsets; the shading fills the same box, the border strokes centred on it.
    let s = &l[0].shapes;
    assert!(is_rect(&s[0], 0.0, 0.0, 100.0, 20.0 + 2.0), "{:?}", bounds(&s[0]));
    assert!(s.iter().any(|x| is_rect(x, -2.0 - 1.0, -1.0 - 0.5, 100.0 + 2.0, -1.0 + 0.5)), "top edge");
    assert!(s.iter().any(|x| is_rect(x, 100.0 - 2.0, -1.5, 100.0 + 2.0, 22.0 + 1.5)), "right edge");
    assert!(l[1].shapes.is_empty(), "the second paragraph shares the first's box");
    // Leading-to-baseline shading with offsets.
    let b3 = l[2].baseline;
    assert!(is_rect(&l[2].shapes[0], -4.0, b3 - 12.0, 105.0, b3 + 3.0), "{:?}", bounds(&l[2].shapes[0]));
    // Text width: from the indent to the text's end.
    let b4 = l[3].baseline;
    assert!(is_rect(&l[3].shapes[0], 15.0, b4 - 8.0, 25.0, b4 + 2.0), "{:?}", bounds(&l[3].shapes[0]));
}

#[test]
fn border_split_across_columns_stays_open() {
    let mut p = para("ab\ncd\nef\ngh");
    p.attrs.border = Some(ParagraphBorder { paint: black(), weights: [1.0; 4], merge: true, ..ParagraphBorder::default() });
    let frame = text_frame("f", "s", 210.0, 30.0, |tf| {
        tf.columns = 2;
        tf.gutter = 10.0;
    });
    let doc = document(vec![frame], vec![story("s", vec![p])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert_eq!(l.len(), 4);
    let first = &l[0].shapes;
    let second = &l[2].shapes;
    // A border of four equal solid edges is stroked the way InDesign paints it: one open path with mitered corners per
    // column piece (not one bar per edge). First column: top, left, right; second: bottom, left, right. The path
    // stays open on the side where the paragraph runs on, so the first column has no bottom edge, the second no top edge.
    assert_eq!((first.len(), second.len()), (1, 1));
    let (a, b) = (&first[0].paths, &second[0].paths);
    assert!(a.len() == 1 && a[0].open && a[0].points.len() == 4, "first column: one open path, left side up, across the top, right side down");
    assert!(b.len() == 1 && b[0].open && b[0].points.len() == 4, "second column: one open path, left side down, across the bottom, right side up");
    let y = |p: &SubPath| p.points.iter().map(|q| q.anchor.y).collect::<Vec<_>>();
    let (ya, yb) = (y(&a[0]), y(&b[0]));
    // the box of the first column: top at the first line's ascent, bottom at the second line's descent
    assert!(close(ya[1], 0.0) && close(ya[2], 0.0) && close(ya[0], l[1].baseline + 2.0) && close(ya[3], l[1].baseline + 2.0), "{ya:?}");
    assert!(close(yb[0], 0.0) && close(yb[3], 0.0) && close(yb[1], l[3].baseline + 2.0) && close(yb[2], l[3].baseline + 2.0), "{yb:?}");
}

#[test]
fn objects_anchored_in_table_cells_are_laid_out() {
    // A text frame anchored in a cell: placed at its character in the cell's line, and its own
    // story laid out (frames anchored in cells are found like those in stories).
    let anchored = text_frame("inner", "s2", 40.0, 20.0, |_| {});
    let mut t = table(1, 1);
    t.columns = vec![100.0];
    t.cells[0].paragraphs[0].runs = vec![TextRun { text: "a\u{FFFC}".into(), attrs: chars(10.0), inline: vec![anchored], ..TextRun::default() }];
    let doc = document(
        vec![text_frame("f", "s", 300.0, 400.0, |_| {})],
        vec![story("s", vec![table_para(t)]), story("s2", vec![para("xy")])],
    );
    let layout = layout_document(&doc, &FontStore::empty());
    let cell_line = &lines(&layout, "f")[1];
    assert_eq!(cell_line.objects.len(), 1);
    assert_eq!(cell_line.objects[0].item, "inner");
    assert!(close(cell_line.objects[0].x, 0.5 + 4.0 + 5.0), "{}", cell_line.objects[0].x);
    assert_eq!(lines(&layout, "inner").len(), 1);
}

#[test]
fn rotated_cells_turn_their_lines() {
    // 90°: lines run down from the text box's top, the first line's baseline its ascent in
    // from the box's right edge; glyphs turned with the line.
    let mut t = table(1, 2);
    t.rows[0] = TableRow { height: 60.0, auto_grow: false, ..TableRow::default() };
    t.cells[0].rotation = 90.0;
    t.cells[1].rotation = 270.0;
    let doc = document(vec![text_frame("f", "s", 300.0, 400.0, |_| {})], vec![story("s", vec![table_para(t)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    let (a, b) = (&l[1], &l[2]);
    assert_eq!((a.rotation, b.rotation), (90.0, 270.0));
    // Box: x 4.5–46.5, y 4.5–56.5 (cell 1: x 54.5–96.5).
    assert!(close(a.glyphs[0].x, 46.5 - 8.0) && close(a.glyphs[0].y, 4.5), "{:?}", (a.glyphs[0].x, a.glyphs[0].y));
    assert!(close(a.glyphs[1].y, 4.5 + 5.0));
    // 270°: up from the bottom, the first line at the left edge.
    assert!(close(b.glyphs[0].x, 54.5 + 8.0) && close(b.glyphs[0].y, 56.5), "{:?}", (b.glyphs[0].x, b.glyphs[0].y));
    assert!(close(b.glyphs[1].y, 56.5 - 5.0));
}

#[test]
fn stroke_types_of_rules_borders_and_table_edges() {
    // InDesign 2025 (artifacts/next-20261018/fonts/indesign/text-strokes-x4.pdf): a dashed
    // rule starts and ends on a full dash; a dashed border is one box with a dash on each
    // corner; dashed table edges are patterned cell by cell, centred on the crossings.
    let mut p = para("abcd");
    p.attrs.rule_below = Some(ParagraphRule { stroke_type: "Dashed".into(), ..rule(4.0, 4.0, RuleWidth::Column, (0.0, 0.0)) });
    p.attrs.border = Some(ParagraphBorder { paint: black(), weights: [2.0; 4], stroke_type: "Dashed".into(), ..ParagraphBorder::default() });
    let mut t = table(2, 2);
    for c in &mut t.cells {
        for e in &mut c.edges {
            e.stroke_type = "Canned Dotted".into();
            e.weight = 3.0;
        }
    }
    let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![p, table_para(t)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    let pieces = |s: &TextShape| s.paths.len();
    // Rule below, 100 pt of "Dashed" (12 pt): 5 dashes and 4 gaps of 100 / 9.
    let at_rule = |s: &&TextShape| close(bounds(s).top, l[0].baseline + 4.0) && close(bounds(s).bottom, l[0].baseline + 8.0);
    let rule = l[0].shapes.iter().find(at_rule).expect("rule below");
    assert_eq!(pieces(rule), 5, "{:?}", rule.paths.iter().map(|p| p.bounds()).collect::<Vec<_>>());
    assert!(close(rule.paths[0].bounds().unwrap().left, 0.0) && close(rule.paths[4].bounds().unwrap().right, 100.0));
    // Border: dashed pieces, more than the four edges of a solid box.
    let border: usize = l[0].shapes.iter().filter(|s| !at_rule(s)).map(pieces).sum();
    assert!(border > 4, "dashed border pieces {border}");
    // The table sits inside its 3 pt outer strokes and its edges are dots about 9 pt apart. A horizontal edge puts one on each crossing
    // and starts half a dot in from the table's edge (oracle alt-row-stroke-dotted): 50 pt edges take 6 dots; a vertical edge keeps its
    // dots inside it (alt-column-stroke-dotted): the 16 pt row edges take 2.
    let tl = l.iter().find(|x| x.glyphs.is_empty() && !x.shapes.is_empty() && bounds(&x.shapes[0]).top > l[0].baseline).expect("table line");
    let dots: usize = tl.shapes.iter().map(pieces).sum();
    assert_eq!(dots, 6 * 6 + 6 * 2, "6 horizontal edges of 6 dots, 6 vertical edges of 2");
    let left = tl.shapes.iter().map(|s| bounds(s).left).fold(f64::MAX, f64::min);
    assert!(close(left, 0.0), "outer stroke starts at the frame edge: {left}");
}

#[test]
fn a_patterned_rule_and_border_show_their_gap_colour_underneath() {
    // The gap colour (IDML ParagraphBorderGapColor / RuleBelowGapColor) is a solid bar or stroke of the same extent drawn before the pattern;
    // a solid line has no gap to show, and no gap colour draws nothing extra.
    let gap = Paint::Swatch { id: "Color/Gap".into(), tint: 60.0 };
    let with = |rule_type: &str, border_type: &str, gap_paint: Paint| {
        let mut p = para("abcd");
        p.attrs.rule_below = Some(ParagraphRule { stroke_type: rule_type.into(), gap_paint: gap_paint.clone(), gap_overprint: true, ..rule(4.0, 4.0, RuleWidth::Column, (0.0, 0.0)) });
        p.attrs.border = Some(ParagraphBorder { paint: black(), weights: [2.0; 4], stroke_type: border_type.into(), gap_paint, gap_overprint: true, ..ParagraphBorder::default() });
        let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![p])]);
        let layout = layout_document(&doc, &FontStore::empty());
        lines(&layout, "f")[0].shapes.clone()
    };
    let gaps = |shapes: &[TextShape]| shapes.iter().filter(|s| s.paint == gap).count();
    // dashed rule and dashed border with a gap colour: a bar under the rule, a stroke under the border, each before its pattern
    let dashed = with("Dashed", "Dashed", gap.clone());
    assert_eq!(gaps(&dashed), 2, "rule bar and border box: {:?}", dashed.iter().map(|s| (&s.paint, s.paths.len())).collect::<Vec<_>>());
    assert!(dashed.iter().filter(|s| s.paint == gap).all(|s| s.overprint), "the gap overprint travels with it");
    let first_gap = dashed.iter().position(|s| s.paint == gap).unwrap();
    assert!(dashed[first_gap + 1..].iter().any(|s| s.paint == black()), "the pattern comes after its gap");
    // solid lines show no gap; no gap colour adds nothing
    assert_eq!(gaps(&with("Solid", "Solid", gap.clone())), 0);
    assert_eq!(gaps(&with("Dashed", "Dashed", Paint::None)), 0);
}

#[test]
fn adjust_font_size_reads_the_composition_of_a_frame() {
    // Adjust Layout's font fit needs, per text frame, the lines of each paragraph at the old size and the width of the column; what it
    // cannot describe it says why (layoutadj-c7).
    let fonts = FontStore::empty();
    let two = |f: &mut TextFrame| f.columns = 2;
    let doc = document(
        vec![text_frame("one", "a", 100.0, 200.0, |_| {}), text_frame("cols", "b", 100.0, 200.0, two), text_frame("inset", "c", 100.0, 200.0, |f| f.insets = [3.0; 4]), text_frame("empty", "d", 100.0, 200.0, |_| {})],
        vec![story("a", vec![para("abc def"), para("ghi")]), story("b", vec![para("abc")]), story("c", vec![para("abc")]), story("d", vec![])],
    );
    let layout = layout_document(&doc, &fonts);
    let fits = folio_text::fontfit::frame_fits(&doc, &layout, &fonts);
    let one = fits.frames["one"].as_ref().expect("one column, one font");
    assert!((one.width - 100.0).abs() < 1e-9 && (one.size - 10.0).abs() < 1e-9 && one.leading.is_none(), "{one:?}");
    assert_eq!(one.paragraphs.len(), 2, "a paragraph each");
    assert!(one.paragraphs.iter().all(|p| p.0 >= 1 && p.1 >= 0.0));
    assert!(fits.frames["cols"].as_ref().unwrap_err().contains("several columns"));
    assert!(fits.frames["inset"].as_ref().unwrap_err().contains("inset"));
    let empty = fits.frames["empty"].as_ref().expect("a frame with no text has nothing to adjust");
    assert!(empty.paragraphs.is_empty());
}
