//! Transform panel ▸ "Transformations are Totals" (InDesign 20.5.5.119, rules `next/oracle/rules/transform-totals.ko.md`):
//! the rotation / shear a person types into the panel for an object inside a group. Off, the value is the object's own (relative to
//! its group); on, the object's total on the page. Measured with real input into the Transform panel
//! (artifacts/smallmarks-c1-20261023/tools/t6_child.py, out/t6_input_ADJUST.json): a group scaled 150 % and turned 30°, its child
//! rectangle (own transform the identity, so 30° and 150 % in total):
//!   off, typed rotation 45 -> relative rotation 45°, total 75°;  typed 0 -> relative 0°, total 30°
//!   on,  typed rotation 45 -> total 45°, relative 15°;           typed 0 -> total 0°, relative -30°
//! The document is the one InDesign made for that measurement (`measured/transform-totals/child-in-group.idml`).
use folio_edit::{commands::RegisteredCommand, Anchor, Command, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

fn load() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/transform-totals/child-in-group.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len()).filter_map(|i| {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() { return None; }
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        Some((name, text))
    }).collect()
}

const SPREAD: &str = "Spreads/Spread_ucf.xml";
type M = [f64; 6];

/// The ItemTransform of the element `<tag Self="id" ...>`.
fn transform_of(spread: &str, tag: &str, id: &str) -> M {
    let at = spread.find(&format!("<{tag} Self=\"{id}\"")).unwrap();
    let open = &spread[at..at + spread[at..].find('>').unwrap()];
    let value = open.split("ItemTransform=\"").nth(1).unwrap().split('"').next().unwrap();
    let v: Vec<f64> = value.split_whitespace().map(|n| n.parse().unwrap()).collect();
    [v[0], v[1], v[2], v[3], v[4], v[5]]
}

/// `own` after `parent` the way IDML composes them: a point goes through `own` first.
fn then(parent: &M, own: &M) -> M {
    [
        parent[0] * own[0] + parent[2] * own[1], parent[1] * own[0] + parent[3] * own[1],
        parent[0] * own[2] + parent[2] * own[3], parent[1] * own[2] + parent[3] * own[3],
        parent[0] * own[4] + parent[2] * own[5] + parent[4], parent[1] * own[4] + parent[3] * own[5] + parent[5],
    ]
}

/// Counter-clockwise rotation in degrees (what the panel shows).
fn rotation(m: &M) -> f64 { -m[1].atan2(m[0]).to_degrees() }

fn type_rotation(totals: bool, degrees: f64) -> (f64, f64, Editor) {
    let mut ed = Editor::new(load());
    let cmd: Command = RegisteredCommandless::angles("u115", degrees, totals);
    ed.apply("rotation", &[cmd]).unwrap();
    let spread = ed.part(SPREAD).unwrap().to_owned();
    let group = transform_of(&spread, "Group", "u117");
    let own = transform_of(&spread, "Rectangle", "u115");
    (rotation(&own), rotation(&then(&group, &own)), ed)
}

struct RegisteredCommandless;
impl RegisteredCommandless {
    fn angles(id: &str, rotation: f64, totals: bool) -> Command {
        Command::SetAngles { id: id.into(), rotation: Some(rotation), shear: None, anchor: Anchor::TopLeft, content: false, totals }
    }
}

fn near(a: f64, b: f64, what: &str) { assert!((a - b).abs() < 1e-6, "{what}: {a} vs {b}"); }

#[test]
fn the_group_is_a_150_percent_30_degree_turn_and_the_child_carries_none_of_it() {
    let spread = load()[SPREAD].clone();
    let group = transform_of(&spread, "Group", "u117");
    let own = transform_of(&spread, "Rectangle", "u115");
    near(rotation(&group), 30.0, "group rotation");
    near(group[0].hypot(group[1]), 1.5, "group scale");
    near(rotation(&own), 0.0, "own rotation");
}

#[test]
fn off_a_typed_rotation_is_relative_to_the_group() {
    let (own, total, _) = type_rotation(false, 45.0);
    near(own, 45.0, "own (the panel's value)");
    near(total, 75.0, "total");
    let (own, total, _) = type_rotation(false, 0.0);
    near(own, 0.0, "own");
    near(total, 30.0, "total");
}

#[test]
fn on_a_typed_rotation_is_the_total_on_the_page() {
    let (own, total, _) = type_rotation(true, 45.0);
    near(total, 45.0, "total (the panel's value)");
    near(own, 15.0, "own");
    let (own, total, _) = type_rotation(true, 0.0);
    near(total, 0.0, "total");
    near(own, -30.0, "own");
}

#[test]
fn the_reference_point_stays_on_the_page_either_way_and_one_undo_gives_the_document_back() {
    // the rectangle's top-left corner (anchor 55 295, no stroke) before and after, through its own and its group's transform
    let page_point = |ed: &Editor| {
        let spread = ed.part(SPREAD).unwrap();
        let group = transform_of(spread, "Group", "u117");
        let own = transform_of(spread, "Rectangle", "u115");
        let full = then(&group, &own);
        (full[0] * 55.0 + full[2] * 295.0 + full[4], full[1] * 55.0 + full[3] * 295.0 + full[5])
    };
    let before = page_point(&Editor::new(load()));
    for totals in [false, true] {
        let (_, _, mut ed) = type_rotation(totals, 45.0);
        let after = page_point(&ed);
        near(after.0, before.0, "x");
        near(after.1, before.1, "y");
        ed.undo().unwrap();
        assert_eq!(ed.parts(), &load(), "totals {totals}: one undo");
    }
}

#[test]
fn a_command_without_the_totals_field_is_the_total_one_as_before() {
    // the registered wire form: older callers (and the oracle) do not send `totals`
    let json = serde_json::json!({"op": "setAngles", "id": "u115", "rotation": 45.0, "anchor": "topLeft"});
    let cmd: Command = serde_json::from_value(json).unwrap();
    let mut ed = Editor::new(load());
    ed.apply("rotation", &[cmd]).unwrap();
    let spread = ed.part(SPREAD).unwrap().to_owned();
    let total = rotation(&then(&transform_of(&spread, "Group", "u117"), &transform_of(&spread, "Rectangle", "u115")));
    near(total, 45.0, "total");
    let _ = RegisteredCommand::recognizes("setAngles");
}
