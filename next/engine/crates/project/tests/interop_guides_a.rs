#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

const PART: &str = "Spreads/Spread_uf3.xml";

fn check(field: &str, attr: &str, value: &str, expected: Value) {
    support::check(&format!("Guide.{field}"), &support::oracle().join("fixtures/basic.idml"), PART,
        |_| support::attribute(&support::golden_part("basic/guide-new-h", PART), "<Guide ", attr, value),
        |doc| serde_json::to_value(&doc.spreads[0].guides[0]).unwrap()[field].clone(), expected);
}

#[test]
fn guide_scalar_fields_survive_idml_export() {
    for (field, attr, values) in [
        ("id", "Self", ["u17c", "u17d"]),
        ("guide_type", "GuideType", ["Ruler", "Liquid"]),
        ("layer", "ItemLayer", ["ub8", "u17a"]),
    ] {
        for value in values { check(field, attr, value, json!(value)); }
    }
    check("layer", "ItemLayer", "n", Value::Null);
    for (field, attr) in [("location", "Location"), ("view_threshold", "ViewThreshold")] {
        for value in [12.5, 48.25] { check(field, attr, &value.to_string(), json!(value)); }
    }
    for (field, attr, yes, no) in [
        ("horizontal", "Orientation", "Horizontal", "Vertical"),
        ("fit_to_page", "FitToPage", "true", "false"),
        ("locked", "Locked", "true", "false"),
    ] {
        check(field, attr, yes, json!(true));
        check(field, attr, no, json!(false));
    }
}

#[test]
fn guide_named_and_rgb_colors_survive_idml_export() {
    for (value, expected) in [
        ("Red", json!({"Named":"Red"})), ("DarkBlue", json!({"Named":"DarkBlue"})),
        ("10 20 30", json!({"Rgb":[10.0,20.0,30.0]})),
        ("240 128 0", json!({"Rgb":[240.0,128.0,0.0]})),
    ] {
        support::check("Guide.color", &support::oracle().join("fixtures/basic.idml"), PART,
            |_| support::property(&support::golden_part("basic/guide-new-h", PART), "<Guide ", "GuideColor", value),
            |doc| serde_json::to_value(&doc.spreads[0].guides[0].color).unwrap(), expected);
    }
    for rgb in [[10.0, 20.0, 30.0], [240.0, 128.0, 0.0]] {
        support::check("Guide.color list", &support::oracle().join("fixtures/basic.idml"), PART,
            |_| {
                let xml = support::golden_part("basic/guide-new-h", PART);
                let xml = support::attribute(&xml, "<GuideColor ", "type", "list");
                let values: String = rgb.iter().map(|v| format!("<ListItem type=\"real\">{v}</ListItem>")).collect();
                support::property(&xml, "<Guide ", "GuideColor", &values)
            },
            |doc| serde_json::to_value(&doc.spreads[0].guides[0].color).unwrap(), json!({"Rgb":rgb}));
    }
}
