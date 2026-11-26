//! Ink Manager commands on the basic fixture (an "Oracle Spot" ink beside the four process inks): the settings they write, the
//! limits (process inks are fixed, an alias may not loop), undo, and that the document reads them back. How InDesign itself writes
//! them is measured by the oracle `inks` scenarios.
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

fn spot(name: &str) -> Command {
    Command::NewSwatch { name: name.into(), spot: true, space: "cmyk".into(), value: vec![0.0, 95.0, 100.0, 0.0] }
}

fn set(name: &str, ink_type: Option<&str>, convert: Option<bool>, alias: Option<&str>) -> Command {
    Command::SetInk { name: name.into(), ink_type: ink_type.map(Into::into), convert_to_process: convert, alias: alias.map(Into::into) }
}

/// The Ink element for `name` as written in Graphic.xml.
fn ink(e: &Editor, name: &str) -> String {
    let t = &e.parts()["Resources/Graphic.xml"];
    t.lines().find(|l| l.trim_start().starts_with("<Ink ") && l.contains(&format!("Name=\"{name}\""))).unwrap_or_default().trim().to_owned()
}

fn read(e: &Editor) -> folio_model::Document {
    folio_idml::read_parts(e.parts(), Path::new("."), &folio_idml::ReaderData::default()).unwrap()
}

#[test]
fn type_conversion_and_alias_are_written_on_the_ink_and_read_back() {
    let mut e = fixture();
    e.apply("spot", &[spot("PMS 485")]).unwrap();
    e.apply("type", &[set("Oracle Spot", Some("Opaque"), None, None)]).unwrap();
    assert!(ink(&e, "Oracle Spot").contains("InkType=\"Opaque\""), "{}", ink(&e, "Oracle Spot"));
    e.apply("convert", &[set("PMS 485", None, Some(true), None)]).unwrap();
    assert!(ink(&e, "PMS 485").contains("ConvertToProcess=\"true\""));
    e.apply("alias", &[set("PMS 485", None, None, Some("Oracle Spot"))]).unwrap();
    assert!(ink(&e, "PMS 485").contains("AliasInkName=\"Oracle Spot\""), "{}", ink(&e, "PMS 485"));
    let doc = read(&e);
    let row = doc.print.inks.iter().find(|k| k.name == "PMS 485").unwrap();
    assert!(row.convert_to_process);
    assert_eq!(row.alias.as_deref(), Some("Oracle Spot"));
    assert_eq!(doc.print.inks.iter().find(|k| k.name == "Oracle Spot").unwrap().ink_type, "Opaque");
    // an empty alias takes it away, undo brings it back
    e.apply("no alias", &[set("PMS 485", None, None, Some(""))]).unwrap();
    assert!(!ink(&e, "PMS 485").contains("AliasInkName"));
    e.undo().unwrap();
    assert!(ink(&e, "PMS 485").contains("AliasInkName"));
}

#[test]
fn process_inks_are_fixed_and_an_alias_cannot_loop() {
    let mut e = fixture();
    e.apply("spot", &[spot("PMS 485")]).unwrap();
    let before = e.parts().clone();
    for bad in [
        set("Process Cyan", None, Some(true), None),
        set("Process Black", Some("Opaque"), None, None),
        set("Process Magenta", None, None, Some("Oracle Spot")),
        set("Oracle Spot", Some("Shiny"), None, None),
        set("Oracle Spot", None, None, Some("Oracle Spot")),
        set("Oracle Spot", None, None, Some("Nowhere")),
        set("Nowhere", None, Some(true), None),
    ] {
        assert!(e.apply("bad", std::slice::from_ref(&bad)).is_err(), "{bad:?}");
    }
    assert_eq!(e.parts(), &before, "a refused command changes nothing");
    // a spot may print on a process plate; the other way round loops
    e.apply("a", &[set("Oracle Spot", None, None, Some("Process Cyan"))]).unwrap();
    e.apply("b", &[set("PMS 485", None, None, Some("Oracle Spot"))]).unwrap();
    assert!(e.apply("loop", &[set("Oracle Spot", None, None, Some("PMS 485"))]).is_err());
}

#[test]
fn all_spots_to_process_sets_every_spot_ink_only() {
    let mut e = fixture();
    e.apply("spot", &[spot("PMS 485")]).unwrap();
    e.apply("all", &[Command::AllSpotsToProcess { on: true }]).unwrap();
    for name in ["Oracle Spot", "PMS 485"] {
        assert!(ink(&e, name).contains("ConvertToProcess=\"true\""), "{name}");
    }
    for name in ["$ID/Process Cyan", "$ID/Process Black"] {
        assert!(ink(&e, name).contains("ConvertToProcess=\"false\""), "{name}");
    }
    e.apply("off", &[Command::AllSpotsToProcess { on: false }]).unwrap();
    assert!(ink(&e, "PMS 485").contains("ConvertToProcess=\"false\""));
}

/// A document with no Ink at all (File > New made one before it had the four process inks): a new spot swatch gets its Ink all the same,
/// the process inks go in first (InDesign's values, TrapOrder 1-4) so the spot ink is the fifth, and the document reads them back.
#[test]
fn a_new_spot_swatch_in_a_document_without_any_ink_brings_the_process_inks() {
    let mut parts = fixture().parts().clone();
    let graphic = parts.get_mut("Resources/Graphic.xml").unwrap();
    *graphic = graphic.lines().filter(|l| !l.contains("<Ink ") && !(l.contains("<Color ") && l.contains("Model=\"Spot\""))).collect::<Vec<_>>().join("\n") + "\n";
    let mut e = Editor::new(parts.clone());
    assert!(!e.parts()["Resources/Graphic.xml"].contains("<Ink "));
    e.apply("spot", &[spot("Brick")]).unwrap();
    let doc = read(&e);
    assert_eq!(doc.print.inks.iter().map(|k| (k.name.as_str(), k.process)).collect::<Vec<_>>(),
        [("Process Cyan", true), ("Process Magenta", true), ("Process Yellow", true), ("Process Black", true), ("Brick", false)]);
    assert!(ink(&e, "Brick").contains("TrapOrder=\"5\""), "{}", ink(&e, "Brick"));
    assert!(ink(&e, "$ID/Process Cyan").contains("Angle=\"75\"") && ink(&e, "$ID/Process Black").contains("NeutralDensity=\"1.7\""));
    e.undo().unwrap();
    assert_eq!(e.parts(), &parts);
}
