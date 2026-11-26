//! Insert MathML / change an equation / delete it: a rectangle with a MathObject that holds the
//! source and the drawing, readable again by the IDML reader (rules/mathml.ko.md).
use folio_edit::{commands::RegisteredCommand, xml::Index, Editor};
use folio_model::mathml::MathObject;
use std::collections::BTreeMap;
use std::path::Path;

const FRAC: &str = "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mfrac><mn>1</mn><mn>2</mn></mfrac></math>";
const ROOT: &str = "<math><msqrt><mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow></msqrt></math>";

fn fixture() -> (Editor, String) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic-create.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut entry, &mut s).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), s);
    }
    let dm = &parts["designmap.xml"];
    let dx = Index::scan(dm).unwrap();
    let spread = (0..dx.nodes.len()).find(|&i| dx.local(dm, i) == "Spread").unwrap();
    let t = &parts[dx.attr(dm, spread, "src").unwrap()];
    let ix = Index::scan(t).unwrap();
    let p = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page").unwrap();
    let page = ix.value(t, p, "Self").unwrap();
    (Editor::new(parts), page)
}

fn equations(ed: &Editor) -> Vec<(String, MathObject, [f64; 4])> {
    let doc = folio_idml::read_parts(ed.parts(), Path::new("memory.idml"), &folio_idml::ReaderData::default()).unwrap();
    let mut found = Vec::new();
    folio_idml::for_each_item(&doc, &mut |item| {
        if let Some(math) = &item.math {
            let b = folio_model::paths_bounds(&item.paths).unwrap();
            found.push((item.id.clone(), math.clone(), [b.left, b.top, b.right, b.bottom]));
        }
    });
    found
}

fn insert(page: &str, mathml: &str, at: [f64; 2], size: f64, color: [f64; 3]) -> folio_edit::Command {
    RegisteredCommand::InsertMathml { page: page.into(), mathml: mathml.into(), at, size, color }.into()
}

#[test]
fn insert_makes_a_frame_with_source_and_drawing() {
    let (mut ed, page) = fixture();
    let out = ed.apply("insert", &[insert(&page, FRAC, [36.0, 72.0], 12.0, [17.0, 83.0, 149.0])]).unwrap();
    assert_eq!(out.created.len(), 1);
    let found = equations(&ed);
    assert_eq!(found.len(), 1);
    let (id, math, b) = &found[0];
    assert_eq!(id, &out.created[0]);
    // InDesign reads the size and colour from the source: mathsize / mathcolor on <math>
    let source = math.source.as_deref().unwrap();
    assert!(source.starts_with("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" mathsize=\"12pt\" mathcolor=\"#115395\">"), "{source}");
    assert_eq!(math.font_size, Some(12.0));
    assert_eq!(math.rgb, Some([17.0, 83.0, 149.0]));
    assert_eq!(math.tint, Some(100.0));
    assert_eq!(math.is_mathml, Some(true));
    assert_eq!(math.swatch.as_deref(), Some("Nothing"));
    // the drawing is InDesign's kind: JSON with the size, an SVG of outlines, no font
    let svg: serde_json::Value = serde_json::from_str(math.svg_data.as_deref().unwrap()).unwrap();
    let svg_str = svg["svg_str"].as_str().unwrap();
    assert!(svg_str.starts_with("<svg ") && svg_str.contains("viewBox=") && svg_str.contains("<path d=\"M") && svg_str.contains("currentColor"));
    assert!(!svg_str.contains("font-family"));
    // InDesign shows the stored drawing as it is, so the colour is written on it
    assert!(svg_str.contains("fill=\"#115395\""));
    // the frame is the drawing's size, its corner where it was asked for
    assert!((b[2] - b[0] - svg["width"].as_f64().unwrap()).abs() < 1e-6, "{b:?} {}", svg["width"]);
    assert!((b[3] - b[1] - svg["height"].as_f64().unwrap()).abs() < 1e-6);
    // its corner is where a rectangle drawn there would have it
    let (mut plain_ed, plain_page) = fixture();
    let plain_id = plain_ed
        .apply(
            "rect",
            &[folio_edit::Command::CreateItem { page: plain_page, kind: folio_edit::ItemKind::Rectangle, rect: [36.0, 72.0, 36.0 + (b[2] - b[0]), 72.0 + (b[3] - b[1])], layer: None, sides: 6 }],
        )
        .unwrap()
        .created[0]
        .clone();
    let plain_doc = folio_idml::read_parts(plain_ed.parts(), Path::new("memory.idml"), &folio_idml::ReaderData::default()).unwrap();
    let mut plain_bounds = None;
    folio_idml::for_each_item(&plain_doc, &mut |item| {
        if item.id == plain_id {
            plain_bounds = folio_model::paths_bounds(&item.paths);
        }
    });
    let pb = plain_bounds.unwrap();
    assert!((pb.left - b[0]).abs() < 1e-6 && (pb.top - b[1]).abs() < 1e-6, "{pb:?} {b:?}");
    // a 12 pt equation is 1.2 times a 10 pt one
    let (mut ed10, page10) = fixture();
    ed10.apply("insert", &[insert(&page10, FRAC, [0.0, 0.0], 10.0, [0.0; 3])]).unwrap();
    let w10 = equations(&ed10)[0].2;
    assert!(((b[2] - b[0]) / (w10[2] - w10[0]) - 1.2).abs() < 1e-6);
}

