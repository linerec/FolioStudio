//! Page item edits and their undo behavior.

use super::*;
use crate::Editor;
use std::collections::BTreeMap;

const SPREAD: &str = r#"<idPkg:Spread><Spread Self="s"><Rectangle Self="r" StrokeWeight="2" StrokeColor="Color/Black" ItemTransform="1 0 0 1 5 6"><Properties><PathGeometry><GeometryPathType PathOpen="false"><PathPointArray><PathPointType Anchor="0 0" LeftDirection="0 0" RightDirection="0 0"/><PathPointType Anchor="0 50" LeftDirection="0 50" RightDirection="0 50"/><PathPointType Anchor="100 50" LeftDirection="100 50" RightDirection="100 50"/><PathPointType Anchor="100 0" LeftDirection="100 0" RightDirection="100 0"/></PathPointArray></GeometryPathType></PathGeometry></Properties></Rectangle></Spread></idPkg:Spread>"#;

fn editor() -> Editor {
    Editor::new(BTreeMap::from([("Spreads/Spread_s.xml".to_string(), SPREAD.to_string())]))
}

#[test]
fn redefining_the_scale_moves_it_from_the_transform_into_the_path() {
    // a 40 x 20 rectangle at the origin of its own space, turned 90 degrees, then scaled 2 x 3 (in its transform)
    let spread = "<idPkg:Spread><Spread Self=\"s\"><Rectangle Self=\"r\" StrokeWeight=\"2\" ItemTransform=\"0 2 -3 0 10 20\"><Properties><PathGeometry><GeometryPathType PathOpen=\"false\"><PathPointArray><PathPointType Anchor=\"0 0\" LeftDirection=\"0 0\" RightDirection=\"0 0\" /><PathPointType Anchor=\"40 0\" LeftDirection=\"40 0\" RightDirection=\"40 0\" /><PathPointType Anchor=\"40 20\" LeftDirection=\"40 20\" RightDirection=\"40 20\" /><PathPointType Anchor=\"0 20\" LeftDirection=\"0 20\" RightDirection=\"0 20\" /></PathPointArray></GeometryPathType></PathGeometry></Properties></Rectangle></Spread></idPkg:Spread>";
    let mut e = Editor::new(BTreeMap::from([("Spreads/Spread_s.xml".to_string(), spread.to_string())]));
    e.apply("redefine", &[Command::RedefineScaling { ids: vec!["r".into()] }]).unwrap();
    let t = e.part("Spreads/Spread_s.xml").unwrap().to_owned();
    // the path takes 2 x 3; what stays is the turn, and the stroke is as it was
    assert!(t.contains("ItemTransform=\"0 1 -1 0 10 20\""), "{t}");
    assert!(t.contains("Anchor=\"80 60\""), "{t}");
    assert!(t.contains("StrokeWeight=\"2\""), "{t}");
    // already 100 %: nothing to do
    let before = e.part("Spreads/Spread_s.xml").unwrap().to_owned();
    e.apply("again", &[Command::RedefineScaling { ids: vec!["r".into()] }]).unwrap();
    assert_eq!(e.part("Spreads/Spread_s.xml").unwrap(), before);
}

fn prefs(grid: &str) -> Editor {
    let xml = format!("<idPkg:Preferences>\n\t<DocumentPreference />\n{grid}\n\t<GuidePreference />\n</idPkg:Preferences>");
    Editor::new(BTreeMap::from([("Resources/Preferences.xml".to_string(), xml)]))
}

fn set(e: &mut Editor, key: &str, value: serde_json::Value) -> Result<(), EditError> {
    e.apply("pref", &[Command::SetViewPref { key: key.into(), value }]).map(|_| ())
}

