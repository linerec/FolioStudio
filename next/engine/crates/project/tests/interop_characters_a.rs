#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

const PART: &str = "Stories/Story_u118.xml";

fn attributes(doc: &folio_model::Document) -> Value {
    serde_json::to_value(&doc.stories["u118"].paragraphs[0].runs[0].attrs).unwrap()
}

fn check(field: &str, attr: &str, value: &str, expected: Value) {
    support::check(&format!("CharacterAttrs.{field}"), &support::oracle().join("fixtures/basic.idml"), PART,
        |xml| support::attribute(xml, "<CharacterStyleRange ", attr, value),
        |doc| attributes(doc)[field].clone(), expected);
}

#[test]
fn character_numeric_and_boolean_fields_survive_idml_export() {
    for (field, attr, values) in [
        ("point_size", "PointSize", [12.5, 24.0]),
        ("tracking", "Tracking", [-25.0, 75.0]),
        ("horizontal_scale", "HorizontalScale", [80.0, 120.0]),
        ("vertical_scale", "VerticalScale", [85.0, 125.0]),
        ("baseline_shift", "BaselineShift", [-3.5, 4.5]),
        ("skew", "Skew", [-12.0, 18.0]),
    ] {
        for value in values { check(field, attr, &value.to_string(), json!(value)); }
    }
    for (field, attr) in [("ligatures","Ligatures"),("no_break","NoBreak"),("overprint_fill","OverprintFill"),("overprint_stroke","OverprintStroke")] {
        for value in [false,true] { check(field, attr, if value { "true" } else { "false" }, json!(value)); }
    }
    // AutoLeading belongs to the paragraph even though the resolved value is on each run.
    for value in [110.0,150.0] {
        support::check("CharacterAttrs.auto_leading", &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| support::attribute(xml, "<ParagraphStyleRange ", "AutoLeading", &value.to_string()),
            |doc| attributes(doc)["auto_leading"].clone(), json!(value));
    }
}

#[test]
fn character_font_and_language_fields_survive_idml_export() {
    for value in ["Arial","Malgun Gothic"] {
        support::check("CharacterAttrs.font_family", &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| support::property(xml, "<CharacterStyleRange ", "AppliedFont", value),
            |doc| attributes(doc)["font_family"].clone(), json!(value));
    }
    for value in ["Regular","Bold"] { check("font_style","FontStyle",value,json!(value)); }
    for (value,expected) in [("Language/$ID/English: USA","English: USA"),("Language/$ID/Korean","Korean")] {
        check("language","AppliedLanguage",value,json!(expected));
    }
}

#[test]
fn character_leading_and_enum_fields_survive_idml_export() {
    for (field, attr, values) in [
        ("capitalization", "Capitalization", &[("Normal","Normal"),("SmallCaps","SmallCaps"),("AllCaps","AllCaps"),("CapToSmallCap","CapToSmallCap"),("LowerCase","LowerCase")][..]),
        ("position", "Position", &[("Normal","Normal"),("Superscript","Superscript"),("Subscript","Subscript"),
            ("OTSuperscript","OtSuperscript"),("OTSubscript","OtSubscript"),("OTNumerator","OtNumerator"),("OTDenominator","OtDenominator")][..]),
    ] {
        for (value,expected) in values { check(field,attr,value,json!(expected)); }
    }
    for (kind,value,expected) in [("enumeration","Auto",json!("Auto")),("unit","14.5",json!({"Fixed":14.5})),("unit","25.5",json!({"Fixed":25.5}))] {
        support::check("CharacterAttrs.leading", &support::oracle().join("fixtures/basic.idml"), PART,
            |xml| xml.replacen("<AppliedFont ", &format!("<Leading type=\"{kind}\">{value}</Leading><AppliedFont "), 1),
            |doc| attributes(doc)["leading"].clone(), expected);
    }
}
