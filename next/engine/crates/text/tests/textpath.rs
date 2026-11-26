//! Baselines, brackets, alignment and overset measured in probe-path-layout.jsx.

use folio_model::{ItemKind, Justification};
use folio_text::{FontStore, layout_document};
use std::path::Path;

#[test]
fn curve_centers_and_tangents_match_measured_pdf() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/typepath-c3.idml");
    let original = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &original.font_table).exact { return; }
    for flipped in [false, true] {
        let mut doc = original.clone();
        let item = doc.spreads.iter_mut().flat_map(|s| &mut s.items).find(|i| i.text_path.is_some()).unwrap();
        let id = item.id.clone();
        let transform = item.transform;
        item.text_path.as_mut().unwrap().flipped = flipped;
        let layout = layout_document(&doc, &fonts);
        let glyph = &layout.frames[&id].lines[0].glyphs[0];
        let page = &doc.spreads[0].pages[0];
        let origin = page.transform.invert().unwrap().apply(transform.apply(folio_model::Point::new(glyph.x,glyph.y)));
        let (x,y) = if flipped {(320.38694236739735,100.34098146325586)} else {(79.61305763260265,99.65901853674414)};
        assert!((origin.x-page.bounds.left-x).abs()<0.001, "{origin:?} != {x}");
        assert!((origin.y-page.bounds.top-y).abs()<0.001, "{origin:?} != {y}");
        let m = glyph.path_transform.unwrap();
        let angle = m.b.atan2(m.a).to_degrees();
        let expected = if flipped {133.70313452023753} else {-46.296865479762474};
        assert!((angle-expected).abs()<0.001, "{angle}");
    }
}

#[test]
fn spacing_walks_the_curve_above_the_baseline_like_indesign() {
    // PDF text matrices (the space has no glyph in Folio) of InDesign 2025 for PathSpacing=12 on the typepath-c3 S curve (Arial 24pt, "Arial path"):
    // artifacts/textpath2-20261023/measurement/textpath2-curves-extra/spacing-after.pdf, extracted to pdf-matrices.json.
    let measured: [(f64, f64, f64); 9] = [
        (78.9678,100.579,-47.3907),
        (88.697,90.2743,-38.6455),
        (93.9521,86.1422,-32.4564),
        (96.7905,83.9211,-24.0615),
        (107.2876,79.4688,-14.8941),
        (116.2061,77.114,-1.5104),
        (127.8042,76.7043,6.9075),
        (140.0056,78.3105,11.8871),
        (145.7539,79.3983,15.8449),
    ];
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/typepath-c3.idml");
    let mut doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact { return; }
    let item = doc.spreads.iter_mut().flat_map(|s| &mut s.items).find(|i| i.text_path.is_some()).unwrap();
    let (id, transform) = (item.id.clone(), item.transform);
    item.text_path.as_mut().unwrap().spacing = 12.0;
    let layout = layout_document(&doc, &fonts);
    let page = &doc.spreads[0].pages[0];
    let glyphs = &layout.frames[&id].lines[0].glyphs;
    assert_eq!(glyphs.len(), measured.len());
    for (glyph, (x, y, angle)) in glyphs.iter().zip(measured) {
        let origin = page.transform.invert().unwrap().apply(transform.apply(folio_model::Point::new(glyph.x, glyph.y)));
        let m = glyph.path_transform.unwrap();
        let turned = m.b.atan2(m.a).to_degrees();
        let error = (origin.x-page.bounds.left-x).hypot(origin.y-page.bounds.top-y);
        assert!(error < 0.35, "origin error {error} at {x},{y}");
        assert!((turned-angle).abs() < 1.0, "angle {turned} != {angle}");
    }
}

