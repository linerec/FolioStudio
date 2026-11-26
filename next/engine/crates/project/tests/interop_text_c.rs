#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, check, fixture, roundtrip};

const STORY: &str = "Stories/Story_u118.xml";

fn content(xml: &mut String, replacement: &str) {
    let start = xml.find("<Content>").unwrap();
    let end = start + xml[start..].find("</Content>").unwrap() + "</Content>".len();
    xml.replace_range(start..end, replacement);
}

#[test]
fn named_variable_identity_instance_and_cached_result_survive_export() {
    for (id, instance, result) in [("var-a", "instance-a", "제목 A"), ("var-b", "instance-b", "제목 B")] {
        let mut parts = fixture("basic");
        content(parts.get_mut(STORY).unwrap(), &format!(
            "<TextVariableInstance Self=\"{instance}\" AssociatedTextVariable=\"{id}\" ResultText=\"{result}\" />"));
        let (before, after, written) = roundtrip("basic", &parts);
        check(&before, &after, "/stories/u118/paragraphs/0/runs/0/named",
            json!({"id":id,"instance":instance,"result":result}));
        check(&before, &after, "/stories/u118/paragraphs/0/runs/0/variable", json!("Named"));
        assert_eq!(written[STORY], parts[STORY]);
    }
}

#[test]
fn page_variable_tokens_preserve_kind_without_claiming_layout_equivalence() {
    for (code, attribute, expected) in [
        ("18", "CurrentPageNumber", "PageNumber"),
        ("18", "NextPageNumber", "NextPageNumber"),
        ("18", "PreviousPageNumber", "PreviousPageNumber"),
        ("19", "CurrentPageNumber", "SectionMarker"),
    ] {
        let mut parts = fixture("basic");
        let xml = parts.get_mut(STORY).unwrap();
        attr(xml, "CharacterStyleRange", 0, "PageNumberType", attribute);
        content(xml, &format!("<Content><?ACE {code}?></Content>"));
        let (before, after, written) = roundtrip("basic", &parts);
        check(&before, &after, "/stories/u118/paragraphs/0/runs/0/variable", json!(expected));
        assert_eq!(written[STORY], parts[STORY]);
    }
}

#[test]
fn character_and_paragraph_style_references_survive_export() {
    for label in ["Alpha", "Beta"] {
        let mut parts = fixture("basic");
        let styles = parts.get_mut("Resources/Styles.xml").unwrap();
        for kind in ["Character", "Paragraph"] {
            *styles = styles.replace(&format!("</Root{kind}StyleGroup>"), &format!(
                "<{kind}Style Self=\"{kind}Style/{label}\" Name=\"{label}\" /></Root{kind}StyleGroup>"));
        }
        let story = parts.get_mut(STORY).unwrap();
        attr(story, "ParagraphStyleRange", 0, "AppliedParagraphStyle", &format!("ParagraphStyle/{label}"));
        attr(story, "CharacterStyleRange", 0, "AppliedCharacterStyle", &format!("CharacterStyle/{label}"));
        let (before, after, written) = roundtrip("basic", &parts);
        check(&before, &after, "/stories/u118/paragraphs/0/style", json!(label));
        check(&before, &after, "/stories/u118/paragraphs/0/runs/0/attrs/character_style", json!(label));
        assert_eq!(written[STORY], parts[STORY]);
        assert_eq!(written["Resources/Styles.xml"], parts["Resources/Styles.xml"]);
    }
}

#[test]
fn style_catalog_identity_name_and_nested_group_survive_export() {
    for kind in ["Paragraph", "Character", "Object"] {
        for label in ["Alpha", "Beta"] {
            let mut parts = fixture("basic");
            let styles = parts.get_mut("Resources/Styles.xml").unwrap();
            *styles = styles.replace(&format!("</Root{kind}StyleGroup>"), &format!(
                "<{kind}StyleGroup Self=\"group-a\" Name=\"Outer\"><{kind}StyleGroup Self=\"group-b\" Name=\"Outer:Inner\"><{kind}Style Self=\"{kind}Style/{label}\" Name=\"{label}\" /></{kind}StyleGroup></{kind}StyleGroup></Root{kind}StyleGroup>"));
            let (before, after, written) = roundtrip("basic", &parts);
            let key = kind.to_lowercase();
            let entries = before["styles"][&key].as_array().unwrap();
            let index = entries.len() - 1;
            check(&before, &after, &format!("/styles/{key}/{index}"), json!({
                "id":format!("{kind}Style/{label}"),"name":label,"group":["Outer","Inner"],
                "based_on":"","settings":{}}));
            assert_eq!(before["styles"][&key], after["styles"][&key]);
            assert_eq!(written["Resources/Styles.xml"], parts["Resources/Styles.xml"]);
        }
    }
}

#[test]
fn section_marker_and_last_page_follow_stored_section_boundaries() {
    for marker in ["앞부분", "뒷부분"] {
        let mut parts = fixture("basic");
        attr(parts.get_mut("designmap.xml").unwrap(), "Section", 0, "Marker", marker);
        for (name, xml) in &mut parts {
            if name.starts_with("Spreads/") {
                let count = xml.matches("<Page ").count();
                for index in 0..count {
                    attr(xml, "Page", index, "Name", marker);
                }
            }
        }
        let (before, after, written) = roundtrip("basic", &parts);
        let pages: Vec<_> = before["spreads"].as_array().unwrap().iter()
            .flat_map(|s| s["pages"].as_array().unwrap()).collect();
        let last = pages.last().unwrap()["name"].clone();
        assert_eq!(last, json!(marker));
        assert_eq!(pages.len(), 3);
        for (s, spread) in before["spreads"].as_array().unwrap().iter().enumerate() {
            for p in 0..spread["pages"].as_array().unwrap().len() {
                check(&before, &after, &format!("/spreads/{s}/pages/{p}/section_marker"), json!(marker));
                check(&before, &after, &format!("/spreads/{s}/pages/{p}/section_last"), last.clone());
            }
        }
        assert_eq!(written["designmap.xml"], parts["designmap.xml"]);
    }
}
