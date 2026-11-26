#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

const PART: &str = "Spreads/Spread_uf3.xml";

#[test]
fn affine_linear_coefficients_survive_idml_export() {
    for matrix in [[1.5,0.25,-0.5,2.0,10.0,20.0], [-1.0,0.0,0.0,1.0,30.0,40.0], [0.0,1.0,-1.0,0.0,50.0,60.0]] {
        let text = matrix.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
        support::check("Affine.a/b/c/d", &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| support::attribute(xml, "<Rectangle Self=\"ufe\"", "ItemTransform", &text),
            |doc| {
                let a = doc.spreads[0].items.iter().find(|i| i.id == "ufe").unwrap().transform;
                json!([a.a,a.b,a.c,a.d])
            }, json!(&matrix[..4]));
    }
}

#[test]
fn page_scalar_and_array_fields_survive_idml_export() {
    for (field, attr, value, expected) in [
        ("id", "Self", "ufc", json!("ufc")), ("id", "Self", "u17d", json!("u17d")),
        ("bounds", "GeometricBounds", "0 0 600 400", json!({"top":0.0,"left":0.0,"bottom":600.0,"right":400.0})),
        ("bounds", "GeometricBounds", "-20 -10 620 410", json!({"top":-20.0,"left":-10.0,"bottom":620.0,"right":410.0})),
        ("master", "AppliedMaster", "ub9", json!("ub9")), ("master", "AppliedMaster", "n", Value::Null),
        ("master_overrides", "OverrideList", "u1 u2 u3 n", json!(["u1","u3"])),
        ("master_overrides", "OverrideList", "u4 n", json!(["u4"])),
        ("master_overrides", "OverrideList", "", json!([])),
    ] {
        support::check(&format!("Page.{field}"), &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| support::attribute(xml, "<Page ", attr, value),
            |doc| serde_json::to_value(&doc.spreads[0].pages[0]).unwrap()[field].clone(), expected);
    }
    for values in [[10.0,20.0,30.0,40.0], [15.5,25.5,35.5,45.5]] {
        support::check("Page.margins", &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| {
                let mut out = xml.to_owned();
                for (name, value) in ["Top","Left","Bottom","Right"].into_iter().zip(values) {
                    out = support::attribute(&out, "<MarginPreference ", name, &value.to_string());
                }
                out
            },
            |doc| json!(doc.spreads[0].pages[0].margins), json!(values));
    }
}

#[test]
fn page_column_configuration_survives_idml_export() {
    for (count,gutter,direction,positions) in [(2,12.5,"Horizontal",vec![0.0,100.0,112.5,212.5]), (3,18.25,"Vertical",vec![0.0,80.0,98.25,178.25,196.5,276.5])] {
        support::check("Page.columns", &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| {
                let mut out = xml.to_owned();
                for (name, value) in [("ColumnCount",count.to_string()),("ColumnGutter",gutter.to_string()),
                    ("ColumnDirection",direction.to_owned()),("ColumnsPositions",positions.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "))] {
                    out = support::attribute(&out, "<MarginPreference ", name, &value);
                }
                out
            },
            |doc| serde_json::to_value(&doc.spreads[0].pages[0].columns).unwrap(),
            json!({"count":count,"gutter":gutter,"positions":positions,"direction":direction}));
    }
}

#[test]
fn item_print_boolean_fields_survive_idml_export() {
    for (field, selector, attr) in [
        ("nonprinting", "<Rectangle Self=\"ufe\"", "Nonprinting"),
        ("overprint_fill", "<Rectangle Self=\"ufe\"", "OverprintFill"),
        ("overprint_stroke", "<Rectangle Self=\"ufe\"", "OverprintStroke"),
        ("overprint_gap", "<Rectangle Self=\"ufe\"", "OverprintGap"),
        ("isolate_blending", "<BlendingSetting ", "IsolateBlending"),
        ("knockout_group", "<BlendingSetting ", "KnockoutGroup"),
    ] {
        for value in [false, true] {
            let blending = selector == "<BlendingSetting ";
            let id = if blending { "u105" } else { "ufe" };
            support::check(&format!("ItemPrint.{field}"), &support::oracle().join("fixtures/basic.idml"), PART,
                |xml| {
                    let xml = if blending { support::golden_part("basic/fx-isolate", PART) } else { xml.to_owned() };
                    support::attribute(&xml, selector, attr, if value { "true" } else { "false" })
                },
                |doc| serde_json::to_value(&doc.spreads[0].items.iter().find(|i| i.id == id).unwrap().print).unwrap()[field].clone(), json!(value));
        }
    }
}
