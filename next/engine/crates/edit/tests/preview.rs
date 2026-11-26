//! Preview session (preview.rs): shown provisionally, taken back exactly, committed as one undo step,
//! never in the history, never "unsaved".
use folio_edit::{Command, Editor, commands::RegisteredCommand, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, BTreeMap<String, String>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let (mut parts, mut labels) = (BTreeMap::new(), BTreeMap::new());
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        if name.starts_with("Spreads/") {
            let ix = Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") {
                    let owner = ix.ancestors(i).find(|&a| ix.attr(&t, a, "Self").is_some()).unwrap();
                    labels.insert(ix.value(&t, i, "Value").unwrap(), ix.attr(&t, owner, "Self").unwrap().into());
                }
            }
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), labels)
}

fn weight(l: &BTreeMap<String, String>, w: f64) -> Command {
    Command::SetPaint { ids: vec![l["r-plain"].clone()], fill: None, fill_tint: None, stroke: None, stroke_tint: None, stroke_weight: Some(w) }
}
fn moved(l: &BTreeMap<String, String>, dx: f64) -> Command {
    Command::Move { ids: vec![l["r-plain"].clone()], by: [dx, 0.0] }
}
fn reg(c: RegisteredCommand) -> Vec<Command> {
    vec![Command::Registered(c)]
}
fn show(l: &BTreeMap<String, String>, w: f64) -> Vec<Command> {
    reg(RegisteredCommand::PreviewApply { commands: vec![weight(l, w)] })
}

#[test]
fn cancel_gives_back_every_byte_and_leaves_no_history() {
    let (mut e, l) = fixture();
    e.apply("first", &[moved(&l, 5.0)]).unwrap();
    let (parts, version, undo, redo, touched) = (e.parts().clone(), e.version(), e.undo_label().map(str::to_owned), e.redo_label().map(str::to_owned), e.touched().clone());
    let rev = e.rev();
    e.apply("", &reg(RegisteredCommand::PreviewStart { label: "획 두께".into() })).unwrap();
    let o = e.apply("", &show(&l, 7.0)).unwrap();
    assert!(!o.parts.is_empty(), "the preview changed something");
    assert_ne!(e.parts(), &parts, "the document shows the preview");
    assert!(e.rev() > rev, "a shown preview is never mistaken for an older document");
    assert_eq!(e.version(), version, "a preview is not an unsaved change");
    assert_eq!(e.undo_label().map(str::to_owned), undo);
    assert_eq!(e.redo_label().map(str::to_owned), redo);
    // a second value replaces the first: base + the last apply
    e.apply("", &show(&l, 12.0)).unwrap();
    let twelve = e.parts().clone();
    e.apply("", &show(&l, 3.0)).unwrap();
    assert_ne!(e.parts(), &twelve);
    let c = e.apply("", &reg(RegisteredCommand::PreviewCancel {})).unwrap();
    assert!(!c.parts.is_empty(), "the parts that went back are reported");
    assert_eq!(e.parts(), &parts, "cancel restores the exact bytes");
    assert_eq!((e.version(), e.undo_label().map(str::to_owned), e.redo_label().map(str::to_owned)), (version, undo, redo));
    assert_eq!(e.touched(), &touched);
    assert!(e.preview_label().is_none());
    assert!(e.rev() > rev);
}

#[test]
fn commit_is_one_undo_step_named_by_the_session() {
    let (mut e, l) = fixture();
    let base = e.parts().clone();
    e.apply("", &reg(RegisteredCommand::PreviewStart { label: "획 두께".into() })).unwrap();
    e.apply("", &show(&l, 4.0)).unwrap();
    e.apply("", &show(&l, 6.0)).unwrap();
    let previewed = e.parts().clone();
    let o = e.apply("", &reg(RegisteredCommand::PreviewCommit {})).unwrap();
    assert!(!o.parts.is_empty());
    assert_eq!(e.parts(), &previewed, "the committed document is what the last preview showed");
    assert_eq!(e.undo_label(), Some("획 두께"));
    assert_eq!(e.version(), 1);
    assert_eq!(e.history().0, vec!["획 두께"], "one step, not one per preview");
    e.undo().unwrap();
    assert_eq!(e.parts(), &base);
    assert!(e.redo().is_some());
    assert_eq!(e.parts(), &previewed);
    // the same edit without a preview writes the same bytes
    let (mut d, ld) = fixture();
    d.apply("획 두께", &[weight(&ld, 6.0)]).unwrap();
    assert_eq!(d.parts(), &previewed);
}

