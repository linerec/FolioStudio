#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

fn check(field: &str, attr: &str, value: &str, expected: Value) {
    support::check(&format!("Kenten.{field}"), &support::oracle().join("fixtures/basic.idml"), "Stories/Story_u118.xml",
        |xml| {
            let xml = support::attribute(xml, "<CharacterStyleRange ", "KentenKind", "Custom");
            support::attribute(&xml, "<CharacterStyleRange ", attr, value)
        }, |doc| serde_json::to_value(doc.stories["u118"].paragraphs[0].runs[0].attrs.kenten.as_ref().unwrap()).unwrap()[field].clone(), expected);
}

#[test]
fn kenten_kind_and_placement_fields_survive_idml_export() {
    for kind in ["SesameDot","WhiteSesameDot","Fisheye","Bullseye","BlackCircle","WhiteCircle",
        "BlackTriangle","WhiteTriangle","SmallBlackCircle","SmallWhiteCircle","Custom"] {
        let xml = if kind == "Custom" { kind.to_owned() } else { format!("Kenten{kind}") };
        check("kind", "KentenKind", &xml, json!(kind));
    }
    for (value, expected) in [("AboveRight", false), ("BelowLeft", true)] {
        check("below", "KentenPosition", value, json!(expected));
    }
    for value in [-2.5, 4.5] { check("placement", "KentenPlacement", &value.to_string(), json!(value)); }
    for value in ["★", "※"] { check("custom", "KentenCustomCharacter", value, json!(value)); }
    for (value, expected) in [("-1", Value::Null), ("6.5", json!(6.5)), ("12", json!(12.0))] {
        check("size", "KentenFontSize", value, expected);
    }
}
