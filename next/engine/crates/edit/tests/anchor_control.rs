//! Anchoring an existing object by dragging its control into text (`anchorObject`): every case measured in InDesign 2025 with a real mouse
//! (next/oracle/measured/anchor-control-2025.json, rules in next/oracle/rules/liveinput-c2.ko.md) is rebuilt here, the object is anchored at the
//! character InDesign anchored it at, and the offsets (and where measured, the object's ItemTransform in the story) must come out as InDesign's.
//! The object must also stay where it was.
use folio_edit::{Command, Editor, xml::Index};
use folio_model::{Affine, ItemKind, PageItem, Rect};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::{Path, PathBuf}, sync::OnceLock};

fn command(v: Value) -> Command { serde_json::from_value(v).unwrap() }

fn fixture() -> (BTreeMap<String, String>, String, PathBuf) {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic-create.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
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
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&text, i) == "Page") { page = ix.value(&text, i, "Self").unwrap(); }
        }
        parts.insert(name, text);
    }
    (parts, page, file)
}

fn fonts() -> &'static folio_text::FontStore {
    static FONTS: OnceLock<folio_text::FontStore> = OnceLock::new();
    FONTS.get_or_init(|| folio_text::FontStore::discover(&[]))
}

fn read(ed: &Editor, path: &Path) -> folio_model::Document { folio_idml::read_parts(ed.parts(), path, &folio_idml::ReaderData::default()).unwrap() }

fn find<'d>(items: &'d [PageItem], id: &str, parent: Affine) -> Option<(&'d PageItem, Affine)> {
    for item in items {
        let m = parent.then_inner(&item.transform);
        if item.id == id { return Some((item, m)); }
        if let ItemKind::Group { children } = &item.kind && let Some(f) = find(children, id, m) { return Some(f); }
    }
    None
}

/// The object's visible bounds on the spread (stroke included), wherever it is: on the spread or anchored in text.
fn bounds_on_spread(doc: &folio_model::Document, layout: &folio_model::layout::DocumentLayout, id: &str) -> Rect {
    let anchored = folio_model::layout::anchored_items(doc, layout);
    for spread in &doc.spreads {
        if let Some((item, m)) = find(&spread.items, id, Affine::IDENTITY) { return visible(item, &m); }
        for item in &spread.items {
            if let Some(children) = anchored.get(&item.id) && let Some(child) = children.iter().find(|c| c.id == id) {
                let (frame, fm) = find(&spread.items, &item.id, Affine::IDENTITY).unwrap();
                let _ = frame;
                return visible(child, &fm.then_inner(&child.transform));
            }
        }
    }
    panic!("{id} is nowhere");
}
fn visible(item: &PageItem, m: &Affine) -> Rect {
    let inner = folio_model::paths_bounds(&item.paths).unwrap();
    let outset = if matches!(item.stroke.paint, folio_model::Paint::None) || item.stroke.weight <= 0.0 { 0.0 } else { item.stroke.weight / 2.0 };
    let mut r = m.apply_rect(&inner);
    r.left -= outset; r.top -= outset; r.right += outset; r.bottom += outset;
    r
}

fn story_of(ed: &Editor, frame: &str) -> String {
    for t in ed.parts().values() {
        let Ok(ix) = Index::scan(t) else { continue };
        if let Some(i) = ix.by_self(t, frame) && let Some(s) = ix.attr(t, i, "ParentStory") { return s.to_owned(); }
    }
    panic!("{frame} has no story");
}

fn node_attr(ed: &Editor, id: &str, attr: &str) -> Option<String> {
    for t in ed.parts().values() {
        let Ok(ix) = Index::scan(t) else { continue };
        if let Some(i) = ix.by_self(t, id) { return ix.attr(t, i, attr).map(str::to_owned); }
    }
    None
}

fn rect(v: &Value) -> [f64; 4] { let a = v.as_array().unwrap(); [0, 1, 2, 3].map(|k| a[k].as_f64().unwrap()) }

struct Built { ed: Editor, path: PathBuf, story: String, frame: String, object: String }

