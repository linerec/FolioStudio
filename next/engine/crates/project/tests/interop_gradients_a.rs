#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

const GRAPHIC: &str = "Resources/Graphic.xml";
const GRADIENT: &str = "<Gradient Self=\"Gradient/Ramp\"";

fn check(field: &str, attr: &str, value: &str, expected: Value) {
    support::check(&format!("Gradient.{field}"), &support::oracle().join("fixtures/color-gradient.idml"), GRAPHIC,
        |xml| support::attribute(xml, GRADIENT, attr, value),
        |doc| serde_json::to_value(&doc.gradients["Gradient/Ramp"]).unwrap()[field].clone(), expected);
}

#[test]
fn gradient_identity_and_kind_survive_idml_export() {
    for value in ["Ramp A", "Ramp B"] { check("name", "Name", value, json!(value)); }
    for value in ["Linear", "Radial"] { check("kind", "Type", value, json!(value)); }
    for value in ["Gradient/ChangedA", "Gradient/ChangedB"] {
        support::check("Gradient.id", &support::oracle().join("fixtures/color-gradient.idml"), GRAPHIC,
            |xml| support::attribute(xml, GRADIENT, "Self", value),
            |doc| json!(doc.gradients[value].id), json!(value));
    }
}

#[test]
fn gradient_stops_survive_idml_export() {
    for (color, location, midpoint) in [("Color/Blue", 15.0, 25.0), ("Color/Orange", 35.0, 75.0)] {
        support::check("Gradient.stops and GradientStop fields", &support::oracle().join("fixtures/color-gradient.idml"), GRAPHIC,
            |xml| {
                let xml = support::attribute(xml, "<GradientStop ", "StopColor", color);
                let xml = support::attribute(&xml, "<GradientStop ", "Location", &location.to_string());
                support::attribute(&xml, "<GradientStop ", "Midpoint", &midpoint.to_string())
            },
            |doc| serde_json::to_value(&doc.gradients["Gradient/Ramp"].stops).unwrap(),
            json!([{"color":color,"location":location,"midpoint":midpoint},
                {"color":"Color/Black","location":100.0,"midpoint":50.0}]));
    }
}

#[test]
fn gradient_fill_and_stroke_geometry_survive_idml_export() {
    let source = support::oracle().join("fixtures/color-gradient.idml");
    let part = "Spreads/Spread_u112.xml";
    let item = "<Rectangle Self=\"u12e\"";
    for prefix in ["Fill", "Stroke"] {
        for (field, suffix, values) in [
            ("angle", "Angle", [-45.0, 90.0]),
            ("length", "Length", [100.0, 250.0]),
            ("hilite_length", "HiliteLength", [10.0, 25.0]),
            ("hilite_angle", "HiliteAngle", [15.0, 60.0]),
        ] {
            for value in values {
                support::check(&format!("GradientGeometry.{field} {prefix}"), &source, part,
                    |xml| {
                        let xml = support::attribute(xml, item, &format!("{prefix}Color"), "Gradient/Ramp");
                        support::attribute(&xml, item, &format!("Gradient{prefix}{suffix}"), &value.to_string())
                    },
                    |doc| {
                        let item = doc.spreads[0].items.iter().find(|i| i.id == "u12e").unwrap();
                        let geometry = if prefix == "Fill" { item.fill_gradient } else { item.stroke_gradient };
                        serde_json::to_value(geometry.unwrap()).unwrap()[field].clone()
                    }, json!(value));
            }
        }
        for (value, expected) in [("12.5 25", json!({"x":12.5,"y":25.0})), ("-20 40", json!({"x":-20.0,"y":40.0}))] {
            support::check("GradientGeometry.start", &source, part,
                |xml| {
                    let xml = support::attribute(xml, item, &format!("{prefix}Color"), "Gradient/Ramp");
                    support::attribute(&xml, item, &format!("Gradient{prefix}Start"), value)
                },
                |doc| {
                    let item = doc.spreads[0].items.iter().find(|i| i.id == "u12e").unwrap();
                    let geometry = if prefix == "Fill" { item.fill_gradient } else { item.stroke_gradient };
                    serde_json::to_value(geometry.unwrap().start).unwrap()
                }, expected);
        }
    }
}

#[test]
fn ink_name_process_flag_and_print_fields_survive_idml_export() {
    let source = support::oracle().join("fixtures/color-gradient.idml");
    for (name, process) in [("$ID/Process Cyan", true), ("$ID/Process Magenta", true),
        ("$ID/Process Yellow", true), ("$ID/Process Black", true), ("Spot Test A", false), ("Spot Test B", false)] {
        support::check("Ink.name/process", &source, GRAPHIC,
            |xml| support::attribute(xml, "<Ink ", "Name", name),
            |doc| json!([doc.print.inks[0].name, doc.print.inks[0].process]),
            json!([name.trim_start_matches("$ID/"), process]));
    }
    for value in [false, true] {
        support::check("Ink.print", &source, GRAPHIC,
            |xml| support::attribute(xml, "<Ink ", "PrintInk", if value { "true" } else { "false" }),
            |doc| json!(doc.print.inks[0].print), json!(value));
    }
    for value in [0.35, 1.25] {
        support::check("Ink.neutral_density", &source, GRAPHIC,
            |xml| support::attribute(xml, "<Ink ", "NeutralDensity", &value.to_string()),
            |doc| json!(doc.print.inks[0].neutral_density), json!(value));
    }
}