#[test]
fn a_grid_colour_goes_where_indesign_writes_it() {
    let colour = |name: &str| serde_json::json!(name);
    // a grid element with nothing inside (as a new document has it): Properties appear, GridColor first
    let mut e = prefs("\t<GridPreference GridsInBack=\"true\" />");
    set(&mut e, "baselineColor", colour("Red")).unwrap();
    set(&mut e, "gridColor", serde_json::json!([10, 20.5, 30])).unwrap();
    assert_eq!(
        e.part("Resources/Preferences.xml").unwrap(),
        "<idPkg:Preferences>\n\t<DocumentPreference />\n\t<GridPreference GridsInBack=\"true\">\n\t\t<Properties>\n\t\t\t<GridColor type=\"list\">\n\t\t\t\t<ListItem type=\"double\">10</ListItem>\n\t\t\t\t<ListItem type=\"double\">20.5</ListItem>\n\t\t\t\t<ListItem type=\"double\">30</ListItem>\n\t\t\t</GridColor>\n\t\t\t<BaselineColor type=\"enumeration\">Red</BaselineColor>\n\t\t</Properties>\n\t</GridPreference>\n\t<GuidePreference />\n</idPkg:Preferences>"
    );
    // a list becomes a name again, and a name another name; each in place
    set(&mut e, "gridColor", colour("Charcoal")).unwrap();
    set(&mut e, "baselineColor", colour("LightBlue")).unwrap();
    let t = e.part("Resources/Preferences.xml").unwrap();
    assert!(t.contains("\t\t\t<GridColor type=\"enumeration\">Charcoal</GridColor>\n\t\t\t<BaselineColor type=\"enumeration\">LightBlue</BaselineColor>\n\t\t</Properties>"), "{t}");
    // a grid element with a start and an end tag but no Properties
    let mut e = prefs("\t<GridPreference GridsInBack=\"true\">\n\t</GridPreference>");
    set(&mut e, "gridColor", colour("Fiesta")).unwrap();
    let t = e.part("Resources/Preferences.xml").unwrap();
    assert!(t.contains("<GridPreference GridsInBack=\"true\">\n\t\t<Properties>\n\t\t\t<GridColor type=\"enumeration\">Fiesta</GridColor>\n\t\t</Properties>\n\t</GridPreference>"), "{t}");
    // Properties with only a GridColor: the baseline colour follows it
    let mut e = prefs("\t<GridPreference>\n\t\t<Properties>\n\t\t\t<GridColor type=\"enumeration\">Gray</GridColor>\n\t\t</Properties>\n\t</GridPreference>");
    set(&mut e, "baselineColor", colour("Gold")).unwrap();
    let t = e.part("Resources/Preferences.xml").unwrap();
    assert!(t.contains("Gray</GridColor>\n\t\t\t<BaselineColor type=\"enumeration\">Gold</BaselineColor>\n\t\t</Properties>"), "{t}");
}

#[test]
fn grid_settings_keep_to_the_range_indesign_accepts() {
    let mut e = prefs("\t<GridPreference BaselineStart=\"36\" />");
    let before = e.part("Resources/Preferences.xml").unwrap().to_owned();
    for (key, bad) in [
        ("baselineStart", serde_json::json!(-1)), ("baselineStart", serde_json::json!(1001)), ("baselineDivision", serde_json::json!(0.5)),
        ("baselineThreshold", serde_json::json!(4)), ("baselineThreshold", serde_json::json!(4001)), ("horizontalDivision", serde_json::json!(0)),
        ("verticalDivision", serde_json::json!(1001)), ("horizontalSubdivision", serde_json::json!(0)), ("verticalSubdivision", serde_json::json!(1.5)),
        ("baselineRelativeTo", serde_json::json!("spread")), ("gridColor", serde_json::json!("Chartreuse")), ("gridColor", serde_json::json!([1, 2])),
        ("gridColor", serde_json::json!([1, 2, 300])), ("guidesLocked", serde_json::json!("yes")), ("nothing", serde_json::json!(true)),
        ("horizontalUnits", serde_json::json!("Q")), ("strokeUnits", serde_json::json!("Inches")), ("rulerOrigin", serde_json::json!("Page")),
        ("cursorKeyIncrement", serde_json::json!(101)), ("snapZone", serde_json::json!(37)), ("kerningKeyIncrement", serde_json::json!(0)),
        ("rulerGuidesThreshold", serde_json::json!(4)), ("rulerGuidesColor", serde_json::json!("Chartreuse")), ("pasteboardMargins", serde_json::json!([0, -1])),
        ("pasteboardMargins", serde_json::json!([1])), ("matchPreviewBackground", serde_json::json!("yes")),
    ] {
        assert!(set(&mut e, key, bad.clone()).is_err(), "{key} {bad}");
    }
    assert_eq!(e.part("Resources/Preferences.xml").unwrap(), before, "a refused value changes nothing");
    set(&mut e, "baselineStart", serde_json::json!(0)).unwrap();
    set(&mut e, "baselineDivision", serde_json::json!(14.4)).unwrap();
    set(&mut e, "baselineThreshold", serde_json::json!(4000)).unwrap();
    set(&mut e, "verticalSubdivision", serde_json::json!(1000)).unwrap();
    set(&mut e, "baselineRelativeTo", serde_json::json!("margin")).unwrap();
    let t = e.part("Resources/Preferences.xml").unwrap();
    assert!(t.contains("BaselineStart=\"0\"") && t.contains("BaselineDivision=\"14.4\"") && t.contains("BaselineViewThreshold=\"4000\"") && t.contains("VerticalGridSubdivision=\"1000\"") && t.contains("BaselineGridRelativeOption=\"TopOfMarginOfBaselineGridRelativeOption\""), "{t}");
}

