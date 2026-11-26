//! The Type on a Path tool on a closed shape and the bracket drags (InDesign 2025: next/oracle/rules/typepath-c8.ko.md sections 2 and 4).
use folio_edit::{Command, Editor};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

fn named_fixture(name: &str) -> Editor {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../oracle/fixtures/{name}.idml"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut entry = zip.by_index(k).unwrap();
        if entry.is_dir() { continue; }
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), text);
    }
    Editor::new(parts)
}

fn command(value: serde_json::Value) -> Command { serde_json::from_value(value).unwrap() }

/// A shape of a fixture with its text taken off, ready for the tool.
fn bare(name: &str) -> (Editor, String) {
    let mut ed = named_fixture(name);
    let id = ed.text_path_items().unwrap().remove(0);
    ed.apply("remove type", &[command(json!({"op":"removeTextFromPath","ids":[id]}))]).unwrap();
    (ed, id)
}

fn props(ed: &Editor, id: &str) -> BTreeMap<String, String> {
    serde_json::from_value(ed.text_path_options(id).unwrap()["props"].clone()).unwrap()
}

#[test]
fn a_press_on_a_clockwise_shape_starts_there_and_ends_one_point_behind() {
    // clockwise rectangle (60,-96) (540,-96) (540,104) (60,104): the top edge at x = 360 is 300 along the outline
    let (mut ed, id) = bare("typepath-c6-rect-cw");
    ed.apply("press", &[command(json!({"op":"createTextPathAt","id":id,"point":[360.0,-96.0],"reach":1.0,"text":"Hello"}))]).unwrap();
    let p = props(&ed, &id);
    assert_eq!((p["StartBracket"].as_str(), p["EndBracket"].as_str()), ("300", "299"));
    assert_eq!(p["FlipPathEffect"], "NotFlipped");
    assert!(ed.text_path_options(&id).unwrap()["supported"].as_bool().unwrap());
    ed.undo().unwrap();
    assert!(ed.text_path_items().unwrap().is_empty());
}

#[test]
fn a_press_within_the_first_point_wraps_the_end_bracket_round_to_the_path_end() {
    let (mut ed, id) = bare("typepath-c6-rect-cw");
    ed.apply("press", &[command(json!({"op":"createTextPathAt","id":id,"point":[60.5,-96.0],"reach":1.0,"text":""}))]).unwrap();
    let p = props(&ed, &id);
    assert_eq!(p["StartBracket"], "0.5");
    assert_eq!(p["EndBracket"], "1359.5");
}

#[test]
fn a_press_on_a_counter_clockwise_shape_ends_one_point_after_the_start() {
    // counter-clockwise rectangle (60,-96) (60,104) (540,104) (540,-96): the left edge at y = 4 is 100 along the outline
    let (mut ed, id) = bare("typepath-c6-rect");
    ed.apply("press", &[command(json!({"op":"createTextPathAt","id":id,"point":[60.0,4.0],"reach":1.0,"text":""}))]).unwrap();
    let p = props(&ed, &id);
    assert_eq!((p["StartBracket"].as_str(), p["EndBracket"].as_str()), ("100", "101"));
}

#[test]
fn a_press_off_the_outline_or_on_an_open_path_is_refused() {
    let (mut ed, id) = bare("typepath-c6-rect-cw");
    let before = ed.parts().clone();
    for point in [[360.0, 0.0], [360.0, -90.0], [0.0, 0.0]] {
        let error = ed.apply("press", &[command(json!({"op":"createTextPathAt","id":id,"point":point,"reach":1.0,"text":""}))]).unwrap_err();
        assert!(error.to_string().contains("notOnPath") || error.to_string().contains("not on the outline"), "{error}");
        assert_eq!(ed.parts(), &before);
    }
    let (mut line, line_id) = bare("tools2-path");
    assert!(line.apply("press", &[command(json!({"op":"createTextPathAt","id":line_id,"point":[100.0,-296.0],"reach":1.0,"text":""}))]).is_err());
}

#[test]
fn a_bracket_range_that_wraps_past_the_path_end_is_read_and_set() {
    let mut ed = named_fixture("typepath-c6-rect-cw");
    let id = ed.text_path_items().unwrap().remove(0);
    ed.apply("wrap", &[command(json!({"op":"setTextPathOptions","ids":[id],"props":{"StartBracket":"300","EndBracket":"299"}}))]).unwrap();
    assert!(ed.text_path_options(&id).unwrap()["supported"].as_bool().unwrap());
    // an open path still needs end after start
    let mut line = named_fixture("tools2-path");
    let line_id = line.text_path_items().unwrap().remove(0);
    assert!(line.apply("wrap", &[command(json!({"op":"setTextPathOptions","ids":[line_id],"props":{"StartBracket":"200","EndBracket":"100"}}))]).is_err());
}

