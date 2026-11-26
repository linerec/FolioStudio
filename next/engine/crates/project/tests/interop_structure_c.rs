#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, by_id, check, fixture, roundtrip};

#[test]
fn frame_story_and_thread_references_preserve_empty_and_named_targets() {
    let part = "Spreads/Spread_u10c.xml";
    for (key, field) in [("ParentStory", "story"), ("PreviousTextFrame", "previous"), ("NextTextFrame", "next")] {
        for value in ["n", "interop-reference"] {
            let mut parts = fixture("basic");
            attr(parts.get_mut(part).unwrap(), "TextFrame", 0, key, value);
            let xml = &parts[part];
            let at = xml.find("<TextFrame Self=\"").unwrap() + "<TextFrame Self=\"".len();
            let id = &xml[at..at + xml[at..].find('"').unwrap()];
            let (before, after, written) = roundtrip("basic", &parts);
            let expected = if field != "story" && value == "n" { json!(null) } else { json!(value) };
            check(by_id(&before, id), by_id(&after, id), &format!("/kind/TextFrame/{field}"), expected);
            assert_eq!(written[part], parts[part]);
        }
    }
}

#[test]
fn chapter_number_preserves_stored_text() {
    for number in ["3", "17"] {
        let mut parts = fixture("basic");
        attr(parts.get_mut("Resources/Preferences.xml").unwrap(), "ChapterNumberPreference", 0, "ChapterNumber", number);
        let (before, after, written) = roundtrip("basic", &parts);
        check(&before, &after, "/chapter_number", json!(number));
        assert_eq!(written["Resources/Preferences.xml"], parts["Resources/Preferences.xml"]);
    }
}

#[test]
fn plain_unicode_text_and_note_references_survive_export() {
    let part = "Stories/Story_u118.xml";
    for (id, text) in [("endnote-a", "한글 A😀"), ("endnote-b", "다른 Bé")] {
        let mut parts = fixture("basic");
        parts.insert(part.into(), format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><idPkg:Story xmlns:idPkg=\"http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging\"><Story Self=\"u118\"><ParagraphStyleRange><CharacterStyleRange><Content>{text}</Content><Footnote><ParagraphStyleRange><CharacterStyleRange><Content>각주 {text}</Content></CharacterStyleRange></ParagraphStyleRange></Footnote><Endnote Self=\"{id}\"/></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"));
        let (before, after, written) = roundtrip("basic", &parts);
        let runs = &before["stories"]["u118"]["paragraphs"][0]["runs"];
        assert_eq!(runs[0]["text"], json!(text));
        let foot = runs.as_array().unwrap().iter().find(|r| r["variable"] == "FootnoteNumber").unwrap();
        assert_eq!(foot["note"]["paragraphs"][0]["runs"][0]["text"], json!(format!("각주 {text}")));
        let end = runs.as_array().unwrap().iter().find(|r| r["variable"] == "EndnoteNumber").unwrap();
        assert_eq!(end["note"]["id"], json!(id));
        assert_eq!(before["stories"]["u118"]["paragraphs"], after["stories"]["u118"]["paragraphs"]);
        assert_eq!(written[part], parts[part]);
    }
}

#[test]
fn frame_shape_tag_and_optional_content_survive_export() {
    let part = "Spreads/Spread_u10f.xml";
    for tag in ["Rectangle", "Oval", "Polygon"] {
        for content in [true, false] {
            let mut parts = fixture("objopts-clip");
            let xml = parts.get_mut(part).unwrap();
            let image = xml.find("<Image Self=\"u130\"").unwrap();
            let start = xml[..image].rfind("<Rectangle ").unwrap();
            let end = image + xml[image..].find("</Rectangle>").unwrap() + "</Rectangle>".len();
            let mut frame = xml[start..end].to_owned();
            if !content {
                let a = frame.find("<Image ").unwrap();
                let b = a + frame[a..].find("</Image>").unwrap() + "</Image>".len();
                frame.replace_range(a..b, "");
            }
            frame = frame.replacen("<Rectangle ", &format!("<{tag} "), 1).replace("</Rectangle>", &format!("</{tag}>"));
            let id_at = frame.find("Self=\"").unwrap() + 6;
            let id = frame[id_at..id_at + frame[id_at..].find('"').unwrap()].to_owned();
            xml.replace_range(start..end, &frame);
            let (before, after, written) = roundtrip("objopts-clip", &parts);
            let b = &by_id(&before, &id)["kind"];
            assert!(b.get(tag).is_some());
            assert_eq!(!b[tag]["content"].is_null(), content);
            if content { assert_eq!(b[tag]["content"]["id"], "u130"); }
            assert_eq!(b, &by_id(&after, &id)["kind"]);
            assert_eq!(written[part], parts[part]);
        }
    }
}
