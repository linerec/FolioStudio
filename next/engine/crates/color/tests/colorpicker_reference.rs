//! The Color Picker's conversions (`folio_color::pick_color`) against InDesign 2025's own: `Color.space` of 34 colours
//! (RGB, CMYK and Lab, each converted into the other two) in a document with sRGB and U.S. Web Coated (SWOP) v2
//! (`colorpicker_convert.json`, from `artifacts/colorpicker-c1-20261023/tools/convert-probe.jsx`). InDesign's numbers
//! are fractions of 8-bit table lookups; Folio's are 8-bit RGB and 8-bit ink, so the allowed differences are those steps
//! plus InDesign's own Lab shift of -0.5 on a and b for neutral colours.

use folio_color::{Colors, PickSpace, ProfileIndex, pick_color, read_profile};
use serde_json::Value;

const SWOP: &str = "U.S. Web Coated (SWOP) v2";

fn swop() -> Option<Vec<u8>> {
    let index = ProfileIndex::scan_installed();
    match index.find(SWOP) {
        Some(p) => Some(read_profile(&p).expect("installed profile reads")),
        None => {
            eprintln!("SKIPPED: ICC profile \"{SWOP}\" is not installed");
            None
        }
    }
}

fn numbers(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

fn worst(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

#[test]
fn conversions_agree_with_indesign() {
    let Some(profile) = swop() else { return };
    let colors = Colors::from_profiles(Some(&profile), None).unwrap();
    let data: Value = serde_json::from_str(include_str!("colorpicker_convert.json")).unwrap();
    // the worst difference per (from, to): RGB levels, ink percent, Lab units
    let mut seen: std::collections::BTreeMap<String, f64> = Default::default();
    for row in data["rows"].as_array().unwrap() {
        let from = row["from"].as_str().unwrap();
        let value = numbers(&row["value"]);
        let picked = pick_color(&colors, PickSpace::parse(from).unwrap(), &value).unwrap();
        for (to, got) in [("RGB", picked.rgb.to_vec()), ("CMYK", picked.cmyk.to_vec()), ("LAB", picked.lab.to_vec())] {
            if to == from || row.get(to).is_none() {
                continue;
            }
            let want = numbers(&row[to]);
            // Lab and CMYK of a colour outside sRGB: Folio goes through the clipped screen colour, InDesign through the profiles
            // (pure cyan: Lab a -20.6 against -44.5), so only colours whose InDesign RGB is not clipped are compared there
            let clipped = ["RGB", "LAB", "CMYK"].iter().filter(|s| row.get(**s).is_some() || **s == from).any(|_| {
                let rgb = if from == "RGB" { value.clone() } else { numbers(&row["RGB"]) };
                rgb.iter().any(|c| *c <= 0.5 || *c >= 254.5)
            });
            if clipped && (to == "LAB" && from == "CMYK" || to == "CMYK" && from == "LAB") {
                eprintln!("{from} {value:?} -> {to}: skipped (outside sRGB)");
                continue;
            }
            let d = worst(&got, &want);
            eprintln!("{from} {value:?} -> {to}: Folio {got:?} InDesign {want:?} diff {d:.2}");
            let key = format!("{from}>{to}");
            let e = seen.entry(key).or_insert(0.0);
            *e = e.max(d);
        }
    }
    eprintln!("worst per conversion: {seen:?}");
    // the allowed worst difference per conversion (RGB levels, ink percent, Lab units); CMYK > RGB is 3.6 at the deepest black (3 levels against 0.3)
    let limits = [("CMYK>RGB", 4.0), ("CMYK>LAB", 2.0), ("RGB>CMYK", 3.0), ("RGB>LAB", 1.0), ("LAB>RGB", 2.0), ("LAB>CMYK", 4.0)];
    for (key, limit) in limits {
        let d = seen[key];
        assert!(d <= limit, "{key}: worst difference {d} > {limit}");
    }
}

/// The numbers the picker dialog itself shows (integers, read from InDesign 2025 with UI Automation after `app.invokeColorPicker`;
/// `colorpicker_dialog.json`) against Folio's, for 59 colours in RGB, CMYK and Lab. RGB and CMYK colours convert within a few units
/// (the dialog rounds, Folio goes through 8-bit steps). A Lab colour outside sRGB does not: Folio clips the screen colour channel by channel,
/// InDesign maps it another way (Lab 50 -100 60: CMYK 100 5 100 2 in InDesign, 85 15 100 4 here; the field of the L, a and b views
/// differs by up to 153 levels outside sRGB), so Lab colours are compared only where InDesign's own RGB is not clipped.
#[test]
fn conversions_agree_with_the_picker_dialog() {
    let Some(profile) = swop() else { return };
    let colors = Colors::from_profiles(Some(&profile), None).unwrap();
    let data: Value = serde_json::from_str(include_str!("colorpicker_dialog.json")).unwrap();
    let mut worst: std::collections::BTreeMap<String, f64> = Default::default();
    for case in data["cases"].as_array().unwrap() {
        let from = case["space"].as_str().unwrap();
        let value = numbers(&case["value"]);
        let picked = pick_color(&colors, PickSpace::parse(from).unwrap(), &value).unwrap();
        let rgb = numbers(&case["rgb"]);
        let clipped = rgb.iter().any(|c| *c <= 0.5 || *c >= 254.5);
        if from == "LAB" && clipped { continue; }
        for (to, got) in [("RGB", picked.rgb.to_vec()), ("CMYK", picked.cmyk.to_vec()), ("LAB", picked.lab.to_vec())] {
            if to == from { continue; }
            let key = format!("{from}>{to}");
            let e = worst.entry(key).or_insert(0.0);
            *e = e.max(worst_of(&got, &numbers(&case[to.to_lowercase()])));
        }
    }
    eprintln!("worst difference against the dialog: {worst:?}");
    // measured worst: CMYK>RGB 4.0 (deepest black), CMYK>LAB 1.6, RGB>CMYK 2.8 (black), RGB>LAB 1.2; Lab inside sRGB: see the limits
    let limits = [("CMYK>RGB", 4.5), ("CMYK>LAB", 2.0), ("RGB>CMYK", 3.0), ("RGB>LAB", 1.5), ("LAB>RGB", 3.0), ("LAB>CMYK", 6.0)];
    for (key, limit) in limits {
        if let Some(d) = worst.get(key) { assert!(*d <= limit, "{key}: worst difference {d} > {limit}"); }
    }
}

fn worst_of(a: &[f64], b: &[f64]) -> f64 { worst(a, b) }
