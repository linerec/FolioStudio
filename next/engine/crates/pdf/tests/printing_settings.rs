//! Print preset settings against InDesign 2026 (21.6.0.57; 2025 not checked): the dialog's Summary text, the defaults, and which settings apply
//! in each state a temporary preset was put in (`artifacts/print-c1-20261023/out/p4_deps.txt`, copied to `tests/data/print_preset_2026.json`).
use folio_pdf::printing::presets::{PrintPreset, default_preset};
use folio_pdf::printing::settings::*;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

fn fixture() -> Value { serde_json::from_str(include_str!("data/print_preset_2026.json")).unwrap() }

fn setup(spec: &Map<String, Value>) -> Map<String, Value> {
    let mut values = defaults();
    for (k, v) in spec {
        if k == "__allPrinterMarks" { values = set(&values, "allPrinterMarks", v).unwrap(); } else { values.insert(k.clone(), v.clone()); }
    }
    normalize(&values).unwrap()
}

#[test]
fn what_does_not_apply_matches_every_measured_state() {
    for state in fixture()["states"].as_array().unwrap() {
        let values = setup(state["setup"].as_object().unwrap());
        let want: BTreeSet<String> = state["notApplicable"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let got: BTreeSet<String> = not_applicable(&values).into_iter().map(String::from).collect();
        assert_eq!(got, want, "state {}", state["label"]);
    }
}

#[test]
fn defaults_match_the_measured_preset() {
    let want = fixture();
    let have = defaults();
    let mut checked = 0;
    for (key, dom) in want["defaults"].as_object().unwrap() {
        let f = field(key).unwrap_or_else(|| panic!("measured property {key} is not a setting"));
        assert_eq!(dom_value(f, &have[key]), dom.as_str().unwrap(), "default of {key}");
        checked += 1;
    }
    assert!(checked > 60, "{checked} defaults compared");
    // every setting the model has is a measured InDesign property, and none is listed twice
    let keys: BTreeSet<_> = FIELDS.iter().map(|f| f.key).collect();
    assert_eq!(keys.len(), FIELDS.len());
}

#[test]
fn summary_text_is_the_measured_one_for_default() {
    let measured = include_str!("data/print_preset_default_summary_2026.txt").replace("\r\n", "\n");
    let mine = summary_en("[Default]", &default_preset().values, "sRGB IEC61966-2.1", "EPSON4D1A51 (ET-2760 Series)").replace("\r\n", "\n");
    assert_eq!(mine, measured);
}

#[test]
fn the_all_marks_box_sets_the_five_marks_and_follows_them() {
    let on = set(&defaults(), "allPrinterMarks", &json!(true)).unwrap();
    for k in ["cropMarks", "bleedMarks", "registrationMarks", "colorBars", "pageInformationMarks"] { assert_eq!(on[k], json!(true), "{k}"); }
    let off = set(&on, "colorBars", &json!(false)).unwrap();
    assert_eq!(off["allPrinterMarks"], json!(false));
    assert_eq!(off["cropMarks"], json!(true));
}

#[test]
fn bad_values_and_unknown_keys_are_refused() {
    assert!(set(&defaults(), "copies", &json!(0)).is_err());
    assert!(set(&defaults(), "colorOutput", &json!("nope")).is_err());
    assert!(set(&defaults(), "noSuchSetting", &json!(1)).is_err());
    let mut m = Map::new();
    m.insert("mystery".into(), json!(1));
    assert!(PrintPreset { name: "x".into(), values: m }.complete().is_err());
}

#[test]
fn an_older_saved_preset_is_completed() {
    // before the full settings, a preset held only `spreads` and `printBlankPages`
    let old: PrintPreset = serde_json::from_value(json!({"name": "Proof", "spreads": true, "printBlankPages": true})).unwrap();
    let done = old.complete().unwrap();
    assert!(done.flag("printSpreads") && done.flag("printBlankPages"));
    assert_eq!(done.values["copies"], json!(1));
    assert!(!done.values.contains_key("spreads"));
}

#[test]
fn only_a_postscript_file_can_separate() {
    assert!(!allowed(&defaults(), "colorOutput", "separations"));
    let ps = set(&defaults(), "printer", &json!("postscriptFile")).unwrap();
    assert!(allowed(&ps, "colorOutput", "separations"));
}
