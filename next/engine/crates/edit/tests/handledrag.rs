//! Dragging a handle and turning from outside it (`resizeByHandle`, `rotateByDrag`): one engine command each, so the screen sends
//! only the drag. The pure rules are in `ops/handledrag.rs` (shared vectors with the screen); here the document changes.
use folio_edit::{Command, Editor, xml::Index};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn command(v: Value) -> Command { serde_json::from_value(v).unwrap() }

fn fixture() -> (Editor, String) {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic-create.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut page = String::new();
    for k in 0..zip.len() {
        let mut entry = zip.by_index(k).unwrap();
        if entry.is_dir() { continue; }
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        let name = entry.name().unwrap().into_owned();
        if name.starts_with("Spreads/") {
            let ix = Index::scan(&text).unwrap();
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&text, i) == "Page") { page = ix.value(&text, i, "Self").unwrap(); }
        }
        parts.insert(name, text);
    }
    (Editor::new(parts), page)
}

fn rect(ed: &mut Editor, page: &str, r: [f64; 4]) -> String {
    ed.apply("create", &[command(json!({"op":"createItem","page":page,"kind":"rectangle","rect":r}))]).unwrap().created[0].clone()
}

/// The corners of an item's path in spread coordinates (any transform), in the path's order.
fn corners(ed: &Editor, id: &str) -> Vec<(f64, f64)> {
    for t in ed.parts().values() {
        let Ok(ix) = Index::scan(t) else { continue };
        let Some(i) = ix.by_self(t, id) else { continue };
        let m: Vec<f64> = ix.attr(t, i, "ItemTransform").unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
        return (0..ix.nodes.len())
            .filter(|&k| ix.nodes[k].start > ix.nodes[i].start && ix.nodes[k].end <= ix.nodes[i].end && ix.local(t, k) == "PathPointType")
            .map(|k| {
                let v: Vec<f64> = ix.attr(t, k, "Anchor").unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
                (m[0] * v[0] + m[2] * v[1] + m[4], m[1] * v[0] + m[3] * v[1] + m[5])
            })
            .collect();
    }
    panic!("{id} not found")
}

fn bounds(ed: &Editor, id: &str) -> [f64; 4] {
    let c = corners(ed, id);
    let xs: Vec<f64> = c.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = c.iter().map(|p| p.1).collect();
    let lo = |v: &[f64]| v.iter().cloned().fold(f64::MAX, f64::min);
    let hi = |v: &[f64]| v.iter().cloned().fold(f64::MIN, f64::max);
    [lo(&xs), lo(&ys), hi(&xs), hi(&ys)]
}

fn near(a: [f64; 4], b: [f64; 4]) { assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6), "{a:?} != {b:?}"); }

fn drag(ids: &[&str], handle: [f64; 2], by: [f64; 2], keep: bool, center: bool) -> Command {
    command(json!({"op":"resizeByHandle","ids":ids,"handle":handle,"by":by,"keep_proportions":keep,"from_center":center}))
}

#[test]
fn a_plain_drag_moves_the_edges_under_the_handle() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    ed.apply("drag", &[drag(&[&r], [1.0, 1.0], [40.0, 10.0], false, false)]).unwrap();
    near(bounds(&ed, &r), [at[0], at[1], at[2] + 40.0, at[3] + 10.0]);
    ed.apply("drag", &[drag(&[&r], [0.0, 0.5], [-20.0, 77.0], false, false)]).unwrap();
    near(bounds(&ed, &r), [at[0] - 20.0, at[1], at[2] + 40.0, at[3] + 10.0]);
}