#[test]
fn dragging_a_bracket_puts_it_where_the_pointer_lands_on_the_path() {
    // the line runs from (80,-296) to (320,-296); brackets 0 and 240
    let mut ed = named_fixture("tools2-path");
    let id = ed.text_path_items().unwrap().remove(0);
    let drag = |ed: &mut Editor, which: &str, to: [f64; 2], from: [f64; 2]| ed.apply("drag", &[command(json!({"op":"setTextPathBracket","id":id,"which":which,"point":to,"from":from}))]);
    ed.apply("set", &[command(json!({"op":"setTextPathOptions","ids":[id],"props":{"StartBracket":"40","EndBracket":"200"}}))]).unwrap();
    drag(&mut ed, "start", [150.0, -296.0], [0.0, 0.0]).unwrap();
    assert_eq!(props(&ed, &id)["StartBracket"], "70");
    // sideways off the path changes nothing in the value (the projection)
    drag(&mut ed, "start", [150.0, -266.0], [0.0, 0.0]).unwrap();
    assert_eq!(props(&ed, &id)["StartBracket"], "70");
    // past the other bracket it stops at it; past the path end the end bracket stops at the length
    drag(&mut ed, "start", [310.0, -296.0], [0.0, 0.0]).unwrap();
    assert_eq!(props(&ed, &id)["StartBracket"], "199.99");
    drag(&mut ed, "end", [900.0, -296.0], [0.0, 0.0]).unwrap();
    assert_eq!(props(&ed, &id)["EndBracket"], "240");
    drag(&mut ed, "start", [-40.0, -296.0], [0.0, 0.0]).unwrap();
    assert_eq!(props(&ed, &id)["StartBracket"], "0");
    // the centre bracket moves both by the pointer's travel along the path, and stops at the path end
    ed.apply("set", &[command(json!({"op":"setTextPathOptions","ids":[id],"props":{"StartBracket":"40","EndBracket":"100"}}))]).unwrap();
    drag(&mut ed, "center", [130.0, -297.0], [100.0, -297.0]).unwrap();
    let p = props(&ed, &id);
    assert_eq!((p["StartBracket"].as_str(), p["EndBracket"].as_str()), ("70", "130"));
    drag(&mut ed, "center", [1000.0, -297.0], [100.0, -297.0]).unwrap();
    let p = props(&ed, &id);
    assert_eq!((p["StartBracket"].as_str(), p["EndBracket"].as_str()), ("180", "240"));
    // across the path the text flips and the two values swap (InDesign 2025: 60 / 300 carried +28.51 below the path -> 328.51 / 88.51, Flipped)
    ed.apply("set", &[command(json!({"op":"setTextPathOptions","ids":[id],"props":{"StartBracket":"60","EndBracket":"200"}}))]).unwrap();
    drag(&mut ed, "center", [130.0, -270.0], [100.0, -296.0]).unwrap();
    let p = props(&ed, &id);
    assert_eq!((p["StartBracket"].as_str(), p["EndBracket"].as_str(), p["FlipPathEffect"].as_str()), ("230", "90", "Flipped"));
    assert!(ed.text_path_options(&id).unwrap()["supported"].as_bool().unwrap());
    // a flipped text path's brackets are not dragged (not measured)
    assert!(drag(&mut ed, "start", [150.0, -296.0], [0.0, 0.0]).is_err());
}

#[test]
fn dragging_a_bracket_round_a_closed_shape_wraps_like_indesign() {
    // InDesign 2025, 300 x 200 clockwise rectangle (perimeter 1000), brackets 100 and 600, Selection tool (typepath-c14 p5): the start bracket takes the
    // pointer's distance round the outline (200 / 400 / 600 / 900 / 0), past the end bracket too; the end bracket the same (50 / 100 / 700 / 900); the centre
    // moves both by the shorter way round (a drag from 945 to 20 moved them +75, not -925). Here the rectangle (60,-96) (540,-96) (540,104) (60,104): 1360 round.
    let mut ed = named_fixture("typepath-c6-rect-cw");
    let id = ed.text_path_items().unwrap().remove(0);
    let drag = |ed: &mut Editor, which: &str, to: [f64; 2], from: [f64; 2]| ed.apply("drag", &[command(json!({"op":"setTextPathBracket","id":id,"which":which,"point":to,"from":from}))]);
    ed.apply("set", &[command(json!({"op":"setTextPathOptions","ids":[id],"props":{"StartBracket":"100","EndBracket":"600"}}))]).unwrap();
    let values = |ed: &Editor| { let p = props(ed, &id); (p["StartBracket"].parse::<f64>().unwrap(), p["EndBracket"].parse::<f64>().unwrap()) };
    drag(&mut ed, "start", [260.0, -96.0], [0.0, 0.0]).unwrap();            // top edge, 200 along
    assert_eq!(values(&ed), (200.0, 600.0));
    drag(&mut ed, "start", [540.0, 0.0], [0.0, 0.0]).unwrap();              // right edge, 480 + 96 = 576 along
    assert_eq!(values(&ed), (576.0, 600.0));
    drag(&mut ed, "start", [60.0, 0.0], [0.0, 0.0]).unwrap();               // left edge: 480 + 200 + 480 + 104 = 1264 along, past the end bracket
    assert_eq!(values(&ed), (1264.0, 600.0));
    assert!(ed.text_path_options(&id).unwrap()["supported"].as_bool().unwrap(), "a range that wraps over the path start is a valid range");
    drag(&mut ed, "end", [60.0, -96.0], [0.0, 0.0]).unwrap();               // the first anchor: 0
    assert_eq!(values(&ed), (1264.0, 0.0));
    drag(&mut ed, "end", [100.0, -96.0], [0.0, 0.0]).unwrap();              // 40 along
    assert_eq!(values(&ed), (1264.0, 40.0));
    // the centre moves both by the pointer's travel (here 40 back to the first anchor)
    drag(&mut ed, "center", [60.0, -96.0], [100.0, -96.0]).unwrap();
    assert_eq!(values(&ed), (1224.0, 0.0));
    // and by the shorter way round the path start: a press 20 before the start dragged to 20 after it moves them +40, not -1320
    drag(&mut ed, "center", [80.0, -96.0], [60.0, -76.0]).unwrap();
    assert_eq!(values(&ed), (1264.0, 40.0));
}
