//! The Erase and Smooth tools on a closed path and on a path of a compound path (pen-c2, Folio's rules: the open-path tools of
//! pencilerase.rs / pencilsmooth.rs applied to the path read round from its anchor farthest from the drag; ops/pencilcurve.rs `erase_closed`).
use folio_edit::{Command, Editor, xml::Index};
use serde_json::json;
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

/// The paths of an item: `(open, anchors)` each, the anchors in page coordinates of the fixture's first page (the item's transform
/// applied, then the page's place on the spread taken off).
fn paths(e: &Editor, id: &str, page: &str) -> Vec<(bool, Vec<P>)> {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) else { continue };
        let nums = |s: &str| -> Vec<f64> { s.split_whitespace().map(|x| x.parse().unwrap()).collect() };
        let m = nums(ix.attr(t, i, "ItemTransform").unwrap_or("1 0 0 1 0 0"));
        let pg = (0..ix.nodes.len()).find(|&k| ix.attr(t, k, "Self") == Some(page)).unwrap();
        let pm = nums(ix.attr(t, pg, "ItemTransform").unwrap_or("1 0 0 1 0 0"));
        let gb = nums(ix.attr(t, pg, "GeometricBounds").unwrap());
        // page (0, 0) on the spread
        let (ox, oy) = (pm[0] * gb[1] + pm[2] * gb[0] + pm[4], pm[1] * gb[1] + pm[3] * gb[0] + pm[5]);
        return ix.own_descendants(t, i, "GeometryPathType").into_iter().map(|g| {
            let open = ix.attr(t, g, "PathOpen") == Some("true");
            let pts = ix.own_descendants(t, g, "PathPointType").into_iter().map(|p| {
                let a = nums(ix.attr(t, p, "Anchor").unwrap());
                (m[0] * a[0] + m[2] * a[1] + m[4] - ox, m[1] * a[0] + m[3] * a[1] + m[5] - oy)
            }).collect();
            (open, pts)
        }).collect();
    }
    panic!("no item {id}")
}

fn apply(e: &mut Editor, c: serde_json::Value) -> Vec<String> {
    let cmd: Command = serde_json::from_value(c).unwrap();
    e.apply("step", &[cmd]).unwrap().created
}

fn square(e: &mut Editor, page: &str, x: f64, y: f64, s: f64) -> String {
    apply(e, json!({"op": "createPath", "page": page, "closed": true,
        "points": [{"anchor": [x, y]}, {"anchor": [x + s, y]}, {"anchor": [x + s, y + s]}, {"anchor": [x, y + s]}]}))[0].clone()
}

fn near(a: P, b: P, tol: f64) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

#[test]
fn erasing_the_middle_of_a_side_of_a_closed_path_opens_it_there() {
    let (mut e, page) = fixture();
    let id = square(&mut e, &page, 100.0, 100.0, 90.0);
    let made = apply(&mut e, json!({"op": "erasePath", "id": id, "page": page, "points": [[191.0, 130.0], [191.0, 145.0], [191.0, 160.0]], "fidelity": 2.5, "px_per_pt": 1.0}));
    assert!(made.is_empty(), "a closed path stays one object: {made:?}");
    let p = paths(&e, &id, &page);
    assert_eq!(p.len(), 1);
    let (open, pts) = &p[0];
    assert!(*open, "the erased closed path is open");
    // it starts where the erased part ends and runs round, by every corner, to where it began
    assert!(near(pts[0], (190.0, 160.0), 1.0), "start {:?}", pts[0]);
    assert!(near(*pts.last().unwrap(), (190.0, 130.0), 1.0), "end {:?}", pts.last());
    for c in [(190.0, 190.0), (100.0, 190.0), (100.0, 100.0), (190.0, 100.0)] {
        assert!(pts.iter().any(|q| near(*q, c, 0.01)), "the corner {c:?} stays: {pts:?}");
    }
}

#[test]
fn a_drag_from_far_away_does_not_erase_a_closed_path() {
    let (mut e, page) = fixture();
    let id = square(&mut e, &page, 100.0, 100.0, 90.0);
    let before = paths(&e, &id, &page);
    apply(&mut e, json!({"op": "erasePath", "id": id, "page": page, "points": [[145.0, 145.0], [160.0, 150.0]], "fidelity": 2.5, "px_per_pt": 1.0}));
    assert_eq!(paths(&e, &id, &page), before);
}

#[test]
fn smoothing_a_closed_path_keeps_it_closed() {
    let (mut e, page) = fixture();
    // a closed ring of 24 corner points, a little uneven (0.3 pt in and out: under the tolerance)
    let pts: Vec<serde_json::Value> = (0..24).map(|k| {
        let a = k as f64 / 24.0 * std::f64::consts::TAU;
        let r = 80.0 + if k % 2 == 0 { 0.3 } else { -0.3 };
        json!({"anchor": [300.0 + r * a.cos(), 300.0 + r * a.sin()]})
    }).collect();
    let id = apply(&mut e, json!({"op": "createPath", "page": page, "closed": true, "points": pts}))[0].clone();
    let n0 = paths(&e, &id, &page)[0].1.len();
    // along the right side of the ring
    let drag: Vec<[f64; 2]> = (-8..=8).map(|k| { let a = k as f64 / 24.0 * std::f64::consts::TAU / 2.0; [300.0 + 80.0 * a.cos(), 300.0 + 80.0 * a.sin()] }).collect();
    // zoomed out to 50 %: the tolerance (1.8 pt) is over the unevenness and the corners
    apply(&mut e, json!({"op": "smoothPath", "id": id, "page": page, "points": drag, "fidelity": 2.5, "px_per_pt": 0.5}));
    let p = paths(&e, &id, &page);
    assert_eq!(p.len(), 1);
    assert!(!p[0].0, "the smoothed ring is still closed");
    assert!(p[0].1.len() < n0, "the smoothed part has fewer anchors: {n0} -> {}", p[0].1.len());
    // the far side is as it was
    assert!(p[0].1.iter().any(|q| near(*q, (300.0 - 80.3, 300.0), 0.01)), "{:?}", p[0].1);
}

#[test]
fn erasing_a_path_of_a_compound_path_keeps_the_pieces_in_the_object() {
    let (mut e, page) = fixture();
    let a = square(&mut e, &page, 100.0, 100.0, 200.0);
    let b = square(&mut e, &page, 150.0, 150.0, 60.0);
    apply(&mut e, json!({"op": "makeCompoundPath", "ids": [a, b]}));
    // the drag over the top side of the inner square
    let made = apply(&mut e, json!({"op": "erasePath", "id": a, "page": page, "points": [[170.0, 149.0], [180.0, 149.0], [190.0, 149.0]], "fidelity": 2.5, "px_per_pt": 1.0}));
    assert!(made.is_empty(), "no new object: {made:?}");
    let p = paths(&e, &a, &page);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(!p[0].0 && p[0].1.len() == 4, "the outer square is untouched: {:?}", p[0]);
    assert!(p[1].0, "the inner one is open now");
    assert!(p[1].1.iter().all(|q| !(q.1 < 151.0 && q.0 > 172.0 && q.0 < 188.0)), "the erased part is gone: {:?}", p[1].1);
}
