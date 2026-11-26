//! The Gap tool against InDesign 2025 (every case in next/oracle/measured/gap-tool-2025.json) is a drag measured in InDesign with REAL mouse and key input
//! (docinput-c3, gaptool-c1: frames before, pointer from/to, frames after; the 54 cases InDesign 2026 had given are in it again, dragged in 2025).
//! Rules: next/oracle/rules/gaptool.ko.md. InDesign moves the gap in 0.75 pt steps, so the comparison tolerance is the fixture's own.
use folio_edit::{Anchor, Command, Editor, xml::Index};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn command(v: Value) -> Command { serde_json::from_value(v).unwrap() }

/// The fixture's page 1, optionally as a page of `size` ([width, height], at the spread's origin): the 2025 cases were measured on a Letter page.
fn fixture(size: Option<[f64; 2]>, facing: Option<bool>) -> (Editor, String) {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic-create.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut page = String::new();
    for k in 0..zip.len() {
        let mut entry = zip.by_index(k).unwrap();
        if entry.is_dir() { continue; }
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        let name = entry.name().unwrap().into_owned();
        if name.starts_with("Spreads/") {
            let ix = Index::scan(&text).unwrap();
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&text, i) == "Page") {
                page = ix.value(&text, i, "Self").unwrap();
                if let Some([w, h]) = size {
                    // the page tag's own GeometricBounds and ItemTransform
                    let start = text.find("<Page ").unwrap();
                    let end = start + text[start..].find('>').unwrap();
                    let mut tag = text[start..end].to_owned();
                    for (attr, value) in [("GeometricBounds", format!("0 0 {h} {w}")), ("ItemTransform", "1 0 0 1 0 0".to_owned())] {
                        let a = tag.find(&format!("{attr}=\"")).unwrap() + attr.len() + 2;
                        let b = a + tag[a..].find('"').unwrap();
                        tag.replace_range(a..b, &value);
                    }
                    text.replace_range(start..end, &tag);
                }
            }
        }
        if let (Some(facing), true) = (facing, name == "Resources/Preferences.xml") {
            // the case was measured in a document with or without facing pages (the Gap tool differs, rules/gaptool.ko.md)
            text = text.replace("FacingPages=\"true\"", "FacingPages=\"@\"").replace("FacingPages=\"false\"", "FacingPages=\"@\"").replace("FacingPages=\"@\"", &format!("FacingPages=\"{facing}\""));
            assert!(text.contains(&format!("FacingPages=\"{facing}\"")), "the fixture stores FacingPages");
        }
        parts.insert(name, text);
    }
    (Editor::new(parts), page)
}

/// The fixture's own page items are taken off: the cases are about the frames they put on the page.
fn clear_page_items(ed: &mut Editor) {
    let mut ids = Vec::new();
    for (name, text) in ed.parts() {
        if !name.starts_with("Spreads/") { continue; }
        let ix = Index::scan(text).unwrap();
        for i in 0..ix.nodes.len() {
            if !matches!(ix.local(text, i), "Rectangle" | "TextFrame" | "Oval" | "Polygon" | "GraphicLine" | "Group") { continue; }
            let Some(parent) = ix.nodes[i].parent else { continue; };
            if ix.local(text, parent) != "Spread" { continue; }
            if let Some(id) = ix.value(text, i, "Self") { ids.push(id); }
        }
    }
    ed.apply("clear", &[Command::Delete { ids }]).unwrap();
}

fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect() }

#[test]
fn gap_tool_matches_the_indesign_2025_measurements() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/gap-tool-2025.json");
    let data: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    // FOLIO_GAP_TOLERANCE=0 shows InDesign's 0.75 pt rounding (proof that the comparison is not vacuous)
    let tolerance = std::env::var("FOLIO_GAP_TOLERANCE").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| data["tolerance"].as_f64().unwrap());
    let only = std::env::var("FOLIO_GAP_ONLY").ok();
    let cases = data["cases"].as_array().unwrap();
    assert!(cases.len() >= 200, "the fixture lost cases");
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| !name.contains(o)) { continue; }
        let tolerance = case["tolerance"].as_f64().unwrap_or(tolerance);
        let size = case.get("page").and_then(|p| nums(p).try_into().ok());
        let (mut ed, page) = fixture(size, case["facing"].as_bool());
        clear_page_items(&mut ed);
        let mut ids = Vec::new();
        for f in case["frames"].as_array().unwrap() {
            let rect = nums(&f["rect"]);
            let made = ed.apply("create", &[command(json!({"op":"createItem","page":page,"kind":f["kind"],"rect":rect}))]).unwrap().created;
            if f["locked"].as_bool().unwrap() {
                ed.apply("lock", &[Command::SetItem { id: made[0].clone(), locked: Some(true), visible: None }]).unwrap();
            }
            if f["hidden"].as_bool() == Some(true) {
                ed.apply("hide", &[Command::SetItem { id: made[0].clone(), locked: None, visible: Some(false) }]).unwrap();
            }
            ids.push(made[0].clone());
        }
        // spread coordinates -> page coordinates: the offset the first frame shows (before anything is turned)
        let first = nums(&case["frames"][0]["rect"]);
        let b0 = ed.bounds(&ids[0]).unwrap();
        let origin = [b0[0] - first[0], b0[1] - first[1]];
        // some of the frames are turned about their middle
        for (k, f) in case["frames"].as_array().unwrap().iter().enumerate() {
            if let Some(angle) = f["rot"].as_f64() {
                ed.apply("turn", &[Command::Rotate { ids: vec![ids[k].clone()], by: angle, anchor: Anchor::Center }]).unwrap();
            }
        }
        // some of the frames are grouped (InDesign's Gap tool sees the children, not the group)
        if let Some(members) = case["group"].as_array() {
            let ids_in: Vec<String> = members.iter().map(|k| ids[k.as_u64().unwrap() as usize].clone()).collect();
            ed.apply("group", &[command(json!({"op":"group","ids":ids_in}))]).unwrap();
        }
        let from = nums(&case["from"]);
        let to = nums(&case["to"]);
        let nearest = case["nearest"].as_bool().unwrap_or(false);
        let result = ed.apply("gap", &[command(json!({"op":"adjustGap","page":page,"from":from,"to":to,"nearest":nearest,"resize":false,"move_items":false}))]);
        let changed = case["changed"].as_bool().unwrap();
        match result {
            Err(e) if !changed && e.code == "notFound" => continue,
            Err(e) => { failures.push(format!("{name}: {e:?}")); continue; }
            Ok(_) => {}
        }
        for (k, id) in ids.iter().enumerate() {
            let want = nums(&case["expected"][k]);
            let b = ed.bounds(id).unwrap();
            let got = [b[0] - origin[0], b[1] - origin[1], b[2] - origin[0], b[3] - origin[1]];
            if got.iter().zip(&want).any(|(g, w)| (g - w).abs() > tolerance) {
                failures.push(format!("{name}: frame {k} is {got:?}, InDesign 2025 gave {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} measured cases differ:
{}", failures.len(), cases.len(), failures.join("
"));
}
