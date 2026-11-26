//! Adding, deleting and converting the points of a path (Add / Delete Anchor Point and Convert Direction Point tools):
//! the new point keeps the curve's shape, the others keep their handles. Delete and plain are a script's `remove()` and
//! `pointType = PLAIN` (measured: oracle basic-paths); adding is exact subdivision, checked here against the curve.
use folio_edit::{Command, Editor, PathPt, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

type P = (f64, f64);

fn fixture() -> (Editor, String) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut page = String::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        if name.starts_with("Spreads/") && page.is_empty() {
            let ix = Index::scan(&t).unwrap();
            page = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Page").and_then(|i| ix.attr(&t, i, "Self")).unwrap().into();
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), page)
}

fn pt(a: P, l: Option<P>, r: Option<P>) -> PathPt {
    PathPt { anchor: [a.0, a.1], left: l.map(|v| [v.0, v.1]), right: r.map(|v| [v.0, v.1]) }
}

/// The path points of an item as `(anchor, left, right)`.
fn points(e: &Editor, id: &str) -> Vec<(P, P, P)> {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) else { continue };
        let pair = |p: usize, k: &str| {
            let v: Vec<f64> = ix.attr(t, p, k).unwrap().split_whitespace().map(|x| x.parse().unwrap()).collect();
            (v[0], v[1])
        };
        return ix.own_descendants(t, i, "PathPointType").into_iter().map(|p| (pair(p, "Anchor"), pair(p, "LeftDirection"), pair(p, "RightDirection"))).collect();
    }
    panic!("no item {id}");
}

fn made(e: &mut Editor, page: &str, closed: bool, pts: Vec<PathPt>) -> String {
    let o = e.apply("path", &[Command::CreatePath { page: page.into(), points: pts, closed, layer: None }]).unwrap();
    o.created[0].clone()
}

fn bezier(a: P, b: P, c: P, d: P, t: f64) -> P {
    let u = 1.0 - t;
    let f = |p: f64, q: f64, r: f64, s: f64| u * u * u * p + 3.0 * u * u * t * q + 3.0 * u * t * t * r + t * t * t * s;
    (f(a.0, b.0, c.0, d.0), f(a.1, b.1, c.1, d.1))
}

fn near(a: P, b: P) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

/// The page's coordinates are the item's own plus a shift (the new item's transform): compare differences.
fn closed_curve() -> Vec<PathPt> {
    vec![
        pt((100., 100.), None, Some((160., 40.))),
        pt((300., 100.), Some((240., 40.)), Some((360., 160.))),
        pt((300., 200.), None, None),
        pt((100., 200.), None, None),
    ]
}

#[test]
fn a_point_added_to_a_curve_leaves_the_curve_as_it_was() {
    let (mut e, page) = fixture();
    let id = made(&mut e, &page, true, closed_curve());
    let before = points(&e, &id);
    for t in [0.25, 0.5, 0.8] {
        let (mut e2, page2) = fixture();
        let id2 = made(&mut e2, &page2, true, closed_curve());
        e2.apply("add", &[Command::AddPathPoint { id: id2.clone(), path: 0, segment: 0, t }]).unwrap();
        let after = points(&e2, &id2);
        assert_eq!(after.len(), 5);
        // the curve through points 0, new, 1 is the old curve from 0 to 1
        let (a, n, b) = (after[0], after[1], after[2]);
        assert!(near(n.0, bezier(before[0].0, before[0].2, before[1].1, before[1].0, t)), "the new point is on the curve at t");
        for u in [0.0, 0.3, 0.7, 1.0] {
            assert!(near(bezier(a.0, a.2, n.1, n.0, u), bezier(before[0].0, before[0].2, before[1].1, before[1].0, t * u)), "first half at {u}");
            assert!(near(bezier(n.0, n.2, b.1, b.0, u), bezier(before[0].0, before[0].2, before[1].1, before[1].0, t + (1.0 - t) * u)), "second half at {u}");
        }
        // the other points are untouched, and the new one is smooth: its handles in line, on opposite sides
        assert_eq!((after[3], after[4]), (before[2], before[3]));
        let (l, r) = ((n.1 .0 - n.0 .0, n.1 .1 - n.0 .1), (n.2 .0 - n.0 .0, n.2 .1 - n.0 .1));
        assert!((l.0 * r.1 - l.1 * r.0).abs() < 1e-9 && l.0 * r.0 + l.1 * r.1 < 0.0);
    }
}

