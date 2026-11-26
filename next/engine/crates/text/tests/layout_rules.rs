//! Layout rules on small synthetic documents. `FontStore::empty()` gives the synthetic face
//! (every glyph 0.5 em, word spaces 0.25 em, ascent 0.8 em, cap height 0.7 em, x height
//! 0.5 em), so every number here follows from the rules alone.

use folio_model::layout::{DocumentLayout, LaidLine};
use folio_model::{
    Affine, CharacterAttrs, Document, FirstBaseline, ItemKind, Justification, Leading, Page, PageItem, Paragraph,
    ParagraphAttrs, PathPoint, Point, Rect, Spread, Story, SubPath, TabAlignment, TabStop, TextFrame, TextRun,
    TextWrap, TextWrapMode, VerticalJustification,
};
use folio_text::{DocumentFrames, FontStore, layout_document, layout_story};

const EPS: f64 = 1e-6;

fn rect_path(w: f64, h: f64) -> Vec<SubPath> {
    let p = |x, y| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)], open: false }]
}

fn text_frame(id: &str, story: &str, w: f64, h: f64, f: impl FnOnce(&mut TextFrame)) -> PageItem {
    let mut tf = TextFrame { story: story.into(), columns: 1, gutter: 12.0, ..TextFrame::default() };
    f(&mut tf);
    PageItem { id: id.into(), kind: ItemKind::TextFrame(tf), paths: rect_path(w, h), ..PageItem::default() }
}

fn chars(size: f64) -> CharacterAttrs {
    CharacterAttrs { point_size: size, language: "Korean".into(), ..CharacterAttrs::default() }
}

fn para(text: &str, size: f64) -> Paragraph {
    Paragraph {
        style: "Body".into(),
        attrs: ParagraphAttrs { hyphenation: false, ..ParagraphAttrs::default() },
        runs: vec![TextRun { text: text.into(), attrs: chars(size), ..TextRun::default() }],
    }
}

