//! The bracket handles of a selected text path: where InDesign 2025 draws them (artifacts/typepath-c14-20261023 screenshots of the document view,
//! rules next/oracle/rules/typepath-c14.ko.md). Measured with a 360 pt line (100,400)-(460,400), StartBracket 60, EndBracket 300, point size 24.
use folio_model::{PageItem, PathPoint, Point, SubPath, TextFrame, textpath::{HorizontalTextPath, PathAlignment, PathEffect, PathTextAlignment}};
use folio_text::{FontStore, textpath_handles::bracket_handles};

fn line_item(a: (f64, f64), b: (f64, f64), start: f64, end: f64, flipped: bool, alignment: PathAlignment, weight: f64) -> PageItem {
    let point = |p: (f64, f64)| PathPoint { anchor: Point::new(p.0, p.1), left: Point::new(p.0, p.1), right: Point::new(p.0, p.1) };
    let mut item = PageItem::default();
    item.stroke.weight = weight;
    item.text_path = Some(HorizontalTextPath {
        frame: TextFrame::default(), start, end, baseline: 0.0, path_alignment: alignment,
        curve: Some(SubPath { points: vec![point(a), point(b)], open: true }), flipped, effect: PathEffect::Rainbow,
        text_alignment: PathTextAlignment::Baseline, spacing: 0.0,
    });
    item
}

fn near(a: [f64; 2], b: [f64; 2], tol: f64, what: &str) { assert!((a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol, "{what}: {a:?} != {b:?}"); }

#[test]
fn brackets_stand_on_the_path_at_their_distances_and_the_centre_between() {
    // measured: lines at x = 160 and 400, the centre tick at x = 280 (screenshots at 100 / 200 / 300 %, the column of each line within 0.2 pt)
    let item = line_item((100.0, 400.0), (460.0, 400.0), 60.0, 300.0, false, PathAlignment::Center, 1.0);
    let h = bracket_handles(&item, 60.0, 300.0, 25.0, 8.0).unwrap();
    near(h.start.point, [160.0, 400.0], 1e-9, "start");
    near(h.end.point, [400.0, 400.0], 1e-9, "end");
    near(h.center.point, [280.0, 400.0], 1e-9, "center");
    near(h.start.tangent, [1.0, 0.0], 1e-9, "tangent");
    assert!(!h.closed && !h.flipped);
}

#[test]
fn a_flipped_path_draws_its_handles_where_the_unflipped_ones_stand() {
    // measured (DOM FLIPPED): the lines, squares and tick stood on the same pixels
    let a = line_item((100.0, 400.0), (460.0, 400.0), 60.0, 300.0, false, PathAlignment::Center, 1.0);
    let b = line_item((100.0, 400.0), (460.0, 400.0), 60.0, 300.0, true, PathAlignment::Center, 1.0);
    assert_eq!(bracket_handles(&a, 60.0, 300.0, 25.0, 8.0).unwrap().start.point, bracket_handles(&b, 60.0, 300.0, 25.0, 8.0).unwrap().start.point);
}

#[test]
fn top_and_bottom_alignment_move_the_handles_by_half_the_stroke_like_the_text() {
    // measured, stroke 6 pt: Top shifted the lines 3 pt (19 px at 300 %) to the glyph-top side (up on a left-to-right line), Bottom 3 pt down
    for (alignment, dy) in [(PathAlignment::Top, -3.0), (PathAlignment::Bottom, 3.0)] {
        let item = line_item((100.0, 400.0), (460.0, 400.0), 60.0, 300.0, false, alignment, 6.0);
        let h = bracket_handles(&item, 60.0, 300.0, 25.0, 8.0).unwrap();
        near(h.start.point, [160.0, 400.0 + dy], 0.05, "start");
        near(h.center.point, [280.0, 400.0 + dy], 0.05, "center");
    }
}

#[test]
fn the_bracket_lines_reach_the_faces_bounding_box_times_the_size_like_indesign() {
    // measured at 300 % (6.25 px/pt), 24 pt, "Hello world": rows above / below the path row of the line, in points (+-1 px = 0.16)
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let faces: [(&str, &str, f64, f64); 5] = [
        ("Arial", "Regular", 24.80, 7.68), ("Times New Roman", "Regular", 24.80, 7.36), ("Georgia", "Regular", 22.72, 7.20),
        ("Verdana", "Regular", 25.12, 7.20), ("Malgun Gothic", "Regular", 22.24, 5.92),
    ];
    let table = Vec::new();
    for (family, style, above, below) in faces {
        let resolved = fonts.resolve(family, style, &table);
        if !resolved.exact { continue; }
        let m = fonts.metrics(resolved.font);
        assert!((m.bbox_top * 24.0 - above).abs() < 0.3, "{family} above {} != {above}", m.bbox_top * 24.0);
        assert!((m.bbox_bottom * 24.0 - below).abs() < 0.3, "{family} below {} != {below}", m.bbox_bottom * 24.0);
    }
}

#[test]
fn a_plain_horizontal_line_is_kept_as_a_baseline_and_its_handles_stand_on_it() {
    // the model keeps a left-to-right line with the default options as `baseline` + brackets already counted along x (no curve)
    let mut item = line_item((100.0, 400.0), (460.0, 400.0), 160.0, 400.0, false, PathAlignment::Center, 1.0);
    let path = item.text_path.as_mut().unwrap();
    path.curve = None;
    path.baseline = 400.0;
    let h = bracket_handles(&item, 160.0, 400.0, 25.0, 8.0).unwrap();
    near(h.start.point, [160.0, 400.0], 1e-9, "start");
    near(h.end.point, [400.0, 400.0], 1e-9, "end");
    near(h.center.point, [280.0, 400.0], 1e-9, "center");
    near(h.start.tangent, [1.0, 0.0], 1e-9, "tangent");
}
