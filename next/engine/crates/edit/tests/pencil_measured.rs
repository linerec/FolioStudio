//! The Pencil tool against InDesign 2025: every stroke in next/oracle/measured/pencil-2025.json was dragged with a real mouse
//! (artifacts/pencil-c9-20261023); `points` are the page points the tracker received (the mouse positions rounded to the
//! view's drag grid, read back with line-tool drags), `ctrl` is the control polygon of the path InDesign made. The `pencilFit`
//! command must make the same path: anchors to 0.1 pt, handles to 0.5 pt (the port is exact to the last digit).
use folio_edit::{Command, Editor, xml::Index};
use serde_json::{Value, json};
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

/// The control polygon of an item's first path, in the item's own coordinates.
fn polygon(e: &Editor, id: &str) -> Vec<P> {
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
        let pts: Vec<(P, P, P)> = ix.own_descendants(t, i, "PathPointType").into_iter().map(|p| (pair(p, "Anchor"), pair(p, "LeftDirection"), pair(p, "RightDirection"))).collect();
        let mut out = Vec::new();
        for (k, (a, l, r)) in pts.iter().enumerate() {
            if k > 0 {
                out.push(*l);
            }
            out.push(*a);
            if k + 1 < pts.len() {
                out.push(*r);
            }
        }
        return out;
    }
    panic!("no item {id}");
}

fn pairs(v: &Value) -> Vec<P> {
    v.as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect()
}

#[test]
fn the_pencil_makes_the_path_indesign_made_of_the_same_drag() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/pencil-2025.json");
    let table: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    let cases = table["cases"].as_array().unwrap();
    assert!(cases.len() >= 60, "the table has {} strokes", cases.len());
    let fidelity = table["settings"]["fidelity"].as_f64().unwrap();
    let smoothness = table["settings"]["smoothness"].as_f64().unwrap();
    let (mut worst_anchor, mut worst_handle) = (0.0f64, 0.0f64);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let (mut e, page) = fixture();
        let cmd: Command = serde_json::from_value(json!({"op": "pencilFit", "page": page, "points": c["points"], "fidelity": fidelity,
            "smoothness": smoothness, "px_per_pt": c["px_per_pt"], "layer": ""})).unwrap();
        let o = e.apply("pencil", &[cmd]).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let got = polygon(&e, &o.created[0]);
        let want = pairs(&c["ctrl"]);
        assert_eq!(got.len(), want.len(), "{name}: control points");
        // the item's own coordinates are the page's less a shift
        let (dx, dy) = (want[0].0 - got[0].0, want[0].1 - got[0].1);
        for (k, (g, w)) in got.iter().zip(&want).enumerate() {
            let d = ((g.0 + dx - w.0).powi(2) + (g.1 + dy - w.1).powi(2)).sqrt();
            if k % 3 == 0 {
                worst_anchor = worst_anchor.max(d);
                assert!(d < 0.1, "{name}: anchor {} is {d} pt off", k / 3);
            } else {
                worst_handle = worst_handle.max(d);
                assert!(d < 0.5, "{name}: handle {k} is {d} pt off");
            }
        }
    }
    eprintln!("{} strokes, worst anchor {worst_anchor:.2e} pt, worst handle {worst_handle:.2e} pt", cases.len());
}

/// Smoothness above 0 % runs InDesign's low-pass fit (pen-c3, ops/pencilfit.rs `smoothing` / `lowpass`): a path is made, also for two points.
#[test]
fn a_smoothness_above_zero_makes_a_path() {
    let (mut e, page) = fixture();
    let cmd: Command = serde_json::from_value(json!({"op": "pencilFit", "page": page, "points": [[10.0, 10.0], [50.0, 20.0]], "fidelity": 2.5,
        "smoothness": 40.0, "px_per_pt": 1.0, "layer": ""})).unwrap();
    assert_eq!(e.apply("pencil", &[cmd]).expect("a smoothed path").created.len(), 1);
}