#[test]
fn shift_keeps_the_proportions_and_alt_works_from_the_middle() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    let ratio = (at[2] - at[0]) / (at[3] - at[1]);
    ed.apply("shift", &[drag(&[&r], [1.0, 1.0], [50.0, 3.0], true, false)]).unwrap();
    let b = bounds(&ed, &r);
    assert!(((b[2] - b[0]) / (b[3] - b[1]) - ratio).abs() < 1e-9, "{b:?}");
    assert!((b[0] - at[0]).abs() < 1e-9 && (b[1] - at[1]).abs() < 1e-9, "the top left corner stays: {b:?}");
    // back: both sides shrink by the same factor (InDesign takes the larger factor of the two, so the smaller shrink wins)
    ed.apply("back", &[drag(&[&r], [1.0, 1.0], [-50.0, -25.0], true, false)]).unwrap();
    near(bounds(&ed, &r), at);
    // one side grows while the other shrinks: the growing side wins (InDesign 2025), the box grows
    ed.apply("mixed", &[drag(&[&r], [1.0, 1.0], [40.0, -30.0], true, false)]).unwrap();
    let m = bounds(&ed, &r);
    assert!(((m[2] - m[0]) - 1.2 * (at[2] - at[0])).abs() < 1e-9 && ((m[3] - m[1]) - 1.2 * (at[3] - at[1])).abs() < 1e-9, "{m:?}");
    ed.apply("undo mixed", &[drag(&[&r], [1.0, 1.0], [-40.0, -20.0], true, false)]).unwrap();
    near(bounds(&ed, &r), at);
    ed.apply("alt", &[drag(&[&r], [1.0, 0.5], [10.0, 0.0], false, true)]).unwrap();
    near(bounds(&ed, &r), [at[0] - 10.0, at[1], at[2] + 10.0, at[3]]);
}

#[test]
fn a_turned_object_is_resized_in_its_own_frame() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    ed.apply("turn", &[command(json!({"op":"rotate","ids":[r],"by":90.0}))]).unwrap();
    // the path runs top left, bottom left, bottom right, top right: the first side is the height, the second the width
    let side = |c: &[(f64, f64)]| ((c[1].0 - c[0].0).hypot(c[1].1 - c[0].1), (c[2].0 - c[1].0).hypot(c[2].1 - c[1].1));
    let (h0, w0) = side(&corners(&ed, &r));
    // turned a quarter: the object's right edge is drawn along the page's vertical, so a drag down lengthens or shortens
    // its width by the drag, and the other side stays
    ed.apply("drag", &[drag(&[&r], [1.0, 0.5], [0.0, 30.0], false, false)]).unwrap();
    let (h1, w1) = side(&corners(&ed, &r));
    assert!((h1 - h0).abs() < 1e-9, "the other side is unchanged: {h0} {h1}");
    assert!(((w1 - w0).abs() - 30.0).abs() < 1e-9, "the dragged side changed by the drag along its axis: {w0} {w1}");
}

#[test]
fn several_objects_are_resized_together_in_the_spread_axes() {
    let (mut ed, page) = fixture();
    let a = rect(&mut ed, &page, [100.0, 100.0, 200.0, 150.0]);
    let b = rect(&mut ed, &page, [220.0, 130.0, 300.0, 200.0]);
    let (ba, bb) = (bounds(&ed, &a), bounds(&ed, &b));
    let u = [ba[0].min(bb[0]), ba[1].min(bb[1]), ba[2].max(bb[2]), ba[3].max(bb[3])];
    ed.apply("drag", &[drag(&[&a, &b], [1.0, 1.0], [u[2] - u[0], 0.0], false, false)]).unwrap();
    let (na, nb) = (bounds(&ed, &a), bounds(&ed, &b));
    // the box doubled in width about its left edge, so each object's left edge and width double
    assert!((na[0] - u[0]).abs() < 1e-6 && ((na[2] - na[0]) - 2.0 * (ba[2] - ba[0])).abs() < 1e-6, "{na:?}");
    assert!((nb[0] - (u[0] + 2.0 * (bb[0] - u[0]))).abs() < 1e-6, "{nb:?}");
    assert!(((na[3] - na[1]) - (ba[3] - ba[1])).abs() < 1e-6, "heights stay");
}

#[test]
fn a_drag_is_one_undoable_step_and_a_bad_one_changes_nothing() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let before = ed.parts().clone();
    ed.apply("drag", &[drag(&[&r], [1.0, 1.0], [40.0, 10.0], true, true)]).unwrap();
    assert_ne!(&before, ed.parts());
    ed.undo().unwrap();
    assert_eq!(&before, ed.parts());
    for bad in [
        drag(&[&r], [0.5, 0.5], [1.0, 1.0], false, false),
        drag(&[&r], [0.3, 1.0], [1.0, 1.0], false, false),
        drag(&[], [1.0, 1.0], [1.0, 1.0], false, false),
        drag(&["nope"], [1.0, 1.0], [1.0, 1.0], false, false),
    ] {
        assert!(ed.apply("bad", &[bad]).is_err());
        assert_eq!(&before, ed.parts());
    }
}

