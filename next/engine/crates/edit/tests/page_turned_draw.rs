//! Drawing on pages turned by their own transform inside the IDML (pen-c2; fixture oracle/fixtures/page-rotated.idml from make-page-rotated.py:
//! the first page turned 90 degrees, the last 30 degrees). Folio shows such a page as its bounding box on the spread with the objects as
//! they lie there, so page points are measured from that box's top-left corner: the Pen's createPath, the Pencil and the Erase tool take
//! them so (they were refused on a turned page before).
use folio_edit::{Command, Editor, xml::Index};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

type P = (f64, f64);

fn fixture() -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/page-rotated.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    Editor::new(parts)
}

fn nums(s: &str) -> Vec<f64> {
    s.split_whitespace().map(|x| x.parse().unwrap()).collect()
}

/// The top-left corner of a page's bounding box on its spread.
fn page_origin(e: &Editor, page: &str) -> P {
    for (name, t) in e.parts() {
        let ix = Index::scan(t).unwrap();
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page" && ix.attr(t, i, "Self") == Some(page)) else { continue };
        assert!(name.starts_with("Spreads/"));
        let b = nums(ix.attr(t, i, "GeometricBounds").unwrap());
        let m = nums(ix.attr(t, i, "ItemTransform").unwrap());
        let c = [(b[1], b[0]), (b[3], b[0]), (b[3], b[2]), (b[1], b[2])].map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
        return c.iter().fold((f64::MAX, f64::MAX), |o, q| (o.0.min(q.0), o.1.min(q.1)));
    }
    panic!("no page {page}")
}

/// The anchors of an item's paths on its spread (its transform applied), each path as `(open, anchors)`.
fn anchors(e: &Editor, id: &str) -> Vec<(bool, Vec<P>)> {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) else { continue };
        let m = nums(ix.attr(t, i, "ItemTransform").unwrap_or("1 0 0 1 0 0"));
        return ix.own_descendants(t, i, "GeometryPathType").into_iter().map(|g| {
            let pts = ix.own_descendants(t, g, "PathPointType").into_iter().map(|p| {
                let a = nums(ix.attr(t, p, "Anchor").unwrap());
                (m[0] * a[0] + m[2] * a[1] + m[4], m[1] * a[0] + m[3] * a[1] + m[5])
            }).collect();
            (ix.attr(t, g, "PathOpen") == Some("true"), pts)
        }).collect();
    }
    panic!("no item {id}")
}

fn apply(e: &mut Editor, c: serde_json::Value) -> Vec<String> {
    let cmd: Command = serde_json::from_value(c).unwrap();
    e.apply("step", &[cmd]).unwrap().created
}

fn near(a: P, b: P, tol: f64) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

#[test]
fn the_pen_draws_on_pages_turned_by_their_own_transform() {
    for page in ["ufc", "u116"] {
        let mut e = fixture();
        let o = page_origin(&e, page);
        let id = apply(&mut e, json!({"op": "createPath", "page": page, "closed": false,
            "points": [{"anchor": [100.0, 100.0]}, {"anchor": [200.0, 150.0], "left": [180.0, 140.0], "right": [220.0, 160.0]}]}))[0].clone();
        let p = anchors(&e, &id);
        assert_eq!(p.len(), 1);
        assert!(near(p[0].1[0], (o.0 + 100.0, o.1 + 100.0), 1e-6) && near(p[0].1[1], (o.0 + 200.0, o.1 + 150.0), 1e-6), "{page}: {:?} (origin {o:?})", p[0].1);
    }
}

/// The drawing tools on a turned page (pen-c3): a rectangle, an ellipse, a polygon, a line, a frame and a text frame, dragged (createByDrag), typed
/// (createInView) or by script bounds (createItem), all measured from the bounding-box corner and upright on the spread, where the drag was.
#[test]
fn the_drawing_tools_draw_on_pages_turned_by_their_own_transform() {
    for page in ["ufc", "u116"] {
        for op in ["createByDrag", "createInView", "createItem"] {
            for kind in ["rectangle", "ellipse", "polygon", "line", "rectangleFrame", "ellipseFrame", "polygonFrame", "textFrame"] {
                let mut e = fixture();
                let o = page_origin(&e, page);
                let rect = [100.0, 120.0, 250.0, 210.0];
                let cmd = if op == "createItem" {
                    json!({"op": op, "page": page, "kind": kind, "rect": rect})
                } else {
                    json!({"op": op, "page": page, "kind": kind, "rect": rect, "layer": "", "sides": 6, "inset": 0.0})
                };
                let created = apply(&mut e, cmd);
                let id = created.last().unwrap().clone();
                let pts: Vec<P> = anchors(&e, &id).into_iter().flat_map(|(_, p)| p).collect();
                let (x0, y0) = pts.iter().fold((f64::MAX, f64::MAX), |m, p| (m.0.min(p.0), m.1.min(p.1)));
                let (x1, y1) = pts.iter().fold((f64::MIN, f64::MIN), |m, p| (m.0.max(p.0), m.1.max(p.1)));
                // a text frame keeps its height from the rect, its width from the frame preferences; the others fill the rect
                assert!(near((x0, y0), (o.0 + 100.0, o.1 + 120.0), 1e-6), "{page} {op} {kind}: top-left {:?} (origin {o:?})", (x0, y0));
                if kind != "textFrame" {
                    assert!(near((x1, y1), (o.0 + 250.0, o.1 + 210.0), 1e-6), "{page} {op} {kind}: bottom-right {:?}", (x1, y1));
                }
            }
        }
    }
}

#[test]
fn the_pencil_and_the_eraser_work_on_a_turned_page() {
    let mut e = fixture();
    let page = "u116";
    let o = page_origin(&e, page);
    let drag: Vec<[f64; 2]> = (0..=20).map(|k| [100.0 + 10.0 * k as f64, 300.0]).collect();
    let id = apply(&mut e, json!({"op": "pencilStroke", "page": page, "points": drag, "fidelity": 2.5, "smoothness": 0.0, "px_per_pt": 1.0,
        "within_px": 12.0, "alt": false, "selected": [], "layer": ""}))[0].clone();
    let p = anchors(&e, &id);
    let (s, last) = (p[0].1[0], *p[0].1.last().unwrap());
    assert!(near(s, (o.0 + 100.0, o.1 + 300.0), 1.0) && near(last, (o.0 + 300.0, o.1 + 300.0), 1.0), "the stroke where it was dragged: {s:?} .. {last:?}");
    // the middle erased: two pieces, the cut ends near 170 and 230
    let made = apply(&mut e, json!({"op": "erasePath", "id": id, "page": page, "points": [[170.0, 301.0], [200.0, 301.0], [230.0, 301.0]], "fidelity": 2.5, "px_per_pt": 1.0}));
    assert_eq!(made.len(), 1, "the back piece is a new object");
    let a = anchors(&e, &id)[0].1.clone();
    assert!(near(*a.last().unwrap(), (o.0 + 170.0, o.1 + 300.0), 1.5), "front piece ends at the cut: {a:?}");
}
