use folio_edit::{Command, Editor};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Read, path::Path};

const SCENARIOS: [&str; 8] = ["nested-words", "nested-sentences", "nested-delimiters", "nested-repeat", "nested-inherit", "nested-local-char", "nested-grep-overlap", "nested-reorder-clear"];

fn fixture() -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/nested-styles.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for n in 0..zip.len() {
        let mut entry = zip.by_index(n).unwrap();
        if entry.is_dir() { continue; }
        let name = entry.name().unwrap().into_owned();
        if name.ends_with(".xml") { let mut text = String::new(); entry.read_to_string(&mut text).unwrap(); parts.insert(name, text); }
    }
    Editor::new(parts)
}
fn set(style: &str, items: Value) -> Command { serde_json::from_value(json!({"op":"setNestedStyles","style":style,"items":items})).unwrap() }
fn item(style: &str, delimiter: &str, repetition: u32, inclusive: bool) -> Value { json!({"style":style,"delimiter":delimiter,"repetition":repetition,"inclusive":inclusive}) }

#[test]
fn invalid_lists_leave_no_history() {
    let mut e = fixture();
    let before = e.parts().clone();
    for items in [
        json!([item("Red", "Bogus", 1, true)]),
        json!([item("Missing", "AnyWord", 1, true)]),
        json!([item("Red", "AnyWord", 0, true)]),
        json!([item("Red", "Repeat", 1, true)]),
        json!([{"style":"Red","delimiter":"AnyWord","repetition":1,"inclusive":true,"source":9}]),
        json!([{"style":"Red","delimiter":"","custom":true,"repetition":1,"inclusive":true}]),
    ] {
        assert!(e.apply("invalid", &[set("ParagraphStyle/NS 0", items)]).is_err());
        assert_eq!(e.parts(), &before);
        assert_eq!(e.undo_label(), None);
    }
}

fn drop_cap_style(style: &str, character_style: &str) -> Command { serde_json::from_value(json!({"op":"setDropCapStyle","style":style,"character_style":character_style})).unwrap() }

#[test]
fn drop_cap_style_is_the_first_item_and_keeps_the_others() {
    let mut e = fixture();
    let own = |e: &Editor| folio_idml::nested::rules(&e.parts()["Resources/Styles.xml"], "ParagraphStyle/NS 0").unwrap();
    e.apply("list", &[set("ParagraphStyle/NS 0", json!([item("Red", "AnyWord", 1, true)]))]).unwrap();
    let before = e.parts().clone();
    e.apply("drop cap style", &[drop_cap_style("ParagraphStyle/NS 0", "Blue")]).unwrap();
    let items = own(&e);
    assert_eq!(items.len(), 2);
    assert_eq!((items[0].style.as_str(), items[0].delimiter.as_str(), items[0].repetition, items[0].inclusive), ("CharacterStyle/Blue", "Dropcap", 1, true));
    assert_eq!((items[1].style.as_str(), items[1].delimiter.as_str()), ("CharacterStyle/Red", "AnyWord"));
    // changing it keeps the one item, clearing it removes it
    e.apply("other", &[drop_cap_style("ParagraphStyle/NS 0", "Bold")]).unwrap();
    assert_eq!(own(&e).iter().filter(|i| i.delimiter == "Dropcap").count(), 1);
    assert_eq!(own(&e)[0].style, "CharacterStyle/Bold");
    e.apply("clear", &[drop_cap_style("ParagraphStyle/NS 0", "")]).unwrap();
    let items = own(&e);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].delimiter, "AnyWord");
    // clearing again changes nothing, an unknown style is refused
    assert!(e.apply("again", &[drop_cap_style("ParagraphStyle/NS 0", "")]).is_ok());
    let parts = e.parts().clone();
    assert!(e.apply("missing", &[drop_cap_style("ParagraphStyle/NS 0", "Missing")]).is_err());
    assert_eq!(e.parts(), &parts);
    // a Dropcap item anywhere but first is refused by setNestedStyles
    assert!(e.apply("late", &[set("ParagraphStyle/NS 0", json!([item("Red", "AnyWord", 1, true), item("Blue", "Dropcap", 1, true)]))]).is_err());
    let _ = before;
}

