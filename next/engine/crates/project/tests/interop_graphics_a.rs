#[path = "support/interop_roundtrip.rs"]
mod support;
use folio_model::{Document, Graphic, ItemKind};
use serde_json::json;

const PART: &str = "Spreads/Spread_u10f.xml";

fn graphic(doc: &Document) -> &Graphic {
    doc.spreads[0].items.iter().find_map(|i| match &i.kind {
        ItemKind::Rectangle { content: Some(g) } => Some(g), _ => None,
    }).unwrap()
}

#[test]
fn graphic_identity_resolution_and_type_survive_idml_export() {
    for (field, attr, value, expected) in [
        ("id", "Self", "u130a", json!("u130a")), ("id", "Self", "u130b", json!("u130b")),
        ("actual_ppi", "ActualPpi", "72 96", json!([72.0,96.0])),
        ("actual_ppi", "ActualPpi", "300 240", json!([300.0,240.0])),
        ("effective_ppi", "EffectivePpi", "150 200", json!([150.0,200.0])),
        ("effective_ppi", "EffectivePpi", "250 350", json!([250.0,350.0])),
        ("image_space", "Space", "$ID/#Links_RGB", json!("RGB")),
        ("image_space", "Space", "$ID/#Links_CMYK", json!("CMYK")),
        ("image_type", "ImageTypeName", "$ID/JPEG", json!("JPEG")),
        ("image_type", "ImageTypeName", "$ID/Photoshop", json!("Photoshop")),
    ] {
        support::check(&format!("Graphic.{field}"), &support::oracle().join("fixtures/objopts-clip.idml"), PART,
            |xml| support::attribute(xml, "<Image ", attr, value),
            |doc| serde_json::to_value(graphic(doc)).unwrap()[field].clone(), expected);
    }
}

#[test]
fn graphic_bounds_transform_and_clip_inversion_survive_idml_export() {
    let source = support::oracle().join("fixtures/objopts-clip.idml");
    for matrix in [[1.5,0.25,-0.5,2.0,10.0,20.0], [-1.0,0.0,0.0,1.0,30.0,40.0]] {
        support::check("Graphic.transform", &source, PART,
            |xml| support::attribute(xml, "<Image ", "ItemTransform", &matrix.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")),
            |doc| { let a = graphic(doc).transform; json!([a.a,a.b,a.c,a.d,a.tx,a.ty]) }, json!(matrix));
    }
    for bounds in [[-10.0,-20.0,150.0,200.0], [5.0,10.0,205.0,310.0]] {
        support::check("Graphic.bounds", &source, PART,
            |xml| {
                let mut xml = xml.to_owned();
                for (key, value) in ["Left","Top","Right","Bottom"].iter().zip(bounds) {
                    xml = support::attribute(&xml, "<GraphicBounds ", key, &value.to_string());
                }
                xml
            }, |doc| { let b = graphic(doc).bounds; json!([b.left,b.top,b.right,b.bottom]) }, json!(bounds));
    }
    for value in [false,true] {
        support::check("GraphicClip.invert", &source, PART,
            |xml| support::attribute(xml, "<ClippingPathSettings ", "InvertPath", if value { "true" } else { "false" }),
            |doc| json!(graphic(doc).clip.as_ref().unwrap().invert), json!(value));
    }
}