fn document(items: Vec<PageItem>, stories: Vec<Story>) -> Document {
    Document {
        spreads: vec![Spread {
            id: "s1".into(),
            pages: vec![Page {
                id: "p1".into(),
                name: "7".into(),
                bounds: Rect { left: 0.0, top: 0.0, right: 600.0, bottom: 800.0 },
                ..Page::default()
            }],
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

fn lines<'a>(layout: &'a DocumentLayout, frame: &str) -> &'a [LaidLine] {
    &layout.frames[frame].lines
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

/// "abcd" words: 4 × 5 pt at 10 pt, spaces 2.5 pt. Four words and three spaces take 87.5 pt.
fn words(n: usize) -> String {
    (0..n).map(|_| "abcd").collect::<Vec<_>>().join(" ")
}

#[test]
fn breaks_and_baselines_in_one_frame() {
    let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![para(&words(10), 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert_eq!(l.len(), 3, "4 + 4 + 2 words");
    // First baseline: Ascent = 0.8 em; then auto leading 120% = 12 pt.
    assert!(close(l[0].baseline, 8.0) && close(l[1].baseline, 20.0) && close(l[2].baseline, 32.0));
    // Story ranges: each line ends after its trailing space; the last at the end.
    assert_eq!((l[0].start, l[0].end), (0, 20));
    assert_eq!((l[1].start, l[1].end), (20, 40));
    assert_eq!((l[2].start, l[2].end), (40, 49));
    // Glyphs: none for spaces, 5 pt apart, from the frame's left edge.
    assert_eq!(l[0].glyphs.len(), 16);
    assert!(close(l[0].glyphs[0].x, 0.0) && close(l[0].glyphs[1].x, 5.0) && close(l[0].glyphs[4].x, 22.5));
    assert!(close(l[0].glyphs[0].y, 8.0) && close(l[0].glyphs[0].size, 10.0));
    assert!(layout.overset.is_empty());
    assert_eq!(layout.stats.lines, 3);
}

#[test]
fn justified_lines_fill_the_measure_and_tracking_drops_at_line_end() {
    // With 1 pt tracking a word is 24 pt and three words fit: 3 + 3 + 3 + 2.
    let mut p = para(&words(11), 10.0);
    p.attrs.justification = Justification::LeftJustified;
    p.runs[0].attrs.tracking = 100.0; // 1 pt after every character, 0.1 em
    let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![p])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    // A full line: its last glyph ends at the right edge (its own tracking dropped).
    let last = l[0].glyphs.last().unwrap();
    assert!(close(last.x + 5.0, 100.0), "last glyph ends at the measure: {}", last.x + 5.0);
    // Inside a word glyphs are 5 + 1 pt apart (letter spacing never changes).
    assert!(close(l[0].glyphs[1].x - l[0].glyphs[0].x, 6.0));
    // The paragraph's last line is not justified: natural spacing.
    let end = l.last().unwrap();
    assert!(close(end.glyphs[4].x - end.glyphs[3].x, 6.0 + 2.5 + 1.0));
}

#[test]
fn columns_and_threaded_frames() {
    let f1 = text_frame("f1", "s", 212.0, 30.0, |t| {
        t.columns = 2;
        t.gutter = 12.0;
        t.next = Some("f2".into());
    });
    let mut f2 = text_frame("f2", "s", 100.0, 30.0, |t| t.previous = Some("f1".into()));
    f2.transform = Affine::translate(0.0, 300.0);
    // 100 pt columns hold 4 words per line, 30 pt deep frames hold baselines 8 and 20.
    let doc = document(vec![f2, f1], vec![story("s", vec![para(&words(24), 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let a = lines(&layout, "f1");
    let b = lines(&layout, "f2");
    assert_eq!(a.len(), 4, "two lines in each of two columns");
    assert!(close(a[0].left, 0.0) && close(a[2].left, 112.0), "second column after the gutter");
    assert!(close(a[2].baseline, 8.0), "a new column starts at the first-baseline offset");
    assert_eq!(b.len(), 2);
    assert!(close(b[0].baseline, 8.0) && close(b[0].left, 0.0), "frame coordinates are the frame's own");
    assert_eq!(b[0].start, a[3].end, "the story continues where the first frame ended");
    assert!(layout.overset.is_empty());
}

#[test]
fn overset_and_empty_last_paragraph() {
    // One line of room: the second line is overset.
    let doc = document(vec![text_frame("f", "s", 100.0, 15.0, |_| {})], vec![story("s", vec![para(&words(6), 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    assert_eq!(lines(&layout, "f").len(), 1);
    assert_eq!(layout.overset, vec!["s".to_string()]);
    // A story ending with a return whose empty last paragraph does not fit is not overset.
    let doc = document(
        vec![text_frame("f", "s", 100.0, 15.0, |_| {})],
        vec![story("s", vec![para("abcd", 10.0), para("", 10.0)])],
    );
    let layout = layout_document(&doc, &FontStore::empty());
    assert_eq!(lines(&layout, "f").len(), 1);
    assert!(layout.overset.is_empty());
}

#[test]
fn first_baseline_options() {
    let base = |fb: FirstBaseline, min: f64| {
        let doc = document(
            vec![text_frame("f", "s", 100.0, 100.0, |t| {
                t.first_baseline = fb;
                t.min_first_baseline_offset = min;
                t.insets = [5.0, 0.0, 0.0, 0.0];
            })],
            vec![story("s", vec![para("abcd", 10.0)])],
        );
        layout_document(&doc, &FontStore::empty()).frames["f"].lines[0].baseline
    };
    assert!(close(base(FirstBaseline::Ascent, 0.0), 5.0 + 8.0));
    assert!(close(base(FirstBaseline::CapHeight, 0.0), 5.0 + 7.0));
    assert!(close(base(FirstBaseline::XHeight, 0.0), 5.0 + 5.0));
    assert!(close(base(FirstBaseline::Leading, 0.0), 5.0 + 12.0));
    assert!(close(base(FirstBaseline::Fixed, 20.0), 5.0 + 20.0));
    assert!(close(base(FirstBaseline::Ascent, 15.0), 5.0 + 15.0), "never less than the minimum");
}

#[test]
fn leading_space_before_and_after() {
    let mut p1 = para("abcd", 10.0);
    p1.attrs.space_before = 50.0; // ignored at the top of a column
    p1.attrs.space_after = 4.0;
    let mut p2 = para("abcd", 10.0);
    p2.attrs.space_before = 3.0;
    p2.runs[0].attrs.leading = Leading::Fixed(18.0);
    let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![p1, p2])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert!(close(l[0].baseline, 8.0));
    assert!(close(l[1].baseline, 8.0 + 4.0 + 3.0 + 18.0));
    assert_eq!((l[0].start, l[0].end, l[1].start, l[1].end), (0, 5, 5, 9), "the return belongs to its line");
}

#[test]
fn forced_breaks_tabs_and_indents() {
    let mut p = para("ab\tcd\nef", 10.0);
    p.attrs.left_indent = 4.0;
    p.attrs.first_line_indent = 6.0;
    let doc = document(vec![text_frame("f", "s", 200.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert_eq!(l.len(), 2, "forced line break");
    // First line starts at left + first-line indent; the tab goes to the default stop at 36.
    assert!(close(l[0].glyphs[0].x, 10.0) && close(l[0].glyphs[2].x, 36.0));
    assert!(close(l[1].glyphs[0].x, 4.0), "later lines: left indent only");

    let mut p = para("ab\t12", 10.0);
    p.attrs.tabs = vec![TabStop { position: 80.0, alignment: TabAlignment::Right, ..TabStop::default() }];
    let doc = document(vec![text_frame("f", "s", 200.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    let last = l[0].glyphs.last().unwrap();
    assert!(close(last.x + 5.0, 80.0), "right tab: text ends at the stop");
}

#[test]
fn alignment_and_vertical_justification() {
    let mut p = para("abcd", 10.0);
    p.attrs.justification = Justification::CenterAlign;
    let doc = document(
        vec![text_frame("f", "s", 100.0, 100.0, |t| t.vertical_justification = VerticalJustification::Bottom)],
        vec![story("s", vec![p])],
    );
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert!(close(l[0].glyphs[0].x, 40.0), "centred: (100 - 20) / 2");
    // Bottom: the last baseline rests on the frame bottom, descenders below it (InDesign,
    // #347: all eight bottom-aligned frames).
    assert!(close(l[0].baseline, 100.0), "{}", l[0].baseline);
}

#[test]
fn text_wrap_jump_and_side() {
    let mut image = PageItem { id: "img".into(), paths: rect_path(100.0, 40.0), wraps: true, ..PageItem::default() };
    image.kind = ItemKind::Rectangle { content: None };
    image.text_wrap = Some(TextWrap { mode: TextWrapMode::JumpObject, offsets: [0.0, 0.0, 5.0, 0.0], ..TextWrap::default() });
    let doc = document(
        vec![text_frame("f", "s", 100.0, 200.0, |_| {}), image.clone()],
        vec![story("s", vec![para(&words(4), 10.0)])],
    );
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert!(close(l[0].baseline, 45.0 + 8.0), "below the wrap (40 + 5 offset) at the first-baseline offset");

    // Bounding box wrap on the left half: lines beside it get the right half.
    image.paths = rect_path(50.0, 30.0);
    image.text_wrap = Some(TextWrap { mode: TextWrapMode::BoundingBox, ..TextWrap::default() });
    let doc = document(
        vec![text_frame("f", "s", 100.0, 200.0, |_| {}), image],
        vec![story("s", vec![para(&words(10), 10.0)])],
    );
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert!(close(l[0].left, 50.0) && close(l[1].left, 50.0), "beside the wrap");
    assert_eq!(l[0].end - l[0].start, 10, "two words fit in 50 pt");
    let below = l.iter().find(|x| x.baseline - 12.0 >= 30.0 - EPS).unwrap();
    assert!(close(below.left, 0.0), "full width once the line's leading clears the wrap");
}

#[test]
fn frames_in_groups_and_layout_story_alone() {
    let inner = text_frame("f", "s", 100.0, 100.0, |_| {});
    let group = PageItem {
        id: "g".into(),
        kind: ItemKind::Group { children: vec![inner] },
        transform: Affine::new(2.0, 0.0, 0.0, 2.0, 50.0, 60.0),
        ..PageItem::default()
    };
    let other = text_frame("f2", "t", 100.0, 100.0, |_| {});
    let doc = document(
        vec![group, other],
        vec![story("s", vec![para(&words(5), 10.0)]), story("t", vec![para("abcd", 9.0)])],
    );
    let fonts = FontStore::empty();
    let layout = layout_document(&doc, &fonts);
    let l = lines(&layout, "f");
    assert_eq!(l.len(), 2);
    assert!(close(l[0].baseline, 8.0), "positions stay in the frame's inner coordinates");
    let frames = DocumentFrames::new(&doc);
    for id in ["s", "t"] {
        let alone = layout_story(&doc, &frames, id, &fonts).unwrap();
        for f in alone.frames {
            assert_eq!(Some(&f), layout.frames.get(&f.frame));
        }
    }
    assert!(layout_story(&doc, &frames, "missing", &fonts).is_none());
}

#[test]
fn page_number_variable_takes_the_page_name() {
    let mut p = para("#", 10.0);
    p.runs[0].variable = Some(folio_model::TextVariable::PageNumber);
    let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert_eq!(l[0].glyphs.len(), 1, "page \"7\": one glyph");
    // The variable is one character of the story, however long the number: "p. #" and a
    // following paragraph on page "114" keep InDesign's character indices (#347 folios).
    let mut a = para("p. ", 10.0);
    a.runs.push(TextRun { text: "#".into(), attrs: chars(10.0), variable: Some(folio_model::TextVariable::PageNumber), ..TextRun::default() });
    let mut doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![a, para("ab", 10.0)])]);
    doc.spreads[0].pages[0].name = "114".into();
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert_eq!(l[0].glyphs.len(), 5, "\"p.\" and \"114\"");
    assert_eq!((l[0].start, l[0].end, l[1].start, l[1].end), (0, 5, 5, 7));
}

#[test]
fn missing_fonts_are_reported() {
    let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![para("abcd", 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    assert_eq!(layout.missing_fonts, vec![("Minion Pro".to_string(), "Regular".to_string())]);
}

#[test]
fn underline_and_strike_through_spans() {
    let mut p = para("abcd abcd ", 10.0);
    p.runs[0].attrs.underline = true;
    p.runs[0].attrs.underline_offset = Some(6.0);
    p.runs[0].attrs.underline_weight = Some(0.4);
    let mut tail = p.runs[0].clone();
    tail.text = "abcd".into();
    tail.attrs.underline = false;
    tail.attrs.strike_through = true;
    p.runs.push(tail);
    let doc = document(vec![text_frame("f", "s", 200.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    let d = &l[0].decorations;
    assert_eq!(d.len(), 2, "{d:?}");
    // One underline across both words and the space between them, ending under the
    // space before the next run (not a trailing blank of the line).
    assert!(close(d[0].x0, 0.0) && close(d[0].x1, 45.0), "{:?}", d[0]);
    assert!(close(d[0].y, 8.0 + 6.0) && close(d[0].thickness, 0.4), "offset to the rule's centre");
    // InDesign format-strike: automatic position is 1/3 em above the baseline;
    // weight is the font's underline weight (synthetic fallback: 0.05 em).
    assert!(close(d[1].x0, 45.0) && close(d[1].x1, 65.0) && close(d[1].y, 8.0 - 10.0 / 3.0));
    assert!(close(d[1].thickness, 0.5));
}

#[test]
fn master_page_numbers_per_document_page() {
    use folio_text::layout_master_page_numbers;
    let page = |id: &str, name: &str, tx: f64, index: usize, master: Option<&str>| Page {
        id: id.into(),
        name: name.into(),
        bounds: Rect { left: 0.0, top: 0.0, right: 300.0, bottom: 400.0 },
        transform: Affine::translate(tx, 0.0),
        master: master.map(Into::into),
        index,
        ..Page::default()
    };
    // Master "A": left page at x -300..0, right page 0..300; the number frame is on the right.
    let mut number = text_frame("num", "pn", 40.0, 20.0, |_| {});
    number.transform = Affine::translate(250.0, 370.0);
    let mut p = para("#", 10.0);
    p.runs[0].variable = Some(folio_model::TextVariable::PageNumber);
    let master = Spread {
        id: "m".into(),
        name: Some("A-Master".into()),
        pages: vec![page("mp1", "A", -300.0, 0, None), page("mp2", "A", 0.0, 1, None)],
        items: vec![number],
        ..Spread::default()
    };
    let spread = Spread {
        id: "s".into(),
        pages: vec![page("p2", "12", -300.0, 1, Some("m")), page("p3", "13", 0.0, 2, Some("m"))],
        ..Spread::default()
    };
    let doc = Document {
        spreads: vec![spread],
        master_spreads: vec![master],
        stories: [("pn".to_string(), story("pn", vec![p]))].into_iter().collect(),
        ..Document::default()
    };
    let fonts = FontStore::empty();
    // On the master itself the frame shows the prefix.
    let main = layout_document(&doc, &fonts);
    assert_eq!(main.frames["num"].lines[0].glyphs.len(), 1);
    let pages = layout_master_page_numbers(&doc, &fonts, &main);
    assert_eq!(pages.len(), 1, "only the right-hand page uses the right master page");
    assert_eq!(pages[0].page, 2);
    assert_eq!(pages[0].frame.frame, "num");
    assert_eq!(pages[0].frame.lines[0].glyphs.len(), 2, "\"13\"");
}

#[test]
fn a_word_wider_than_the_column_is_overset_not_set_overfull() {
    // InDesign 2025 (artifacts/next-20261018/fit/indesign/result-fit6.json): a word wider
    // than the measure, even by 0.1 pt, is not placed; the text after the last word that fits
    // is overset. "abcd" is 20 pt at 10 pt.
    let doc = document(vec![text_frame("f", "s", 19.9, 200.0, |_| {})], vec![story("s", vec![para("abcd", 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    assert!(lines(&layout, "f").is_empty());
    assert_eq!(layout.overset, vec!["s".to_string()]);
    // Within InDesign's 1/2048 pt tolerance it still fits.
    let doc = document(vec![text_frame("f", "s", 19.9997, 200.0, |_| {})], vec![story("s", vec![para("abcd", 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    assert_eq!(lines(&layout, "f").len(), 1);
    // "ab abcdefgh": the second word (40 pt) cannot be set in 30 pt.
    let doc = document(vec![text_frame("f", "s", 30.0, 200.0, |_| {})], vec![story("s", vec![para("ab abcdefgh", 10.0)])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert_eq!((l.len(), l[0].end), (1, 3), "only \"ab \" is set");
    assert_eq!(layout.overset, vec!["s".to_string()]);
}

#[test]
fn a_slot_beside_a_wrap_holds_the_text_up_to_the_first_break() {
    // #347 u4457f: InDesign sets an empty paragraph in a 4.8 pt slot beside a wrap; a slot
    // narrower than the first word is left and the line goes below the wrap.
    let mut image = PageItem { id: "img".into(), paths: rect_path(95.0, 30.0), wraps: true, ..PageItem::default() };
    image.kind = ItemKind::Rectangle { content: None };
    image.text_wrap = Some(TextWrap { mode: TextWrapMode::BoundingBox, ..TextWrap::default() });
    // 5 pt beside the wrap: an empty paragraph fits there.
    let doc = document(
        vec![text_frame("f", "s", 100.0, 20.0, |_| {}), image.clone()],
        vec![story("s", vec![para("", 10.0), para("", 10.0)])],
    );
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert!(!l.is_empty() && close(l[0].left, 95.0), "the empty line sits in the 5 pt slot");
    // "ab" (10 pt) does not fit in 5 pt: the line moves below the wrap.
    let doc = document(
        vec![text_frame("f", "s", 100.0, 200.0, |_| {}), image],
        vec![story("s", vec![para("ab", 10.0)])],
    );
    let layout = layout_document(&doc, &FontStore::empty());
    let l = lines(&layout, "f");
    assert!(close(l[0].left, 0.0) && l[0].baseline > 30.0, "below the wrap");
}

#[test]
fn advances_are_whole_multiples_of_1_65536_em() {
    // InDesign's glyph widths: NEXON Lv1 Gothic Bold 944/1000 em at 42.7107 pt measured
    // 40.318283 pt, i.e. 61865/65536 em (not 40.318922 = 0.944 em). Kerning separately.
    let size = 42.71072196307679;
    assert!((folio_text::shape::em_fixed(944, 1000.0) * size - 40.318280).abs() < 1e-5);
    assert_eq!(folio_text::shape::em_fixed(944, 1000.0) * 65536.0, 61865.0);
    assert_eq!(folio_text::shape::em_fixed(-20, 1000.0) * 65536.0, -1310.0, "toward zero");
    assert_eq!(folio_text::shape::em_fixed(1024, 2048.0), 0.5);
}

#[test]
fn tabs_at_the_line_end() {
    // #347 u1dcdd / u4a52c: a line may end after a tab; a tab with no room left on its line
    // starts the next one and is measured from there. "abcd" (20 pt) and a tab fill the
    // 30 pt line (no stop before the edge); the second tab begins line 2 and goes to 15.
    let mut p = para("abcd\t\tab", 10.0);
    p.attrs.tabs = vec![TabStop { position: 15.0, alignment: TabAlignment::Left, ..TabStop::default() }];
    let doc = document(vec![text_frame("f", "s", 30.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert_eq!((l.len(), l[0].end, l[1].start), (2, 5, 5));
    assert!(close(l[1].glyphs[0].x, 15.0), "line 2 text after the tab at the 15 pt stop");

    // A tab ending the first word never makes it too wide for a slot (#347 u41e02): "ab\t"
    // fits beside a wrap that leaves 15 pt, although the tab's stop is at 36.
    let mut image = PageItem { id: "img".into(), paths: rect_path(85.0, 20.0), wraps: true, ..PageItem::default() };
    image.kind = ItemKind::Rectangle { content: None };
    image.transform = Affine::translate(15.0, 0.0);
    image.text_wrap = Some(TextWrap { mode: TextWrapMode::BoundingBox, ..TextWrap::default() });
    let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {}), image], vec![story("s", vec![para("ab\tcd", 10.0)])]);
    let l = layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone();
    assert!(close(l[0].baseline, 8.0) && l[0].end == 3, "\"ab<tab>\" beside the wrap: {:?}", (l[0].baseline, l[0].end));
}

#[test]
fn text_fills_both_sides_of_a_wrap() {
    // InDesign 2025 (fit/indesign/result-both10.json, #347 u47295): with "both sides", a row
    // is the part left of the wrap and then the part right of it, each a line of its own.
    let mut image = PageItem { id: "img".into(), paths: rect_path(20.0, 30.0), wraps: true, ..PageItem::default() };
    image.kind = ItemKind::Rectangle { content: None };
    image.transform = Affine::translate(40.0, 0.0);
    image.text_wrap = Some(TextWrap { mode: TextWrapMode::BoundingBox, side: folio_model::TextWrapSide::BothSides, ..TextWrap::default() });
    let run = |j: Justification| {
        let mut p = para(&words(4), 10.0);
        p.attrs.justification = j;
        let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {}), image.clone()], vec![story("s", vec![p])]);
        layout_document(&doc, &FontStore::empty()).frames["f"].lines.clone()
    };
    let l = run(Justification::LeftAlign);
    assert!(close(l[0].baseline, l[1].baseline), "one row");
    assert!(close(l[0].glyphs[0].x, 0.0) && close(l[1].glyphs[0].x, 60.0));
    assert_eq!((l[0].end, l[1].end), (5, 10), "\"abcd \" on each side");
    // Centred: the two parts keep to the wrap.
    let l = run(Justification::CenterAlign);
    assert!(close(l[0].glyphs[3].x + 5.0, 40.0), "left part flush right");
    assert!(close(l[1].glyphs[0].x, 60.0), "right part flush left");
}

#[test]
fn right_to_left_runs_are_reordered_on_their_line() {
    // Unicode bidi in a left-to-right World-Ready paragraph: the Hebrew
    // word and the number after it form one right-to-left run, the number keeping its own
    // order inside it. Glyphs stay in typed order; their positions are visual.
    let positions = |composer: &str| {
        let mut p = para("ab \u{5D0}\u{5D1}\u{5D2} 12", 10.0);
        p.attrs.composer = composer.into();
        let doc = document(vec![text_frame("f", "s", 200.0, 100.0, |_| {})], vec![story("s", vec![p])]);
        let layout = layout_document(&doc, &FontStore::empty());
        lines(&layout, "f")[0].glyphs.iter().map(|g| g.x).collect::<Vec<_>>()
    };
    let x = positions("HL Composer Optyca");
    let want = [0.0, 5.0, 35.0, 30.0, 25.0, 12.5, 17.5];
    assert!(x.len() == want.len() && x.iter().zip(want).all(|(a, b)| close(*a, b)), "{x:?}");
    let x = positions("HL Composer");
    assert!(x.windows(2).all(|w| w[1] > w[0]), "{x:?}");
}

#[test]
fn right_to_left_paragraphs_set_from_the_right() {
    // `ParagraphDirection="RightToLeftDirection"` (InDesign 2025, content-drops specimen
    // drops-rtl): bidi paragraph level 1, so a Latin word typed first sits rightmost; the
    // first-line indent is on the right; left alignment and the other indents stay physical;
    // a line's trailing space hangs left of the line.
    let rtl = |text: &str, f: &dyn Fn(&mut Paragraph)| {
        let mut p = para(text, 10.0);
        p.attrs.right_to_left = true;
        f(&mut p);
        let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![p])]);
        lines(&layout_document(&doc, &FontStore::empty()), "f").to_vec()
    };
    // "ab אב 12": visual "12 בא ab" from the left.
    let l = rtl("ab \u{5D0}\u{5D1} 12", &|_| {});
    let x: Vec<f64> = l[0].glyphs.iter().map(|g| g.x).collect();
    let want = [25.0, 30.0, 17.5, 12.5, 0.0, 5.0];
    assert!(x.len() == want.len() && x.iter().zip(want).all(|(a, b)| close(*a, b)), "{x:?}");
    // First-line indent 20 on the right: three words fit in 80 pt, flush left at 0.
    let l = rtl(&words(6), &|p| p.attrs.first_line_indent = 20.0);
    assert_eq!(l[0].glyphs.len(), 12, "three words on the first line");
    assert!(close(l[0].glyphs[0].x, 0.0), "flush left, trailing space hung outside: {}", l[0].glyphs[0].x);
    assert!(close(l[1].glyphs[0].x, 0.0), "{:?}", l[1].glyphs.iter().map(|g| g.x).collect::<Vec<_>>());
    // The same paragraph left to right: indented on the left.
    let mut p = para(&words(6), 10.0);
    p.attrs.first_line_indent = 20.0;
    let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    assert!(close(lines(&layout_document(&doc, &FontStore::empty()), "f")[0].glyphs[0].x, 20.0));
}

#[test]
fn kenten_are_drawn_over_each_character_by_japanese_composers() {
    let set = |composer: &str| {
        let mut p = para("a b", 10.0);
        p.attrs.composer = composer.into();
        p.runs[0].attrs.fill = folio_model::Paint::Swatch { id: "Color/Black".into(), tint: 100.0 };
        p.runs[0].attrs.kenten = Some(folio_model::Kenten { kind: folio_model::KentenKind::BlackCircle, ..folio_model::Kenten::default() });
        let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![p])]);
        lines(&layout_document(&doc, &FontStore::empty()), "f")[0].clone()
    };
    let l = set("HL Composer J");
    assert_eq!(l.shapes.len(), 2, "one mark per character, none on the space");
    // 5 pt marks (half the size): 3.5 pt circles centred on the 5 pt advance, their em box
    // (0.88 / 0.12) on the text's em box top (0.88 em above the baseline).
    let b = l.shapes[0].bounds().unwrap();
    assert!(close((b.left + b.right) / 2.0, l.glyphs[0].x + 2.5) && close(b.right - b.left, 3.5), "{b:?}");
    assert!(close((b.top + b.bottom) / 2.0, l.baseline - 8.8 - 0.6 - 0.38 * 5.0), "{b:?}");
    assert!(set("HL Composer").shapes.is_empty(), "the Paragraph Composer draws no kenten");
    assert!(set("HL Composer Optyca").shapes.is_empty(), "nor the World-Ready composer");
}

#[test]
fn an_above_line_object_takes_room_before_its_line() {
    // A 30 × 18 pt object anchored above the first line (InDesign 2025, content-drops
    // specimen drops-anchor2): the line starts a leading (12 pt) plus the object below the
    // frame top; the object's bottom meets the line's em box top (0.88 em above the
    // baseline) and it takes no width in the line.
    let obj = PageItem {
        id: "obj".into(),
        kind: ItemKind::Rectangle { content: None },
        paths: rect_path(30.0, 18.0),
        anchor: Some(folio_model::AnchorSettings { position: folio_model::AnchorPosition::AboveLine, ..folio_model::AnchorSettings::default() }),
        ..PageItem::default()
    };
    let p = Paragraph {
        runs: vec![TextRun { text: "a\u{FFFC}b".into(), attrs: chars(10.0), inline: vec![obj], ..TextRun::default() }],
        ..para("", 10.0)
    };
    let doc = document(vec![text_frame("f", "s", 100.0, 100.0, |_| {})], vec![story("s", vec![p])]);
    let layout = layout_document(&doc, &FontStore::empty());
    let l = &lines(&layout, "f")[0];
    assert!(close(l.baseline, 30.0), "{}", l.baseline);
    let o = &l.objects[0];
    assert!(close(o.x, 0.0) && close(o.y, 30.0 - 8.8 - 18.0), "{o:?}");
    assert!(close(l.glyphs[1].x, 5.0), "no width in the line");
}

#[test]
fn discretionary_hyphen_width_counts_against_the_line_measure() {
    let run = |width| {
        let mut p = para("abcd\u{ad}efgh", 10.0);
        p.attrs.hyphenation = true;
        let doc = document(vec![text_frame("f", "s", width, 200.0, |_| {})],
            vec![story("s", vec![p])]);
        layout_document(&doc, &FontStore::empty())
    };
    let fitting = run(24.0);
    assert_eq!(lines(&fitting, "f").len(), 2);
    assert!(fitting.overset.is_empty());
    // Four letters need 20 pt, plus a 3.3 pt discretionary hyphen.
    let narrow = run(21.0);
    assert!(lines(&narrow, "f").is_empty());
    assert!(!narrow.overset.is_empty());
}

fn dictionary_hyphen_layout(after: u32, before: u32) -> DocumentLayout {
    let mut p = para("representation", 10.0);
    p.runs[0].attrs.language = "English: USA".into();
    p.attrs.hyphenation = true;
    p.attrs.hyphenate_after_first = after;
    p.attrs.hyphenate_before_last = before;
    let doc = document(vec![text_frame("f", "s", 40.0, 200.0, |_| {})], vec![story("s", vec![p])]);
    layout_document(&doc, &FontStore::empty())
}

#[test]
fn dictionary_hyphen_respects_minimum_prefix() {
    let permitted = dictionary_hyphen_layout(1, 1);
    assert!(lines(&permitted, "f").len() > 1);
    assert!(permitted.overset.is_empty());
    let forbidden = dictionary_hyphen_layout(14, 1);
    assert!(lines(&forbidden, "f").is_empty());
    assert!(!forbidden.overset.is_empty());
}

#[test]
fn dictionary_hyphen_respects_minimum_suffix() {
    let permitted = dictionary_hyphen_layout(1, 1);
    assert!(lines(&permitted, "f").len() > 1);
    assert!(permitted.overset.is_empty());
    let forbidden = dictionary_hyphen_layout(1, 14);
    assert!(lines(&forbidden, "f").is_empty());
    assert!(!forbidden.overset.is_empty());
}
