//! Adjust Layout, "Auto-adjust margins to page size changes" (layoutadj-c7): what the dialog's margin boxes show for a page size, and what
//! the command `adjustLayout` with `auto_adjust_margins` does to the document's default margins, the parent pages, the pages and the
//! remembered setting. Measured with InDesign 2025 and real input (`next/oracle/goldens/layoutadj7-dialog/auto-margins.json`,
//! `artifacts/layoutadj-c7-20261023/tools`); the document edits are also in the oracle scenarios `layoutadj7*`.
use folio_edit::{Command, Editor, xml::Index};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture(name: &str) -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(format!("{name}.idml"));
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

fn margins_in(t: &str) -> Vec<[f64; 4]> {
    let ix = Index::scan(t).unwrap();
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "MarginPreference").map(|i| {
        let g = |k: &str| ix.attr(t, i, k).unwrap().parse::<f64>().unwrap();
        [g("Top"), g("Bottom"), g("Left"), g("Right")]
    }).collect()
}

/// (document defaults, parent pages, pages)
fn all_margins(ed: &Editor) -> (Vec<[f64; 4]>, Vec<[f64; 4]>, Vec<[f64; 4]>) {
    let (mut masters, mut pages) = (Vec::new(), Vec::new());
    for (name, t) in ed.parts() {
        if name.starts_with("MasterSpreads/") { masters.extend(margins_in(t)); }
        if name.starts_with("Spreads/") { pages.extend(margins_in(t)); }
    }
    (margins_in(&ed.parts()["Resources/Preferences.xml"]), masters, pages)
}

fn auto_flag(ed: &Editor) -> bool {
    let t = &ed.parts()["designmap.xml"];
    let ix = Index::scan(t).unwrap();
    let i = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "AdjustLayoutPreference").unwrap();
    ix.attr(t, i, "EnableAutoAdjustMargins") == Some("true")
}

fn run(ed: &mut Editor, step: Value) {
    let command: Command = serde_json::from_value(step).unwrap();
    ed.apply("adjust", &[command]).unwrap();
}

fn close(a: &[f64; 4], b: [f64; 4]) {
    assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6), "{a:?} vs {b:?}");
}

/// The dialog's boxes (three decimals) and the margins it applies (exact) for every measured case.
#[test]
fn margin_boxes_match_the_dialog() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/layoutadj7-dialog/auto-margins.json");
    let golden: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let cases = golden["cases"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for c in cases {
        let nums = |k: &str| c[k].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
        let (doc, old, new, boxes) = (nums("docMargins"), nums("oldSize"), nums("newSize"), nums("boxes"));
        let got = folio_edit::auto_margins(doc.clone().try_into().unwrap(), old.try_into().unwrap(), new.try_into().unwrap());
        for k in 0..4 {
            // the boxes show three decimals
            assert!((got[k] - boxes[k]).abs() < 5.1e-4, "{}: box {k}: {got:?} vs {boxes:?}", c["name"]);
        }
        if let Some(applied) = c.get("appliedDocMargins") {
            let applied: Vec<f64> = applied.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            for k in 0..4 {
                // what is set is the unrounded product (the case log keeps five decimals)
                assert!((got[k] - applied[k]).abs() < 1e-5, "{}: applied {k}: {got:?} vs {applied:?}", c["name"]);
            }
        }
    }
}

/// Document defaults = parent = pages (30): all take the scaled margins, the document remembers the setting; a second adjustment starts
/// from the defaults the first one left.
#[test]
fn auto_margins_scale_defaults_parents_and_pages() {
    let mut ed = fixture("layoutadj7");
    assert!(!auto_flag(&ed));
    run(&mut ed, serde_json::json!({"op": "adjustLayout", "width": 450, "height": 500, "auto_adjust_margins": true}));
    let (doc, masters, pages) = all_margins(&ed);
    close(&doc[0], [37.5, 37.5, 45.0, 45.0]);
    // the first page of the parent follows the defaults; its second page (36) does not (layoutadj-c4)
    close(&masters[0], [37.5, 37.5, 45.0, 45.0]);
    close(&masters[1], [36.0, 36.0, 36.0, 36.0]);
    // the first two pages follow the parent, the third has its own left margin
    close(&pages[0], [37.5, 37.5, 45.0, 45.0]);
    close(&pages[1], [37.5, 37.5, 45.0, 45.0]);
    close(&pages[2], [30.0, 30.0, 50.0, 30.0]);
    assert!(auto_flag(&ed));
    run(&mut ed, serde_json::json!({"op": "adjustLayout", "width": 300, "height": 600, "auto_adjust_margins": true}));
    let (doc, ..) = all_margins(&ed);
    close(&doc[0], [45.0, 45.0, 30.0, 30.0]);
}

/// Defaults other than the parent's: only the defaults change (layoutadj7d, measured).
#[test]
fn a_parent_that_is_not_on_the_defaults_keeps_its_margins() {
    let mut ed = fixture("layoutadj7d");
    run(&mut ed, serde_json::json!({"op": "adjustLayout", "width": 450, "height": 600, "auto_adjust_margins": true}));
    let (doc, masters, pages) = all_margins(&ed);
    close(&doc[0], [18.0, 27.0, 36.0, 54.0]);
    close(&masters[0], [30.0, 30.0, 30.0, 30.0]);
    close(&pages[0], [30.0, 30.0, 30.0, 30.0]);
    let mut ed = fixture("layoutadj7d");
    run(&mut ed, serde_json::json!({"op": "adjustLayout", "top_margin": 50, "left_margin": 60}));
    let (doc, masters, _) = all_margins(&ed);
    close(&doc[0], [50.0, 18.0, 60.0, 36.0]);
    close(&masters[0], [30.0, 30.0, 30.0, 30.0]);
}

/// The margins are the dialog's: typing margins while the box is ticked is refused, and the settings query shows what the dialog would.
#[test]
fn margins_and_auto_adjust_do_not_mix_and_the_query_shows_the_boxes() {
    let mut ed = fixture("layoutadj7");
    let command: Command = serde_json::from_value(serde_json::json!({"op": "adjustLayout", "width": 450, "auto_adjust_margins": true, "top_margin": 10})).unwrap();
    assert!(ed.apply("adjust", &[command]).is_err());
    let s = ed.layout_adjust_settings(Some(450.0), None).unwrap();
    close(&s.doc_margins, [30.0, 30.0, 30.0, 30.0]);
    close(&s.auto_margins, [30.0, 30.0, 45.0, 45.0]);
    assert!(!s.auto_adjust_margins);
    run(&mut ed, serde_json::json!({"op": "adjustLayout", "auto_adjust_margins": true}));
    assert!(ed.layout_adjust_settings(None, None).unwrap().auto_adjust_margins);
    run(&mut ed, serde_json::json!({"op": "adjustLayout", "auto_adjust_margins": false}));
    assert!(!ed.layout_adjust_settings(None, None).unwrap().auto_adjust_margins);
}