#[test]
fn reorder_keeps_unknown_record_data_and_undo_is_exact() {
    let mut e = fixture();
    e.apply("add", &[set("ParagraphStyle/NS 0", json!([item("Red", "AnyWord", 1, true), item("Blue", "Sentence", 2, false)]))]).unwrap();
    let mut parts = e.parts().clone();
    let xml = parts.get_mut("Resources/Styles.xml").unwrap();
    *xml = xml.replace("<AllNestedStyles type=\"list\">", "<AllNestedStyles type=\"list\" Future=\"yes\"><Unknown>retain</Unknown>")
        .replacen("<ListItem type=\"record\">", "<ListItem type=\"record\" FutureRecord=\"yes\"><UnknownRecord>retain</UnknownRecord>", 1)
        .replacen("<Repetition type=\"long\">", "<Repetition type=\"long\" FutureCount=\"yes\">", 1);
    let mut e = Editor::new(parts);
    let before = e.parts().clone();
    e.apply("reorder", &[set("ParagraphStyle/NS 0", json!([
        {"style":"Blue","delimiter":"Sentence","repetition":2,"inclusive":false,"source":1},
        {"style":"Red","delimiter":":","custom":true,"repetition":3,"inclusive":false,"source":0}]))]).unwrap();
    let xml = &e.parts()["Resources/Styles.xml"];
    for token in ["Future=\"yes\"", "FutureRecord=\"yes\"", "FutureCount=\"yes\"", "<Unknown>retain</Unknown>", "<UnknownRecord>retain</UnknownRecord>"] { assert!(xml.contains(token), "{token}"); }
    let items = folio_idml::nested::rules(xml, "ParagraphStyle/NS 0").unwrap();
    assert_eq!((items[0].style.as_str(), items[0].delimiter.as_str(), items[0].custom), ("CharacterStyle/Blue", "Sentence", false));
    assert_eq!((items[1].delimiter.as_str(), items[1].custom, items[1].repetition, items[1].inclusive), (":", true, 3, false));
    assert!(xml.contains("<Delimiter type=\"string\">:</Delimiter>"));
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
    e.redo().unwrap();
    assert_eq!(folio_idml::nested::rules(&e.parts()["Resources/Styles.xml"], "ParagraphStyle/NS 0").unwrap(), items);
}

#[test]
fn effective_characters_match_indesign_dom() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/nested-styles");
    let mut checked = 0;
    for name in SCENARIOS {
        let mut parts = fixture().parts().clone();
        let after = root.join(name).join("after");
        let mut todo = vec![after.clone()];
        while let Some(dir) = todo.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() { todo.push(p); continue; }
                let rel = p.strip_prefix(&after).unwrap().to_string_lossy().replace('\\', "/");
                parts.insert(rel, std::fs::read_to_string(p).unwrap());
            }
        }
        let doc = folio_idml::read_parts(&parts, Path::new("nested-styles.idml"), &Default::default()).unwrap();
        let dom: Value = serde_json::from_str(&std::fs::read_to_string(root.join(name).join("indesign.json")).unwrap()).unwrap();
        for frame in dom["extension"]["frames"].as_array().unwrap() {
            let label = frame["label"].as_str().unwrap();
            let Some(index) = label.strip_prefix("ns-") else { continue };
            let style = format!("NS {index}");
            let story = doc.stories.values().find(|s| s.paragraphs.first().is_some_and(|p| p.style == style || p.style == format!("ParagraphStyle/{style}")))
                .unwrap_or_else(|| panic!("missing {style}"));
            let actual: Vec<_> = story.paragraphs.iter().flat_map(|p| p.runs.iter()).flat_map(|r| r.text.chars().map(move |_| &r.attrs)).collect();
            let expected: Vec<_> = frame["characters"].as_array().unwrap().iter().filter(|v| v["text"] != "\r").collect();
            assert_eq!(actual.len(), expected.len(), "{name}/{label} length");
            for (position, (a, e)) in actual.iter().zip(expected).enumerate() {
                let fill = match &a.fill { folio_model::Paint::Swatch { id, .. } => id.strip_prefix("Color/").unwrap_or(id), _ => "None" };
                // On a nested style / GREP style overlap the DOM reports the nested style, the exported PDF prints the
                // GREP style (as for two GREP rules: rules/grep-styles.toml). The PDF is the truth.
                let expected_fill = if name == "nested-grep-overlap" && label == "ns-13" && (4..9).contains(&position) { "Nest Blue" } else { e["fill"].as_str().unwrap() };
                assert_eq!(fill, expected_fill, "{name}/{label}/{position} fill");
                assert_eq!(a.font_style, e["fontStyle"].as_str().unwrap(), "{name}/{label}/{position} font");
                let style = if a.character_style.is_empty() { "[None]" } else { &a.character_style };
                assert_eq!(style, e["style"].as_str().unwrap(), "{name}/{label}/{position} explicit style");
                checked += 1;
            }
        }
    }
    assert!(checked > 600, "{checked} characters compared");
}

