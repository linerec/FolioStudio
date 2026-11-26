use folio_edit::{Command, Editor, xml::Index};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn command(v: Value) -> Command { serde_json::from_value(v).unwrap() }

fn fixture() -> (Editor, String, Vec<String>) {
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
    let mut ed = Editor::new(parts);
    let mut ids = Vec::new();
    for rect in [[462.,200.,502.,280.],[518.,200.,558.,280.]] {
        ids.extend(ed.apply("create", &[command(json!({"op":"createItem","page":page,"kind":"rectangle","rect":rect}))]).unwrap().created);
    }
    (ed, page, ids)
}

fn drag(page: &str, to: [f64; 2], resize: bool) -> Command {
    command(json!({"op":"adjustGap","page":page,"from":[510.,240.],"to":to,"nearest":false,"resize":resize,"move_items":false}))
}

#[test]
fn gap_is_one_undoable_transaction() {
    let (mut ed, page, _) = fixture();
    let before = ed.parts().clone();
    ed.apply("gap", &[drag(&page, [516.,240.], false)]).unwrap();
    let after = ed.parts().clone();
    assert_ne!(before, after);
    assert_eq!(ed.undo_label(), Some("gap"));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

#[test]
fn refused_gap_drags_preserve_geometry_and_history() {
    let (mut ed, _page, _) = fixture();
    let before = ed.parts().clone();
    // (Ctrl = `resize` is accepted and changes nothing: InDesign 2025 does not tell it from a plain drag)
    {
        let c = drag("missing", [516.,240.], false);
        assert!(ed.apply("invalid gap", &[c]).is_err());
        assert_eq!(ed.parts(), &before);
        assert_eq!(ed.undo_label(), Some("create"));
    }
}

// These are the existing command's safety boundaries, not measurements of native mouse targeting.
fn unchanged_after_error(ed: &mut Editor, commands: &[Command], code: &str) {
    let before = ed.parts().clone();
    let rev = ed.rev();
    let undo = ed.undo_label().map(str::to_owned);
    let redo = ed.redo_label().map(str::to_owned);
    let recompose = ed.recompose().clone();
    let result = ed.apply("refused gap", commands);
    assert!(result.is_err(), "unexpected success for {commands:?}");
    assert_eq!(result.unwrap_err().code, code);
    assert_eq!(ed.parts(), &before);
    assert_eq!(ed.rev(), rev);
    assert_eq!(ed.undo_label(), undo.as_deref());
    assert_eq!(ed.redo_label(), redo.as_deref());
    assert_eq!(ed.recompose(), &recompose);
}

#[test]
fn gap_rejects_edges_inside_items_and_points_without_neighbours_without_losing_redo() {
    let (mut ed, page, ids) = fixture();
    ed.apply("gap", &[drag(&page, [516.,240.], false)]).unwrap();
    let after = ed.parts().clone();
    ed.undo().unwrap();
    for from in [[502.,240.], [518.,240.], [480.,240.], [510.,200.], [510.,10000.]] {
        let c = command(json!({"op":"adjustGap","page":page,"from":from,"to":[516.,240.],"nearest":false,"resize":false,"move_items":false}));
        unchanged_after_error(&mut ed, &[c], "notFound");
    }
    unchanged_after_error(&mut ed, &[drag(&ids[0], [516.,240.], false)], "invalid");
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

// InDesign 2026 (measured, rules/gaptool.ko.md): the items the gap moves into keep 8 pt instead of the drag being refused.
#[test]
fn gap_drag_into_an_item_stops_at_the_minimum_size() {
    let (mut ed, page, ids) = fixture();
    for (to, left_width, right_width) in [([470., 240.], 8., 72.), ([469., 240.], 8., 72.), ([550., 240.], 72., 8.), ([551., 240.], 72., 8.)] {
        ed.apply("gap", &[drag(&page, to, false)]).unwrap();
        let (a, b) = (ed.bounds(&ids[0]).unwrap(), ed.bounds(&ids[1]).unwrap());
        assert!((a[2] - a[0] - left_width).abs() < 1e-9 && (b[2] - b[0] - right_width).abs() < 1e-9, "{to:?}: {a:?} {b:?}");
        ed.undo().unwrap();
    }
}

// (A pointer in empty space with an item on one side only does nothing, except in the 36 pt strip along the page edge: the measured cases
// one_* and strip_* in gap-tool-2025.json, tests/gap_measured.rs.)
#[test]
fn ctrl_and_alt_make_no_difference_to_the_drag() {
    // InDesign 2025, real keys held before, during and over the gap: the same result as a plain drag
    let (plain, page, ids) = fixture();
    let mut plain = plain;
    plain.apply("gap", &[drag(&page, [516.,240.], false)]).unwrap();
    for (resize, moving) in [(true, false), (false, true), (true, true)] {
        let (mut ed, page, _) = fixture();
        ed.apply("gap", &[command(json!({"op":"adjustGap","page":page,"from":[510.,240.],"to":[516.,240.],"nearest":false,"resize":resize,"move_items":moving}))]).unwrap();
        for id in &ids { assert_eq!(ed.bounds(id).unwrap(), plain.bounds(id).unwrap()); }
    }
}

#[test]
fn gap_rejects_nonfinite_coordinates_even_for_zero_axis_motion() {
    use folio_edit::commands::RegisteredCommand;
    let (mut ed, page, _) = fixture();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for index in 0..4 {
            let mut points = [510.,240.,510.,240.];
            points[index] = bad;
            let c = Command::Registered(RegisteredCommand::AdjustGap {
                page: page.clone(), from: [points[0], points[1]], to: [points[2], points[3]],
                nearest: false, resize: false, move_items: false,
            });
            unchanged_after_error(&mut ed, &[c], "invalid");
        }
    }
}

#[test]
fn a_failing_gap_rolls_back_an_earlier_command_in_the_transaction() {
    let (mut ed, _page, ids) = fixture();
    unchanged_after_error(&mut ed, &[
        Command::Move { ids: vec![ids[0].clone()], by: [-2.,0.] },
        drag("missing", [516.,240.], false),
    ], "notFound");
}

#[test]
fn a_turned_neighbour_stops_the_drag_and_a_group_gives_its_children_to_the_gap() {
    let (ed, page, ids) = fixture();
    // InDesign 2025 (real mouse), document WITHOUT facing pages: a neighbour turned by 5, 20 degrees ... stops the drag, nothing happens
    let mut single = ed.parts().clone();
    let prefs = single.get_mut("Resources/Preferences.xml").unwrap();
    *prefs = prefs.replace("FacingPages=\"true\"", "FacingPages=\"false\"");
    let mut rotated = Editor::new(single);
    rotated.apply("rotate", &[Command::Rotate { ids: vec![ids[0].clone()], by: 1., anchor: folio_edit::Anchor::Center }]).unwrap();
    unchanged_after_error(&mut rotated, &[drag(&page, [516.,240.], false)], "notFound");
    // with facing pages the turned neighbour stays and the other side moves
    let mut turned = Editor::new(ed.parts().clone());
    turned.apply("rotate", &[Command::Rotate { ids: vec![ids[0].clone()], by: 1., anchor: folio_edit::Anchor::Center }]).unwrap();
    let (turned_before, other_before) = (turned.bounds(&ids[0]).unwrap(), turned.bounds(&ids[1]).unwrap());
    turned.apply("gap", &[drag(&page, [516.,240.], false)]).unwrap();
    assert_eq!(turned.bounds(&ids[0]).unwrap(), turned_before);
    assert!((turned.bounds(&ids[1]).unwrap()[0] - other_before[0] - 6.0).abs() < 1e-6);

    // InDesign 2025 (docinput-c3): a group beside the gap gives its edge to the gap and its children are resized
    let mut grouped = Editor::new(ed.parts().clone());
    let extra = grouped.apply("extra", &[command(json!({"op":"createItem","page":page,"kind":"rectangle","rect":[462.,300.,502.,340.]}))]).unwrap().created;
    grouped.apply("group", &[Command::Group { ids: vec![ids[0].clone(), extra[0].clone()] }]).unwrap();
    let before = [grouped.bounds(&ids[0]).unwrap(), grouped.bounds(&extra[0]).unwrap()];
    grouped.apply("gap", &[drag(&page, [516.,240.], false)]).unwrap();
    let after = [grouped.bounds(&ids[0]).unwrap(), grouped.bounds(&extra[0]).unwrap()];
    let right = |b: &[[f64; 4]; 2]| b[0][2].max(b[1][2]);
    assert!((right(&after) - right(&before) - 6.0).abs() < 1e-6, "the group's right edge goes with the gap: {before:?} -> {after:?}");
}

#[test]
fn a_turned_item_on_the_same_line_is_left_where_it_is_while_the_others_move() {
    // InDesign 2025 (docinput-c3, c9c): two rows, the gap between the columns; the second row's right item is turned 20 degrees: the other three
    // move with the gap, the turned one stays (it was turned about its top left corner, so its box keeps its left edge on the line)
    let (ed, page, ids) = fixture();
    let mut ed = Editor::new(ed.parts().clone());
    let b0 = ed.bounds(&ids[0]).unwrap();
    let b1 = ed.bounds(&ids[1]).unwrap();
    let made = ed.apply("rows", &[
        command(json!({"op":"createItem","page":page,"kind":"rectangle","rect":[462.,300.,502.,340.]})),
        command(json!({"op":"createItem","page":page,"kind":"rectangle","rect":[518.,300.,558.,340.]})),
    ]).unwrap().created;
    let (left_low, right_low) = (made[0].clone(), made[1].clone());
    ed.apply("turn", &[Command::Rotate { ids: vec![right_low.clone()], by: 20., anchor: folio_edit::Anchor::TopLeft }]).unwrap();
    let turned_before = ed.bounds(&right_low).unwrap();
    let lower_left_before = ed.bounds(&left_low).unwrap();
    ed.apply("gap", &[drag(&page, [516.,240.], false)]).unwrap();
    assert_eq!(ed.bounds(&right_low).unwrap(), turned_before, "the turned item stays");
    let lower_left_after = ed.bounds(&left_low).unwrap();
    assert!((lower_left_after[2] - lower_left_before[2] - 6.0).abs() < 1e-6, "the item beside it on the same line goes with the gap: {lower_left_before:?} -> {lower_left_after:?}");
    assert!((ed.bounds(&ids[0]).unwrap()[2] - b0[2] - 6.0).abs() < 1e-6 && (ed.bounds(&ids[1]).unwrap()[0] - b1[0] - 6.0).abs() < 1e-6);
}

#[test]
fn gap_negative_fractional_delta_survives_reopening_and_ignores_cross_axis_destination() {
    let (mut ed, page, ids) = fixture();
    let original = ed.parts().clone();
    let before: Vec<_> = ids.iter().map(|id| ed.bounds(id).unwrap()).collect();
    ed.apply("negative gap", &[drag(&page, [503.75, -1000.], false)]).unwrap();
    let mut expected = before;
    expected[0][2] -= 6.25;
    expected[1][0] -= 6.25;
    let mut reopened = Editor::new(ed.parts().clone());
    for (id, bounds) in ids.iter().zip(expected) {
        let actual = reopened.bounds(id).unwrap();
        assert!(actual.iter().zip(bounds).all(|(a,b)| (a-b).abs() < 1e-8), "{actual:?} != {bounds:?}");
    }
    let mut straight = Editor::new(original);
    straight.apply("negative gap", &[drag(&page, [503.75,240.], false)]).unwrap();
    assert_eq!(ed.parts(), straight.parts());
}