#[test]
fn a_point_added_to_a_straight_segment_is_a_corner_point_and_the_others_do_not_move() {
    let (mut e, page) = fixture();
    let id = made(&mut e, &page, true, closed_curve());
    let before = points(&e, &id);
    e.apply("add", &[Command::AddPathPoint { id: id.clone(), path: 0, segment: 2, t: 0.5 }]).unwrap();
    let after = points(&e, &id);
    assert_eq!(after.len(), 5);
    assert!(near(after[3].0, ((before[2].0 .0 + before[3].0 .0) / 2.0, (before[2].0 .1 + before[3].0 .1) / 2.0)));
    assert!(near(after[3].1, after[3].0) && near(after[3].2, after[3].0), "no handles");
    assert_eq!((after[0], after[1], after[2], after[4]), (before[0], before[1], before[2], before[3]));
    // the closing segment (last point back to the first): the new point goes last
    let n = points(&e, &id).len() - 1;
    e.apply("add", &[Command::AddPathPoint { id: id.clone(), path: 0, segment: n, t: 0.5 }]).unwrap();
    let wrapped = points(&e, &id);
    assert_eq!(wrapped.len(), 6);
    assert!(near(wrapped[5].0, ((after[4].0 .0 + after[0].0 .0) / 2.0, (after[4].0 .1 + after[0].0 .1) / 2.0)));
}

#[test]
fn a_point_deleted_leaves_its_neighbours_their_handles() {
    let (mut e, page) = fixture();
    let id = made(&mut e, &page, true, closed_curve());
    let before = points(&e, &id);
    e.apply("delete", &[Command::DeletePathPoint { id: id.clone(), path: 0, index: 1 }]).unwrap();
    assert_eq!(points(&e, &id), vec![before[0], before[2], before[3]]);
    e.apply("delete", &[Command::DeletePathPoint { id: id.clone(), path: 0, index: 0 }]).unwrap();
    assert_eq!(points(&e, &id), vec![before[2], before[3]]);
    // two points are the least
    assert!(e.apply("delete", &[Command::DeletePathPoint { id: id.clone(), path: 0, index: 0 }]).is_err());
    assert_eq!(points(&e, &id), vec![before[2], before[3]]);
}

#[test]
fn convert_direction_point_takes_the_handles_in_or_sets_both() {
    let (mut e, page) = fixture();
    let id = made(&mut e, &page, true, closed_curve());
    let before = points(&e, &id);
    e.apply("plain", &[Command::PlainPathPoint { id: id.clone(), path: 0, index: 1 }]).unwrap();
    let p = points(&e, &id)[1];
    assert!(near(p.1, p.0) && near(p.2, p.0) && near(p.0, before[1].0));
    let set = Command::SetPathHandles { id: id.clone(), path: 0, index: 2, left: [270.0, 210.0], right: [330.0, 190.0] };
    e.apply("handles", &[set]).unwrap();
    let q = points(&e, &id)[2];
    assert!(near(q.0, before[2].0) && near(q.1, (270.0, 210.0)) && near(q.2, (330.0, 190.0)), "handles are the item's own coordinates, as given");
}

#[test]
fn undo_takes_a_structural_edit_back() {
    let (mut e, page) = fixture();
    let id = made(&mut e, &page, false, vec![pt((50., 50.), None, None), pt((150., 80.), Some((120., 60.)), Some((180., 100.))), pt((250., 50.), None, None)]);
    let before = e.parts().clone();
    e.apply("add", &[Command::AddPathPoint { id: id.clone(), path: 0, segment: 0, t: 0.4 }]).unwrap();
    assert_eq!(points(&e, &id).len(), 4);
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
    // an open path has no closing segment
    assert!(e.apply("add", &[Command::AddPathPoint { id, path: 0, segment: 2, t: 0.5 }]).is_err());
}
