//! Style keyboard shortcuts (stylekeys-c1): the `setStyleShortcut` command writes the two IDML attributes the way InDesign 2025 does (the
//! values were measured with real key input, `artifacts/stylekeys-c1-20261023`, rules/stylekeys.ko.md), `styleInfo` and the document's
//! shortcut list read them back, and everything else about the styles is left alone.
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
/// `(KeyboardShortcut, ExtendedKeyboardShortcut)` of the style element named `name` in the Styles part
fn attrs(ed: &Editor, tag: &str, name: &str) -> (String, String) {
    let t = &ed.parts()["Resources/Styles.xml"];
    let start = t.match_indices(&format!("<{tag} ")).map(|(i, _)| i).find(|&i| {
        let end = t[i..].find('>').unwrap() + i;
        t[i..end].contains(&format!(" Name=\"{name}\""))
    }).unwrap_or_else(|| panic!("{tag} {name}"));
    let open = &t[start..start + t[start..].find('>').unwrap()];
    let get = |k: &str| {
        let p = open.find(&format!(" {k}=\"")).map(|p| p + k.len() + 3).unwrap_or_else(|| panic!("{k} missing in {open}"));
        open[p..p + open[p..].find('"').unwrap()].to_owned()
    };
    (get("KeyboardShortcut"), get("ExtendedKeyboardShortcut"))
}

#[test]
fn measured_shortcuts_are_written_like_indesign() {
    let mut ed = Editor::new(fixture("nested-styles"));
    // (shortcut, KeyboardShortcut, ExtendedKeyboardShortcut): rows of the real-input measurement
    for (spec, short, ext) in [("ctrl+num1", "1 97", "0 0 0"), ("ctrl+shift+num1", "1 97", "0 0 0"), ("ctrl+alt+num1", "257 97", "0 0 0"),
        ("ctrl+nummul", "0 0", "1 -1073741782 0"), ("ctrl+alt+f2", "0 0", "257 8 0"), ("shift+f2", "0 0", "16 8 0"),
        ("ctrl+alt+shift+f2", "0 0", "273 8 0"), ("ctrl+alt+a", "0 0", "257 65 1"), ("ctrl+alt+left", "0 0", "257 1073741866 0")] {
        set(&mut ed, "paragraph", "NS 0", spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
        assert_eq!(attrs(&ed, "ParagraphStyle", "NS 0"), (short.to_owned(), ext.to_owned()), "{spec}");
    }
    set(&mut ed, "character", "Red", "ctrl+alt+num3").unwrap();
    assert_eq!(attrs(&ed, "CharacterStyle", "Red"), ("257 99".into(), "0 0 0".into()));
    // clearing
    set(&mut ed, "paragraph", "NS 0", "").unwrap();
    assert_eq!(attrs(&ed, "ParagraphStyle", "NS 0"), ("0 0".into(), "0 0 0".into()));
}

#[test]
fn bare_keys_root_styles_and_unknown_styles_are_refused() {
    let mut ed = Editor::new(fixture("nested-styles"));
    let before = ed.parts().clone();
    for spec in ["f2", "num1", "shift+num1", "a", "ctrl+f13", "ctrl+unknown"] {
        assert!(set(&mut ed, "paragraph", "NS 0", spec).is_err(), "{spec}");
    }
    assert!(set(&mut ed, "paragraph", "[No Paragraph Style]", "ctrl+num1").unwrap_err().ends_with("Invalid request on a root style."));
    assert!(set(&mut ed, "character", "[None]", "ctrl+num1").unwrap_err().ends_with("Invalid request on a root style."));
    assert!(set(&mut ed, "paragraph", "No Such Style", "ctrl+num1").is_err());
    assert_eq!(ed.parts(), &before, "a refused step changes nothing");
}

#[test]
fn style_info_and_the_list_read_the_shortcuts_back() {
    let mut ed = Editor::new(fixture("nested-styles"));
    assert!(ed.style_shortcuts().unwrap().is_empty());
    set(&mut ed, "paragraph", "NS 0", "ctrl+num1").unwrap();
    set(&mut ed, "paragraph", "NS 1", "ctrl+num1").unwrap(); // duplicates are allowed (InDesign only warns)
    set(&mut ed, "character", "Red", "ctrl+alt+f2").unwrap();
    let info = ed.style_info("paragraph", "NS 0").unwrap();
    assert_eq!(info["shortcut"], "ctrl+num1");
    assert_eq!(info["shortcutLabel"], "Ctrl+Num 1");
    assert!(ed.style_info("paragraph", "NS 2").unwrap()["shortcut"].is_null());
    let list = ed.style_shortcuts().unwrap();
    let names: Vec<_> = list.iter().map(|r| (r["kind"].as_str().unwrap(), r["name"].as_str().unwrap(), r["shortcut"].as_str().unwrap())).collect();
    // the style that wins a key press comes first: the later style of the document, character styles before paragraph styles in the list
    assert_eq!(names, [("character", "Red", "ctrl+alt+f2"), ("paragraph", "NS 1", "ctrl+num1"), ("paragraph", "NS 0", "ctrl+num1")]);
}

#[test]
fn an_imported_shortcut_survives_other_edits_and_a_duplicate_has_none() {
    let mut files = fixture("nested-styles");
    let styles = files.get_mut("Resources/Styles.xml").unwrap();
    let at = styles.find("Name=\"NS 3\"").unwrap();
    let open = styles[..at].rfind("<ParagraphStyle").unwrap();
    let end = open + styles[open..].find('>').unwrap();
    let tag = styles[open..end].replace("ExtendedKeyboardShortcut=\"0 0 0\"", "ExtendedKeyboardShortcut=\"257 7 0\"");
    assert_ne!(tag, styles[open..end], "the fixture style carries the extended attribute");
    styles.replace_range(open..end, &tag);
    let mut ed = Editor::new(files);
    assert_eq!(ed.style_info("paragraph", "NS 3").unwrap()["shortcut"], "ctrl+alt+f1");
    ed.apply("step", &[cmd(serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": "NS 3", "props": {"pointSize": 17}}))]).unwrap();
    assert_eq!(ed.style_info("paragraph", "NS 3").unwrap()["shortcut"], "ctrl+alt+f1", "an edit of the values keeps the shortcut");
    ed.apply("step", &[cmd(serde_json::json!({"op": "duplicateTextStyle", "kind": "paragraph", "style": "NS 3", "name": "NS 3 copy"}))]).unwrap();
    assert!(ed.style_info("paragraph", "NS 3 copy").unwrap()["shortcut"].is_null(), "Duplicate Style: the copy has no shortcut");
}
