#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, fixture, roundtrip};

#[test]
fn nonprinting_shading_is_preserved_in_xml_but_omitted_from_the_model() {
    const PART: &str = "Stories/Story_u118.xml";
    for suppress in [false, true] {
        let mut parts = fixture("basic");
        let xml = parts.get_mut(PART).unwrap();
        attr(xml, "ParagraphStyleRange", 0, "ParagraphShadingOn", "true");
        attr(xml, "ParagraphStyleRange", 0, "ParagraphShadingColor", "Color/Black");
        attr(xml, "ParagraphStyleRange", 0, "ParagraphShadingSuppressPrinting", &suppress.to_string());
        let (before, after, written) = roundtrip("basic", &parts);
        for doc in [&before, &after] {
            let shading = &doc["stories"]["u118"]["paragraphs"][0]["attrs"]["shading"];
            if suppress {
                assert!(shading.is_null());
                assert!(doc["warnings"].as_array().unwrap().contains(&json!("paragraph shading set not to print: not drawn")));
            } else {
                assert!(shading.is_object());
                assert_eq!(shading["suppress_printing"], json!(false));
            }
        }
        assert_eq!(written[PART], parts[PART]);
    }
}