fn lines(style: &str, items: Value) -> Command { serde_json::from_value(json!({"op":"setLineStyles","style":style,"items":items})).unwrap() }

#[test]
fn line_styles_are_written_read_back_and_undone() {
    let mut e = fixture();
    let own = |e: &Editor| folio_idml::nested::line_rules(&e.parts()["Resources/Styles.xml"], "ParagraphStyle/NS 0").unwrap();
    let before = e.parts().clone();
    let list = json!([{"style":"Red","lines":2},{"style":"Blue","lines":1},{"style":"$ID/[No character style]","lines":2,"repeat":true}]);
    e.apply("lines", &[lines("ParagraphStyle/NS 0", list)]).unwrap();
    let items = own(&e);
    assert_eq!(items.iter().map(|i| (i.style.as_str(), i.lines, i.repeat)).collect::<Vec<_>>(),
        vec![("CharacterStyle/Red", 2, false), ("CharacterStyle/Blue", 1, false), ("CharacterStyle/$ID/[No character style]", 2, true)]);
    // IDML: LineCount 2 / RepeatLast -1 for an item, LineCount = RepeatLast = the item count for a [Repeat]
    let xml = &e.parts()["Resources/Styles.xml"];
    assert!(xml.contains("<RepeatLast type=\"long\">-1</RepeatLast>") && xml.contains("<RepeatLast type=\"long\">2</RepeatLast>"));
    assert!(xml.contains("EmptyLineStyles=\"false\""));
    // the other list of the style is untouched, one undo gives the file back byte for byte
    assert!(folio_idml::nested::rules(xml, "ParagraphStyle/NS 0").unwrap().is_empty());
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
    e.redo().unwrap();
    assert_eq!(own(&e).len(), 3);
    // clearing the list keeps the style readable and sets the flag
    e.apply("clear", &[lines("ParagraphStyle/NS 0", json!([]))]).unwrap();
    assert!(own(&e).is_empty());
    assert!(e.parts()["Resources/Styles.xml"].contains("EmptyLineStyles=\"true\""));
}

#[test]
fn invalid_line_style_lists_leave_no_history() {
    let mut e = fixture();
    let before = e.parts().clone();
    for items in [
        json!([{"style":"Missing","lines":1}]),
        json!([{"style":"Red","lines":10000}]),
        // a [Repeat] of more items than precede it, or of none
        json!([{"style":"Red","lines":1},{"style":"Red","lines":2,"repeat":true}]),
        json!([{"style":"Red","lines":0,"repeat":true}]),
        json!([{"style":"Red","lines":1,"source":9}]),
    ] {
        assert!(e.apply("invalid", &[lines("ParagraphStyle/NS 0", items)]).is_err());
        assert_eq!(e.parts(), &before);
        assert_eq!(e.undo_label(), None);
    }
}