#[test]
fn a_handle_dragged_past_the_opposite_edge_turns_the_object_inside_out() {
    // InDesign 2025 (real mouse, docinput-c3): the box lies on the other side of the staying edge, as far from it as the pointer, and the
    // object is mirrored about that axis
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    let tl = corners(&ed, &r)[0];
    ed.apply("flip", &[drag(&[&r], [1.0, 1.0], [-250.0, -150.0], false, false)]).unwrap();
    near(bounds(&ed, &r), [at[0] - 50.0, at[1] - 50.0, at[0], at[1]]);
    let c = corners(&ed, &r);
    assert!((c[0].0 - tl.0).abs() < 1e-9 && (c[0].1 - tl.1).abs() < 1e-9, "the staying corner stays: {c:?}");
    assert!((c[2].0 - (tl.0 - 50.0)).abs() < 1e-9 && (c[2].1 - (tl.1 - 50.0)).abs() < 1e-9, "the dragged corner is across it: {c:?}");
    // the way back: dragging the same handle forward again turns it right side out
    ed.apply("back", &[drag(&[&r], [1.0, 1.0], [250.0, 150.0], false, false)]).unwrap();
    near(bounds(&ed, &r), at);
    let c = corners(&ed, &r);
    assert!((c[0].0 - tl.0).abs() < 1e-9 && (c[2].0 - at[2]).abs() < 1e-9 && (c[2].1 - at[3]).abs() < 1e-9, "upright again: {c:?}");
    // one axis only: the right handle past the left edge
    ed.apply("flip x", &[drag(&[&r], [1.0, 0.5], [-260.0, 99.0], false, false)]).unwrap();
    near(bounds(&ed, &r), [at[0] - 60.0, at[1], at[0], at[3]]);
    let c = corners(&ed, &r);
    assert!((c[0].0 - at[0]).abs() < 1e-9 && (c[0].1 - at[1]).abs() < 1e-9, "top left stays {c:?}");
    assert!((c[3].0 - (at[0] - 60.0)).abs() < 1e-9 && (c[3].1 - at[1]).abs() < 1e-9, "top right is left of it {c:?}");
}

#[test]
fn alt_turns_the_object_inside_out_when_the_pointer_passes_the_middle() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    ed.apply("alt flip", &[drag(&[&r], [1.0, 0.5], [-130.0, 0.0], false, true)]).unwrap();
    // 200 - 2 * 130 = -60: a 60 wide box about the same middle, mirrored
    let mid = (at[0] + at[2]) / 2.0;
    near(bounds(&ed, &r), [mid - 30.0, at[1], mid + 30.0, at[3]]);
    let c = corners(&ed, &r);
    assert!((c[0].0 - (mid + 30.0)).abs() < 1e-9 && (c[3].0 - (mid - 30.0)).abs() < 1e-9, "left and right swapped: {c:?}");
}

#[test]
fn a_turned_object_is_turned_inside_out_in_its_own_axes() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    ed.apply("turn", &[command(json!({"op":"rotate","ids":[r],"by":90.0}))]).unwrap();
    let c0 = corners(&ed, &r);
    // a quarter turn: the object's x axis runs up the page; pulling its right handle down by 260 passes its left edge by 60
    ed.apply("flip", &[drag(&[&r], [1.0, 0.5], [0.0, 260.0], false, false)]).unwrap();
    let c1 = corners(&ed, &r);
    let len = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
    assert!((c1[0].0 - c0[0].0).abs() < 1e-9 && (c1[0].1 - c0[0].1).abs() < 1e-9, "the staying edge stays: {c0:?} {c1:?}");
    assert!((len(c1[0], c1[3]) - 60.0).abs() < 1e-9 && (len(c1[0], c1[1]) - len(c0[0], c0[1])).abs() < 1e-9, "{c1:?}");
    let (before, after) = ((c0[3].0 - c0[0].0, c0[3].1 - c0[0].1), (c1[3].0 - c1[0].0, c1[3].1 - c1[0].1));
    assert!(before.0 * after.0 + before.1 * after.1 < 0.0, "the right edge is now on the other side of the staying edge: {before:?} {after:?}");
}

