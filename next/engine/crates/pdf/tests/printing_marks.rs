//! The size of a print sheet with marks and bleed, against InDesign's PostScript file output (`printing-marks-*` and `printing-bleed-*`
//! in `next/oracle/scenarios`: the golden pictures' sizes in points). The pictures themselves are compared by `print-check.py`.
use folio_model::printing::{PrintJob, PrintSelection};
use folio_pdf::{ExportOptions, Marks, printing};

fn size(fixture: &str, marks: Marks, offset: f64, bleed: bool, spreads: bool, range: &str) -> [f64; 2] {
    sized(fixture, marks, offset, bleed, None, spreads, range)
}

fn sized(fixture: &str, marks: Marks, offset: f64, bleed: bool, custom_bleed: Option<[f64; 4]>, spreads: bool, range: &str) -> [f64; 2] {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(fixture);
    let doc = folio_idml::read_package(&path, None).unwrap();
    let job = PrintJob { selection: PrintSelection::Range { value: range.into() }, spreads, print_blank_pages: true };
    let mut opts = ExportOptions { marks, mark_offset: offset, bleed, custom_bleed, ..Default::default() };
    printing::configure(&doc, &job, &mut opts).unwrap();
    printing::sheets(&doc, &opts)[0].size
}
const NONE: Marks = Marks { crop: false, bleed: false, registration: false, page_info: false };

#[test]
fn sheet_sizes_follow_what_each_mark_needs() {
    let page = "printing.idml";
    // 240 x 300 pt page: crop marks offset 6 + 15 pt, registration targets 6 + 7.5 + 6 pt, offset 12 moves both
    assert_eq!(size(page, Marks { crop: true, ..NONE }, 6.0, false, false, "+1"), [282.0, 342.0]);
    assert_eq!(size(page, Marks { registration: true, ..NONE }, 6.0, false, false, "+1"), [279.0, 339.0]);
    assert_eq!(size(page, Marks { registration: true, ..NONE }, 12.0, false, false, "+1"), [291.0, 351.0]);
    assert_eq!(size(page, Marks { crop: true, registration: true, ..NONE }, 12.0, false, false, "+1"), [294.0, 354.0]);
    // bleed marks sit 3 + 18 pt outside the bleed box even when the bleed is 0
    assert_eq!(size(page, Marks { bleed: true, ..NONE }, 6.0, false, false, "+1"), [282.0, 342.0]);
    // page information alone: a 30 pt strip under the page, nothing at the sides, whatever the offset
    assert_eq!(size(page, Marks { page_info: true, ..NONE }, 6.0, false, false, "+1"), [240.0, 330.0]);
    assert_eq!(size(page, Marks { page_info: true, ..NONE }, 12.0, false, false, "+1"), [240.0, 330.0]);
    assert_eq!(size(page, Marks { crop: true, page_info: true, ..NONE }, 6.0, false, false, "+1"), [282.0, 351.0]);
    // a spread: two pages and the marks around both
    assert_eq!(size(page, Marks { crop: true, registration: true, ..NONE }, 6.0, false, true, "+2"), [522.0, 342.0]);
}

#[test]
fn the_document_bleed_is_printed_with_or_without_marks() {
    let page = "printing-bleed.idml"; // the same document with a 9 pt bleed
    assert_eq!(size(page, NONE, 6.0, true, false, "+1"), [258.0, 318.0]);
    assert_eq!(size(page, Marks { crop: true, ..NONE }, 6.0, true, false, "+1"), [282.0, 342.0]);
    assert_eq!(size(page, Marks { crop: true, bleed: true, registration: true, ..NONE }, 6.0, true, false, "+1"), [300.0, 360.0]);
    assert_eq!(size(page, Marks { page_info: true, ..NONE }, 6.0, true, false, "+1"), [258.0, 348.0]);
    assert_eq!(size(page, Marks { crop: true, page_info: true, ..NONE }, 6.0, true, false, "+1"), [282.0, 360.0]);
}

#[test]
fn the_dialogs_own_bleed_values_when_the_document_bleed_is_not_used() {
    // top 3, bottom 5, inside 7, outside 11 on the 240 x 300 pt page (printing-bleed-custom, printing-bleed-custom-marks)
    let own = Some([3.0, 5.0, 7.0, 11.0]);
    assert_eq!(sized("printing.idml", NONE, 6.0, false, own, false, "+1"), [258.0, 308.0]);
    // crop and bleed marks: each side is the larger of offset + 15 and its bleed + 21 (top 24, bottom 26, inside 28, outside 32)
    assert_eq!(sized("printing.idml", Marks { crop: true, bleed: true, ..NONE }, 6.0, false, own, false, "+1"), [300.0, 350.0]);
}
