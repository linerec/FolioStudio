use std::collections::BTreeMap;

use folio_edit::{Command, Editor, commands::RegisteredCommand};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    source: String,
    input: Input,
    output: Value,
}

#[derive(Deserialize)]
struct Input {
    ids: Vec<String>,
    props: BTreeMap<String, Value>,
}

fn sample_document() -> BTreeMap<String, String> {
    let spread = concat!(
        r#"<Spread Self="page">"#,
        r#"<TextFrame Self="first" ParentStory="wide" NextTextFrame="last" FillColor="Color/Leaf"/>"#,
        r#"<TextFrame Self="last" ParentStory="wide" PreviousTextFrame="first"/>"#,
        r#"<TextFrame Self="empty" ParentStory="blank"/>"#,
        r#"<Rectangle Self="shape" FillColor="Color/Leaf"/>"#,
        r#"<GraphicLine Self="line"><TextPath Self="words" ParentStory="narrow"/></GraphicLine>"#,
        "</Spread>"
    );
    let graphics = concat!(
        "<Graphic>",
        r#"<Color Self="Color/Leaf" Name="Leaf" Model="Process" Space="RGB" ColorValue="20 180 70"/>"#,
        r#"<Color Self="Color/Berry" Name="Berry" Model="Process" Space="RGB" ColorValue="222 30 70"/>"#,
        "</Graphic>"
    );
    let mut parts = BTreeMap::from([
        ("Spreads/Spread_page.xml".into(), spread.into()),
        ("Resources/Graphic.xml".into(), graphics.into()),
    ]);
    for (id, content) in [
        ("wide", "<Content>Top</Content><Br/><Content>Bottom</Content>"),
        ("narrow", "<Content>Line</Content>"),
        ("blank", ""),
    ] {
        parts.insert(format!("Stories/Story_{id}.xml"), format!(
            r#"<Story Self="{id}"><ParagraphStyleRange><CharacterStyleRange>{content}</CharacterStyleRange></ParagraphStyleRange></Story>"#
        ));
    }
    parts
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn selected_frames_match_character_paint_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn selected_frames_match_character_paint_vectors() {
    let vectors: Vectors = serde_json::from_str(include_str!(
        "../../../../../cleanroom/vectors/B3/frame-text.json"
    )).unwrap();
    assert!(!vectors.cases.is_empty());
    let original = sample_document();
    for Case { id, source, input, output } in vectors.cases {
        assert!(matches!(source.as_str(), "measured" | "oracle" | "old-impl"), "{id}");
        let mut editor = Editor::new(original.clone());
        let command = Command::Registered(RegisteredCommand::FormatFrameText {
            ids: input.ids, props: input.props,
        });
        let error = editor.apply(&id, &[command]).err().map(|error| error.code);
        let changed: Vec<_> = original.keys().filter(|part| editor.parts().get(*part) != original.get(*part)).collect();
        let paint: Vec<_> = [("wide", 0, 3), ("wide", 4, 10), ("narrow", 0, 4)].into_iter().map(|(story, start, end)| {
            let format = editor.text_format(story, start, end).unwrap();
            json!({"fillColor": format["fillColor"], "strokeColor": format["strokeColor"]})
        }).collect();
        let changed_parts = json!(changed);
        if !changed.is_empty() {
            editor.undo().unwrap();
        }
        let actual = json!({"changedParts": changed_parts, "error": error, "paint": paint, "restored": editor.parts() == &original});
        assert_eq!(actual, output, "{id}");
    }
}