#[test]
fn commit_of_nothing_and_cancel_of_nothing_are_quiet() {
    let (mut e, l) = fixture();
    let base = e.parts().clone();
    assert!(e.apply("", &reg(RegisteredCommand::PreviewCommit {})).unwrap().parts.is_empty());
    assert!(e.apply("", &reg(RegisteredCommand::PreviewCancel {})).unwrap().parts.is_empty());
    e.apply("x", &show(&l, 5.0)).unwrap();
    // show it, then show the value the document already had: the session shows nothing, commit does nothing
    let w = Command::SetPaint { ids: vec![l["r-plain"].clone()], fill: None, fill_tint: None, stroke: None, stroke_tint: None, stroke_weight: None };
    e.apply("", &reg(RegisteredCommand::PreviewApply { commands: vec![w] })).unwrap();
    assert_eq!(e.parts(), &base);
    assert!(e.apply("", &reg(RegisteredCommand::PreviewCommit {})).unwrap().parts.is_empty());
    assert_eq!(e.undo_label(), None);
}

#[test]
fn another_edit_takes_an_open_preview_back_first() {
    let (mut e, l) = fixture();
    let base = e.parts().clone();
    e.apply("x", &show(&l, 9.0)).unwrap();
    assert_eq!(e.preview_label(), Some("x"), "a session opens by itself, named by the request");
    let o = e.apply("이동", &[moved(&l, 5.0)]).unwrap();
    assert!(e.preview_label().is_none());
    assert_eq!(e.undo_label(), Some("이동"));
    assert_eq!(e.history().0, vec!["이동"]);
    assert!(!o.parts.is_empty());
    e.undo().unwrap();
    assert_eq!(e.parts(), &base, "the preview left nothing behind");
    // undo and redo settle too
    e.apply("", &show(&l, 9.0)).unwrap();
    assert!(e.redo().is_some());
    assert!(e.preview_label().is_none());
}

#[test]
fn a_failing_apply_leaves_what_was_shown() {
    let (mut e, l) = fixture();
    e.apply("", &show(&l, 8.0)).unwrap();
    let shown = e.parts().clone();
    let bad = Command::SetPaint { ids: vec!["nope".into()], fill: None, fill_tint: None, stroke: None, stroke_tint: None, stroke_weight: Some(3.0) };
    assert!(e.apply("", &reg(RegisteredCommand::PreviewApply { commands: vec![bad] })).is_err());
    assert_eq!(e.parts(), &shown);
    e.apply("", &reg(RegisteredCommand::PreviewCancel {})).unwrap();
    assert_ne!(e.parts(), &shown);
}

#[test]
fn preview_commands_are_not_mixed_with_others() {
    let (mut e, l) = fixture();
    let mixed = vec![moved(&l, 1.0), Command::Registered(RegisteredCommand::PreviewCancel {})];
    assert_eq!(e.apply("x", &mixed).unwrap_err().code, "invalid");
    assert_eq!(e.version(), 0);
}