/// The measured case's document: the text frame with its Arial text and the object (None when Arial is not installed).
fn build(frame_spec: &Value, object: &Value) -> Option<Built> {
    let (parts, page, path) = fixture();
    let mut ed = Editor::new(parts);
    let font = fonts();
    if font.resource_record("Arial", "Regular", &read(&ed, &path).font_table).is_none() { eprintln!("SKIP no Arial installed"); return None; }
    let create = |ed: &mut Editor, kind: &str, r: [f64; 4]| ed.apply("create", &[command(json!({"op": "createItem", "page": page, "kind": kind, "rect": r}))]).unwrap().created[0].clone();
    let t = create(&mut ed, "textFrame", rect(&frame_spec["rect"]));
    let story = story_of(&ed, &t);
    let text = frame_spec["text"].as_str().unwrap();
    ed.apply("text", &[command(json!({"op": "insertText", "story": story, "at": 0, "text": text}))]).unwrap();
    ed.set_font_records(font.resource_record("Arial", "Regular", &read(&ed, &path).font_table).into_iter().collect());
    let mut props = json!({"appliedFont": "Arial", "fontStyle": "Regular", "pointSize": frame_spec["size"].as_f64().unwrap_or(14.0), "leading": frame_spec["leading"].as_f64().unwrap_or(18.0)});
    if frame_spec["justification"] == "center" { props["justification"] = json!("CENTER_ALIGN"); }
    if let Some(v) = frame_spec.get("spaceBefore") { props["spaceBefore"] = v.clone(); }
    ed.apply("format", &[command(json!({"op": "textFormat", "story": story, "start": 0, "end": text.chars().count(), "props": props}))]).unwrap();
    ed.set_font_records(Vec::new());
    let mut options = serde_json::Map::new();
    for key in ["insets", "columns", "gutter"] { if let Some(v) = frame_spec.get(key) { options.insert(key.into(), v.clone()); } }
    if !options.is_empty() { ed.apply("options", &[command(json!({"op": "textFrameOptions", "id": t, "options": options}))]).unwrap(); }
    let kind = match object["kind"].as_str().unwrap() { "oval" => "ellipse", other => other };
    let b = create(&mut ed, kind, rect(&object["rect"]));
    if let Some(w) = object.get("stroke") { ed.apply("stroke", &[command(json!({"op": "setPaint", "ids": [b], "stroke": "Color/Black", "strokeWeight": w}))]).unwrap(); }
    if let Some(a) = object.get("rotate") { ed.apply("rotate", &[command(json!({"op": "rotate", "ids": [b], "by": a.as_f64().unwrap()}))]).unwrap(); }
    Some(Built { ed, path, story, frame: t, object: b })
}

/// Anchors the object at `at` the way the screen does: the composition as it is now is handed to the editor first.
fn drop(b: &mut Built, at: usize) -> Result<(), String> {
    let doc = read(&b.ed, &b.path);
    let layout = folio_text::layout_document(&doc, fonts());
    b.ed.set_anchor_targets(&doc, &layout);
    b.ed.apply("anchor", &[command(json!({"op": "anchorObject", "id": b.object, "story": b.story, "at": at}))]).map(|_| ()).map_err(|e| e.message)
}

