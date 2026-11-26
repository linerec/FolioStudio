use folio_idml::{read_package, read_parts, ReaderData};
use folio_model::mathml::MathObject;
use std::{collections::BTreeMap, io::Read, path::{Path, PathBuf}};

fn evidence() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/mathml-c1/storage")
}

fn equations(doc: &folio_model::Document) -> Vec<MathObject> {
    let mut result = Vec::new();
    folio_idml::for_each_item(doc, &mut |item| {
        if let Some(math) = &item.math { result.push(math.clone()); }
    });
    result
}

#[test]
fn imported_equations_match_indesign_dom_and_svg_dimensions() {
    let doc = read_package(&evidence().join("after.idml"), None).unwrap();
    let measured: serde_json::Value = serde_json::from_slice(&std::fs::read(evidence().join("indesign.json")).unwrap()).unwrap();
    let math = equations(&doc);
    assert_eq!(math.len(), 4);
    for case in measured["cases"].as_array().unwrap() {
        let source = case["input"].as_str().unwrap();
        let found = math.iter().find(|m| m.source.as_deref() == Some(source)).unwrap();
        let dom = &case["math"];
        assert!(!found.id.is_empty());
        assert_eq!(found.font_size, Some(dom["appliedMathMLFontSize"].as_str().unwrap().parse().unwrap()));
        assert_eq!(found.rgb, Some([0.0; 3]));
        assert_eq!(found.tint, Some(100.0));
        assert_eq!(found.swatch.as_deref(), Some("Nothing"));
        assert_eq!(found.is_mathml, Some(true));
        let svg: serde_json::Value = serde_json::from_str(found.svg_data.as_deref().unwrap()).unwrap();
        let bounds: Vec<f64> = case["frame"]["geometricBounds"].as_str().unwrap().split(',').map(|n| n.parse().unwrap()).collect();
        assert!((svg["width"].as_f64().unwrap() - (bounds[3] - bounds[1])).abs() < 0.00001);
        assert!((svg["height"].as_f64().unwrap() - (bounds[2] - bounds[0])).abs() < 0.00001);
        assert!(svg["svg_str"].as_str().unwrap().starts_with("<svg "));
        assert!(measured["reopened"].as_array().unwrap().iter().any(|m| m["mathmlDescription"].as_str() == Some(source)));
    }
    assert!(doc.warnings.iter().any(|w| w.contains("laid out again with Folio's own MathML layout")));
}

#[test]
fn missing_or_malformed_properties_are_not_invented_and_reading_keeps_source() {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(evidence().join("after.idml")).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new(); entry.read_to_string(&mut text).unwrap();
        if name.starts_with("Spreads/") {
            text = text.replace("AppliedMathMLFontSize=\"10\"", "FutureMathSetting=\"keep &amp; decode\"")
                .replace("AppliedMathMLRgbColor=\"0 0 0\"", "AppliedMathMLRgbColor=\"NaN 0 0\"")
                .replace("TintValue=\"100\"", "TintValue=\"infinite\"")
                .replace("IsMathMLObject=\"true\"", "IsMathMLObject=\"future\"");
        }
        parts.insert(name, text);
    }
    let before = parts.clone();
    let doc = read_parts(&parts, &evidence().join("after.idml"), &ReaderData::default()).unwrap();
    let math = equations(&doc);
    assert_eq!(math.len(), 4);
    for m in math {
        assert_eq!(m.font_size, None);
        assert_eq!(m.rgb, None);
        assert_eq!(m.tint, None);
        assert_eq!(m.is_mathml, None);
        assert!(m.source.unwrap().starts_with("<math "));
    }
    assert_eq!(parts, before);
}

#[test]
fn inline_equations_keep_measured_size_color_and_anchor_settings() {
    let dir = evidence().parent().unwrap().join("inline");
    let doc = read_package(&dir.join("after.idml"), None).unwrap();
    let measured: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("indesign.json")).unwrap()).unwrap();
    let mut items = Vec::new();
    folio_idml::for_each_item(&doc, &mut |item| {
        if item.math.is_some() { items.push(item.clone()); }
    });
    assert_eq!(items.len(), 4);
    for case in measured["cases"].as_array().unwrap() {
        let item = items.iter().find(|item| item.math.as_ref().unwrap().source.as_deref() == case["sourceAfter"].as_str()).unwrap();
        let math = item.math.as_ref().unwrap();
        assert_eq!(math.font_size, Some(case["setSize"].as_str().unwrap().parse().unwrap()));
        assert_eq!(math.tint, Some(case["setTint"].as_str().unwrap().parse().unwrap()));
        let rgb: Vec<f64> = case["setRgb"].as_str().unwrap().split(',').map(|n| n.parse().unwrap()).collect();
        for (actual, expected) in math.rgb.unwrap().iter().zip(rgb) {
            assert!((actual - expected).abs() < 0.00001);
        }
        let anchor = item.anchor.unwrap();
        assert_eq!(anchor.position, folio_model::AnchorPosition::Inline);
        assert_eq!(anchor.y_offset, 0.0);
        let svg: serde_json::Value = serde_json::from_str(math.svg_data.as_deref().unwrap()).unwrap();
        let bounds: Vec<f64> = case["boundsInline"].as_str().unwrap().split(',').map(|n| n.parse().unwrap()).collect();
        assert!((svg["width"].as_f64().unwrap() - (bounds[3] - bounds[1])).abs() < 0.00001);
        assert!((svg["height"].as_f64().unwrap() - (bounds[2] - bounds[0])).abs() < 0.00001);
        let baseline: f64 = case["baseline"].as_str().unwrap().parse().unwrap();
        assert!((bounds[2] - baseline).abs() < 0.00001);
    }
}