#[test]
fn several_objects_are_mirrored_about_the_staying_edge() {
    let (mut ed, page) = fixture();
    let a = rect(&mut ed, &page, [100.0, 100.0, 200.0, 150.0]);
    let b = rect(&mut ed, &page, [220.0, 130.0, 300.0, 200.0]);
    let (ba, bb) = (bounds(&ed, &a), bounds(&ed, &b));
    let u = [ba[0].min(bb[0]), ba[1].min(bb[1]), ba[2].max(bb[2]), ba[3].max(bb[3])];
    let w = u[2] - u[0];
    // the box's right handle dragged to half its width left of its own left edge: the scale is -0.5 about the left edge
    ed.apply("flip", &[drag(&[&a, &b], [1.0, 0.5], [-1.5 * w, 0.0], false, false)]).unwrap();
    let (na, nb) = (bounds(&ed, &a), bounds(&ed, &b));
    let f = -0.5;
    let map = |lo: f64, hi: f64| (u[0] + f * (hi - u[0]), u[0] + f * (lo - u[0]));
    let (ax0, ax1) = map(ba[0], ba[2]);
    let (bx0, bx1) = map(bb[0], bb[2]);
    near(na, [ax0, ba[1], ax1, ba[3]]);
    near(nb, [bx0, bb[1], bx1, bb[3]]);
    // the first one was at the left: it is now the one nearest the staying edge, on its far side
    assert!(na[2] <= u[0] + 1e-9 && nb[2] <= na[0] + 1e-9, "{na:?} {nb:?}");
}

#[test]
fn a_diagonal_line_is_resized_and_turned_inside_out_by_its_box() {
    // InDesign 2025: a line's box changes as a frame's does, and past the opposite corner the line is mirrored
    let (mut ed, page) = fixture();
    let created = ed.apply("line", &[command(json!({"op":"createPath","page":page,"points":[{"anchor":[100.0,100.0]},{"anchor":[300.0,200.0]}],"closed":false}))]).unwrap().created;
    let l = created[0].clone();
    let at = bounds(&ed, &l);
    ed.apply("grow", &[drag(&[&l], [1.0, 1.0], [40.0, 10.0], false, false)]).unwrap();
    near(bounds(&ed, &l), [at[0], at[1], at[2] + 40.0, at[3] + 10.0]);
    ed.apply("flip", &[drag(&[&l], [1.0, 1.0], [-290.0, -160.0], false, false)]).unwrap();
    let b = bounds(&ed, &l);
    near(b, [at[0] - 50.0, at[1] - 50.0, at[0], at[1]]);
    let c = corners(&ed, &l);
    assert!((c[0].0 - at[0]).abs() < 1e-9 && (c[1].0 - (at[0] - 50.0)).abs() < 1e-9, "the line keeps its end on the staying corner and the other end is across it: {c:?}");
}

fn zero_point(ed: &Editor) -> Option<String> {
    let t = &ed.parts()["designmap.xml"];
    let ix = Index::scan(t).unwrap();
    let i = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Document")?;
    ix.attr(t, i, "ZeroPoint").map(str::to_owned)
}

#[test]
fn the_zero_point_is_one_document_attribute_and_nothing_else_moves() {
    // InDesign 2025: IDML keeps `Document/@ZeroPoint` and every item and guide keeps its own coordinates
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let before = ed.parts().clone();
    let at = bounds(&ed, &r);
    ed.apply("zero", &[command(json!({"op":"setViewPref","key":"zeroPoint","value":[99.0, 201.5]}))]).unwrap();
    assert_eq!(zero_point(&ed).as_deref(), Some("99 201.5"));
    near(bounds(&ed, &r), at);
    let changed: Vec<_> = ed.parts().iter().filter(|(k, v)| before.get(*k) != Some(*v)).map(|(k, _)| k.clone()).collect();
    assert_eq!(changed, vec!["designmap.xml".to_string()], "only the designmap changes");
    // back to the origin (a double click on the ruler corner)
    ed.apply("reset", &[command(json!({"op":"setViewPref","key":"zeroPoint","value":[0.0, 0.0]}))]).unwrap();
    assert_eq!(zero_point(&ed).as_deref(), Some("0 0"));
    ed.undo().unwrap();
    assert_eq!(zero_point(&ed).as_deref(), Some("99 201.5"));
    for bad in [json!([1.0]), json!([1.0, 2.0, 3.0]), json!("1 2"), json!([1.0, null]), json!(null)] {
        assert!(ed.apply("bad", &[command(json!({"op":"setViewPref","key":"zeroPoint","value":bad}))]).is_err());
        assert_eq!(zero_point(&ed).as_deref(), Some("99 201.5"));
    }
}