#[test]
fn undo_and_redo_restore_the_document() {
    let (mut ed, page) = fixture();
    let before = ed.parts().clone();
    ed.apply("insert", &[insert(&page, FRAC, [10.0, 10.0], 10.0, [0.0; 3])]).unwrap();
    let after = ed.parts().clone();
    assert_ne!(before, after);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

#[test]
fn set_lays_the_equation_out_again_and_keeps_the_corner() {
    let (mut ed, page) = fixture();
    let id = ed.apply("insert", &[insert(&page, FRAC, [36.0, 72.0], 10.0, [0.0; 3])]).unwrap().created[0].clone();
    let first = equations(&ed).remove(0);
    ed.apply("change", &[RegisteredCommand::SetMathml { id: id.clone(), mathml: ROOT.into(), size: 14.0, color: [255.0, 0.0, 0.0] }.into()]).unwrap();
    let found = equations(&ed);
    assert_eq!(found.len(), 1);
    let (same, math, b) = &found[0];
    assert_eq!(same, &id);
    assert!(math.source.as_deref().unwrap().contains("msqrt") && math.source.as_deref().unwrap().contains("xmlns"));
    assert_eq!(math.font_size, Some(14.0));
    assert_eq!(math.rgb, Some([255.0, 0.0, 0.0]));
    assert_ne!(math.svg_data, first.1.svg_data);
    let svg: serde_json::Value = serde_json::from_str(math.svg_data.as_deref().unwrap()).unwrap();
    assert!((b[2] - b[0] - svg["width"].as_f64().unwrap()).abs() < 1e-6 && (b[3] - b[1] - svg["height"].as_f64().unwrap()).abs() < 1e-6, "{b:?}");
    assert!((b[0] - first.2[0]).abs() < 1e-6 && (b[1] - first.2[1]).abs() < 1e-6, "top-left moved: {b:?} {:?}", first.2);
    ed.undo().unwrap();
    assert_eq!(equations(&ed)[0].1.source.as_deref(), Some(FRAC));
}

#[test]
fn delete_removes_only_equations() {
    let (mut ed, page) = fixture();
    let id = ed.apply("insert", &[insert(&page, FRAC, [36.0, 72.0], 10.0, [0.0; 3])]).unwrap().created[0].clone();
    // an ordinary frame is not an equation
    let plain = ed
        .apply("rect", &[folio_edit::Command::CreateItem { page: page.clone(), kind: folio_edit::ItemKind::Rectangle, rect: [0., 0., 50., 50.], layer: None, sides: 6 }])
        .unwrap()
        .created[0]
        .clone();
    let before = ed.parts().clone();
    assert!(ed.apply("bad", &[RegisteredCommand::DeleteMathml { id: plain }.into()]).is_err());
    assert_eq!(ed.parts(), &before);
    ed.apply("delete", &[RegisteredCommand::DeleteMathml { id }.into()]).unwrap();
    assert!(equations(&ed).is_empty());
}

#[test]
fn refused_input_changes_nothing() {
    let (mut ed, page) = fixture();
    let before = ed.parts().clone();
    for (mathml, size, color, at) in [
        ("<math><mfrac>", 10.0, [0.0; 3], [0.0, 0.0]),
        ("not mathml", 10.0, [0.0; 3], [0.0, 0.0]),
        (FRAC, 0.0, [0.0; 3], [0.0, 0.0]),
        (FRAC, 10.0, [300.0, 0.0, 0.0], [0.0, 0.0]),
        (FRAC, 10.0, [0.0; 3], [f64::NAN, 0.0]),
    ] {
        assert!(ed.apply("bad", &[insert(&page, mathml, at, size, color)]).is_err(), "{mathml} {size}");
        assert_eq!(ed.parts(), &before);
    }
    assert!(ed.apply("missing page", &[insert("nope", FRAC, [0.0, 0.0], 10.0, [0.0; 3])]).is_err());
    assert!(ed.apply("not an equation", &[RegisteredCommand::SetMathml { id: "nope".into(), mathml: FRAC.into(), size: 10.0, color: [0.0; 3] }.into()]).is_err());
    assert_eq!(ed.parts(), &before);
}

#[test]
fn the_idml_parts_carry_the_equation_the_way_indesign_stores_it() {
    let (mut ed, page) = fixture();
    ed.apply("insert", &[insert(&page, FRAC, [36.0, 72.0], 10.0, [0.0; 3])]).unwrap();
    let spread = ed.parts().iter().find(|(k, v)| k.starts_with("Spreads/") && v.contains("<MathObject")).unwrap().1;
    for needle in [
        "<Rectangle ",
        "<MathObject Self=\"u",
        "AppliedMathMLFontSize=\"10\"",
        "AppliedMathMLRgbColor=\"0 0 0\"",
        "TintValue=\"100\"",
        "IsMathMLObject=\"true\"",
        "MathMLDescription=\"&lt;math xmlns=&quot;http://www.w3.org/1998/Math/MathML&quot;&gt;",
        "MathmlSvgData=\"{&quot;height&quot;:",
        "<AppliedMathMLSwatch type=\"enumeration\">Nothing</AppliedMathMLSwatch>",
        "</MathObject>",
    ] {
        assert!(spread.contains(needle), "{needle}");
    }
    // the whole part is still well-formed XML
    Index::scan(spread).unwrap();
}
