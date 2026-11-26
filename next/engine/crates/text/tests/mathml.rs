use folio_model::{layout::anchored_items, paths_bounds, Point};
use folio_text::{layout_document, FontStore};
use std::path::Path;

#[test]
fn imported_inline_math_frames_match_indesign_bounds_and_baselines() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/mathml-c1/inline");
    let doc = folio_idml::read_package(&dir.join("after.idml"), None).unwrap();
    let measured: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("indesign.json")).unwrap()).unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    if !fonts.resolve("Arial", "Regular", &doc.font_table).exact {
        eprintln!("skipped: measured Arial font is not installed");
        return;
    }
    let layout = layout_document(&doc, &fonts);
    let inline = anchored_items(&doc, &layout);
    let spread = &doc.spreads[0];
    let page = &spread.pages[0];
    let page_inverse = page.transform.invert().unwrap();
    let mut checked = 0;
    for frame in &spread.items {
        let Some(objects) = inline.get(&frame.id) else { continue };
        for item in objects {
            let Some(math) = &item.math else { continue };
            let case = measured["cases"].as_array().unwrap().iter().find(|case| case["sourceAfter"].as_str() == math.source.as_deref()).unwrap();
            let expected: Vec<f64> = case["boundsInline"].as_str().unwrap().split(',').map(|n| n.parse().unwrap()).collect();
            let bounds = paths_bounds(&item.paths).unwrap();
            let page_point = |point| page_inverse.apply(frame.transform.apply(item.transform.apply(point)));
            let top_left = page_point(Point::new(bounds.left, bounds.top));
            let bottom_right = page_point(Point::new(bounds.right, bounds.bottom));
            let actual = [top_left.y - page.bounds.top, top_left.x - page.bounds.left,
                bottom_right.y - page.bounds.top, bottom_right.x - page.bounds.left];
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 0.001, "{}: {actual} != {expected}", case["case"]);
            }
            let line = &layout.frames[&frame.id].lines[0];
            let baseline = page_inverse.apply(frame.transform.apply(Point::new(0.0, line.baseline))).y - page.bounds.top;
            let expected: f64 = case["baseline"].as_str().unwrap().parse().unwrap();
            assert!((baseline - expected).abs() < 0.001, "{} baseline: {baseline} != {expected}", case["case"]);
            checked += 1;
        }
    }
    assert_eq!(checked, 4);
}
