//! Page tool (pagetool-c1): `movePage` against what InDesign 2025 did when a page was dragged with the real mouse (or X / Y typed in the
//! control bar) with "Objects Move with Page" off and on. The goldens (`oracle/goldens/pagetool-c1/*.json`) hold the position of every object and
//! guide against the page's top-left corner before and after; the document they started from is `oracle/fixtures/pagetool1-single.idml`.
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

fn fixture() -> Editor { open("pagetool1-single.idml") }

fn open(name: &str) -> Editor {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(root().join("fixtures").join(name)).unwrap()).unwrap();
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

/// The spread parts in document order.
fn spread_parts(ed: &Editor) -> Vec<String> {
    let dm = &ed.parts()["designmap.xml"];
    let ix = Index::scan(dm).unwrap();
    (0..ix.nodes.len()).filter(|&i| ix.local(dm, i) == "Spread").filter_map(|i| ix.attr(dm, i, "src").map(str::to_owned)).collect()
}

/// label -> (translation of its ItemTransform) of the objects lying directly on the spreads.
fn objects(ed: &Editor) -> BTreeMap<String, [f64; 2]> {
    let mut out = BTreeMap::new();
    for part in spread_parts(ed) {
        let t = &ed.parts()[&part];
        let ix = Index::scan(t).unwrap();
        let root = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Spread" && ix.attr(t, i, "Self").is_some()).unwrap();
        for &i in ix.children(root) {
            if ix.attr(t, i, "ItemTransform").is_none() || ix.local(t, i) == "Page" { continue; }
            let Some(k) = ix.path(t, i, &["Properties", "Label", "KeyValuePair"]) else { continue };
            let n: Vec<f64> = ix.attr(t, i, "ItemTransform").unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
            out.insert(ix.attr(t, k, "Value").unwrap().to_owned(), [n[4], n[5]]);
        }
    }
    out
}

/// (orientation, fits the page) -> location of the guides of the first spread.
fn guides(ed: &Editor) -> BTreeMap<(char, bool), f64> {
    let t = &ed.parts()[&spread_parts(ed)[0]];
    let ix = Index::scan(t).unwrap();
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Guide").map(|i| {
        ((ix.attr(t, i, "Orientation").unwrap().chars().next().unwrap(), ix.attr(t, i, "FitToPage") == Some("true")), ix.attr(t, i, "Location").unwrap().parse().unwrap())
    }).collect()
}

fn first_page(ed: &Editor) -> String {
    let t = &ed.parts()[&spread_parts(ed)[0]];
    let ix = Index::scan(t).unwrap();
    let p = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page").unwrap();
    ix.attr(t, p, "Self").unwrap().to_owned()
}

fn movement(ed: &mut Editor, by: [f64; 2], with_objects: bool) {
    let page = first_page(ed);
    let c: Command = serde_json::from_value(serde_json::json!({"op": "movePage", "page": page, "by": by, "objects_move_with_page": with_objects})).unwrap();
    ed.apply("move page", &[c]).unwrap();
}

fn close(a: f64, b: f64, what: &str) { assert!((a - b).abs() < 0.02, "{what}: engine {a} vs InDesign {b}"); }

/// Every golden of the single-page document that moves the page (dragged, or X / Y typed): the engine moves the objects and guides by the same
/// amounts against the page.
#[test]
fn page_moves_like_indesign() {
    let mut checked = 0;
    for f in std::fs::read_dir(root().join("goldens/pagetool-c1")).unwrap() {
        let path = f.unwrap().path();
        let g: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let name = g["name"].as_str().unwrap().to_owned();
        let typed = &g["action"]["typed"];
        let moves = g["action"]["kind"] == "drag" || typed.get("X").is_some() || typed.get("Y").is_some();
        if g["fixture"] != "pagetool1-single.idml" || !moves { continue; }
        let d = [g["pageMovedAgainstRest"][0].as_f64().unwrap(), g["pageMovedAgainstRest"][1].as_f64().unwrap()];
        let with = g["objectsMoveWithPage"].as_bool().unwrap();
        let mut ed = fixture();
        let before = objects(&ed);
        let gb = guides(&ed);
        movement(&mut ed, d, with);
        let after = objects(&ed);
        for (label, b) in &g["before"]["items"].as_object().unwrap().clone() {
            let Some(was) = before.get(label) else { continue };   // children of groups are not on the spread
            let want = [g["after"]["items"][label]["tl"][0].as_f64().unwrap() - b["tl"][0].as_f64().unwrap(), g["after"]["items"][label]["tl"][1].as_f64().unwrap() - b["tl"][1].as_f64().unwrap()];
            let now = after[label];
            close(now[0] - was[0], want[0], &format!("{name} {label} x"));
            close(now[1] - was[1], want[1], &format!("{name} {label} y"));
            checked += 1;
        }
        let ga = guides(&ed);
        for (b, a) in g["before"]["guides"].as_array().unwrap().iter().zip(g["after"]["guides"].as_array().unwrap()) {
            let key = (b["orientation"].as_str().unwrap().chars().next().unwrap(), b["fitToPage"].as_bool().unwrap());
            let key = (if key.0 == 'H' { 'H' } else { 'V' }, key.1);
            let find = |m: &BTreeMap<(char, bool), f64>| m.iter().find(|(k, _)| k.0 == key.0 && k.1 == key.1).map(|(_, v)| *v).unwrap();
            close(find(&ga) - find(&gb), a["location"].as_f64().unwrap() - b["location"].as_f64().unwrap(), &format!("{name} guide {key:?}"));
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} values were compared");
}

#[test]
fn one_undo_restores_the_document_and_a_zero_move_changes_nothing() {
    let mut ed = fixture();
    let before = ed.parts().clone();
    movement(&mut ed, [0.0, 0.0], true);
    assert_eq!(&before, ed.parts());
    movement(&mut ed, [80.0, 60.0], false);
    assert_ne!(&before, ed.parts());
    ed.undo().unwrap();
    assert_eq!(&before, ed.parts());
}

#[test]
fn an_unknown_page_is_refused() {
    // (a spread of several pages is refused too: InDesign keeps its pages' places against each other, the spread layout cannot hold that)
    let mut ed = fixture();
    let page = first_page(&ed);
    let t = ed.parts()["Resources/Preferences.xml"].clone();
    assert!(t.contains("FacingPages=\"false\""));
    // a bogus page is not found
    let c: Command = serde_json::from_value(serde_json::json!({"op": "movePage", "page": format!("{page}x"), "by": [1.0, 1.0], "objects_move_with_page": true})).unwrap();
    assert!(ed.apply("move page", &[c]).is_err());
}

#[test]
fn facing_pages_keep_the_old_way_without_the_option_and_refuse_it_with() {
    // a document with facing pages: InDesign keeps the pages' places against each other, Folio's spread layout cannot (rules/pagetool-c1.ko.md)
    let mut ed = open("layoutadj-facing.idml");
    let page = first_page(&ed);
    let with: Command = serde_json::from_value(serde_json::json!({"op": "movePage", "page": page, "by": [0.0, 20.0], "objects_move_with_page": true})).unwrap();
    assert_eq!(ed.apply("move page", &[with]).unwrap_err().code, "unsupported");
    let without: Command = serde_json::from_value(serde_json::json!({"op": "movePage", "page": page, "by": [0.0, 20.0], "objects_move_with_page": false})).unwrap();
    let before = ed.parts().clone();
    ed.apply("move page", &[without]).unwrap();
    assert_ne!(&before, ed.parts());
}
