#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, by_id, check, fixture, roundtrip};

#[test]
fn character_kenten_and_footnote_rule_paints_preserve_none_and_tints() {
    for (color, tint, expected) in [
        ("Swatch/None", "100", json!("None")),
        ("Color/Oracle Blue", "25", json!({"Swatch":{"id":"Color/Oracle Blue","tint":25.0}})),
        ("Color/Oracle Orange", "75", json!({"Swatch":{"id":"Color/Oracle Orange","tint":75.0}})),
    ] {
        for (part, tag, color_key, tint_key, pointer) in [
            ("Stories/Story_u118.xml", "CharacterStyleRange", "FillColor", "FillTint", "/stories/u118/paragraphs/0/runs/0/attrs/fill"),
            ("Stories/Story_u118.xml", "CharacterStyleRange", "KentenFillColor", "KentenTint", "/stories/u118/paragraphs/0/runs/0/attrs/kenten/paint"),
            ("Resources/Preferences.xml", "FootnoteOption", "RuleColor", "RuleTint", "/footnotes/rule/paint"),
        ] {
            let mut parts = fixture("basic");
            let xml = parts.get_mut(part).unwrap();
            attr(xml, tag, 0, color_key, color);
            attr(xml, tag, 0, tint_key, tint);
            if color_key == "KentenFillColor" { attr(xml, tag, 0, "KentenKind", "KentenSesameDot"); }
            if color_key == "RuleColor" { attr(xml, tag, 0, "RuleOn", "true"); }
            let (before, after, written) = roundtrip("basic", &parts);
            check(&before, &after, pointer, expected.clone());
            assert_eq!(written[part], parts[part]);
        }
    }
}

#[test]
fn effect_color_owners_preserve_none_and_swatch_references() {
    let part = "Spreads/Spread_uf3.xml";
    for (color, expected) in [("Swatch/None", json!("None")),
        ("Color/Oracle Blue", json!({"Swatch":{"id":"Color/Oracle Blue","tint":100.0}}))] {
        for (tag, key, field) in [("BevelAndEmbossSetting", "HighlightColor", "bevel/highlight"),
            ("BevelAndEmbossSetting", "ShadowColor", "bevel/shadow"), ("InnerGlowSetting", "EffectColor", "inner_glow/color")] {
            let mut parts = fixture("basic");
            let xml = parts.get_mut(part).unwrap();
            let start = xml.find("<Rectangle Self=\"uff\"").unwrap();
            let end = start + xml[start..].find("</Rectangle>").unwrap();
            xml.insert_str(end, &format!("<TransparencySetting><{tag} Applied=\"true\" {key}=\"{color}\"/></TransparencySetting>"));
            let (before, after, written) = roundtrip("basic", &parts);
            check(by_id(&before, "uff"), by_id(&after, "uff"), &format!("/print/fx/{field}"), expected.clone());
            assert_eq!(written[part], parts[part]);
        }
    }
}
