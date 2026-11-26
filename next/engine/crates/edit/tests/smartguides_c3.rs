//! Where a new spread guide is written (smartguides-c3, rules/smartguides-c3.ko.md). InDesign 2025, on the same fixture
//! (`artifacts/smartguides-c3-20261023/out/capture-guides.txt`: `spreads[1].guides.add` / `masterSpreads[0].guides.add`, and real Ctrl-drags and
//! double clicks on the ruler in `out/i1b.log`): a vertical spread guide is kept in the page it stands over, a horizontal one in the first page;
//! both with FitToPage="false" and PageIndex="0", the Location from the spread's left edge.
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
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

/// The pages (`Self`) of the first part under `folder` that has `count` pages, in order.
fn pages(ed: &Editor, folder: &str, count: usize) -> Vec<String> {
    for (name, t) in ed.parts() {
        if !name.starts_with(folder) { continue; }
        let ix = Index::scan(t).unwrap();
        let ids: Vec<String> = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Page").map(|i| ix.attr(t, i, "Self").unwrap().to_owned()).collect();
        if ids.len() == count { return ids; }
    }
    panic!("no part with {count} pages under {folder}");
}

/// (host page, orientation, location, FitToPage, PageIndex) of every guide.
fn guides(ed: &Editor) -> Vec<(String, String, f64, String, String)> {
    let mut out = Vec::new();
    for (name, t) in ed.parts() {
        if !name.starts_with("Spreads/") && !name.starts_with("MasterSpreads/") { continue; }
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) != "Guide" { continue; }
            let host = ix.nodes[i].parent.unwrap();
            let a = |k: &str| ix.attr(t, i, k).unwrap().to_owned();
            out.push((ix.attr(t, host, "Self").unwrap().to_owned(), a("Orientation"), a("Location").parse().unwrap(), a("FitToPage"), a("PageIndex")));
        }
    }
    out
}

fn new_guide(ed: &mut Editor, page: &str, orientation: &str, at: f64) {
    let c: Command = serde_json::from_value(serde_json::json!({"op": "newGuide", "page": page, "orientation": orientation, "at": at, "spread": true})).unwrap();
    ed.apply("guide", &[c]).unwrap();
}

fn one(ed: &Editor) -> (String, String, f64, String, String) {
    let g = guides(ed);
    assert_eq!(g.len(), 1, "{g:?}");
    g[0].clone()
}

#[test]
fn a_vertical_spread_guide_is_kept_in_the_page_it_stands_over() {
    // the document's two-page spread: pages 595.276 wide; InDesign: Location 700 -> the right page, Location 100 -> the left page
    let base = fixture();
    let p = pages(&base, "Spreads/", 2);
    for (page, at, host, location) in [(&p[1], 700.0 - 595.276, &p[1], 700.0), (&p[0], 700.0, &p[1], 700.0), (&p[0], 100.0, &p[0], 100.0), (&p[1], 100.0 - 595.276, &p[0], 100.0)] {
        let mut ed = fixture();
        new_guide(&mut ed, page, "vertical", at);
        let g = one(&ed);
        assert_eq!((&g.0, g.1.as_str(), g.3.as_str(), g.4.as_str()), (host, "Vertical", "false", "0"), "{g:?}");
        assert!((g.2 - location).abs() < 1e-6, "{g:?}");
    }
    // beside the pages (the pasteboard): the first page (InDesign keeps such a guide on the spread, leaves it out of its IDML and reads ours)
    let mut ed = fixture();
    new_guide(&mut ed, &p[0], "vertical", -60.0);
    let g = one(&ed);
    assert_eq!((&g.0, g.3.as_str(), g.4.as_str()), (&p[0], "false", "0"), "{g:?}");
    assert!((g.2 + 60.0).abs() < 1e-6, "{g:?}");
}

#[test]
fn a_horizontal_spread_guide_is_kept_in_the_first_page() {
    let base = fixture();
    let p = pages(&base, "Spreads/", 2);
    let mut ed = fixture();
    new_guide(&mut ed, &p[1], "horizontal", 300.0);
    let g = one(&ed);
    assert_eq!((&g.0, g.1.as_str(), g.3.as_str(), g.4.as_str()), (&p[0], "Horizontal", "false", "0"), "{g:?}");
}

#[test]
fn a_parent_keeps_a_vertical_spread_guide_in_the_page_it_stands_over() {
    let base = fixture();
    let p = pages(&base, "MasterSpreads/", 2);
    let mut ed = fixture();
    new_guide(&mut ed, &p[0], "vertical", 700.0);
    let g = one(&ed);
    assert_eq!((&g.0, g.1.as_str(), g.3.as_str(), g.4.as_str()), (&p[1], "Vertical", "false", "0"), "{g:?}");
    assert!((g.2 - 700.0).abs() < 1e-6, "{g:?}");
}

#[test]
fn with_the_page_origin_ruler_the_first_page_stays_the_host() {
    // InDesign measures such a guide from the page it writes it into (Location 299.25 in the right page); the reader measures spread guides
    // from the spread, so the writer keeps the first page there: the same place for both programs
    let base = fixture();
    let p = pages(&base, "Spreads/", 2);
    let mut ed = fixture();
    let pref: Command = serde_json::from_value(serde_json::json!({"op": "setViewPref", "key": "rulerOrigin", "value": "PageOrigin"})).unwrap();
    ed.apply("pref", &[pref]).unwrap();
    new_guide(&mut ed, &p[1], "vertical", 104.724);
    let g = one(&ed);
    assert_eq!((&g.0, g.3.as_str(), g.4.as_str()), (&p[0], "false", "0"), "{g:?}");
    assert!((g.2 - 700.0).abs() < 1e-6, "{g:?}");
}
