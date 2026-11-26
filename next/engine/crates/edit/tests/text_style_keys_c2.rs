//! Style keyboard shortcuts, second part (stylekeys-c2): which of two styles with the same shortcut wins, and the keys the field newly takes. The rules were measured in
//! InDesign 2025 with real key input (`artifacts/stylekeys-c2-20261023`, rules/stylekeys-c2.ko.md).
use folio_edit::{Command, Editor};
use std::collections::BTreeMap;

fn fixture(name: &str) -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../oracle/fixtures/{name}.idml"));
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if !e.is_dir() {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut e, &mut s).unwrap();
            out.insert(e.name().unwrap().into_owned(), s);
        }
    }
    out
}
fn cmd(wire: serde_json::Value) -> Command { serde_json::from_value(wire).unwrap() }
fn set(ed: &mut Editor, kind: &str, style: &str, shortcut: &str) -> Result<(), String> {
    ed.apply("step", &[cmd(serde_json::json!({"op": "setStyleShortcut", "kind": kind, "style": style, "shortcut": shortcut}))]).map(|_| ()).map_err(|e| e.to_string())
}
/// who wins a key press first, as `kind:name`
fn winners(ed: &Editor, spec: &str) -> Vec<String> {
    ed.style_shortcut_info(spec).unwrap()["owners"].as_array().unwrap().iter().map(|o| format!("{}:{}", o["kind"].as_str().unwrap(), o["name"].as_str().unwrap())).collect()
}

#[test]
fn the_style_given_the_shortcut_last_wins_and_a_reloaded_document_falls_back_to_the_document_order() {
    // measured (m5): P5 then P4 get Ctrl+Alt+Num 5 -> P4 wins in the session; after an IDML reload the later style of the document (P5) wins;
    // P2, P3 then P2 again with the same value: P3 (a re-OK of an unchanged field is not a new assignment)
    let mut ed = Editor::new(fixture("nested-styles"));
    set(&mut ed, "paragraph", "NS 1", "ctrl+alt+num5").unwrap();
    set(&mut ed, "paragraph", "NS 0", "ctrl+alt+num5").unwrap();
    assert_eq!(winners(&ed, "ctrl+alt+num5"), ["paragraph:NS 0", "paragraph:NS 1"], "assigned last wins although it is earlier in the document");
    set(&mut ed, "paragraph", "NS 1", "ctrl+alt+num5").unwrap(); // unchanged: no new turn
    assert_eq!(winners(&ed, "ctrl+alt+num5")[0], "paragraph:NS 0");
    set(&mut ed, "paragraph", "NS 1", "ctrl+alt+num6").unwrap();
    set(&mut ed, "paragraph", "NS 1", "ctrl+alt+num5").unwrap(); // a changed value is a new assignment
    assert_eq!(winners(&ed, "ctrl+alt+num5")[0], "paragraph:NS 1");
    // the document list too (the key handler reads it)
    let list = ed.style_shortcuts().unwrap();
    assert_eq!(list[0]["name"], "NS 1");
    // a reloaded document has no session order: the later style of the document wins
    let reloaded = Editor::new(ed.parts().clone());
    assert_eq!(winners(&reloaded, "ctrl+alt+num5"), ["paragraph:NS 1", "paragraph:NS 0"]);
    // across kinds (m5b): the character style given the key last wins in the session; after a reload a paragraph style beats a character style
    set(&mut ed, "character", "Red", "ctrl+alt+num5").unwrap();
    assert_eq!(winners(&ed, "ctrl+alt+num5"), ["character:Red", "paragraph:NS 1", "paragraph:NS 0"]);
    let reloaded = Editor::new(ed.parts().clone());
    assert_eq!(winners(&reloaded, "ctrl+alt+num5"), ["paragraph:NS 1", "paragraph:NS 0", "character:Red"]);
    // clearing gives up the turn
    set(&mut ed, "paragraph", "NS 1", "").unwrap();
    set(&mut ed, "character", "Red", "").unwrap();
    assert_eq!(winners(&ed, "ctrl+alt+num5"), ["paragraph:NS 0"]);
}

#[test]
fn punctuation_and_cursor_keys_are_written_like_indesign_and_the_refused_ones_are_refused() {
    let mut ed = Editor::new(fixture("nested-styles"));
    for (spec, ok) in [("ctrl+;", true), ("ctrl+shift+=", true), ("ctrl+alt+/", true), ("ctrl+left", true), ("shift+pagedown", true), ("alt+shift+p", true), ("shift+f5", true),
        ("alt+a", false), ("alt+f2", false), ("shift+a", false), ("shift+;", false), ("shift+space", false), ("ctrl+a", false), ("ctrl+z", false), ("ctrl+delete", false), ("alt+up", false)] {
        assert_eq!(set(&mut ed, "paragraph", "NS 0", spec).is_ok(), ok, "{spec}");
    }
    set(&mut ed, "paragraph", "NS 0", "ctrl+;").unwrap();
    let info = ed.style_info("paragraph", "NS 0").unwrap();
    assert_eq!((info["shortcut"].as_str(), info["shortcutLabel"].as_str()), (Some("ctrl+;"), Some("Ctrl+;")));
    set(&mut ed, "paragraph", "NS 0", "shift+ctrl+pagedown").unwrap();
    assert_eq!(ed.style_info("paragraph", "NS 0").unwrap()["shortcutLabel"], "Shift+Ctrl+Page Down");
    assert!(ed.parts()["Resources/Styles.xml"].contains("ExtendedKeyboardShortcut=\"17 1073741863 0\""));
}

