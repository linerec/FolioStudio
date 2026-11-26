//! Print Booklet plans against what InDesign 2025 printed (`artifacts/printing-20261023/measurement/booklet-raster-measurements.json`
//! and the `printing-booklet*.json` page labels). 240 x 300 pt pages.
use folio_pdf::printing::booklet::{plan, Binding, BookletOptions, BookletType};

const PAGE: [f64; 2] = [240.0, 300.0];

fn faces(count: usize, o: &BookletOptions) -> Vec<Vec<Option<usize>>> {
    // 1-based page numbers as InDesign prints them
    plan(count, PAGE, o).unwrap().iter().map(|s| s.cells.iter().map(|c| c.page.map(|p| p + 1)).collect()).collect()
}

fn pair(a: usize, b: usize) -> Vec<Option<usize>> {
    vec![Some(a), Some(b)]
}

#[test]
fn saddle_stitch_pairs() {
    let o = BookletOptions::default();
    assert_eq!(faces(4, &o), [pair(4, 1), pair(2, 3)]);
    assert_eq!(faces(8, &o), [pair(8, 1), pair(2, 7), pair(6, 3), pair(4, 5)]);
    // 5 pages: the places after the last page stay empty and the document is not reordered
    assert_eq!(faces(5, &o), [vec![None, Some(1)], vec![Some(2), None], vec![None, Some(3)], pair(4, 5)]);
    let right = BookletOptions { binding: Binding::Right, ..o };
    assert_eq!(faces(4, &right), [pair(1, 4), pair(3, 2)]);
}

#[test]
fn perfect_bound_and_consecutive() {
    let perfect = BookletOptions { booklet_type: BookletType::PerfectBound, signature_size: 4, ..Default::default() };
    assert_eq!(faces(8, &perfect), [pair(4, 1), pair(2, 3), pair(8, 5), pair(6, 7)]);
    let two = BookletOptions { booklet_type: BookletType::Consecutive2, ..Default::default() };
    assert_eq!(faces(8, &two), [pair(1, 2), pair(3, 4), pair(5, 6), pair(7, 8)]);
    let three = BookletOptions { booklet_type: BookletType::Consecutive3, ..Default::default() };
    // Print Blank Printer Spreads: the third sheet is followed by an empty one
    assert_eq!(faces(8, &three), [vec![Some(1), Some(2), Some(3)], vec![Some(4), Some(5), Some(6)], vec![Some(7), Some(8), None], vec![None, None, None]]);
    let four = BookletOptions { booklet_type: BookletType::Consecutive4, ..Default::default() };
    assert_eq!(faces(8, &four), [vec![Some(1), Some(2), Some(3), Some(4)], vec![Some(5), Some(6), Some(7), Some(8)]]);
    assert!(plan(8, PAGE, &BookletOptions { signature_size: 5, booklet_type: BookletType::PerfectBound, ..Default::default() }).is_err());
    assert!(plan(0, PAGE, &perfect).is_err());
}

#[test]
fn right_to_left_binding_reads_every_sheet_from_the_right() {
    // measured (`goldens/booklet-rtl`, a right to left document of 8 pages)
    let right = |t: BookletType| BookletOptions { booklet_type: t, binding: Binding::Right, ..Default::default() };
    assert_eq!(faces(8, &right(BookletType::Saddle)), [pair(1, 8), pair(7, 2), pair(3, 6), pair(5, 4)]);
    assert_eq!(faces(8, &BookletOptions { signature_size: 4, ..right(BookletType::PerfectBound) }), [pair(1, 4), pair(3, 2), pair(5, 8), pair(7, 6)]);
    assert_eq!(faces(8, &right(BookletType::Consecutive2)), [pair(2, 1), pair(4, 3), pair(6, 5), pair(8, 7)]);
    // the empty place of the short sheet moves to the left; the empty sheet stays empty
    assert_eq!(faces(8, &right(BookletType::Consecutive3)), [vec![Some(3), Some(2), Some(1)], vec![Some(6), Some(5), Some(4)], vec![None, Some(8), Some(7)], vec![None, None, None]]);
}

#[test]
fn range_selection_is_planned_from_the_chosen_pages() {
    // +2-+5 is four pages: the plan is that of a 4-page booklet over [2, 3, 4, 5] -> (5, 2), (3, 4)
    let chosen = [2usize, 3, 4, 5];
    let sheets = plan(chosen.len(), PAGE, &BookletOptions::default()).unwrap();
    let seen: Vec<Vec<usize>> = sheets.iter().map(|s| s.cells.iter().filter_map(|c| c.page.map(|p| chosen[p])).collect()).collect();
    assert_eq!(seen, [vec![5, 2], vec![3, 4]]);
}

