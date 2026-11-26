use folio_model::printing::{PrintJob, PrintSelection};
use folio_pdf::{ExportOptions, printing};

#[test]
fn printing_range_order_spreads_and_rejected_options() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/printing.idml");
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let job = PrintJob { selection: PrintSelection::Range { value: "+5,+1,+2-+3".into() }, spreads: false, print_blank_pages: true };
    let mut opts = ExportOptions::default();
    printing::configure(&doc, &job, &mut opts).unwrap();
    let sheets = printing::sheets(&doc, &opts);
    assert_eq!(sheets.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["A-14", "A-10", "A-11", "A-12"]);
    let spread = PrintJob { selection: PrintSelection::Range { value: "+2".into() }, spreads: true, ..job.clone() };
    printing::configure(&doc, &spread, &mut opts).unwrap();
    assert_eq!(printing::sheets(&doc, &opts)[0].label, "A-10-A-11");
    let named = PrintJob { selection: PrintSelection::Range { value: "A-11-A-14".into() }, ..job.clone() };
    printing::configure(&doc, &named, &mut opts).unwrap();
    assert_eq!(printing::sheets(&doc, &opts).len(), 4);
    for value in ["", "+0", "+6", "A-11-no-page", "+1,+1", "+5-+1"] {
        let invalid = PrintJob { selection: PrintSelection::Range { value: value.into() }, ..job.clone() };
        assert!(printing::configure(&doc, &invalid, &mut opts).is_err(), "{value}");
    }
    // Print Blank Pages off: A-12 (empty) and A-13 (only a Nonprinting object) are left out (InDesign: printing-default).
    let default = PrintJob { selection: PrintSelection::All, spreads: false, print_blank_pages: false };
    printing::configure(&doc, &default, &mut opts).unwrap();
    assert_eq!(printing::sheets(&doc, &opts).iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["A-10", "A-11", "A-14"]);
    // A spread goes out unless every page of it is blank (printing-spreads: A-10-A-11, A-14).
    printing::configure(&doc, &PrintJob { spreads: true, ..default.clone() }, &mut opts).unwrap();
    assert_eq!(printing::sheets(&doc, &opts).iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["A-10-A-11", "A-14"]);
    // A range that names only blank pages leaves nothing to print.
    let blank = PrintJob { selection: PrintSelection::Range { value: "+3".into() }, ..default };
    assert!(printing::configure(&doc, &blank, &mut opts).is_err());
    // Print outputs a hidden spread (InDesign: hiddenBlankOn / hiddenRangeInside), which PDF export never does.
    let mut hidden = doc.clone();
    hidden.spreads[1].hidden = true;
    let all = PrintJob { selection: PrintSelection::All, spreads: false, print_blank_pages: true };
    printing::configure(&hidden, &all, &mut opts).unwrap();
    assert_eq!(printing::sheets(&hidden, &opts).len(), 5);
    let inside = PrintJob { selection: PrintSelection::Range { value: "+3".into() }, ..all };
    printing::configure(&hidden, &inside, &mut opts).unwrap();
    assert_eq!(printing::sheets(&hidden, &opts)[0].label, "A-12");
}