#[test]
fn curve_alignment_matches_measured_baseline_offsets() {
    use folio_model::textpath::PathTextAlignment;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/typepath-c3.idml");
    let original = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &original.font_table).exact { return; }
    let item = original.spreads.iter().flat_map(|s| &s.items).find(|i| i.text_path.is_some()).unwrap();
    let id = item.id.clone();
    let baseline = layout_document(&original, &fonts);
    let base = &baseline.frames[&id].lines[0].glyphs[0];
    for (alignment, offset) in [
        (PathTextAlignment::Ascender, 24.94921875),
        (PathTextAlignment::Descender, -7.79296875),
        (PathTextAlignment::Center, 8.578125),
    ] {
        let mut doc = original.clone();
        let item = doc.spreads.iter_mut().flat_map(|s| &mut s.items).find(|i| i.id == id).unwrap();
        item.text_path.as_mut().unwrap().text_alignment = alignment;
        let layout = layout_document(&doc, &fonts);
        let glyph = &layout.frames[&id].lines[0].glyphs[0];
        let m = base.path_transform.unwrap();
        let normal_offset = (glyph.x-base.x)*m.c + (glyph.y-base.y)*m.d;
        assert!((normal_offset-offset).abs()<0.001, "{alignment:?}: {normal_offset}");
    }
}

#[test]
fn horizontal_path_matches_indesign_measurements() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tools2-path.idml");
    let original = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &original.font_table).exact {
        eprintln!("skipped: measured Arial font is not installed");
        return;
    }
    for (case, x, end, characters, overflow) in [
        (0, 80.0, 181.390625, 10, false),
        (1, 100.0, 201.390625, 10, false),
        (2, 80.0, 288.125, 22, true),
        (3, 218.609375, 320.0, 10, false),
        (4, 80.0, 181.390625, 10, false),
        (5, 80.0, 181.390625, 10, false),
    ] {
        let mut doc = original.clone();
        let item = doc.spreads.iter_mut().flat_map(|s| &mut s.items).find(|i| i.text_path.is_some()).unwrap();
        assert!(matches!(item.kind, ItemKind::GraphicLine));
        let id = item.id.clone();
        let path = item.text_path.as_mut().unwrap();
        let mut baseline = path.baseline;
        if case == 4 || case == 5 {
            item.stroke.weight=6.0;
            path.path_alignment=if case==4 {folio_model::textpath::PathAlignment::Top} else {folio_model::textpath::PathAlignment::Bottom};
            baseline+=if case==4 {-3.0} else {3.0};
        }
        let story = path.frame.story.clone();
        if case == 1 { path.start += 20.0; path.end -= 40.0; }
        let paragraph = &mut doc.stories.get_mut(&story).unwrap().paragraphs[0];
        if case == 2 { paragraph.runs[0].text = "Arial path Arial path Arial path Arial path".into(); }
        if case == 3 { paragraph.attrs.justification = Justification::RightAlign; }
        let layout = layout_document(&doc, &fonts);
        let frame = &layout.frames[&id];
        assert_eq!(frame.lines.len(), 1, "case {case}");
        let line = &frame.lines[0];
        assert_eq!(line.end - line.start, characters, "case {case}");
        assert!((line.baseline - baseline).abs() < 1e-6, "case {case}");
        assert!((line.glyphs[0].x - x).abs() < 0.01, "case {case}: {} != {x}", line.glyphs[0].x);
        let last = line.text_clusters.last().unwrap();
        // For the overset line, compare the visible text end, before the trailing space.
        let visible_end = if overflow { last.x0 } else { last.x1 };
        assert!((visible_end - end).abs() < 0.01, "case {case}: visible end {visible_end} != {end}");
        assert_eq!(!layout.overset.is_empty(), overflow, "case {case}");
    }
}

#[test]
fn threaded_path_keeps_story_order_and_measured_baseline() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/textpath-threaded.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let path = doc.spreads.iter().flat_map(|s| &s.items).find(|i| i.text_path.is_some()).unwrap();
    let frame = &path.text_path.as_ref().unwrap().frame;
    let next = doc.spreads.iter().flat_map(|s| &s.items).find(|i| Some(&i.id) == frame.next.as_ref()).unwrap();
    assert_eq!(next.text_frame().unwrap().previous.as_deref(), Some(path.id.as_str()));
    assert_eq!(next.text_frame().unwrap().story, frame.story);
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact {
        eprintln!("skipped: measured Arial font is not installed");
        return;
    }
    let layout = layout_document(&doc, &fonts);
    let line = &layout.frames[&path.id].lines[0];
    assert_eq!((line.start, line.end), (0, 20));
    let page = &doc.spreads[0].pages[0];
    let to_page = page.transform.invert().unwrap();
    let start = to_page.apply(path.transform.apply(folio_model::Point::new(line.glyphs[0].x, line.baseline)));
    let end = to_page.apply(path.transform.apply(folio_model::Point::new(line.text_clusters.last().unwrap().x1, line.baseline)));
    assert!((start.y - page.bounds.top - 100.0).abs() < 1e-6, "{start:?}, {:?}", page.bounds);
    assert!((start.x - page.bounds.left - 80.0).abs() < 0.01);
    assert!((end.x - page.bounds.left - 289.47265625).abs() < 0.01, "{end:?}");
    let tail = &layout.frames[&next.id].lines[0];
    assert_eq!((tail.start, tail.end), (20, 25));
    assert!(layout.overset.is_empty());
}