#[test]
fn sheet_size_and_places() {
    let sheets = plan(8, PAGE, &BookletOptions::default()).unwrap();
    assert!(sheets.iter().all(|s| s.size == [552.0, 372.0]));
    assert_eq!(sheets[0].cells.iter().map(|c| c.origin).collect::<Vec<_>>(), [[36.0, 36.0], [276.0, 36.0]]);
    // automatic margins off: top 10, bottom 20, left 30, right 40
    let margins = BookletOptions { auto_margins: false, margins: [10.0, 20.0, 30.0, 40.0], ..Default::default() };
    let s = &plan(8, PAGE, &margins).unwrap()[0];
    assert_eq!(s.size, [550.0, 330.0]);
    assert_eq!(s.cells.iter().map(|c| c.origin).collect::<Vec<_>>(), [[30.0, 10.0], [270.0, 10.0]]);
    // space between pages 12 pt, two-up consecutive
    let gap = BookletOptions { booklet_type: BookletType::Consecutive2, gap: 12.0, ..Default::default() };
    let s = &plan(8, PAGE, &gap).unwrap()[0];
    assert_eq!(s.size, [564.0, 372.0]);
    assert_eq!(s.cells[1].origin, [288.0, 36.0]);
    // three-up is 36 + 3 * 240 + 36 wide
    let three = BookletOptions { booklet_type: BookletType::Consecutive3, ..Default::default() };
    assert_eq!(plan(3, PAGE, &three).unwrap()[0].size[0], 792.0);
}

#[test]
fn creep_moves_the_outer_sheet_only() {
    // saddle stitch, 8 pages, creep +12: the outer sheet's (two faces) left page moves -6 and the right page +6, the inner sheet stays
    let creep = BookletOptions { creep: 12.0, ..Default::default() };
    let sheets = plan(8, PAGE, &creep).unwrap();
    let x = |i: usize| sheets[i].cells.iter().map(|c| c.origin[0]).collect::<Vec<_>>();
    assert_eq!(x(0), [30.0, 282.0]);
    assert_eq!(x(1), [30.0, 282.0]);
    assert_eq!(x(2), [36.0, 276.0]);
    assert_eq!(x(3), [36.0, 276.0]);
    // the clip stays at the place without creep
    assert_eq!(sheets[0].cells.iter().map(|c| c.slot[0]).collect::<Vec<_>>(), [36.0, 276.0]);
    assert!(sheets.iter().all(|s| s.size == [552.0, 372.0]));
}

/// Every sheet InDesign printed (positions of the drawn rectangles, read back from its PostScript files), against the plan.
#[test]
fn plan_equals_every_measured_booklet() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measurements/printing-booklet-raster.json");
    let measured: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let defaults = BookletOptions::default();
    let kinds = |t: BookletType| BookletOptions { booklet_type: t, ..BookletOptions::default() };
    // name -> (pages in the document, the page numbers chosen by the range, options)
    let cases: Vec<(&str, Vec<usize>, BookletOptions)> = vec![
        ("4", (1..=4).collect(), defaults.clone()),
        ("8", (1..=8).collect(), defaults.clone()),
        ("5", (1..=5).collect(), defaults.clone()),
        ("4-right", (1..=4).collect(), BookletOptions { binding: Binding::Right, ..defaults.clone() }),
        ("perfect8", (1..=8).collect(), BookletOptions { booklet_type: BookletType::PerfectBound, signature_size: 4, ..defaults.clone() }),
        ("consecutive2", (1..=8).collect(), kinds(BookletType::Consecutive2)),
        ("consecutive3", (1..=8).collect(), kinds(BookletType::Consecutive3)),
        ("consecutive4", (1..=8).collect(), kinds(BookletType::Consecutive4)),
        ("range", vec![2, 3, 4, 5], defaults.clone()),
        ("margins", (1..=8).collect(), BookletOptions { auto_margins: false, margins: [10.0, 20.0, 30.0, 40.0], ..defaults.clone() }),
        ("creep", (1..=8).collect(), BookletOptions { creep: 12.0, ..defaults.clone() }),
        ("gap", (1..=8).collect(), BookletOptions { booklet_type: BookletType::Consecutive2, gap: 12.0, ..defaults.clone() }),
    ];
    assert_eq!(cases.len(), measured.as_object().unwrap().len(), "a measured booklet without a case");
    for (name, chosen, options) in cases {
        let m = &measured[name];
        let sheets = plan(chosen.len(), PAGE, &options).unwrap();
        let size: Vec<f64> = m["size"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let rows = m["sheets"].as_array().unwrap();
        assert_eq!(sheets.len(), rows.len(), "{name}: sheet count");
        for (n, (sheet, row)) in sheets.iter().zip(rows).enumerate() {
            assert_eq!(sheet.size.to_vec(), size, "{name}: sheet size");
            for (cell, want) in sheet.cells.iter().zip(row.as_array().unwrap()) {
                assert_eq!(cell.page.map(|p| chosen[p] as u64), want["page"].as_u64(), "{name} sheet {n}: page");
                if let Some(bounds) = want["bounds"].as_array() {
                    // the probe draws its rectangle 20 pt in from the page's top left
                    let (x, y) = (bounds[0].as_f64().unwrap() - 20.0, bounds[1].as_f64().unwrap() - 20.0);
                    assert!((cell.origin[0] - x).abs() < 0.5 && (cell.origin[1] - y).abs() < 0.5, "{name} sheet {n}: origin {:?} vs {x},{y}", cell.origin);
                }
            }
        }
    }
}
