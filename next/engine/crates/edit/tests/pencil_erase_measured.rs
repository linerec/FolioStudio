//! The Erase tool against InDesign 2025: every drag in next/oracle/measured/pencil-erase-2025.json was made with a real mouse over a
//! Pencil path (artifacts/pencil-c9-20261023 recorded it, artifacts/pencil-c11-20261023 read it again); `base` is the path before, `drag`
//! the page points the tracker received, `pieces` the open paths InDesign left (the one that kept the object first). The `erasePath`
//! command must leave the same: anchors to 0.1 pt, handles to 0.5 pt (the port matches to the 1e-4 pt the DOM rounds to).
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

/// The anchors with their handles of an item's first path, as [left, anchor, right] rows, in the item's own coordinates.
fn anchors(e: &Editor, id: &str) -> Option<Vec<[P; 3]>> {
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
        return Some(ix.own_descendants(t, i, "PathPointType").into_iter().map(|p| [pair(p, "LeftDirection"), pair(p, "Anchor"), pair(p, "RightDirection")]).collect());
    }
    None
}

fn rows(v: &Value) -> Vec<[P; 3]> {
    v.as_array().unwrap().iter().map(|r| {
        let f = |k: usize| r[k].as_f64().unwrap();
        [(f(0), f(1)), (f(2), f(3)), (f(4), f(5))]
    }).collect()
}

#[test]
fn the_eraser_leaves_the_paths_indesign_left_of_the_same_drag() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/pencil-erase-2025.json");
    let table: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    let cases = table["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "the table has {} drags", cases.len());
    let fidelity = table["settings"]["fidelity"].as_f64().unwrap();
    let (mut worst_anchor, mut worst_handle) = (0.0f64, 0.0f64);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let (mut e, page) = fixture();
        let base = rows(&c["base"]);
        let pts: Vec<Value> = base.iter().map(|r| json!({"anchor": [r[1].0, r[1].1], "left": [r[0].0, r[0].1], "right": [r[2].0, r[2].1]})).collect();
        let make: Command = serde_json::from_value(json!({"op": "createPath", "page": page, "closed": false, "points": pts})).unwrap();
        let id = e.apply("make", &[make]).unwrap().created[0].clone();
        let shift = {
            let got = anchors(&e, &id).unwrap();
            (base[0][1].0 - got[0][1].0, base[0][1].1 - got[0][1].1)
        };
        let erase: Command = serde_json::from_value(json!({"op": "erasePath", "id": id, "page": page, "points": c["drag"], "fidelity": fidelity,
            "px_per_pt": c["px_per_pt"]})).unwrap();
        let o = e.apply("erase", &[erase]).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        // the object keeps the front part; a new object holds the back part
        let mut got: Vec<Vec<[P; 3]>> = vec![anchors(&e, &id).unwrap_or_else(|| panic!("{name}: the path object is gone"))];
        got.extend(o.created.iter().map(|n| anchors(&e, n).unwrap()));
        let want: Vec<Vec<[P; 3]>> = c["pieces"].as_array().unwrap().iter().map(rows).collect();
        assert_eq!(got.len(), want.len(), "{name}: how many paths are left");
        for (k, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g.len(), w.len(), "{name}: piece {k} anchors");
            for (j, (a, b)) in g.iter().zip(w).enumerate() {
                let near = |x: P, y: P| ((x.0 + shift.0 - y.0).powi(2) + (x.1 + shift.1 - y.1).powi(2)).sqrt();
                let da = near(a[1], b[1]);
                let dh = near(a[0], b[0]).max(near(a[2], b[2]));
                worst_anchor = worst_anchor.max(da);
                worst_handle = worst_handle.max(dh);
                assert!(da < 0.1, "{name}: piece {k} anchor {j} is {da} pt off");
                assert!(dh < 0.5, "{name}: piece {k} handle {j} is {dh} pt off");
            }
        }
    }
    eprintln!("{} drags, worst anchor {worst_anchor:.2e} pt, worst handle {worst_handle:.2e} pt", cases.len());
}

#[test]
fn an_erase_drag_without_points_or_scale_is_refused() {
    let (mut e, page) = fixture();
    let make: Command = serde_json::from_value(json!({"op": "createPath", "page": page, "closed": false,
        "points": [{"anchor": [10.0, 10.0]}, {"anchor": [90.0, 10.0]}]})).unwrap();
    let id = e.apply("make", &[make]).unwrap().created[0].clone();
    for (points, scale) in [(json!([]), 1.0), (json!([[10.0, 10.0], [50.0, 10.0]]), 0.0)] {
        let cmd: Command = serde_json::from_value(json!({"op": "erasePath", "id": id, "page": page, "points": points, "fidelity": 2.5, "px_per_pt": scale})).unwrap();
        assert!(e.apply("erase", &[cmd]).is_err());
    }
}