#[test]
fn horizontal_path_can_start_end_or_bridge_a_frame_chain() {
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    for (fixture, measured) in [
        ("textpath-chain-first", vec![
            vec![(100.0, 256.109375, "Arial path thread")],
            vec![(177.47265625, 278.7734375, "ed story Arial path "), (206.27265625, 293.4453125, "threaded story Arial ")],
        ]),
        ("textpath-chain-last", vec![
            vec![(47.47265625, 256.109375, "Arial path thread")],
            vec![(100.0, 278.7734375, "ed story Arial path ")],
        ]),
        ("textpath-chain-middle", vec![
            vec![(47.47265625, 256.109375, "Arial path thread")],
            vec![(100.0, 278.7734375, "ed story Arial path ")],
            vec![(177.47265625, 293.4453125, "threaded story Arial "), (206.27265625, 126.7109375, "path")],
        ]),
    ] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../oracle/fixtures/{fixture}.idml"));
        let doc = folio_idml::read_package(&source, None).unwrap();
        if !fonts.resolve("Arial", "Regular", &doc.font_table).exact {
            eprintln!("skipped: measured Arial font is not installed");
            return;
        }
        let items = &doc.spreads[0].items;
        let story = &items.iter().find_map(|i| i.text_path.as_ref()).unwrap().frame.story;
        let mut item = items.iter().find(|i| i.text_frame().is_some_and(|f| &f.story == story && f.previous.is_none())).unwrap();
        let page = &doc.spreads[0].pages[0];
        let to_page = page.transform.invert().unwrap();
        let layout = layout_document(&doc, &fonts);
        let mut offset = 0;
        for (index, rows) in measured.iter().enumerate() {
            let laid = &layout.frames[&item.id];
            assert_eq!(laid.lines.len(), rows.len(), "{fixture} frame {index}");
            for (line, &(baseline, end, content)) in laid.lines.iter().zip(rows) {
                assert_eq!((line.start, line.end), (offset, offset + content.chars().count()), "{fixture} frame {index}");
                offset = line.end;
                let start = to_page.apply(item.transform.apply(folio_model::Point::new(line.glyphs[0].x, line.baseline)));
                let last = to_page.apply(item.transform.apply(folio_model::Point::new(line.text_clusters.last().unwrap().x1, line.baseline)));
                assert!((start.y - page.bounds.top - baseline).abs() < 0.01, "{fixture}: {start:?}");
                assert!((start.x - page.bounds.left - 80.0).abs() < 0.01, "{fixture}: {start:?}");
                assert!((last.x - page.bounds.left - end).abs() < 0.01, "{fixture}: {last:?}");
            }
            if index + 1 < measured.len() {
                let next = item.text_frame().unwrap().next.as_ref().unwrap();
                let following = items.iter().find(|i| &i.id == next).unwrap();
                assert_eq!(following.text_frame().unwrap().previous.as_ref(), Some(&item.id));
                item = following;
            }
        }
        assert!(item.text_frame().unwrap().next.is_none());
    }
}

