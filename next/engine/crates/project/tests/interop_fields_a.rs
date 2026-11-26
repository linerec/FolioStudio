#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

#[test]
fn font_metadata_fields_survive_idml_export() {
    let source = support::oracle().join("fixtures/basic.idml");
    for (field, attr, values) in [
        ("family", "FontFamily", ["Arial", "Malgun Gothic"]),
        ("style", "FontStyleName", ["Regular", "Bold"]),
        ("postscript_name", "PostScriptName", ["ArialMT", "MalgunGothic"]),
        ("full_name", "FullName", ["Arial", "Malgun Gothic"]),
        ("font_type", "FontType", ["TrueType", "OpenTypeTT"]),
        ("status", "Status", ["Installed", "NotAvailable"]),
    ] {
        for value in values {
            support::check(&format!("FontInfo.{field}"), &source, "Resources/Fonts.xml",
                |xml| support::attribute(xml, "<Font ", attr, value),
                |doc| serde_json::to_value(&doc.font_table[0]).unwrap()[field].clone(), json!(value));
        }
    }
    for value in [0, 3] {
        support::check("FontInfo.writing_script", &source, "Resources/Fonts.xml",
            |xml| support::attribute(xml, "<Font ", "WritingScript", &value.to_string()),
            |doc| json!(doc.font_table[0].writing_script), json!(value));
    }
}

#[test]
fn grid_preference_fields_survive_idml_export() {
    let source = support::oracle().join("fixtures/basic.idml");
    let fields: &[(&str, &str, [&str; 2])] = &[
        ("baseline_shown", "BaselineGridShown", ["true", "false"]),
        ("baseline_start", "BaselineStart", ["18.5", "42.25"]),
        ("baseline_division", "BaselineDivision", ["10.5", "15.25"]),
        ("baseline_threshold", "BaselineViewThreshold", ["50.0", "125.0"]),
        ("baseline_relative_to", "BaselineGridRelativeOption", ["TopOfPageOfBaselineGridRelativeOption", "TopOfMarginOfBaselineGridRelativeOption"]),
        ("document_shown", "DocumentGridShown", ["true", "false"]),
        ("document_snap", "DocumentGridSnapto", ["true", "false"]),
        ("horizontal_division", "HorizontalGridlineDivision", ["24.5", "36.25"]),
        ("vertical_division", "VerticalGridlineDivision", ["30.5", "48.25"]),
        ("horizontal_subdivision", "HorizontalGridSubdivision", ["3.0", "5.0"]),
        ("vertical_subdivision", "VerticalGridSubdivision", ["4.0", "6.0"]),
        ("grids_in_back", "GridsInBack", ["true", "false"]),
    ];
    for &(field, attr, values) in fields {
        for value in values {
            let expected = serde_json::from_str::<Value>(value).unwrap_or_else(|_| json!(value));
            support::check(&format!("GridPreferences.{field}"), &source, "Resources/Preferences.xml",
                |xml| support::attribute(xml, "<GridPreference ", attr, value),
                |doc| serde_json::to_value(&doc.view.grid).unwrap()[field].clone(), expected);
        }
    }
    for (field, property) in [("baseline_color", "BaselineColor"), ("grid_color", "GridColor")] {
        for value in ["Red", "DarkBlue"] {
            support::check(&format!("GridPreferences.{field}"), &source, "Resources/Preferences.xml",
                |xml| support::property(xml, "<GridPreference ", property, value),
                |doc| serde_json::to_value(&doc.view.grid).unwrap()[field].clone(), json!({"Named": value}));
        }
        for rgb in [[10.0, 20.0, 30.0], [240.0, 128.0, 0.0]] {
            support::check(&format!("GridPreferences.{field}"), &source, "Resources/Preferences.xml",
                |xml| {
                    let xml = support::attribute(xml, &format!("<{property} "), "type", "list");
                    let items = rgb.map(|v| format!("<ListItem type=\"double\">{v}</ListItem>")).join("");
                    support::property(&xml, "<GridPreference ", property, &items)
                },
                |doc| serde_json::to_value(&doc.view.grid).unwrap()[field].clone(), json!({"Rgb": rgb}));
        }
    }
}
