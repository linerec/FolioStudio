#[path = "support/interop.rs"]
mod support;
use serde_json::{Value, json};
use support::{attr, fixture, roundtrip};

fn cell(doc: &Value) -> &Value {
    doc["stories"]["ued"]["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["runs"].as_array().unwrap())
        .flat_map(|r| r["tables"].as_array().unwrap())
        .next()
        .unwrap()
        .pointer("/cells/0")
        .unwrap()
}

#[test]
fn cell_fill_and_all_edge_paints_preserve_none_and_swatch_tints() {
    const PART: &str = "Stories/Story_ued.xml";
    for (color, tint, expected) in [
        ("Swatch/None", "100", json!("None")),
        (
            "Color/Black",
            "25",
            json!({"Swatch":{"id":"Color/Black","tint":25.0}}),
        ),
        (
            "Color/Paper",
            "75",
            json!({"Swatch":{"id":"Color/Paper","tint":75.0}}),
        ),
    ] {
        let mut parts = fixture("tables");
        let xml = parts.get_mut(PART).unwrap();
        attr(xml, "Cell", 0, "FillColor", color);
        attr(xml, "Cell", 0, "FillTint", tint);
        for side in ["Top", "Left", "Bottom", "Right"] {
            for (key, value) in [
                ("Weight", "2"),
                ("Type", "StrokeStyle/$ID/Dashed"),
                ("Color", color),
                ("Tint", tint),
                ("GapColor", color),
                ("GapTint", tint),
            ] {
                attr(xml, "Cell", 0, &format!("{side}EdgeStroke{key}"), value);
            }
        }
        let (before, after, written) = roundtrip("tables", &parts);
        for doc in [&before, &after] {
            let cell = cell(doc);
            assert_eq!(cell["fill"], expected);
            for edge in cell["edges"].as_array().unwrap() {
                assert_eq!(edge["paint"], expected);
                assert_eq!(edge["gap"], expected);
            }
        }
        assert_eq!(written[PART], parts[PART]);
    }
}

#[test]
fn solid_or_zero_width_cell_strokes_suppress_gap_paint_but_preserve_attributes() {
    const PART: &str = "Stories/Story_ued.xml";
    for (kind, width) in [("Solid", "2"), ("Dashed", "0")] {
        let mut parts = fixture("tables");
        let xml = parts.get_mut(PART).unwrap();
        for (key, value) in [
            ("Weight", width),
            ("Color", "Color/Black"),
            ("GapColor", "Color/Paper"),
            ("GapTint", "50"),
        ] {
            attr(xml, "Cell", 0, &format!("TopEdgeStroke{key}"), value);
        }
        attr(
            xml,
            "Cell",
            0,
            "TopEdgeStrokeType",
            &format!("StrokeStyle/$ID/{kind}"),
        );
        let (before, after, written) = roundtrip("tables", &parts);
        for doc in [&before, &after] {
            assert_eq!(cell(doc)["edges"][0]["gap"], json!("None"));
            if width == "0" {
                assert_eq!(cell(doc)["edges"][0]["paint"], json!("None"));
            }
        }
        assert_eq!(written[PART], parts[PART]);
    }
}