#[test]
fn the_measured_drops_anchor_as_in_indesign() {
    let all: Value = serde_json::from_str(include_str!("../../../../oracle/measured/anchor-control-2025.json")).unwrap();
    let mut checked = 0;
    let mut problems = Vec::new();
    for case in all["cases"].as_array().unwrap() {
        let label = case["label"].as_str().unwrap();
        let Some(frame_spec) = case.get("frame") else { continue }; // a turned text frame: see the last test
        let Some(mut built) = build(frame_spec, &case["object"]) else { return };
        let b = built.object.clone();
        let doc = read(&built.ed, &built.path);
        let layout = folio_text::layout_document(&doc, fonts());
        let before = bounds_on_spread(&doc, &layout, &b);
        let at = case["at"].as_u64().unwrap() as usize;
        if let Err(e) = drop(&mut built, at) { problems.push(format!("{label}: {e}")); continue; }
        let ed = &mut built.ed;
        let got = ed.anchored_object(&b).unwrap();
        for (key, measured) in [("anchorXoffset", &case["xOffset"]), ("anchorYoffset", &case["yOffset"])] {
            let (have, want) = (got["props"][key].as_f64().unwrap(), measured.as_f64().unwrap());
            if (have - want).abs() > 0.06 { problems.push(format!("{label}: {key} {have} (InDesign {want})")); }
        }
        for (key, want) in [("anchoredPosition", "Anchored"), ("anchorPoint", "BottomRightAnchor"), ("horizontalReferencePoint", "TextFrame"), ("verticalReferencePoint", "LineBaseline"),
            ("horizontalAlignment", "LeftAlign"), ("verticalAlignment", "TopAlign")] {
            if got["props"][key] != want { problems.push(format!("{label}: {key} {} (InDesign {want})", got["props"][key])); }
        }
        if got["props"]["pinPosition"] != false { problems.push(format!("{label}: pinPosition is on")); }
        if got["at"].as_u64() != Some(at as u64) { problems.push(format!("{label}: anchored at {} not {at}", got["at"])); }
        // the object stays where it was
        let doc2 = read(ed, &built.path);
        let layout2 = folio_text::layout_document(&doc2, fonts());
        let after = bounds_on_spread(&doc2, &layout2, &b);
        for (side, x, y) in [("left", before.left, after.left), ("top", before.top, after.top), ("right", before.right, after.right), ("bottom", before.bottom, after.bottom)] {
            if (x - y).abs() > 0.06 { problems.push(format!("{label}: the object moved ({side} {x} -> {y})")); }
        }
        // the ItemTransform InDesign wrote in the story
        if let Some(want) = case.get("idml").and_then(|i| i.get("itemTransform")) {
            let have: Vec<f64> = node_attr(ed, &b, "ItemTransform").unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
            let want: Vec<f64> = want.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            if have.iter().zip(&want).any(|(x, y)| (x - y).abs() > 0.06) { problems.push(format!("{label}: ItemTransform {have:?} (InDesign {want:?})")); }
        }
        // one undo step brings the object back to the spread
        ed.undo().unwrap();
        let back = read(ed, &built.path);
        if !back.spreads.iter().any(|sp| find(&sp.items, &b, Affine::IDENTITY).is_some()) { problems.push(format!("{label}: undo did not bring the object back to the spread")); }
        checked += 1;
    }
    assert!(problems.is_empty(), "{} of {checked} measured drops differ:\n{}", problems.len(), problems.join("\n"));
    assert!(checked >= 14, "only {checked} cases replayed");
}

/// An anchored object dragged to another place moves its anchor (InDesign: 20 then 150 leaves it at 149, the first anchor character having gone).
#[test]
fn an_anchored_object_dragged_on_moves_its_anchor() {
    let all: Value = serde_json::from_str(include_str!("../../../../oracle/measured/anchor-control-2025.json")).unwrap();
    let base = all["cases"].as_array().unwrap().iter().find(|c| c["label"] == "k-base-ip30").unwrap();
    let mut seen = 0;
    for mv in all["moves"].as_array().unwrap() {
        let Some(mut built) = build(&base["frame"], &base["object"]) else { return };
        for at in mv["drops"].as_array().unwrap() { drop(&mut built, at.as_u64().unwrap() as usize).unwrap(); }
        let got = built.ed.anchored_object(&built.object).unwrap();
        assert_eq!(got["at"], mv["at"], "{}", mv["label"]);
        for (key, measured) in [("anchorXoffset", &mv["xOffset"]), ("anchorYoffset", &mv["yOffset"])] {
            assert!((got["props"][key].as_f64().unwrap() - measured.as_f64().unwrap()).abs() < 0.06, "{} {key}: {} (InDesign {measured})", mv["label"], got["props"][key]);
        }
        // one anchor character, and one undo step per drop
        let story = built.ed.parts().iter().find(|(n, _)| n.starts_with("Stories/") && n.contains(&built.story)).unwrap().1.clone();
        assert_eq!(story.matches(&format!("Self=\"{}\"", built.object)).count(), 1);
        built.ed.undo().unwrap();
        seen += 1;
    }
    assert_eq!(seen, 2);
}

/// The object's matrix on the spread, wherever it is: on the spread or anchored in text (the frame's matrix after the object's own, as laid out).
fn world_of(doc: &folio_model::Document, layout: &folio_model::layout::DocumentLayout, id: &str) -> Affine {
    let anchored = folio_model::layout::anchored_items(doc, layout);
    for spread in &doc.spreads {
        if let Some((_, m)) = find(&spread.items, id, Affine::IDENTITY) { return m; }
        for item in &spread.items {
            if let Some(children) = anchored.get(&item.id) && let Some(child) = children.iter().find(|c| c.id == id) {
                let (_, fm) = find(&spread.items, &item.id, Affine::IDENTITY).unwrap();
                return fm.then_inner(&child.transform);
            }
        }
    }
    panic!("{id} is nowhere");
}

