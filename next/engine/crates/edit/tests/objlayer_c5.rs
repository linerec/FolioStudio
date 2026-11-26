//! Object Layer Options (objlayer-c5): layer comps, manual layer switches and the link update policy of a placed PSD, against what InDesign 2025
//! wrote after the same choice in its Object Layer Options dialog (real mouse input). The goldens are `oracle/goldens/objlayer-c5/<case>/dialog.json`: the
//! layers, `AppliedLayerComp` and `UpdateLinkOption` of the picture afterwards. The documents are `oracle/fixtures/objlayer-c5.idml` (own PSD
//! with three layer comps) and `objlayer-c4.idml` (own PSD without comps).
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

fn open(name: &str) -> Editor {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(root().join("fixtures").join(name)).unwrap()).unwrap();
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

fn spreads(ed: &Editor) -> Vec<String> {
    let dm = &ed.parts()["designmap.xml"];
    let ix = Index::scan(dm).unwrap();
    (0..ix.nodes.len()).filter(|&i| ix.local(dm, i) == "Spread").filter_map(|i| ix.attr(dm, i, "src").map(str::to_owned)).collect()
}

/// The picture frame (the only rectangle on the first spread).
fn frame(ed: &Editor) -> String {
    let t = &ed.parts()[&spreads(ed)[0]];
    let ix = Index::scan(t).unwrap();
    let i = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Rectangle").unwrap();
    ix.attr(t, i, "Self").unwrap().to_owned()
}

/// (UpdateLinkOption, AppliedLayerComp, layers as (id, name, original, current)) of the picture.
fn options(ed: &Editor) -> (String, Option<i32>, Vec<(u32, String, bool, bool)>) {
    let t = &ed.parts()[&spreads(ed)[0]];
    let ix = Index::scan(t).unwrap();
    let option = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "GraphicLayerOption").expect("the picture has layer options");
    let comp = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "LayerCompOption").and_then(|i| ix.attr(t, i, "AppliedLayerComp")).and_then(|v| v.parse().ok());
    let rows = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "GraphicLayer").map(|i| {
        (ix.attr(t, i, "Id").unwrap().parse().unwrap(), ix.attr(t, i, "Name").unwrap().to_owned(), ix.attr(t, i, "OriginalVisibility") == Some("true"), ix.attr(t, i, "CurrentVisibility") == Some("true"))
    }).collect();
    (ix.attr(t, option, "UpdateLinkOption").unwrap().to_owned(), comp, rows)
}

fn command(v: serde_json::Value) -> Command {
    serde_json::from_value(v).unwrap()
}

#[test]
fn dialog_choices_match_indesign() {
    let mut checked = 0;
    for f in std::fs::read_dir(root().join("goldens/objlayer-c5")).unwrap() {
        let path = f.unwrap().path().join("dialog.json");
        if !path.exists() {
            continue; // the other goldens of objlayer-c5 (cells) belong to the composition test of links
        }
        let g: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let name = g["name"].as_str().unwrap().to_owned();
        let file = folio_links::psdlayers::read(&std::fs::read(root().join("fixtures/Links").join(g["psd"].as_str().unwrap())).unwrap()).unwrap().unwrap();
        let (layers, comps) = (serde_json::to_value(&file.layers).unwrap(), serde_json::to_value(&file.comps).unwrap());
        let has_comps = !file.comps.is_empty();
        if g["action"]["kind"] == "popup" {
            // the Layer Comp popup: [Custom] (only while the layers were switched by hand) and Last Document State, then the comps of the file by name
            let names: Vec<&str> = file.comps.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, ["All", "A only", "B and C"], "{name}: comp names");
            assert_eq!(g["expect"]["popup"].as_array().unwrap().len(), names.len() + 2);
            checked += 1;
            continue;
        }
        let mut ed = open(g["fixture"].as_str().unwrap());
        let id = frame(&ed);
        let action = &g["action"];
        let visibility = |ed: &mut Editor| {
            let c = command(serde_json::json!({"op": "setGraphicLayerVisibility", "ids": [id], "layer": action["layer"], "visible": action["visible"], "layers": layers, "has_comps": has_comps}));
            ed.apply("layer visibility", &[c]).unwrap();
        };
        let comp = |ed: &mut Editor, n: &serde_json::Value| {
            let c = command(serde_json::json!({"op": "setGraphicLayerComp", "ids": [id], "comp": n, "comps": comps, "layers": layers}));
            ed.apply("layer comp", &[c]).unwrap();
        };
        match action["kind"].as_str().unwrap() {
            "visibility" => visibility(&mut ed),
            "comp" => comp(&mut ed, &action["comp"]),
            "comp+visibility" => {
                comp(&mut ed, &action["comp"]);
                visibility(&mut ed);
            }
            "comp+comp" => {
                comp(&mut ed, &action["comp"]);
                comp(&mut ed, &action["then"]);
            }
            "policy" => {
                let c = command(serde_json::json!({"op": "setGraphicLayerUpdateLink", "ids": [id], "policy": action["policy"], "layers": layers}));
                ed.apply("link update policy", &[c]).unwrap();
            }
            other => panic!("{name}: unknown action {other}"),
        }
        let (policy, applied, rows) = options(&ed);
        let want = &g["expect"];
        assert_eq!(policy, want["updateLinkOption"].as_str().unwrap(), "{name}: update link option");
        assert_eq!(applied, want["appliedLayerComp"].as_i64().map(|v| v as i32), "{name}: AppliedLayerComp");
        let expected: Vec<_> = want["layers"].as_array().unwrap().iter().map(|l| (l["id"].as_u64().unwrap() as u32, l["name"].as_str().unwrap().to_owned(), l["original"].as_bool().unwrap(), l["current"].as_bool().unwrap())).collect();
        assert_eq!(rows, expected, "{name}: layers");
        checked += 1;
    }
    assert!(checked >= 9, "only {checked} goldens checked");
}

/// The comps of the own PSD as the engine reads them: three comps (names are not read by InDesign from this file either: it shows "Default Comp").
#[test]
fn comps_are_listed_with_the_layers_they_show() {
    let file = folio_links::psdlayers::read(&std::fs::read(root().join("fixtures/Links/objlayer-c5.psd")).unwrap()).unwrap().unwrap();
    let shown: Vec<_> = file.comps.iter().map(|c| (c.id, c.shown.clone())).collect();
    assert_eq!(shown, [(101, vec![0, 1, 2, 3]), (102, vec![0, 1]), (103, vec![0, 2, 3])]);
}