fn turn_drag(ids: &[&str], from: [f64; 2], to: [f64; 2], constrain: bool, anchor: [f64; 2]) -> Command {
    command(json!({"op":"rotateByDrag","ids":ids,"from":from,"to":to,"constrain":constrain,"anchor":anchor}))
}

#[test]
fn turning_by_a_drag_goes_about_the_reference_point() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    let c = [(at[0] + at[2]) / 2.0, (at[1] + at[3]) / 2.0];
    // from the right of the middle to above it: a quarter turn counter-clockwise, the middle stays
    ed.apply("turn", &[turn_drag(&[&r], [c[0] + 50.0, c[1]], [c[0], c[1] - 50.0], false, [0.5, 0.5])]).unwrap();
    let (hw, hh) = ((at[3] - at[1]) / 2.0, (at[2] - at[0]) / 2.0);
    near(bounds(&ed, &r), [c[0] - hw, c[1] - hh, c[0] + hw, c[1] + hh]);
    // about the top left corner of the selection instead
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    ed.apply("turn", &[turn_drag(&[&r], [at[0] + 50.0, at[1]], [at[0], at[1] - 50.0], false, [0.0, 0.0])]).unwrap();
    near(bounds(&ed, &r), [at[0], at[1] - (at[2] - at[0]), at[0] + (at[3] - at[1]), at[1]]);
}

#[test]
fn shift_turns_in_steps_of_45_degrees() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    let c = [(at[0] + at[2]) / 2.0, (at[1] + at[3]) / 2.0];
    // a drag of about 6 degrees with Shift is no turn at all
    let before = ed.parts().clone();
    ed.apply("turn", &[turn_drag(&[&r], [c[0] + 100.0, c[1]], [c[0] + 100.0, c[1] - 10.0], true, [0.5, 0.5])]).unwrap();
    near(bounds(&ed, &r), at);
    assert_eq!(&before, ed.parts(), "no change when the angle rounds to nothing");
    // about 40 degrees with Shift is 45
    ed.apply("turn", &[turn_drag(&[&r], [c[0] + 100.0, c[1]], [c[0] + 100.0, c[1] - 84.0], true, [0.5, 0.5])]).unwrap();
    let b = bounds(&ed, &r);
    let half = (at[2] - at[0] + at[3] - at[1]) / 2.0 * std::f64::consts::FRAC_1_SQRT_2;
    near(b, [c[0] - half, c[1] - half, c[0] + half, c[1] + half]);
}

/// The turn of a rectangle's first edge, in degrees modulo 90 (counter-clockwise on the screen, y down).
fn edge_angle(ed: &Editor, id: &str) -> f64 {
    let c = corners(ed, id);
    (-(c[1].1 - c[0].1)).atan2(c[1].0 - c[0].0).to_degrees().rem_euclid(90.0)
}

#[test]
fn shift_rounds_the_pointer_turn_not_the_objects_angle() {
    // InDesign 2025 (real mouse, docinput-c2): a frame turned 20 degrees, dragged 28 degrees with Shift, ends at 20 + 45
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    let c = [(at[0] + at[2]) / 2.0, (at[1] + at[3]) / 2.0];
    let on = |deg: f64| [c[0] + 100.0 * deg.to_radians().cos(), c[1] - 100.0 * deg.to_radians().sin()];
    ed.apply("turn", &[turn_drag(&[&r], on(0.0), on(20.0), false, [0.5, 0.5])]).unwrap();
    assert!((edge_angle(&ed, &r) - 20.0).abs() < 1e-6, "{}", edge_angle(&ed, &r));
    ed.apply("turn", &[turn_drag(&[&r], on(0.0), on(28.0), true, [0.5, 0.5])]).unwrap();
    assert!((edge_angle(&ed, &r) - 65.0).abs() < 1e-6, "{}", edge_angle(&ed, &r));
    // 13 degrees with Shift changes nothing
    let before = ed.parts().clone();
    ed.apply("turn", &[turn_drag(&[&r], on(0.0), on(13.0), true, [0.5, 0.5])]).unwrap();
    assert_eq!(&before, ed.parts());
}

