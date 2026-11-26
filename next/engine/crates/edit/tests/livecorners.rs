//! Live corners (`liveCornerDrag`, `cycleCornerShape`): the commands on a real frame, undo as one step, and a text frame's inset.
//! The rules and numbers are in `src/ops/livecorners.rs`; the measured InDesign 2025 cases are in `live_corner_measured.rs`.
use folio_edit::{Command, Editor, xml::Index};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn command(v: Value) -> Command { serde_json::from_value(v).unwrap() }

fn fixture() -> (Editor, String) {
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
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&text, i) == "Page") { page = ix.value(&text, i, "Self").unwrap(); }
        }
        parts.insert(name, text);
    }
    (Editor::new(parts), page)
}

fn frame(ed: &mut Editor, page: &str, kind: &str, rect: [f64; 4]) -> String {
    ed.apply("create", &[command(json!({"op":"createItem","page":page,"kind":kind,"rect":rect}))]).unwrap().created[0].clone()
}

/// (shape, radius) of the four corners as the item's attributes say (none written: no shape, 12 pt).
fn corners(ed: &Editor, id: &str) -> Vec<(String, f64)> {
    for t in ed.parts().values() {
        let Ok(ix) = Index::scan(t) else { continue };
        let Some(i) = ix.by_self(t, id) else { continue };
        return ["TopLeft", "TopRight", "BottomLeft", "BottomRight"].iter()
            .map(|c| (
                ix.attr(t, i, &format!("{c}CornerOption")).unwrap_or("None").to_owned(),
                ix.attr(t, i, &format!("{c}CornerRadius")).and_then(|v| v.parse().ok()).unwrap_or(12.0),
            ))
            .collect();
    }
    panic!("{id} not found")
}

fn drag(ed: &mut Editor, id: &str, corner: usize, by: f64, only: bool) {
    ed.apply("drag", &[command(json!({"op":"liveCornerDrag","ids":[id],"corner":corner,"by":by,"only_corner":only}))]).unwrap();
}

fn cycle(ed: &mut Editor, id: &str, corner: usize, only: bool) {
    ed.apply("cycle", &[command(json!({"op":"cycleCornerShape","ids":[id],"corner":corner,"only_corner":only}))]).unwrap();
}

#[test]
fn a_drag_rounds_every_corner_and_is_one_undo_step() {
    let (mut ed, page) = fixture();
    let id = frame(&mut ed, &page, "rectangle", [100., 100., 300., 200.]);
    assert!(corners(&ed, &id).iter().all(|c| c.0 == "None"));
    drag(&mut ed, &id, 0, 25.0, false);
    assert!(corners(&ed, &id).iter().all(|c| c == &("RoundedCorner".to_string(), 25.0)), "{:?}", corners(&ed, &id));
    // dragged from the top left inward again by 10: the corner has a shape now, so it starts from its 25 pt
    drag(&mut ed, &id, 0, 10.0, false);
    assert!(corners(&ed, &id).iter().all(|c| c.1 == 35.0));
    // past half the shorter side (100 / 2) it stops
    drag(&mut ed, &id, 3, 400.0, false);
    assert!(corners(&ed, &id).iter().all(|c| c.1 == 50.0));
    // and outward past the start it ends at 0, still a rounded corner
    drag(&mut ed, &id, 1, -400.0, false);
    assert!(corners(&ed, &id).iter().all(|c| c == &("RoundedCorner".to_string(), 0.0)));
    // each drag is one step
    ed.undo().unwrap();
    assert!(corners(&ed, &id).iter().all(|c| c.1 == 50.0));
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert!(corners(&ed, &id).iter().all(|c| c.1 == 25.0));
    ed.undo().unwrap();
    assert!(corners(&ed, &id).iter().all(|c| c.0 == "None"));
}

#[test]
fn shift_drags_one_corner_and_the_others_keep_their_shape_and_size() {
    let (mut ed, page) = fixture();
    let id = frame(&mut ed, &page, "rectangle", [100., 100., 300., 200.]);
    drag(&mut ed, &id, 0, 20.0, false);
    cycle(&mut ed, &id, 3, true); // bottom right: next shape after rounded is fancy
    assert_eq!(corners(&ed, &id)[3].0, "FancyCorner");
    assert_eq!(corners(&ed, &id)[0].0, "RoundedCorner");
    drag(&mut ed, &id, 1, 8.0, true);
    let c = corners(&ed, &id);
    assert_eq!((c[1].0.as_str(), c[1].1), ("RoundedCorner", 28.0));
    assert_eq!(c.iter().map(|k| k.1).collect::<Vec<_>>(), vec![20.0, 28.0, 20.0, 20.0]);
    // without Shift they all follow the dragged one (a bevel next to rounded corners stays a bevel)
    cycle(&mut ed, &id, 2, true);
    cycle(&mut ed, &id, 2, true);
    assert_eq!(corners(&ed, &id)[2].0, "BevelCorner");
    drag(&mut ed, &id, 2, 5.0, false);
    assert_eq!(corners(&ed, &id).iter().map(|k| (k.0.as_str(), k.1)).collect::<Vec<_>>(),
        vec![("RoundedCorner", 25.0), ("RoundedCorner", 25.0), ("BevelCorner", 25.0), ("FancyCorner", 25.0)]);
}

#[test]
fn alt_click_goes_round_the_six_shapes_for_all_corners() {
    let (mut ed, page) = fixture();
    let id = frame(&mut ed, &page, "rectangle", [100., 100., 300., 200.]);
    let mut seen = Vec::new();
    for _ in 0..7 {
        cycle(&mut ed, &id, 0, false);
        let c = corners(&ed, &id);
        assert!(c.iter().all(|k| k.0 == c[0].0 && k.1 == 12.0), "{c:?}");
        seen.push(c[0].0.clone());
    }
    assert_eq!(seen, ["RoundedCorner", "FancyCorner", "BevelCorner", "InsetCorner", "InverseRoundedCorner", "None", "RoundedCorner"]);
}

#[test]
fn a_text_frame_takes_the_corners_too_and_other_items_are_refused() {
    let (mut ed, page) = fixture();
    let tf = frame(&mut ed, &page, "textFrame", [100., 100., 260., 160.]);
    drag(&mut ed, &tf, 2, 18.0, false);
    assert!(corners(&ed, &tf).iter().all(|c| c == &("RoundedCorner".to_string(), 18.0)));
    // 160 x 60: half the shorter side is 30
    drag(&mut ed, &tf, 0, 100.0, false);
    assert!(corners(&ed, &tf).iter().all(|c| c.1 == 30.0));
    assert!(ed.apply("bad", &[command(json!({"op":"liveCornerDrag","ids":[tf],"corner":4,"by":1.0,"only_corner":false}))]).is_err());
    let line = frame(&mut ed, &page, "line", [100., 300., 200., 300.]);
    assert!(ed.apply("bad", &[command(json!({"op":"cycleCornerShape","ids":[line],"corner":0,"only_corner":false}))]).is_err());
}