#[test]
fn wire_names_are_registered() {
    for op in ["previewStart", "previewApply", "previewCommit", "previewCancel"] {
        assert!(RegisteredCommand::recognizes(op), "{op}");
    }
    let c: Command = serde_json::from_str(r#"{"op":"previewApply","commands":[{"op":"move","ids":["u1"],"by":[1,0]}]}"#).unwrap();
    assert!(matches!(c, Command::Registered(RegisteredCommand::PreviewApply { ref commands }) if commands.len() == 1));
}

// --- text formatting (the character and paragraph panels' number fields): the same session, a story's byte splices ---

fn format(start: usize, end: usize, props: serde_json::Value) -> Command {
    Command::TextFormat { story: "u12e".into(), start, end, props: serde_json::from_value(props).unwrap() }
}
fn story(e: &Editor) -> String {
    e.parts()["Stories/Story_u12e.xml"].clone()
}
fn insert(at: usize) -> Command {
    Command::InsertText { story: "u12e".into(), at, text: "Z".into() }
}
fn preview(e: &mut Editor, label: &str, commands: Vec<Command>) {
    e.apply(label, &reg(RegisteredCommand::PreviewApply { commands })).unwrap();
}

#[test]
fn text_format_preview_cancel_gives_back_every_byte() {
    let (mut e, _) = fixture();
    let (parts, version, touched) = (e.parts().clone(), e.version(), e.touched().clone());
    // character and paragraph numbers, a leading that is "auto", a second value replacing the first
    preview(&mut e, "글 서식", vec![format(2, 12, serde_json::json!({"pointSize": 14}))]);
    assert_ne!(story(&e), parts["Stories/Story_u12e.xml"], "the story shows the preview");
    let fourteen = story(&e);
    preview(&mut e, "글 서식", vec![format(2, 12, serde_json::json!({"pointSize": 18, "tracking": 40, "leading": "auto", "leftIndent": 12, "spaceBefore": 6}))]);
    assert_ne!(story(&e), fourteen);
    assert_eq!((e.version(), e.undo_label()), (version, None), "a preview is not an undo step or an unsaved change");
    let c = e.apply("", &reg(RegisteredCommand::PreviewCancel {})).unwrap();
    assert!(c.parts.contains(&"Stories/Story_u12e.xml".to_owned()));
    assert_eq!(e.parts(), &parts, "cancel restores the exact bytes");
    assert_eq!((e.version(), e.undo_label().map(str::to_owned)), (version, None));
    assert_eq!(e.touched(), &touched);
    // typing afterwards writes what it would have in a document that never previewed
    let (mut fresh, _) = fixture();
    e.apply("t", &[insert(5)]).unwrap();
    fresh.apply("t", &[insert(5)]).unwrap();
    assert_eq!(e.parts(), fresh.parts());
}

#[test]
fn text_format_commit_is_one_undo_step_and_the_same_bytes_as_the_edit_without_preview() {
    let props = serde_json::json!({"pointSize": 16.5, "tracking": -20, "baselineShift": 2, "leftIndent": 9, "firstLineIndent": 3, "spaceAfter": 4});
    let (mut e, _) = fixture();
    let base = e.parts().clone();
    e.apply("", &reg(RegisteredCommand::PreviewStart { label: "글 서식".into() })).unwrap();
    preview(&mut e, "", vec![format(0, 30, serde_json::json!({"pointSize": 12}))]);
    preview(&mut e, "", vec![format(0, 30, props.clone())]);
    let shown = e.parts().clone();
    let o = e.apply("", &reg(RegisteredCommand::PreviewCommit {})).unwrap();
    assert!(o.parts.contains(&"Stories/Story_u12e.xml".to_owned()));
    assert_eq!(e.parts(), &shown, "the committed document is what the last preview showed");
    assert_eq!(e.history().0, vec!["글 서식"], "one step, not one per value");
    assert_eq!(e.version(), 1);
    // the same edit without a preview: the same bytes, and the same insertion style afterwards
    let (mut d, _) = fixture();
    d.apply("글 서식", &[format(0, 30, props)]).unwrap();
    assert_eq!(d.parts(), e.parts());
    d.apply("t", &[insert(10)]).unwrap();
    e.apply("t", &[insert(10)]).unwrap();
    assert_eq!(d.parts(), e.parts());
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.parts(), &base);
}

#[test]
fn text_format_at_a_caret_is_shown_to_the_next_typing_and_taken_back() {
    // a caret has no bytes to show: the session holds the insertion style, a commit applies it, a cancel forgets it
    let (mut fresh, _) = fixture();
    fresh.apply("글 서식", &[format(4, 4, serde_json::json!({"pointSize": 20}))]).unwrap();
    fresh.apply("t", &[insert(4)]).unwrap();
    let typed_with_format = fresh.parts().clone();
    let (mut e, _) = fixture();
    let base = e.parts().clone();
    preview(&mut e, "글 서식", vec![format(4, 4, serde_json::json!({"pointSize": 10}))]);
    preview(&mut e, "글 서식", vec![format(4, 4, serde_json::json!({"pointSize": 20}))]);
    assert_eq!(e.parts(), &base, "nothing to see at a caret");
    e.apply("", &reg(RegisteredCommand::PreviewCommit {})).unwrap();
    assert_eq!(e.undo_label(), None, "formatting a caret is no undo step, as without a preview");
    e.apply("t", &[insert(4)]).unwrap();
    assert_eq!(e.parts(), &typed_with_format);
    // cancelled: typing is plain
    let (mut c, _) = fixture();
    let (mut plain, _) = fixture();
    preview(&mut c, "글 서식", vec![format(4, 4, serde_json::json!({"pointSize": 20}))]);
    c.apply("", &reg(RegisteredCommand::PreviewCancel {})).unwrap();
    c.apply("t", &[insert(4)]).unwrap();
    plain.apply("t", &[insert(4)]).unwrap();
    assert_eq!(c.parts(), plain.parts());
}

#[test]
fn text_format_that_fails_leaves_what_was_shown() {
    let (mut e, _) = fixture();
    preview(&mut e, "글 서식", vec![format(0, 8, serde_json::json!({"pointSize": 13}))]);
    let shown = e.parts().clone();
    let bad = Command::TextFormat { story: "nope".into(), start: 0, end: 1, props: serde_json::from_value(serde_json::json!({"pointSize": 9})).unwrap() };
    assert!(e.apply("", &reg(RegisteredCommand::PreviewApply { commands: vec![bad] })).is_err());
    assert_eq!(e.parts(), &shown);
}
