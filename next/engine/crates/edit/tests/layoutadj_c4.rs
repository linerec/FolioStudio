//! Adjust Layout, guides of a spread (layoutadj-c4): the spread guides of a document with facing pages and right-to-left binding go
//! through the map of the page they lie over (measured with InDesign 2025, `artifacts/layoutadj-c4-20261023/measurement/layoutadj-c4e.json`).
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/layoutadj4f.idml");
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
    Editor::new(parts)
}

/// (orientation, fits the page, location) of every guide of the spreads, in document order.
fn guides(ed: &Editor) -> Vec<(String, bool, f64)> {
    let mut out = Vec::new();
    for (name, t) in ed.parts() {
        if !name.starts_with("Spreads/") { continue; }
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "Guide" {
                out.push((ix.attr(t, i, "Orientation").unwrap().to_owned(), ix.attr(t, i, "FitToPage") == Some("true"), ix.attr(t, i, "Location").unwrap().parse().unwrap()));
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn adjust(width: f64, height: f64) -> Vec<(String, bool, f64)> {
    let mut ed = fixture();
    let command: Command = serde_json::from_value(serde_json::json!({"op": "adjustLayout", "width": width, "height": height})).unwrap();
    ed.apply("adjust", &[command]).unwrap();
    guides(&ed)
}

fn near(a: &[(String, bool, f64)], b: &[(&str, bool, f64)]) {
    assert_eq!(a.len(), b.len(), "{a:?}");
    for (x, y) in a.iter().zip(b) {
        assert!(x.0 == y.0 && x.1 == y.1 && (x.2 - y.2).abs() < 1e-6, "{x:?} vs {y:?} in {a:?}");
    }
}

#[test]
fn spread_guides_follow_the_page_they_lie_over() {
    // before: spread guides vertical 0, 100, 250, 400 and horizontal 200; page guides vertical 100 and horizontal 150
    let before = guides(&fixture());
    near(&before, &[("Horizontal", false, 200.0), ("Horizontal", true, 150.0), ("Vertical", false, 0.0), ("Vertical", false, 100.0), ("Vertical", false, 250.0), ("Vertical", false, 400.0), ("Vertical", true, 100.0)]);
    // 300 x 400 to 450 x 500 (e-guides-rtl): vertical 100 -> 137.5, 250 -> 381.25, 400 -> 600, horizontal 200 -> 250, page guides by their page
    near(&adjust(450.0, 500.0), &[("Horizontal", false, 250.0), ("Horizontal", true, 185.294117647059), ("Vertical", false, 0.0), ("Vertical", false, 137.5), ("Vertical", false, 381.25), ("Vertical", false, 600.0), ("Vertical", true, 137.5)]);
    // to 200 x 250 (e-guides-ltr-small): 100 -> 75, 250 -> 162.5, 400 -> 266.667, horizontal 200 -> 125
    near(&adjust(200.0, 250.0), &[("Horizontal", false, 125.0), ("Horizontal", true, 97.0588235294118), ("Vertical", false, 0.0), ("Vertical", false, 75.0), ("Vertical", false, 162.5), ("Vertical", false, 266.666666666667), ("Vertical", true, 75.0)]);
}