/// The caret, a selection and a click on path text follow the path: every cluster (a space too) carries the map from
/// the straight line it was composed on to where it landed, and the map agrees with the glyph placed there.
#[test]
fn clusters_of_path_text_know_where_they_landed() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/typepath-c3.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact { return; }
    let id = doc.spreads.iter().flat_map(|s| &s.items).find(|i| i.text_path.is_some()).unwrap().id.clone();
    let layout = layout_document(&doc, &fonts);
    let line = &layout.frames[&id].lines[0];
    assert_eq!(line.text_clusters.len(), 10, "Arial path");
    assert!(line.text_clusters.iter().all(|c| c.path_map.is_some()), "the space has a map too");
    let glyph = &line.glyphs[0];
    let first = &line.text_clusters[0];
    let m = first.path_map.unwrap();
    // the first glyph's origin is where the straight line's (x0, baseline) went
    let o = m.apply(folio_model::Point::new(first.x0, line.baseline));
    assert!((o.x-glyph.x).abs() < 1e-9 && (o.y-glyph.y).abs() < 1e-9, "{o:?} != ({}, {})", glyph.x, glyph.y);
    // the box's far edge is one advance along the glyph's baseline
    let e = m.apply(folio_model::Point::new(first.x1, line.baseline));
    let advance = first.x1-first.x0;
    assert!(((e.x-o.x).hypot(e.y-o.y)-advance).abs() < 1e-9);
    // later clusters are further along the curve, not on the straight line
    let last = line.text_clusters.last().unwrap();
    let far = last.path_map.unwrap().apply(folio_model::Point::new(last.x0, line.baseline));
    assert!((far.y-o.y).abs() > 5.0, "the curve climbs: {far:?} vs {o:?}");
}

/// Where Folio puts every glyph of the (only) path text of a fixture, in page points: (x, y, angle in degrees).
fn placed_glyphs(fixture: &str, fonts: &FontStore) -> Option<Vec<(f64, f64, f64)>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(fixture);
    let doc = folio_idml::read_package(&path, None).unwrap();
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact { return None; }
    let item = doc.spreads.iter().flat_map(|s| &s.items).find(|i| i.text_path.is_some()).unwrap();
    let layout = layout_document(&doc, fonts);
    let page = &doc.spreads[0].pages[0];
    Some(layout.frames[&item.id].lines.iter().flat_map(|line| &line.glyphs).map(|g| {
        let o = page.transform.invert().unwrap().apply(item.transform.apply(folio_model::Point::new(g.x, g.y)));
        let angle = g.path_transform.map_or(0.0, |m| m.b.atan2(m.a).to_degrees());
        (o.x-page.bounds.left, o.y-page.bounds.top, angle)
    }).collect())
}

fn assert_measured(placed: &[(f64, f64, f64)], measured: &[(f64, f64, f64)]) {
    for &(x, y, angle) in measured {
        let nearest = placed.iter().min_by(|a, b| (a.0-x).hypot(a.1-y).total_cmp(&(b.0-x).hypot(b.1-y))).unwrap();
        assert!((nearest.0-x).hypot(nearest.1-y) < 0.02, "no glyph at ({x}, {y}): nearest {nearest:?}");
        let turn = (nearest.2-angle+540.0).rem_euclid(360.0)-180.0; // 180 and -180 are the same direction
        assert!(turn.abs() < 0.02, "angle at ({x}, {y}): {} != {angle}", nearest.2);
    }
}

#[test]
fn closed_oval_path_matches_indesign_measurements() {
    // InDesign 2025 PDF text matrices of next/oracle/fixtures/typepath-c5-oval.idml (artifacts/oracle/typepath-c5-oval/noop/after.pdf):
    // text starts at the first anchor and runs along the anchors; each Bezier quarter is cut into 128 steps.
    let measured = [
        (300.0016, 309.9118, 1.5240), (337.2103, 312.7729, 9.4462), (370.9705, 320.5805, 19.7801), (407.4277, 336.9993, 34.3863),
        (443.5191, 373.6508, 65.1954), (450.2784, 403.3494, 100.1977), (430.4270, 444.7610, 137.2745), (400.5167, 466.9523, 153.9335),
        (369.7678, 479.8339, 164.5865), (324.0160, 488.9212, 175.8575), (278.7013, 489.1778, -173.6626),
    ];
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let Some(placed) = placed_glyphs("typepath-c5-oval.idml", &fonts) else { return };
    assert_measured(&placed, &measured);
}

