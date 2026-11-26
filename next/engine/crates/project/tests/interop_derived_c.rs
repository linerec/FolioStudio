#[path = "support/interop.rs"]
mod support;
use folio_idml::ReaderData;
use serde_json::json;
use support::{attr, by_id, fixture, root, roundtrip};

#[test]
fn graphic_preview_location_comes_from_reader_data_and_is_not_an_idml_attribute() {
    let parts = fixture("objopts-clip");
    let (_, after, written) = roundtrip("objopts-clip", &parts);
    assert!(by_id(&after, "u130")["preview"].is_null());
    for name in ["preview-a.png", "preview-b.png"] {
        let path = root()
            .join("../artifacts/interop-c1-20261023/userdata")
            .join(name)
            .to_string_lossy()
            .into_owned();
        let data = ReaderData {
            previews: [("u130".into(), path.clone())].into(),
            ..Default::default()
        };
        let doc = folio_idml::read_parts(
            &parts,
            &root().join("oracle/fixtures/objopts-clip.idml"),
            &data,
        )
        .unwrap();
        assert_eq!(by_id(&json!(doc), "u130")["preview"], json!(path));
        assert!(!written.values().any(|s| s.contains(name)));
    }
}

#[test]
fn page_indexes_are_recomputed_from_document_and_master_page_order() {
    for name in ["17", "ix"] {
        let mut parts = fixture("basic");
        for (part, xml) in &mut parts {
            if part.starts_with("Spreads/") {
                attr(xml, "Page", 0, "Name", name);
            }
        }
        let (before, after, written) = roundtrip("basic", &parts);
        for doc in [&before, &after] {
            for (key, base) in [
                ("spreads", 0),
                ("master_spreads", folio_model::MASTER_PAGE_BASE),
            ] {
                let pages: Vec<_> = doc[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|s| s["pages"].as_array().unwrap())
                    .collect();
                assert!(!pages.is_empty());
                for (offset, page) in pages.iter().enumerate() {
                    assert_eq!(page["index"], json!(base + offset));
                }
            }
        }
        for (part, xml) in &parts {
            assert_eq!(&written[part], xml);
        }
    }
}

#[test]
fn used_font_summary_is_rebuilt_from_text_without_an_independent_stored_list() {
    const PART: &str = "Stories/Story_u118.xml";
    for family in ["Arial", "Calibri"] {
        let mut parts = fixture("basic");
        parts.insert(PART.into(), format!("<idPkg:Story xmlns:idPkg=\"http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging\"><Story Self=\"u118\"><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\"><CharacterStyleRange FontStyle=\"Regular\"><Properties><AppliedFont type=\"string\">{family}</AppliedFont></Properties><Content>summary</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"));
        let (before, after, written) = roundtrip("basic", &parts);
        for doc in [&before, &after] {
            assert!(
                doc["fonts_used"]
                    .as_array()
                    .unwrap()
                    .contains(&json!([family, "Regular"]))
            );
        }
        assert_eq!(before["fonts_used"], after["fonts_used"]);
        assert_eq!(written[PART], parts[PART]);
        assert!(!written.values().any(|s| s.contains("fonts_used")));
    }
}