fn run(ed: &mut Editor, wire: serde_json::Value) -> Result<(), String> {
    ed.apply("step", &[cmd(wire)]).map(|_| ()).map_err(|e| e.code.to_string())
}
fn story_len(ed: &Editor, story: &str) -> usize {
    folio_edit::text::contents(&ed.parts()[&format!("Stories/Story_{story}.xml")], story).unwrap().chars().count()
}
fn applied(ed: &mut Editor, story: &str, key: &str) -> Vec<String> {
    // the style at every character of the story, once per run
    let n = story_len(&*ed, story);
    let mut out: Vec<String> = Vec::new();
    for at in 0..n {
        let v = ed.text_format(story, at, at + 1).unwrap()[key].as_str().unwrap_or("?").to_owned();
        if out.last() != Some(&v) { out.push(v); }
    }
    out
}

#[test]
fn a_shortcut_with_text_frames_selected_styles_the_whole_story_of_each_frame() {
    // measured (m6): a selected text frame takes the style over its whole story (overset text too), several frames one story each, a rectangle nothing;
    // a character style goes over every character; an empty frame takes both on its (only) insertion point
    let mut ed = Editor::new(fixture("nested-styles"));
    let (a, b) = ("u142", "u158");
    run(&mut ed, serde_json::json!({"op": "applyStyleToFrames", "kind": "paragraph", "style": "NS 2", "ids": ["u154", "u16a"], "clear": false})).unwrap();
    for story in [a, b] {
        assert_eq!(applied(&mut ed, story, "paragraphStyle"), ["ParagraphStyle/NS 2"], "{story}");
    }
    assert_ne!(applied(&mut ed, "u16e", "paragraphStyle"), ["ParagraphStyle/NS 3"], "a frame that was not selected keeps its styles");
    assert_eq!(applied(&mut ed, "u16e", "paragraphStyle"), ["ParagraphStyle/NS 2"], "(it had NS 2 already)");
    run(&mut ed, serde_json::json!({"op": "applyStyleToFrames", "kind": "character", "style": "Red", "ids": ["u154"], "clear": false})).unwrap();
    assert_eq!(applied(&mut ed, a, "characterStyle"), ["CharacterStyle/Red"]);
    assert_ne!(applied(&mut ed, b, "characterStyle"), ["CharacterStyle/Red"]);
    ed.undo().unwrap();
    assert_ne!(applied(&mut ed, a, "characterStyle"), ["CharacterStyle/Red"], "one undo step");
    // an empty frame: its only paragraph and insertion point take the styles
    run(&mut ed, serde_json::json!({"op": "applyStyleToFrames", "kind": "paragraph", "style": "NS 3", "ids": ["u13e"], "clear": false})).unwrap();
    run(&mut ed, serde_json::json!({"op": "applyStyleToFrames", "kind": "character", "style": "Red", "ids": ["u13e"], "clear": false})).unwrap();
    let empty = &ed.parts()["Stories/Story_u12c.xml"];
    assert!(empty.contains("AppliedParagraphStyle=\"ParagraphStyle/NS 3\"") && empty.contains("AppliedCharacterStyle=\"CharacterStyle/Red\""), "{empty}");
    // nothing with text: an error, nothing changes
    let before = ed.parts().clone();
    assert_eq!(run(&mut ed, serde_json::json!({"op": "applyStyleToFrames", "kind": "paragraph", "style": "NS 2", "ids": [], "clear": false})).unwrap_err(), "notText");
    assert_eq!(ed.parts(), &before);
    assert!(run(&mut ed, serde_json::json!({"op": "applyStyleToFrames", "kind": "paragraph", "style": "No Such", "ids": ["u154"], "clear": false})).is_err());
}

#[test]
fn a_shortcut_with_nothing_selected_changes_the_default_style_and_no_text() {
    // measured (m6): the document's text defaults change (`textDefaults.appliedParagraphStyle` / `appliedCharacterStyle`), the text does not
    let mut ed = Editor::new(fixture("nested-styles"));
    let before = ed.parts().clone();
    run(&mut ed, serde_json::json!({"op": "setTextDefaultStyle", "kind": "paragraph", "style": "NS 1"})).unwrap();
    run(&mut ed, serde_json::json!({"op": "setTextDefaultStyle", "kind": "character", "style": "Red"})).unwrap();
    let prefs = &ed.parts()["Resources/Preferences.xml"];
    assert!(prefs.contains("AppliedParagraphStyle=\"ParagraphStyle/NS 1\"") && prefs.contains("AppliedCharacterStyle=\"CharacterStyle/Red\""));
    for (name, text) in ed.parts() {
        if name != "Resources/Preferences.xml" { assert_eq!(text, &before[name], "{name}"); }
    }
    // the same value again is no change; a bad style is refused
    let kept = ed.parts().clone();
    run(&mut ed, serde_json::json!({"op": "setTextDefaultStyle", "kind": "paragraph", "style": "NS 1"})).unwrap();
    assert_eq!(ed.parts(), &kept);
    assert!(run(&mut ed, serde_json::json!({"op": "setTextDefaultStyle", "kind": "paragraph", "style": "No Such"})).is_err());
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
}