#[test]
fn several_segments_are_cut_by_their_own_step_count_like_indesign() {
    // InDesign 2025 matrices of typepath-c5-multi.idml (a straight line, a wave, a hairpin, a long arc): the segments are cut
    // into 1, 128, 128 and 256 steps. A rule by control-polygon flatness (smallest power of two with h^2*max|B''|/8 <= 0.01)
    // gives all of them, the quarter ellipses of the oval (128) and the typepath-c3 S curve (256) as well.
    let measured = [
        (50.0000, 400.0000, 0.0), (146.7606, 404.1817, -52.6860), (172.4518, 378.5831, -6.7312), (205.3026, 380.8103, 23.6829),
        (234.1788, 394.6305, 23.1459), (258.1732, 402.6408, 2.2729), (278.0838, 400.4105, -34.3337), (297.1544, 383.8998, -53.8827),
        (307.2821, 368.2303, -58.6032), (316.9374, 352.2393, -59.7385), (330.2385, 329.1075, -61.2245), (340.1252, 321.6550, 90.9996),
        (345.0494, 346.1133, 61.5604), (359.2935, 368.6521, 51.1798), (371.2378, 383.0125, 46.9084), (386.2584, 398.1630, 41.9142),
        (405.8076, 414.2901, 34.0140), (428.4480, 428.4308, 25.1006), (459.5025, 440.4849, 12.9406),
    ];
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let Some(placed) = placed_glyphs("typepath-c5-multi.idml", &fonts) else { return };
    assert_measured(&placed, &measured);
}

#[test]
fn closed_paths_with_straight_sides_match_indesign_measurements() {
    // InDesign 2025 PDF text matrices (one per run: a run is the glyphs of one straight side) of typepath-c6-rect-cw.idml (a rectangle
    // written top-left, top-right, bottom-right, bottom-left) and typepath-c6-poly.idml (a pentagon, clockwise from the top). A glyph
    // belongs to the side its centre is on, so the first glyph of the next side starts a little before or after the corner.
    let rect = [(60.0000, 300.0000, 0.0), (540.0000, 300.2344, 90.0000), (543.6055, 500.0000, -180.0000)];
    let polygon = [(300.0000, 300.0000, 24.2279), (501.5338, 387.1516, 118.3008), (431.4576, 520.0000, -180.0000), (173.1337, 525.8198, -118.3008)];
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    for (fixture, measured) in [("typepath-c6-rect-cw.idml", &rect[..]), ("typepath-c6-poly.idml", &polygon[..])] {
        let Some(placed) = placed_glyphs(fixture, &fonts) else { return };
        assert!(!placed.is_empty(), "{fixture} draws its text");
        assert_measured(&placed, measured);
    }
}

