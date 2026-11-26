#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, check, fixture, roundtrip};

#[test]
fn layer_attribute_mutations_survive_idml_export() {
    let original = fixture("basic");
    for (key, field, values) in [
        ("Self", "id", ["layer-interop-a", "layer-interop-b"]),
        ("Name", "name", ["Interop layer A", "한글 레이어 B"]),
        ("Visible", "visible", ["false", "true"]),
        ("Printable", "printable", ["false", "true"]),
        ("Locked", "locked", ["true", "false"]),
        ("ShowGuides", "show_guides", ["false", "true"]),
        ("LockGuides", "lock_guides", ["true", "false"]),
        ("LayerColor", "color", ["Cyan", "17 83 211"]),
    ] {
        for value in values {
            let mut parts = original.clone();
            attr(parts.get_mut("designmap.xml").unwrap(), "Layer", 0, key, value);
            let (before, after, written) = roundtrip("basic", &parts);
            let expected = match key {
                "Self" | "Name" => json!(value),
                "LayerColor" if value == "Cyan" => json!({"Named": "Cyan"}),
                "LayerColor" => json!({"Rgb": [17.0, 83.0, 211.0]}),
                _ => json!(value == "true"),
            };
            check(&before, &after, &format!("/layers/0/{field}"), expected);
            assert!(written["designmap.xml"].contains(&format!("{key}=\"{value}\"")));
            assert_eq!(before["layers"], after["layers"]);
        }
    }
}

#[test]
fn layer_order_is_document_order_after_export() {
    let mut parts = fixture("basic");
    let xml = parts.get_mut("designmap.xml").unwrap();
    let start = xml.find("<Layer ").unwrap();
    let end = start + xml[start..].find("</Layer>").unwrap() + "</Layer>".len();
    let first = xml[start..end].to_owned();
    xml.replace_range(start..end, "");
    let at = xml.rfind("</Layer>").unwrap() + "</Layer>".len();
    xml.insert_str(at, &first);
    let (before, after, _) = roundtrip("basic", &parts);
    for (index, id) in ["ue8", "ued", "ub8"].iter().enumerate() {
        check(&before, &after, &format!("/layers/{index}/id"), json!(id));
        check(&before, &after, &format!("/layers/{index}/order"), json!(index));
    }
}

#[test]
fn toc_attribute_mutations_survive_idml_export() {
    let mut original = fixture("toc");
    original.insert("Resources/Styles.xml".into(), std::fs::read_to_string(
        support::root().join("oracle/goldens/toc/toc-create/after/Resources/Styles.xml")).unwrap());
    for (tag, index, key, pointer, value, expected) in [
        ("TOCStyle", 1, "Self", "id", "TOCStyle/Interop", json!("TOCStyle/Interop")),
        ("TOCStyle", 1, "Name", "name", "상호운용 목차", json!("상호운용 목차")),
        ("TOCStyle", 1, "Title", "title", "다른 목차 제목", json!("다른 목차 제목")),
        ("TOCStyle", 1, "TitleStyle", "titleStyle", "ParagraphStyle/H1", json!("ParagraphStyle/H1")),
        ("TOCStyle", 1, "IncludeHidden", "includeHidden", "true", json!(true)),
        ("TOCStyle", 1, "IncludeBookDocuments", "includeBookDocuments", "true", json!(true)),
        ("TOCStyle", 1, "CreateBookmarks", "createBookmarks", "false", json!(false)),
        ("TOCStyle", 1, "MakeAnchor", "makeAnchor", "true", json!(true)),
        ("TOCStyle", 1, "RunIn", "runIn", "true", json!(true)),
        ("TOCStyle", 1, "RemoveForcedLineBreak", "removeForcedLineBreak", "true", json!(true)),
        ("TOCStyle", 1, "NumberedParagraphs", "numberedParagraphs", "IncludeNumbers", json!("IncludeNumbers")),
        ("TOCStyleEntry", 0, "Name", "entries/0/name", "H2", json!("H2")),
        ("TOCStyleEntry", 0, "FormatStyle", "entries/0/formatStyle", "ParagraphStyle/TOC Title", json!("ParagraphStyle/TOC Title")),
        ("TOCStyleEntry", 0, "Level", "entries/0/level", "3", json!(3)),
        ("TOCStyleEntry", 0, "PageNumberPosition", "entries/0/pageNumberPosition", "BeforeEntry", json!("BeforeEntry")),
        ("TOCStyleEntry", 0, "PageNumberStyle", "entries/0/pageNumberStyle", "CharacterStyle/Interop", json!("CharacterStyle/Interop")),
        ("TOCStyleEntry", 0, "Separator", "entries/0/separator", " — ", json!(" — ")),
        ("TOCStyleEntry", 0, "SeparatorStyle", "entries/0/separatorStyle", "CharacterStyle/Interop2", json!("CharacterStyle/Interop2")),
        ("TOCStyleEntry", 0, "SortAlphabet", "entries/0/sortAlphabet", "true", json!(true)),
    ] {
        let mut parts = original.clone();
        attr(parts.get_mut("Resources/Styles.xml").unwrap(), tag, index, key, value);
        let (before, after, written) = roundtrip("toc", &parts);
        check(&before, &after, &format!("/toc_styles/1/{pointer}"), expected);
        assert_eq!(before["toc_styles"], after["toc_styles"]);
        assert!(written["Resources/Styles.xml"].contains(&format!("{key}=\"{value}\"")));
    }
}
