//! Behavior of story formatting and its internal clipboard.

use super::*;
use crate::{Command, Editor};
fn editor() -> Editor {
    Editor::new(BTreeMap::from([
        ("Resources/Styles.xml".into(),r#"<Styles><ParagraphStyle Self="ParagraphStyle/Body" Name="Body" PointSize="12" FontStyle="Regular" LeftIndent="0"><Properties><AppliedFont type="string">Arial</AppliedFont></Properties></ParagraphStyle><CharacterStyle Self="CharacterStyle/None" Name="None"/><CharacterStyle Self="CharacterStyle/Bold" Name="Bold" FontStyle="Bold"/></Styles>"#.into()),
        ("Stories/Story_s.xml".into(),r#"<Story Self="s"><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Body"><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/None"><Content>AB</Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/Bold" PointSize="14"><Content>CD</Content><Br/><Content>EF</Content></CharacterStyleRange></ParagraphStyleRange></Story>"#.into()),
    ]))
}
fn command(start: usize, end: usize, props: Value) -> Command {
    Command::TextFormat {
        story: "s".into(),
        start,
        end,
        props: serde_json::from_value(props).unwrap(),
    }
}
#[test]
fn stroke_weight_bounds_atomicity_and_undo() {
    let mut e = editor();
    let before = e.parts().clone();
    assert_eq!(e.text_format("s", 0, 1).unwrap()["strokeWeight"], 1.0);
    for bad in [-1.0, 1000.01] {
        assert!(e.apply("bad weight", &[command(0, 2, serde_json::json!({"pointSize":20,"strokeWeight":bad}))]).is_err());
        assert_eq!(e.parts(), &before);
    }
    for weight in [0.0, 1.5, 1000.0] {
        e.apply("weight", &[command(0, 2, serde_json::json!({"strokeWeight":weight}))]).unwrap();
        assert_eq!(e.text_format("s", 0, 2).unwrap()["strokeWeight"], weight);
        assert_eq!(e.text_format("s", 2, 3).unwrap()["strokeWeight"], 1.0);
        assert!(e.text_format("s", 0, 3).unwrap()["strokeWeight"].is_null());
        e.undo().unwrap();
        assert_eq!(e.parts(), &before);
    }
}
#[test]
fn parjust_start_position_validation_mixed_and_undo() {
    let mut e = editor();
    let before = e.parts().clone();
    assert!(e.apply("invalid", &[command(0, 1, serde_json::json!({"startParagraph":"MISSING"}))]).is_err());
    assert_eq!(e.parts(), &before);
    e.apply("start", &[command(0, 1, serde_json::json!({"startParagraph":"NEXT_COLUMN"}))]).unwrap();
    assert_eq!(e.text_format("s", 2, 3).unwrap()["startParagraph"], "NEXT_COLUMN");
    assert!(e.text_format("s", 0, 7).unwrap()["startParagraph"].is_null());
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}
#[test]
fn parjust_hyphen_limits_mixed_and_undo() {
    let mut e = editor();
    let before = e.parts().clone();
    for props in [serde_json::json!({"hyphenateAfterFirst":0}), serde_json::json!({"hyphenateLadderLimit":1.5}), serde_json::json!({"hyphenation":"true"})] {
        assert!(e.apply("invalid", &[command(0, 1, props)]).is_err());
        assert_eq!(e.parts(), &before);
    }
    e.apply("hyphen", &[command(0, 1, serde_json::json!({"hyphenation":true,"hyphenateLastWord":false,"hyphenateAfterFirst":3}))]).unwrap();
    assert_eq!(e.text_format("s", 2, 3).unwrap()["hyphenateLastWord"], false);
    assert!(e.text_format("s", 0, 7).unwrap()["hyphenateLastWord"].is_null());
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}
#[test]
fn parjust_range_validation_mixed_selection_and_undo() {
    let mut e = editor();
    let before = e.parts().clone();
    assert!(e.apply("bad", &[command(0, 1, serde_json::json!({"minimumWordSpacing":101}))]).is_err());
    assert_eq!(e.parts(), &before);
    e.apply("spacing", &[command(0, 1, serde_json::json!({"minimumWordSpacing":70,"desiredWordSpacing":110,"maximumWordSpacing":150}))]).unwrap();
    assert_eq!(e.text_format("s", 2, 3).unwrap()["desiredWordSpacing"], 110.0);
    assert!(e.text_format("s", 0, 7).unwrap()["desiredWordSpacing"].is_null());
    assert!(!e.part("Stories/Story_s.xml").unwrap().contains("CharacterStyleRange DesiredWordSpacing"));
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}
#[test]
fn mixed_readout_and_pending_format_do_not_change_document() {
    let mut e = editor();
    let before = e.parts().clone();
    let f = e.text_format("s", 0, 4).unwrap();
    assert!(f["pointSize"].is_null());
    assert!(f["characterStyle"].is_null());
    assert_eq!(f["appliedFont"], "Arial");
    e.apply(
        "cursor format",
        &[command(
            1,
            1,
            serde_json::json!({"pointSize":20,"underline":true}),
        )],
    )
    .unwrap();
    assert_eq!(e.parts(), &before);
    assert_eq!(e.rev(), 0);
    assert_eq!(e.text_format("s", 1, 1).unwrap()["pointSize"], 20.0);
    e.apply(
        "type",
        &[Command::InsertText {
            story: "s".into(),
            at: 1,
            text: "x".into(),
        }],
    )
    .unwrap();
    assert_eq!(e.text_format("s", 1, 2).unwrap()["pointSize"], 20.0);
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
    e.reset_insertion_style("s");
    assert_eq!(e.text_format("s", 1, 1).unwrap()["pointSize"], 12.0);
}
#[test]
fn opentype_set_button_preserves_mixed_bits_and_undo() {
    let mut e = editor();
    e.apply("sets", &[
        command(0, 2, serde_json::json!({"otfStylisticSets": 10})),
        command(2, 4, serde_json::json!({"otfStylisticSets": 18})),
    ]).unwrap();
    let before = e.parts().clone();
    let f = e.text_format("s", 0, 4).unwrap();
    assert!(f["otfStylisticSets"].is_null());
    assert_eq!(f["otfStylisticSet1"], true);
    assert_eq!(f["otfStylisticSet2"], false);
    assert!(f["otfStylisticSet3"].is_null());
    e.apply("set 2", &[command(0, 4, serde_json::json!({"otfStylisticSet2":true}))]).unwrap();
    assert_eq!(e.text_format("s", 0, 2).unwrap()["otfStylisticSets"], 14);
    assert_eq!(e.text_format("s", 2, 4).unwrap()["otfStylisticSets"], 22);
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
    assert!(e.apply("invalid mask", &[command(0, 4, serde_json::json!({"otfStylisticSets":2097152}))]).is_err());
    assert_eq!(e.parts(), &before);
}
#[test]
fn invalid_format_rolls_back_bytes_and_pending_style() {
    let mut e = editor();
    let before = e.parts().clone();
    let bad = command(0, 2, serde_json::json!({"madeUp":true}));
    assert!(
        e.apply(
            "invalid",
            &[
                command(1, 1, serde_json::json!({"pointSize":20})),
                command(0, 2, serde_json::json!({"pointSize":24})),
                bad
            ]
        )
        .is_err()
    );
    assert_eq!(e.parts(), &before);
    assert_eq!(e.text_format("s", 1, 1).unwrap()["pointSize"], 12.0);
    assert!(
        e.apply(
            "invalid size",
            &[command(0, 2, serde_json::json!({"pointSize":-1}))]
        )
        .is_err()
    );
    assert!(e.copy_text("s", 0, 99).is_err());
    assert_eq!(
        e.apply(
            "optical",
            &[command(
                0,
                2,
                serde_json::json!({"kerningMethod":"Optical"})
            )]
        )
        .unwrap_err()
        .code,
        "unsupported"
    );
}
#[test]
fn clipboard_snapshot_survives_cut_and_undo() {
    let mut e = editor();
    let before = e.parts().clone();
    let fragment = e.copy_text("s", 2, 4).unwrap();
    e.apply(
        "cut",
        &[Command::DeleteText {
            story: "s".into(),
            start: 2,
            end: 4,
        }],
    )
    .unwrap();
    e.apply(
        "paste",
        &[Command::PasteText {
            story: "s".into(),
            at: 0,
            fragment,
        }],
    )
    .unwrap();
    assert_eq!(e.text_format("s", 0, 2).unwrap()["fontStyle"], "Bold");
    assert_eq!(e.text_format("s", 0, 2).unwrap()["pointSize"], 14.0);
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}
#[test]
fn clipboard_refuses_opaque_inline_objects_without_mutation() {
    let mut e = editor();
    let t=e.parts["Stories/Story_s.xml"].replace("<Content>AB</Content>","<Content>A</Content><Table Self=\"table\"><Cell><Content>cell</Content></Cell></Table>");
    e.parts.insert("Stories/Story_s.xml".into(), t);
    let before = e.parts().clone();
    assert_eq!(e.copy_text("s", 0, 2).unwrap_err().code, "unsupported");
    assert_eq!(e.parts(), &before);
}
#[test]
fn clipboard_refuses_unmeasured_inherited_properties_atomically() {
    let mut e = editor();
    let styles = e.parts["Resources/Styles.xml"].replace(
        "</Styles>",
        "<ParagraphStyle Self=\"ParagraphStyle/Red\" Name=\"Red\" PointSize=\"12\" FillColor=\"Color/Red\"/></Styles>",
    );
    e.parts.insert("Resources/Styles.xml".into(), styles);
    let target = e.parts["Stories/Story_s.xml"]
        .replace("Self=\"s\"", "Self=\"target\"")
        .replace("ParagraphStyle/Body", "ParagraphStyle/Red");
    e.parts.insert("Stories/Story_target.xml".into(), target);
    let fragment = e.copy_text("s", 0, 2).unwrap();
    let before = e.parts().clone();
    assert_eq!(
        e.apply(
            "replace with unsupported inheritance",
            &[
                Command::DeleteText {
                    story: "target".into(),
                    start: 0,
                    end: 1
                },
                Command::PasteText {
                    story: "target".into(),
                    at: 0,
                    fragment
                },
            ]
        )
        .unwrap_err()
        .code,
        "unsupported",
    );
    assert_eq!(e.parts(), &before);
    assert_eq!(e.rev(), 0);
}
#[test]
fn clearing_styles_does_not_drop_condition_membership() {
    let mut e = editor();
    let story = e.parts["Stories/Story_s.xml"].replace(
        "PointSize=\"14\"",
        "PointSize=\"14\" AppliedConditions=\"Condition/review\"",
    );
    e.parts.insert("Stories/Story_s.xml".into(), story);
    let before = e.parts().clone();
    // A caret clear affects the whole paragraph, including the later range.
    assert_eq!(
        e.apply(
            "clear style",
            &[Command::CharacterStyle {
                story: "s".into(),
                start: 1,
                end: 1,
                style: "CharacterStyle/Bold".into(),
                clear: true,
            }]
        )
        .unwrap_err()
        .code,
        "unsupported"
    );
    assert_eq!(e.parts(), &before);
    assert_eq!(e.rev(), 0);
}
