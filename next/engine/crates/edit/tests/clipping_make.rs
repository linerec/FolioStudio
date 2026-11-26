//! Object ▸ Clipping Path ▸ Options making a path (build-b): `setClipping` with a made path writes the settings and the
//! `ClippingPathGeometry` where InDesign keeps them (before `GraphicBounds`), replaces a path the picture had, and is one undo step.
//! The fixture is `oracle/fixtures/objlayer.idml` (one PSD four times: none, edges, alpha, photoshop).
use folio_edit::{Command, Editor, xml::Index};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

fn open() -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/objlayer.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
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

/// (spread part, frame id) of the frame labelled `oracle.id` = `label`.
fn frame(ed: &Editor, label: &str) -> (String, String) {
    for (name, t) in ed.parts() {
        if !name.starts_with("Spreads/") { continue; }
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "Rectangle" && t[ix.nodes[i].start..ix.nodes[i].end].contains(&format!("Key=\"oracle.id\" Value=\"{label}\"")) {
                return (name.clone(), ix.attr(t, i, "Self").unwrap().to_owned());
            }
        }
    }
    panic!("no frame {label}")
}

fn image(ed: &Editor, part: &str, id: &str) -> String {
    let t = &ed.parts()[part];
    let at = t.find(&format!("Self=\"{id}\"")).unwrap();
    let start = at + t[at..].find("<Image ").unwrap();
    let end = start + t[start..].find("</Image>").unwrap();
    t[start..end].to_owned()
}

fn cmd(v: serde_json::Value) -> Command {
    serde_json::from_value(v).unwrap()
}

#[test]
fn a_made_path_is_written_with_its_settings_and_undone_in_one_step() {
    let mut ed = open();
    let before = ed.parts().clone();
    let (part, id) = frame(&ed, "none");
    let tri = json!([{"points": [[[20, 120], [20, 135], [40, 142.5]], [[80, 15], [100, 7.5], [120, 15]], [[160, 142.5], [180, 135], [180, 120]]]}]);
    ed.apply("clipping", &[cmd(json!({"op": "setClipping", "ids": [id], "kind": "PhotoshopPath", "settings": {"pathName": "Box", "inset": 1.5, "invert": true}, "paths": tri}))]).unwrap();
    let img = image(&ed, &part, &id);
    assert!(img.contains("ClippingType=\"PhotoshopPath\"") && img.contains("AppliedPathName=\"Box\"") && img.contains("InsetFrame=\"1.5\"") && img.contains("InvertPath=\"true\""), "{img}");
    let geo = img.find("<ClippingPathGeometry>").expect("geometry written");
    assert!(geo < img.find("<GraphicBounds").unwrap(), "before GraphicBounds as InDesign writes it");
    assert!(img.contains("Anchor=\"100 7.5\" LeftDirection=\"80 15\" RightDirection=\"120 15\""), "{img}");
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before, "one undo step");
}

#[test]
fn a_new_path_replaces_the_one_the_picture_had_and_none_takes_it_away() {
    let mut ed = open();
    let (part, id) = frame(&ed, "edges");
    let square = json!([{"points": [[[10, 10], [10, 10], [10, 10]], [[50, 10], [50, 10], [50, 10]], [[50, 50], [50, 50], [50, 50]], [[10, 50], [10, 50], [10, 50]]]}]);
    ed.apply("clipping", &[cmd(json!({"op": "setClipping", "ids": [id.clone()], "kind": "DetectEdges", "settings": {"threshold": 40, "tolerance": 0}, "paths": square}))]).unwrap();
    let img = image(&ed, &part, &id);
    assert_eq!(img.matches("<ClippingPathGeometry>").count(), 1);
    assert_eq!(img.matches("<GeometryPathType").count(), 1);
    assert!(img.contains("Threshold=\"40\"") && img.contains("Tolerance=\"0\"") && img.contains("Anchor=\"50 50\""), "{img}");
    // without a made path (the host could not read the file) nothing changes
    let err = ed.apply("clipping", &[cmd(json!({"op": "setClipping", "ids": [id.clone()], "kind": "AlphaChannel"}))]).unwrap_err();
    assert_eq!(err.code, "unsupported");
    ed.apply("clipping", &[cmd(json!({"op": "setClipping", "ids": [id.clone()], "kind": "None"}))]).unwrap();
    let img = image(&ed, &part, &id);
    assert!(img.contains("ClippingType=\"None\"") && !img.contains("<ClippingPathGeometry>"), "{img}");
}