/// Dropping into a turned or flipped text frame (InDesign 2025, rules/anchorturn-c1.ko.md): the object keeps its own turn, scale and flip, and the
/// place of its origin on the spread; it takes the frame's turn about that origin. The offsets for InDesign's own numbers are checked against the
/// measurements in the unit test of `anchoring`; here the whole path (offsets written, composition, undo) must put the object where that law says.
fn turned_drop_keeps_the_origin(frame_turn: Value, object_turn: Option<f64>) {
    let all: Value = serde_json::from_str(include_str!("../../../../oracle/measured/anchor-control-2025.json")).unwrap();
    let base = all["cases"].as_array().unwrap().iter().find(|c| c["label"] == "k-base-ip30").unwrap();
    let mut object = base["object"].clone();
    if let Some(a) = object_turn { object["rotate"] = json!(a); }
    let Some(mut built) = build(&base["frame"], &object) else { return };
    let (frame, b) = (built.frame.clone(), built.object.clone());
    built.ed.apply("turn", &[command(frame_turn.as_object().map(|o| { let mut o = o.clone(); o.insert("ids".into(), json!([frame])); Value::Object(o) }).unwrap())]).unwrap();
    let doc = read(&built.ed, &built.path);
    let layout = folio_text::layout_document(&doc, fonts());
    let (q, before) = (world_of(&doc, &layout, &frame), world_of(&doc, &layout, &b));
    drop(&mut built, 30).unwrap();
    let doc2 = read(&built.ed, &built.path);
    let layout2 = folio_text::layout_document(&doc2, fonts());
    let after = world_of(&doc2, &layout2, &b);
    // the origin stays, the linear part is the frame's after the object's own
    let want = Affine::new(q.a, q.b, q.c, q.d, 0.0, 0.0).then_inner(&Affine::new(before.a, before.b, before.c, before.d, 0.0, 0.0));
    for (name, x, y) in [("a", after.a, want.a), ("b", after.b, want.b), ("c", after.c, want.c), ("d", after.d, want.d), ("tx", after.tx, before.tx), ("ty", after.ty, before.ty)] {
        assert!((x - y).abs() < 0.02, "{name}: {x} but the law says {y}
  before {before:?}
  after {after:?}");
    }
    // one undo step brings the object back to the spread
    built.ed.undo().unwrap();
    let back = read(&built.ed, &built.path);
    assert!(back.spreads.iter().any(|sp| find(&sp.items, &b, Affine::IDENTITY).is_some()));
}

#[test]
fn a_turned_text_frame_takes_the_object_about_its_origin() {
    turned_drop_keeps_the_origin(json!({"op": "rotate", "by": 30}), None);
    turned_drop_keeps_the_origin(json!({"op": "rotate", "by": -30, "anchor": "topLeft"}), None);
    turned_drop_keeps_the_origin(json!({"op": "rotate", "by": 90}), None);
}

#[test]
fn a_flipped_text_frame_takes_the_object_about_its_origin() {
    turned_drop_keeps_the_origin(json!({"op": "flip", "axis": "horizontal"}), None);
    turned_drop_keeps_the_origin(json!({"op": "flip", "axis": "vertical"}), None);
}

#[test]
fn a_turned_object_in_a_turned_frame_keeps_its_own_turn() {
    turned_drop_keeps_the_origin(json!({"op": "rotate", "by": 30}), Some(20.0));
}

/// InDesign shows no control on a locked object; the engine refuses it too and leaves the document as it was.
#[test]
fn a_locked_object_is_refused() {
    let all: Value = serde_json::from_str(include_str!("../../../../oracle/measured/anchor-control-2025.json")).unwrap();
    let base = all["cases"].as_array().unwrap().iter().find(|c| c["label"] == "k-base-ip30").unwrap();
    let Some(mut built) = build(&base["frame"], &base["object"]) else { return };
    let object = built.object.clone();
    built.ed.apply("lock", &[command(json!({"op": "setItem", "id": object, "locked": true}))]).unwrap();
    let before = built.ed.parts().clone();
    let err = drop(&mut built, 30).unwrap_err();
    assert!(err.contains("locked"), "{err}");
    assert_eq!(built.ed.parts(), &before);
}
