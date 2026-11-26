#[path = "support/interop_roundtrip.rs"]
mod support;
use folio_model::{Document, ItemKind};
use serde_json::{Value, json};

fn auto_size(doc: &Document) -> Value {
    let frame = doc.spreads.iter().flat_map(|s| &s.items).find_map(|item| {
        if let ItemKind::TextFrame(frame) = &item.kind { Some(frame) } else { None }
    }).unwrap();
    serde_json::to_value(&frame.auto_size).unwrap()
}

fn check(field: &str, attrs: &[(&str, &str)], expected: Value) {
    support::check(&format!("AutoSize.{field}"), &support::oracle().join("fixtures/basic.idml"),
        "Spreads/Spread_u10c.xml", |xml| {
            let mut xml = support::attribute(xml, "<TextFramePreference ", "AutoSizingType", "HeightOnly");
            for (name, value) in attrs { xml = support::attribute(&xml, "<TextFramePreference ", name, value); }
            xml
        }, |doc| auto_size(doc)[field].clone(), expected);
}

#[test]
fn auto_size_kind_and_reference_points_survive_idml_export() {
    for value in ["HeightOnly", "WidthOnly", "HeightAndWidth", "HeightAndWidthProportionally"] {
        check("kind", &[("AutoSizingType", value)], json!(value));
    }
    for (value, point) in [
        ("TopLeftPoint", [0.0, 0.0]), ("TopCenterPoint", [0.5, 0.0]), ("TopRightPoint", [1.0, 0.0]),
        ("LeftCenterPoint", [0.0, 0.5]), ("CenterPoint", [0.5, 0.5]), ("RightCenterPoint", [1.0, 0.5]),
        ("BottomLeftPoint", [0.0, 1.0]), ("BottomCenterPoint", [0.5, 1.0]), ("BottomRightPoint", [1.0, 1.0]),
    ] {
        check("point", &[("AutoSizingReferencePoint", value)], json!(point));
    }
}

#[test]
fn auto_size_limits_and_line_breaks_survive_idml_export() {
    for (field, enabled, attr) in [
        ("min_height", "UseMinimumHeightForAutoSizing", "MinimumHeightForAutoSizing"),
        ("min_width", "UseMinimumWidthForAutoSizing", "MinimumWidthForAutoSizing"),
    ] {
        for value in [30.5, 60.25] {
            check(field, &[(enabled, "true"), (attr, &value.to_string())], json!(value));
            check(field, &[(enabled, "false"), (attr, &value.to_string())], Value::Null);
        }
    }
    for value in [true, false] {
        check("no_line_breaks", &[("UseNoLineBreaksForAutoSizing", &value.to_string())], json!(value));
    }
}
