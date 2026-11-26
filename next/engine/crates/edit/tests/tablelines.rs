//! Dragging a table's row and column lines (`dragTableLine`): the rules measured in InDesign 2025 with the real mouse (docinput-c3,
//! rules/docinput-c3.ko.md section 8) on the tables fixture: a plain drag changes the column or row before the line, Shift on an inner line
//! gives what one takes to the next one, Shift on the outer edge scales all of them, nothing gets smaller than 3 pt, undo takes it back.
use folio_edit::{Command, Editor, xml::Index};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn command(v: Value) -> Command { serde_json::from_value(v).unwrap() }

fn fixture() -> (Editor, String) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tables.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() { continue; }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    let table = parts.values().find_map(|t| {
        let ix = Index::scan(t).unwrap();
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Table").and_then(|i| ix.attr(t, i, "Self").map(str::to_owned))
    });
    (Editor::new(parts), table.unwrap())
}

/// The sizes of the table's columns or rows, as written.
fn sizes(e: &Editor, table: &str, columns: bool) -> Vec<f64> {
    let (tag, attr) = if columns { ("Column", "SingleColumnWidth") } else { ("Row", "SingleRowHeight") };
    e.parts().values().find_map(|t| {
        let ix = Index::scan(t).ok()?;
        let i = ix.by_self(t, table)?;
        Some(ix.children(i).iter().filter(|&&c| ix.local(t, c) == tag).map(|&c| ix.attr(t, c, attr).unwrap().parse::<f64>().unwrap()).collect::<Vec<_>>())
    }).unwrap()
}

fn drag(table: &str, columns: bool, line: usize, by: f64, shift: bool) -> Command {
    command(json!({"op":"dragTableLine","table":table,"columns":columns,"line":line,"by":by,"shift":shift}))
}

/// Alt held (`insert`).
fn drag_alt(table: &str, columns: bool, line: usize, by: f64) -> Command {
    command(json!({"op":"dragTableLine","table":table,"columns":columns,"line":line,"by":by,"shift":false,"insert":true}))
}

fn near(a: &[f64], b: &[f64]) { assert!(a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6), "{a:?} != {b:?}"); }

#[test]
fn a_plain_drag_changes_only_the_column_or_row_before_the_line() {
    let (mut e, t) = fixture();
    let (w0, h0) = (sizes(&e, &t, true), sizes(&e, &t, false));
    assert!(w0.len() >= 3 && h0.len() >= 3, "the fixture table is too small: {w0:?} {h0:?}");
    e.apply("col", &[drag(&t, true, 1, 18.75, false)]).unwrap();
    let mut want = w0.clone();
    want[0] += 18.75;
    near(&sizes(&e, &t, true), &want);
    near(&sizes(&e, &t, false), &h0);
    // InDesign 2025: dragged back by 21.75 the column was 21.75 narrower; the others stayed
    e.apply("col back", &[drag(&t, true, 1, -21.75, false)]).unwrap();
    want[0] -= 21.75;
    near(&sizes(&e, &t, true), &want);
    // the outer edge: the last column
    let n = w0.len();
    e.apply("edge", &[drag(&t, true, n, 17.75, false)]).unwrap();
    want[n - 1] += 17.75;
    near(&sizes(&e, &t, true), &want);
    // a row line: the row before it by 10, the others stay
    e.apply("row", &[drag(&t, false, 1, 10.0, false)]).unwrap();
    let mut rows = h0.clone();
    rows[0] += 10.0;
    near(&sizes(&e, &t, false), &rows);
}

#[test]
fn shift_on_an_inner_line_gives_to_the_next_one_and_on_the_outer_edge_scales_all() {
    let (mut e, t) = fixture();
    let w0 = sizes(&e, &t, true);
    e.apply("shift", &[drag(&t, true, 1, 18.75, true)]).unwrap();
    let mut want = w0.clone();
    want[0] += 18.75;
    want[1] -= 18.75;
    near(&sizes(&e, &t, true), &want);
    let h0 = sizes(&e, &t, false);
    // (the fixture's first row is a header row: Shift on the line under the header rows does nothing, so the second line)
    e.apply("shift row", &[drag(&t, false, 2, 10.0, true)]).unwrap();
    let mut rows = h0.clone();
    rows[1] += 10.0;
    rows[2] -= 10.0;
    near(&sizes(&e, &t, false), &rows);
    // the outer edge: all columns by the same factor (InDesign 2025: 3 x 100 + 17.8 gave 3 x 105.92)
    let (mut e, t) = fixture();
    let w0 = sizes(&e, &t, true);
    let total: f64 = w0.iter().sum();
    e.apply("scale", &[drag(&t, true, w0.len(), 17.8, true)]).unwrap();
    let f = (total + 17.8) / total;
    near(&sizes(&e, &t, true), &w0.iter().map(|v| v * f).collect::<Vec<_>>());
}

#[test]
fn a_column_stays_at_least_3_pt_wide_and_bad_lines_change_nothing() {
    let (mut e, t) = fixture();
    let w0 = sizes(&e, &t, true);
    e.apply("far", &[drag(&t, true, 1, -10000.0, false)]).unwrap();
    let mut want = w0.clone();
    want[0] = 3.0;
    near(&sizes(&e, &t, true), &want);
    e.undo().unwrap();
    near(&sizes(&e, &t, true), &w0);
    let before = e.parts().clone();
    for bad in [drag(&t, true, 0, 1.0, false), drag(&t, true, w0.len() + 1, 1.0, false), drag("nope", true, 1, 1.0, false)] {
        assert!(e.apply("bad", &[bad]).is_err());
        assert_eq!(&before, e.parts());
    }
}

