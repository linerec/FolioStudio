//! Highlight indicators of conditions (`IndicatorMethod="UseHighlight"`), measured in InDesign 2025
//! (scenarios condx-hl-*, rules next/oracle/rules/condx-c4.ko.md): the rectangle behind a conditioned range reaches, on a
//! line, as far above the baseline as the largest `size x head.yMax` of the faces on the line and as far below as the
//! largest `size x -head.yMin`.

use folio_model::conditions::Indicators;
use folio_model::layout::DecorationKind;
use folio_text::{FontStore, layout_document};
use std::path::Path;

/// (top above the baseline, height, x0, x1) of every highlight of the fixture's formatted story, in line order.
fn highlights(point_size: Option<f64>) -> Vec<(f64, f64, f64, f64)> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/panels-conditions.idml");
    let mut doc = folio_idml::read_package(&fixture, None).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    for style in ["Regular", "Bold", "Italic"] {
        if !fonts.resolve("Arial", style, &doc.font_table).exact {
            eprintln!("skipped: measured Arial {style} is not installed");
            return Vec::new();
        }
    }
    doc.conditions.iter_mut().find(|c| c.name == "Print").unwrap().indicator_method = "UseHighlight".into();
    doc.condition_indicators = Indicators::ShowAndPrint;
    if let Some(size) = point_size {
        for story in doc.stories.values_mut() {
            for run in story.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
                run.attrs.point_size = size;
            }
        }
    }
    let layout = layout_document(&doc, &fonts);
    let mut out = Vec::new();
    for frame in layout.frames.values() {
        for line in &frame.lines {
            for d in line.decorations.iter().filter(|d| d.kind == DecorationKind::Highlight) {
                out.push((line.baseline - d.y, d.thickness, d.x0, d.x1));
            }
        }
    }
    out
}

fn same(found: &[(f64, f64, f64, f64)], expected: &[(f64, f64, f64, f64)]) {
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (f, e) in found.iter().zip(expected) {
        assert!((f.0 - e.0).abs() < 0.01 && (f.1 - e.1).abs() < 0.01 && ((f.3 - f.2) - (e.3 - e.2)).abs() < 0.01, "{f:?} != {e:?}");
    }
}

/// condx-ind-highlight: 12 pt, 14 pt Bold and a 15 pt Italic 'LARGE' on the line: the top comes from the Italic
/// (15 pt x Arial Italic yMax 2043), the bottom from the 14 pt Bold (x Arial Bold yMin 771): 14.963 above, 5.271 below,
/// the same under 'pha ', 'b' and 'o'.
#[test]
fn the_fixtures_line_reaches_as_far_as_its_largest_faces() {
    let found = highlights(None);
    if found.is_empty() {
        return;
    }
    same(&found, &[(14.963, 20.234, 0.0, 23.355), (14.963, 20.234, 0.0, 7.33), (14.963, 20.234, 0.0, 8.552)]);
}

/// condx-hl-arial-24: everything 24 pt: Arial Bold's box (2162 / 771 of 2048) is the line's.
#[test]
fn a_larger_size_scales_the_rectangle() {
    let found = highlights(Some(24.0));
    if found.is_empty() {
        return;
    }
    // (InDesign keeps 'b' and 'o' as one range once their sizes agree: the widths add up the same)
    assert!(found.iter().all(|f| (f.0 - 25.336).abs() < 0.01 && (f.1 - 34.371).abs() < 0.01), "{found:?}");
    let width: f64 = found.iter().map(|f| f.3 - f.2).sum();
    assert!((width - (46.711 + 29.32)).abs() < 0.02, "{found:?}");
}