#[test]
fn a_dragged_handle_is_the_same_change_as_the_control_panel_size() {
    // the bottom-right handle dragged to (140, 70): the top-left stays; with the 2 pt
    // stroke the control panel reads 142 × 72 about the top-left
    let mut a = editor();
    a.apply("box", &[Command::ResizeBox { id: "r".into(), rect: [0.0, 0.0, 140.0, 70.0] }]).unwrap();
    let mut b = editor();
    b.apply("size", &[Command::Resize { id: "r".into(), size: [142.0, 72.0], anchor: Anchor::TopLeft, include_stroke: true }]).unwrap();
    assert_eq!(a.parts(), b.parts());
    // the top-left handle dragged: the bottom-right stays
    let mut c = editor();
    c.apply("box", &[Command::ResizeBox { id: "r".into(), rect: [-20.0, -10.0, 100.0, 50.0] }]).unwrap();
    let mut d = editor();
    d.apply("size", &[Command::Resize { id: "r".into(), size: [122.0, 62.0], anchor: Anchor::BottomRight, include_stroke: true }]).unwrap();
    assert_eq!(c.parts(), d.parts());
    assert!(c.part("Spreads/Spread_s.xml").unwrap().contains(r#"Anchor="-20 -10""#));
}

#[test]
fn a_size_without_the_stroke_is_the_paths() {
    // Dimensions Include Stroke Weight off: 140 × 70 is the path, whatever the 2 pt stroke
    let mut a = editor();
    a.apply("box", &[Command::ResizeBox { id: "r".into(), rect: [0.0, 0.0, 140.0, 70.0] }]).unwrap();
    let mut b = editor();
    b.apply("size", &[Command::Resize { id: "r".into(), size: [140.0, 70.0], anchor: Anchor::TopLeft, include_stroke: false }]).unwrap();
    assert_eq!(a.parts(), b.parts());
}

#[test]
fn a_transformation_with_copy_is_one_step_and_leaves_the_original() {
    let mut e = editor();
    let original = e.part("Spreads/Spread_s.xml").unwrap().to_owned();
    let o = e.apply("rotate copy", &[
        Command::Duplicate { ids: vec!["r".into()], by: [0.0, 0.0] },
        Command::Rotate { ids: vec!["@copies".into()], by: 30.0, anchor: Anchor::Center },
    ]).unwrap();
    assert_eq!(o.created.len(), 1);
    let after = e.part("Spreads/Spread_s.xml").unwrap();
    assert!(after.contains(r#"ItemTransform="1 0 0 1 5 6""#), "the original is as it was");
    assert!(after.contains("ItemTransform=\"0.866025"), "the copy is turned: {after}");
    assert_ne!(after, original);
    assert_eq!(e.undo_label(), Some("rotate copy"));
    e.undo().unwrap();
    assert_eq!(e.part("Spreads/Spread_s.xml").unwrap(), original);
}

#[test]
fn a_failed_command_changes_nothing() {
    let mut e = editor();
    let before = e.parts().clone();
    let r = e.apply("two", &[Command::Move { ids: vec!["r".into()], by: [1.0, 1.0] }, Command::Delete { ids: vec!["nope".into()] }]);
    assert_eq!(r.unwrap_err().code, "notFound");
    assert_eq!(e.parts(), &before);
    assert!(e.undo_label().is_none());
}