#[test]
fn flipped_counterclockwise_path_draws_forward_between_ascending_brackets() {
    // The ascending brackets leave too little room on the backwards arc.
    // Flipping the pentagon fills the long arc in anchor order.
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    for fixture in ["typepath-c6-rect.idml", "typepath-c6-poly-ccw.idml"] {
        let Some(placed) = placed_glyphs(fixture, &fonts) else { return };
        assert!(placed.is_empty(), "{fixture}: the backwards arc is too short");
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/typepath-c6-poly-ccw.idml");
    let mut doc = folio_idml::read_package(&path, None).unwrap();
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact { return }
    for spread in &mut doc.spreads {
        for item in &mut spread.items { if let Some(path) = item.text_path.as_mut() { path.flipped = true; } }
    }
    let item = doc.spreads.iter().flat_map(|s| &s.items).find(|i| i.text_path.is_some()).unwrap();
    let layout = layout_document(&doc, &fonts);
    let page = &doc.spreads[0].pages[0];
    let placed: Vec<(f64, f64, f64)> = layout.frames[&item.id].lines[0].glyphs.iter().map(|g| {
        let o = page.transform.invert().unwrap().apply(item.transform.apply(folio_model::Point::new(g.x, g.y)));
        (o.x-page.bounds.left, o.y-page.bounds.top, g.path_transform.map_or(0.0, |m| m.b.atan2(m.a).to_degrees()))
    }).collect();
    let measured = [(300.0000, 300.0000, 155.7721), (98.4662, 387.1516, 61.6992), (168.5424, 520.0000, -0.0000), (426.8662, 525.8198, -61.6992)];
    assert_measured(&placed, &measured);
}

#[test]
fn closed_path_can_fill_a_backwards_arc_across_the_first_anchor() {
    use folio_model::{PathPoint, Point, SubPath};

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tools2-path.idml");
    let mut doc = folio_idml::read_package(&path, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact { return }
    let item = doc.spreads.iter_mut().flat_map(|s| &mut s.items).find(|i| i.text_path.is_some()).unwrap();
    let id = item.id.clone();
    let path = item.text_path.as_mut().unwrap();
    let points = [(350.0, 40.0), (350.0, 95.0), (530.0, 95.0), (530.0, 40.0)]
        .into_iter()
        .map(|(x, y)| {
            let anchor = Point::new(x, y);
            PathPoint { anchor, left: anchor, right: anchor }
        })
        .collect();
    path.curve = Some(SubPath { points, open: false });
    path.start = 20.0;
    path.end = 290.0;
    let run = &mut doc.stories.get_mut(&path.frame.story).unwrap().paragraphs[0].runs[0];
    run.text = "One two three four five six. One two three four five six.".into();
    run.attrs.point_size = 12.0;
    let layout = layout_document(&doc, &fonts);
    let line = &layout.frames[&id].lines[0];
    let placed: Vec<_> = line.glyphs.iter().map(|glyph| {
        let angle = glyph.path_transform.map_or(0.0, |m| m.b.atan2(m.a).to_degrees());
        (glyph.x, glyph.y, angle)
    }).collect();
    // InDesign's closed-walk PDF: the first run goes up, the next crosses the top.
    assert_measured(&placed, &[(350.0, 60.0, -90.0), (352.6816 + 3.333984, 40.0, 0.0)]);
    assert_eq!(line.end - line.start, "One two three four five six. One two ".len());
}

#[test]
fn text_flows_from_one_path_onto_a_linked_path_like_indesign() {
    // InDesign 2025 matrices of typepath-c6-link.idml (artifacts/oracle/typepath-c6-link/noop/after.pdf): the S curve of typepath-c3 takes
    // the first 20 glyphs and its linked second curve the next 23, each path placing its glyphs by its own geometry.
    let measured = [
        (79.7912, 99.8239, -48.1926), (87.7259, 90.7405, -36.1585), (98.6099, 82.9340, -22.2011), (109.8321, 78.6640, -10.9791),
        (116.3829, 77.3814, -4.5864), (123.0470, 76.6033, 3.9577), (136.3757, 77.5717, 11.6798), (149.4417, 80.3351, 17.1603),
        (162.1844, 84.3166, 20.5987), (173.4023, 88.5781, 22.1678), (179.5760, 91.0977, 23.0077), (185.7129, 93.7056, 23.5787),
        (193.0366, 96.9053, 23.9613), (205.2133, 102.3735, 23.3260), (223.6018, 110.2189, 21.6416), (229.7753, 112.7460, 19.6060),
        (242.3489, 117.2415, 15.6863), (255.2364, 120.7181, 11.1710), (261.7628, 122.1742, 5.3084), (275.0757, 123.1772, -2.8925),
        (79.7904, 300.2782, 35.1584), (90.7222, 307.9862, 25.4443), (102.8298, 313.7460, 12.7998), (115.8810, 316.5573, 5.6926),
        (122.5181, 317.3677, -0.5185), (135.8742, 317.1413, -7.5727), (149.0807, 315.2130, -11.4753), (155.6143, 313.8762, -13.9709),
        (162.1090, 312.3492, -16.3759), (174.9011, 308.5390, -17.7737), (181.2537, 306.5113, -18.9126), (193.8729, 302.1620, -19.4360),
        (206.4607, 297.7227, -19.1873), (212.7493, 295.5042, -18.3083), (225.4323, 291.3443, -16.7677), (231.8168, 289.4201, -15.5049),
        (238.2070, 287.4808, -12.1495), (255.1765, 283.8942, -5.7519), (268.4719, 282.7138, -0.5185), (275.1700, 282.3978, 7.0472),
        (288.4747, 283.9313, 17.4103), (301.2725, 287.8880, 28.7050), (312.8864, 294.4736, 36.7389),
    ];
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/typepath-c6-link.idml");
    let doc = folio_idml::read_package(&path, None).unwrap();
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact { return }
    let layout = layout_document(&doc, &fonts);
    let page = &doc.spreads[0].pages[0];
    let mut placed = Vec::new();
    let mut per_path = Vec::new();
    for item in doc.spreads.iter().flat_map(|s| &s.items).filter(|i| i.text_path.is_some()) {
        let before = placed.len();
        for g in layout.frames[&item.id].lines.iter().flat_map(|l| &l.glyphs) {
            let o = page.transform.invert().unwrap().apply(item.transform.apply(folio_model::Point::new(g.x, g.y)));
            placed.push((o.x-page.bounds.left, o.y-page.bounds.top, g.path_transform.map_or(0.0, |m| m.b.atan2(m.a).to_degrees())));
        }
        per_path.push(placed.len()-before);
    }
    // InDesign's PDF also has a matrix for every space, which Folio does not draw: every glyph Folio draws must be one of the measured
    // matrices, in order, and what is left over are the spaces (the 43 rows hold 33 glyphs and 10 spaces of "Text goes from path one on to path two and ends").
    assert_eq!(per_path, [16, 17], "glyphs on the first and the linked second path");
    let mut next = 0;
    for &(x, y, angle) in &placed {
        let at = measured[next..].iter().position(|m| (m.0-x).hypot(m.1-y) < 0.02 && (m.2-angle+540.0).rem_euclid(360.0)-180.0 < 0.02 && (m.2-angle+540.0).rem_euclid(360.0)-180.0 > -0.02)
            .unwrap_or_else(|| panic!("no measured matrix for glyph ({x}, {y}, {angle}) after row {next}"));
        next += at+1;
    }
    assert_eq!(measured.len()-placed.len(), 10, "only spaces are left over");

}

#[test]
fn flipped_text_on_an_open_path_starts_at_the_start_bracket_and_runs_back_like_indesign() {
    // InDesign 2025 (typepath-c14 p7): IDML with FlipPathEffect="Flipped" on the tools2-path line (80,-296)-(320,-296), length 240, "Arial path" (DOM character
    // origins, page x - 80): the text starts at the start bracket and runs back toward the path's start for as far as the brackets are apart; what goes past the
    // path start comes round to its end.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tools2-path.idml");
    let original = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &original.font_table).exact { return; }
    let cases: [(f64, f64, &[f64]); 3] = [
        (140.0, 40.0, &[140.0, 124.0, 116.0, 110.7, 97.3]),
        (40.0, 140.0, &[40.0, 24.0, 16.0, 10.7, 237.3]),
        (0.0, 240.0, &[240.0, 224.0, 216.0, 210.7, 197.3, 185.3, 172.0, 158.6, 152.0]),
    ];
    for (start, end, expected) in cases {
        let mut doc = original.clone();
        let item = doc.spreads.iter_mut().flat_map(|s| &mut s.items).find(|i| i.text_path.is_some()).unwrap();
        let id = item.id.clone();
        let path = item.text_path.as_mut().unwrap();
        let point = |x: f64| folio_model::PathPoint { anchor: folio_model::Point::new(x, -296.0), left: folio_model::Point::new(x, -296.0), right: folio_model::Point::new(x, -296.0) };
        path.curve = Some(folio_model::SubPath { points: vec![point(80.0), point(320.0)], open: true });
        (path.start, path.end, path.baseline, path.flipped) = (start, end, 0.0, true);
        let layout = layout_document(&doc, &fonts);
        let line = &layout.frames[&id].lines[0];
        let origins: Vec<f64> = line.glyphs.iter().map(|g| g.x - 80.0).collect();
        for (i, want) in expected.iter().enumerate() {
            // (the space has no glyph in Folio, so it is left out of the list)
            let got = origins.iter().find(|o| (**o - want).abs() < 0.06);
            assert!(got.is_some(), "start {start} end {end}: character {i} should start at {want}, glyph origins {origins:?}");
        }
    }
}
