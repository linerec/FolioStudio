//! Print presets as InDesign 2025 handled them (`artifacts/printing-20261023/measurement/printing-presets.json`).
use folio_model::printing::{PrintJob, PrintSelection};
use folio_pdf::printing::presets::*;

#[test]
fn bugfind_invalid_print_spreads_does_not_panic() {
    let result = std::panic::catch_unwind(|| PrintPreset::with("invalid", &[("printSpreads", "bad".into())]));
    let error = result.expect("invalid settings must not panic").unwrap_err();
    assert!(error.contains("invalidPrintSettings"), "{error}");
    assert!(error.contains("printSpreads"), "{error}");
}

fn preset(name: &str, spreads: bool, blank: bool) -> PrintPreset {
    PrintPreset::with(name, &[("printSpreads", spreads.into()), ("printBlankPages", blank.into())]).unwrap()
}

#[test]
fn define_duplicate_rename_remove() {
    let a = apply_op(&[], &PresetOp::Define { preset: preset("Proof", true, true) }).unwrap();
    // a taken name is refused and the list is unchanged
    assert!(apply_op(&a, &PresetOp::Define { preset: preset("Proof", false, false) }).is_err());
    // the copy keeps the values; the first free number follows the name
    let b = apply_op(&a, &PresetOp::Duplicate { name: "Proof".into(), word: "Copy".into() }).unwrap();
    assert_eq!(b[1], preset("Proof Copy 1", true, true));
    let c = apply_op(&b, &PresetOp::Duplicate { name: "Proof".into(), word: "Copy".into() }).unwrap();
    assert_eq!(c[2].name, "Proof Copy 2");
    let d = apply_op(&c, &PresetOp::Rename { name: "Proof Copy 1".into(), to: "Booklet".into() }).unwrap();
    assert_eq!(d.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Proof", "Booklet", "Proof Copy 2"]);
    assert!(apply_op(&d, &PresetOp::Rename { name: "Booklet".into(), to: "Proof".into() }).is_err());
    assert!(apply_op(&d, &PresetOp::Rename { name: "Booklet".into(), to: "  ".into() }).is_err());
    // an edit keeps the place; renaming through an edit must not collide either
    let e = apply_op(&d, &PresetOp::Update { name: "Booklet".into(), preset: preset("Booklet", false, true) }).unwrap();
    assert_eq!(e[1], preset("Booklet", false, true));
    assert!(apply_op(&d, &PresetOp::Update { name: "Booklet".into(), preset: preset("Proof", false, true) }).is_err());
    let f = apply_op(&e, &PresetOp::Remove { name: "Proof".into() }).unwrap();
    assert_eq!(f.len(), 2);
    assert!(apply_op(&f, &PresetOp::Remove { name: "Proof".into() }).is_err());
}

#[test]
fn default_is_built_in_and_protected() {
    assert_eq!(listed(&[]).unwrap()[0], preset("[Default]", false, false));
    for op in [
        PresetOp::Define { preset: preset("[Default]", true, true) },
        PresetOp::Rename { name: "[Default]".into(), to: "x".into() },
        PresetOp::Remove { name: "[Default]".into() },
        PresetOp::Update { name: "[Default]".into(), preset: preset("[Default]", true, true) },
    ] {
        assert!(apply_op(&[], &op).is_err(), "{op:?}");
    }
    // a copy of [Default] is allowed
    let copy = apply_op(&[], &PresetOp::Duplicate { name: "[Default]".into(), word: "".into() }).unwrap();
    assert_eq!(copy[0], preset("[Default] Copy 1", false, false));
}

#[test]
fn applying_keeps_the_jobs_own_range() {
    // InDesign: a preset's copies / blank pages move to the document, the page range typed there (+2) stays
    let job = PrintJob { selection: PrintSelection::Range { value: "+2".into() }, spreads: false, print_blank_pages: false };
    let applied = apply_to_job(&job, &preset("Proof", true, true));
    assert!(applied.spreads && applied.print_blank_pages);
    assert!(matches!(applied.selection, PrintSelection::Range { ref value } if value == "+2"));
    assert!(find(&[], "nope").is_err());
    assert_eq!(find(&[], "[Default]").unwrap(), default_preset());
}