#[test]
fn several_objects_turn_about_the_middle_of_their_box() {
    // InDesign 2025: two selected frames turned by a drag go about the middle of the box that holds both
    let (mut ed, page) = fixture();
    let a = rect(&mut ed, &page, [100.0, 100.0, 200.0, 160.0]);
    let b = rect(&mut ed, &page, [260.0, 200.0, 340.0, 240.0]);
    let (ba, bb) = (bounds(&ed, &a), bounds(&ed, &b));
    let u = [ba[0].min(bb[0]), ba[1].min(bb[1]), ba[2].max(bb[2]), ba[3].max(bb[3])];
    let c = [(u[0] + u[2]) / 2.0, (u[1] + u[3]) / 2.0];
    ed.apply("turn", &[turn_drag(&[&a, &b], [c[0] + 100.0, c[1]], [c[0], c[1] - 100.0], false, [0.5, 0.5])]).unwrap();
    // a quarter turn counter-clockwise about c: a point (x, y) goes to (c.x + (y - c.y), c.y - (x - c.x))
    let (na, nb) = (bounds(&ed, &a), bounds(&ed, &b));
    let mid = |v: [f64; 4]| [(v[0] + v[2]) / 2.0, (v[1] + v[3]) / 2.0];
    let want = |v: [f64; 4]| { let m = mid(v); [c[0] + (m[1] - c[1]), c[1] - (m[0] - c[0])] };
    for (now, was) in [(na, ba), (nb, bb)] {
        let (m, w) = (mid(now), want(was));
        assert!((m[0] - w[0]).abs() < 1e-6 && (m[1] - w[1]).abs() < 1e-6, "{m:?} != {w:?}");
    }
}

#[test]
fn a_turn_with_the_pointer_on_the_reference_point_is_refused() {
    let (mut ed, page) = fixture();
    let r = rect(&mut ed, &page, [100.0, 100.0, 300.0, 200.0]);
    let at = bounds(&ed, &r);
    let before = ed.parts().clone();
    assert!(ed.apply("turn", &[turn_drag(&[&r], [at[0], at[1]], [at[0] + 5.0, at[1] + 5.0], false, [0.0, 0.0])]).is_err());
    assert_eq!(&before, ed.parts());
}

// ---- the pure rules, against the vectors the screen's twin (app/renderer/src/ui/handledrag.ts) is tested with ----

fn vectors() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/renderer/src/ui/handledrag.vectors.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn nums<const N: usize>(v: &Value) -> [f64; N] {
    let mut out = [0.0; N];
    for (k, x) in v.as_array().unwrap().iter().enumerate() {
        out[k] = x.as_f64().unwrap();
    }
    out
}

#[test]
fn handle_rect_agrees_with_the_shared_vectors() {
    for c in vectors()["handleRect"].as_array().unwrap() {
        let (got, flip) = folio_edit::handle_box(nums(&c["box"]), nums(&c["handle"]), nums(&c["by"]), c["keep"].as_bool().unwrap(), c["center"].as_bool().unwrap());
        let want: [f64; 4] = nums(&c["want"]);
        assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9), "{}: {got:?} != {want:?}", c["name"]);
        let want_flip: Vec<bool> = c["flip"].as_array().unwrap().iter().map(|b| b.as_bool().unwrap()).collect();
        assert_eq!(flip.to_vec(), want_flip, "{}: flip", c["name"]);
    }
}

#[test]
fn turn_angle_agrees_with_the_shared_vectors() {
    for c in vectors()["turnAngle"].as_array().unwrap() {
        let got = folio_edit::turn_angle(nums(&c["center"]), nums(&c["from"]), nums(&c["to"]), c["constrain"].as_bool().unwrap());
        match c["want"].as_f64() {
            None => assert!(got.is_none(), "{}", c["name"]),
            Some(want) => assert!((got.unwrap() - want).abs() < 1e-9, "{}: {got:?} != {want}", c["name"]),
        }
    }
}

#[test]
fn a_handle_drag_never_leaves_a_box_smaller_than_the_minimum() {
    for handle in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0], [0.5, 0.0], [1.0, 0.5]] {
        for (keep, center) in [(false, false), (true, false), (false, true), (true, true)] {
            let r = folio_edit::handle_rect([0.0, 0.0, 40.0, 30.0], handle, [-900.0, -900.0], keep, center);
            assert!(r[2] - r[0] >= folio_edit::HANDLE_MIN_SIZE - 1e-9 && r[3] - r[1] >= folio_edit::HANDLE_MIN_SIZE - 1e-9, "{handle:?} {keep} {center}: {r:?}");
        }
    }
}
