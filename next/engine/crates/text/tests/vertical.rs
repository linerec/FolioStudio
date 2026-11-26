//! Vertical text against InDesign 2025's PDF of the vtext fixture (Malgun Gothic 12 pt, Japanese composer):
//! artifacts/vtext-c1-20261023/measure/positions-fixture.pdf (text matrices; Identity-V W2 position vectors 500 917).

use folio_text::{FontStore, layout_document};
use std::path::Path;

fn page_point(doc: &folio_model::Document, frame: &str, x: f64, y: f64) -> (f64, f64) {
    let item = doc.spreads.iter().flat_map(|s| &s.items).find(|i| i.id == frame).unwrap();
    let page = &doc.spreads[0].pages[0];
    let p = page.transform.invert().unwrap().apply(item.transform.apply(folio_model::Point::new(x, y)));
    (p.x - page.bounds.left, p.y - page.bounds.top)
}

#[test]
fn vertical_story_matches_indesign_glyph_origins() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/vtext.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Malgun Gothic", "Regular", &doc.font_table).exact { return; }
    assert!(doc.stories["u135"].vertical, "StoryOrientation=Vertical is read");
    let layout = layout_document(&doc, &fonts);
    let lines = &layout.frames["u147"].lines;
    assert_eq!(lines.len(), 5, "one line per paragraph");
    let origin = |line: usize, glyph: usize| {
        let g = &lines[line].glyphs[glyph];
        (page_point(&doc, "u147", g.x, g.y), g.path_transform)
    };
    let close = |(got, want): ((f64, f64), (f64, f64)), what: &str| {
        assert!((got.0 - want.0).abs() < 0.01 && (got.1 - want.1).abs() < 0.01, "{what}: {got:?} != {want:?}");
    };
    let vorigin = 1878.0 / 2048.0 * 12.0;
    // 세: upright, its vertical origin on the pen (y 60) and centred on x 555.4238
    let (p, t) = origin(0, 0);
    close((p, (549.4238, 60.0 + vorigin)), "세");
    assert_eq!(t, Some(folio_model::Affine::IDENTITY), "upright glyph keeps its own axes");
    // 시 after four 12.75 pt advances and the 4.2188 pt space (a space draws no glyph)
    close((origin(0, 4).0, (549.4238, 115.2188 + vorigin)), "시");
    // third line: English turned from y 60, then 글 upright at y 102.6328
    let (p, t) = origin(2, 0);
    close((p, (510.8887, 60.0)), "E");
    let t = t.expect("turned glyph");
    assert!(t.a.abs() < 1e-9 && (t.b - 1.0).abs() < 1e-9 && (t.c + 1.0).abs() < 1e-9, "90° clockwise: {t:?}");
    let first_hangul = lines[2].glyphs.iter().position(|g| g.path_transform == Some(folio_model::Affine::IDENTITY)).unwrap();
    close((origin(2, first_hangul).0, (509.4238, 102.6328 + vorigin)), "글");
}

#[test]
fn vertical_columns_stack_from_the_top() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/vtext.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Malgun Gothic", "Regular", &doc.font_table).exact { return; }
    let layout = layout_document(&doc, &fonts);
    let lines = &layout.frames["u15d"].lines;
    assert_eq!(lines.len(), 14);
    // InDesign: 11 lines in the top column (x 549.4 … 349.4), the rest from the right edge of the bottom column
    let first = page_point(&doc, "u15d", lines[0].glyphs[0].x, lines[0].glyphs[0].y);
    let twelfth = page_point(&doc, "u15d", lines[11].glyphs[0].x, lines[11].glyphs[0].y);
    assert!((first.0 - 549.4238).abs() < 0.01, "{first:?}");
    assert!((twelfth.0 - 549.4238).abs() < 0.01 && twelfth.1 > 380.0 + 184.0, "{twelfth:?}");
}

#[test]
#[ignore]
fn dump_vertical_glyphs() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/vtext.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let layout = layout_document(&doc, &fonts);
    for frame in ["u147", "u15d"] {
        for (i, l) in layout.frames[frame].lines.iter().enumerate() {
            let gs: Vec<String> = l.glyphs.iter().map(|g| { let p = page_point(&doc, frame, g.x, g.y); format!("{:#x}@({:.3},{:.3}){}", g.glyph, p.0, p.1, if g.path_transform.is_some_and(|t| t.b != 0.0) { "T" } else { "" }) }).collect();
            eprintln!("{frame} line {i}: {}", gs.join(" "));
        }
    }
}

/// vtext-c2, InDesign 2025 (oracle vtext2, print comparison): a vertical frame's lines go round another item's wrap (above and
/// below it, rows 4 to 9 of 11) and stay inside an oval frame (its rows start where the oval crosses the band).
#[test]
fn vertical_frames_take_wraps_and_shapes() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/vtext2.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Malgun Gothic", "Regular", &doc.font_table).exact { return; }
    let layout = layout_document(&doc, &fonts);
    // vt-wrap (u15d): 11 rows, the six crossing the box split in two
    let rows = |frame: &str| {
        let mut xs: Vec<f64> = layout.frames[frame].lines.iter().filter_map(|l| l.glyphs.first()).map(|g| page_point(&doc, frame, g.x, g.y).0).collect();
        xs.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        xs.len()
    };
    assert_eq!(layout.frames["u15d"].lines.len(), 17);
    assert_eq!(rows("u15d"), 11);
    // vt-oval (u147): 11 rows, the second starting lower than the sixth (the oval's top is round)
    let lines = &layout.frames["u147"].lines;
    assert_eq!(lines.len(), 11);
    let start = |k: usize| page_point(&doc, "u147", lines[k].glyphs[0].x, lines[k].glyphs[0].y).1;
    assert!(start(1) > start(5) + 40.0, "{} {}", start(1), start(5));
}

#[test]
#[ignore]
fn dump_vtext2_lines() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/vtext2.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let layout = layout_document(&doc, &fonts);
    for frame in ["u131", "u147", "u15d", "u175", "u18b", "u1a1", "u1b7"] {
        let Some(f) = layout.frames.get(frame) else { eprintln!("{frame}: none"); continue };
        for (i, l) in f.lines.iter().enumerate() {
            let (a, b) = (l.glyphs.first(), l.glyphs.last());
            let p = |g: &folio_model::layout::PlacedGlyph| page_point(&doc, frame, g.x, g.y);
            eprintln!("{frame} line {i}: {} glyphs {:?} .. {:?}", l.glyphs.len(), a.map(p), b.map(p));
        }
    }
}