#[test]
fn alt_on_a_column_line_puts_a_new_column_in_after_the_line() {
    // InDesign 2025 (tabledrag-c1): Alt on the inner line 1 dragged 20 gave [100, 18.75, 100, 100] (the dragged width), on the right edge a new last column.
    let (mut e, t) = fixture();
    let w0 = sizes(&e, &t, true);
    e.apply("alt", &[drag_alt(&t, true, 1, 18.75)]).unwrap();
    let mut want = w0.clone();
    want.insert(1, 18.75);
    near(&sizes(&e, &t, true), &want);
    let n = want.len();
    e.apply("alt edge", &[drag_alt(&t, true, n, 17.75)]).unwrap();
    want.push(17.75);
    near(&sizes(&e, &t, true), &want);
    e.undo().unwrap();
    e.undo().unwrap();
    near(&sizes(&e, &t, true), &w0);
}

#[test]
fn alt_on_the_bottom_edge_adds_rows_as_high_as_the_last_one() {
    // InDesign 2025: 3 rows of 30; Alt dragged 8.75 / 23 / 48.5 / 68.75 / 98.5 added 1 / 1 / 2 / 2 / 3 rows of 30 (round(distance / 30), at least one).
    // (distance / height of the last row, rows added)
    for (ratio, added) in [(0.29, 1), (0.77, 1), (1.62, 2), (2.29, 2), (3.28, 3)] {
        let (mut e, t) = fixture();
        let h0 = sizes(&e, &t, false);
        let n = h0.len();
        e.apply("alt", &[drag_alt(&t, false, n, ratio * h0[n - 1])]).unwrap();
        let mut want = h0.clone();
        for _ in 0..added {
            want.push(*h0.last().unwrap());
        }
        near(&sizes(&e, &t, false), &want);
    }
}

#[test]
fn alt_on_an_inner_row_line_is_a_plain_drag() {
    let (mut e, t) = fixture();
    let h0 = sizes(&e, &t, false);
    e.apply("alt row", &[drag_alt(&t, false, 1, 8.5)]).unwrap();
    let mut want = h0.clone();
    want[0] += 8.5;
    near(&sizes(&e, &t, false), &want);
}

#[test]
fn shift_on_the_line_under_the_header_rows_does_nothing_and_a_shown_size_is_the_old_size() {
    let (mut e, t) = fixture();
    e.apply("header", &[command(json!({"op":"setHeaderRows","table":t,"header":1}))]).unwrap();
    let h0 = sizes(&e, &t, false);
    e.apply("shift", &[drag(&t, false, 1, 10.0, true)]).unwrap();
    near(&sizes(&e, &t, false), &h0);
    // a plain drag there works as anywhere
    e.apply("plain", &[drag(&t, false, 1, 10.0, false)]).unwrap();
    let mut want = h0.clone();
    want[0] += 10.0;
    near(&sizes(&e, &t, false), &want);
    // a row that shows 12.72 pt (grown by its text) dragged by 10 is 22.72
    let (mut e, t) = fixture();
    e.apply("shown", &[command(json!({"op":"dragTableLine","table":t,"columns":false,"line":2,"by":10.0,"shift":false,"size":12.72}))]).unwrap();
    let h = sizes(&e, &t, false);
    assert!((h[1] - 22.72).abs() < 1e-6, "{h:?}");
}

/// InDesign 2025 (docdrag-c1, d3_tablemin): rows 30 / 60 / 30 with three text lines in the middle one (it needs 41.52 pt). Shift on the line above it dragged
/// down 40 left the middle row at 41.52 and gave the row above only 18.48; Shift on the line below it dragged up 40 did the same to the row below (30 -> 48.48).
/// Here the fixture's rows 1..3 stand in and the row in the middle is held at half its height.
/// Without text (`min` absent) the row goes down to the 3 pt floor as before.
#[test]
fn shift_stops_a_row_at_the_height_its_text_needs() {
    let (mut e, t) = fixture();
    let h0 = sizes(&e, &t, false);
    assert!(h0.len() >= 4, "{h0:?}");
    // the fixture's first row is a header row: use the lines 2 and 3 (rows 1, 2 and 3), stand-ins for the measured 30 / 60 / 30
    let (a, b, c) = (h0[1], h0[2], h0[3]);
    let need = b / 2.0;   // stand-in for the measured 41.52 of 60
    let by = |line: usize, by: f64| command(json!({"op":"dragTableLine","table":t,"columns":false,"line":line,"by":by,"shift":true,"min_before":null,"min_after":need}));
    // down: the row after the line may only give b - need
    e.apply("down", &[by(2, 1000.0)]).unwrap();
    near(&sizes(&e, &t, false)[1..4], &[a + (b - need), need, c]);
    e.undo().unwrap();
    // up on the line below it: the row before the line is the middle one (min_before), the row after it takes what it gives
    let up = command(json!({"op":"dragTableLine","table":t,"columns":false,"line":3,"by":-1000.0,"shift":true,"min_before":need,"min_after":null}));
    e.apply("up", &[up]).unwrap();
    near(&sizes(&e, &t, false)[1..4], &[a, need, c + (b - need)]);
    e.undo().unwrap();
    // no minimum given: down to the 3 pt floor, as measured for rows without text (12.72 in InDesign is its empty line)
    e.apply("free", &[drag(&t, false, 2, 1000.0, true)]).unwrap();
    near(&sizes(&e, &t, false)[1..4], &[a + (b - 3.0), 3.0, c]);
}
