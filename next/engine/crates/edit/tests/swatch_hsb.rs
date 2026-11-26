//! The Color Picker's "Add HSB Swatch" (InDesign 2025, measured: next/oracle/rules/colorpicker-c2.ko.md): an RGB colour written with ConvertToHsb="true",
//! named by its H S B; the RGB / CMYK / Lab swatches keep ConvertToHsb="false".
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

fn color(e: &Editor, name: &str) -> String {
    let t = &e.parts()["Resources/Graphic.xml"];
    t.lines().find(|l| l.trim_start().starts_with("<Color ") && l.contains(&format!("Name=\"{name}\""))).unwrap_or_default().trim().to_owned()
}

#[test]
fn an_hsb_swatch_is_an_rgb_colour_flagged_convert_to_hsb() {
    let mut e = fixture();
    e.apply("hsb", &[Command::NewSwatch { name: "H=137 S=15 B=55".into(), spot: false, space: "hsb".into(), value: vec![119.0, 140.0, 125.0] }]).unwrap();
    let c = color(&e, "H=137 S=15 B=55");
    assert!(c.contains("Space=\"RGB\"") && c.contains("ColorValue=\"119 140 125\"") && c.contains("ConvertToHsb=\"true\""), "{c}");
    e.apply("rgb", &[Command::NewSwatch { name: "R=1 G=2 B=3".into(), spot: false, space: "rgb".into(), value: vec![1.0, 2.0, 3.0] }]).unwrap();
    assert!(color(&e, "R=1 G=2 B=3").contains("ConvertToHsb=\"false\""));
    e.apply("lab", &[Command::NewSwatch { name: "L=56 a=-11 b=5".into(), spot: false, space: "lab".into(), value: vec![56.0, -11.0, 5.0] }]).unwrap();
    assert!(color(&e, "L=56 a=-11 b=5").contains("Space=\"LAB\""));
}
