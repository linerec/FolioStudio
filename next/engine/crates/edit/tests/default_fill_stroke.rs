//! The Color Picker's OK with nothing selected (InDesign 2025, measured: next/oracle/rules/colorpicker-c2.ko.md): the objects drawn from now on take the colour,
//! the ones that exist keep theirs, one history step. The document keeps it as `PageItemDefault/@FillColor` / `@StrokeColor`.
use folio_edit::color_commands::{ColorCommand, DefaultPaint, SetDefaultFillStroke};
use folio_edit::{Command, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
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

fn cmd(fill: Option<DefaultPaint>, stroke: Option<DefaultPaint>) -> Command {
    Command::Color(ColorCommand::SetDefaultFillStroke(SetDefaultFillStroke { fill, stroke }))
}
fn unnamed(space: &str, value: &[f64]) -> DefaultPaint {
    DefaultPaint { swatch: None, space: Some(space.into()), value: Some(value.to_vec()) }
}
fn swatch(id: &str) -> DefaultPaint {
    DefaultPaint { swatch: Some(id.into()), space: None, value: None }
}

#[test]
fn the_default_fill_and_stroke_of_new_objects_are_set_and_undone() {
    let mut e = fixture();
    let before = e.default_fill_stroke().expect("PageItemDefault");
    let spreads = |e: &Editor| e.parts().iter().filter(|(k, _)| k.starts_with("Spreads/")).map(|(k, v)| format!("{k}{v}")).collect::<String>();
    let objects = spreads(&e);
    e.apply("Color Picker", &[cmd(Some(unnamed("RGB", &[35.0, 100.0, 166.0])), None)]).unwrap();
    let after = e.default_fill_stroke().unwrap();
    assert!(after.fill.starts_with("Color/u"), "an unnamed colour: {}", after.fill);
    assert_eq!((after.stroke.as_str(), after.stroke_tint), (before.stroke.as_str(), before.stroke_tint), "the stroke stays");
    assert!(e.parts()["Resources/Graphic.xml"].contains(&after.fill) && e.parts()["Resources/Graphic.xml"].contains("ColorValue=\"35 100 166\""));
    // an object drawn afterwards takes it (create reads PageItemDefault); the document's objects were not touched
    assert_eq!(spreads(&e), objects);
    // a swatch by id, fill and stroke together
    e.apply("Default", &[cmd(Some(swatch("Swatch/None")), Some(swatch("Color/Black")))]).unwrap();
    let d = e.default_fill_stroke().unwrap();
    assert_eq!((d.fill.as_str(), d.stroke.as_str()), ("Swatch/None", "Color/Black"));
    // one history step each
    e.undo().unwrap();
    assert_eq!(e.default_fill_stroke().unwrap().fill, after.fill);
    e.undo().unwrap();
    assert_eq!(e.default_fill_stroke().unwrap(), before);
}

#[test]
fn a_default_paint_names_a_swatch_or_a_colour_and_something_to_set() {
    let mut e = fixture();
    assert!(e.apply("none", &[cmd(None, None)]).is_err());
    assert!(e.apply("missing swatch", &[cmd(Some(swatch("Color/NoSuch")), None)]).is_err());
    assert!(e.apply("half a colour", &[cmd(Some(DefaultPaint { swatch: None, space: Some("RGB".into()), value: None }), None)]).is_err());
    assert!(e.apply("both", &[cmd(Some(DefaultPaint { swatch: Some("Color/Black".into()), space: Some("RGB".into()), value: Some(vec![0.0, 0.0, 0.0]) }), None)]).is_err());
}
