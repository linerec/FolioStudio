//! Type ▸ Notes ▸ Split Note, against InDesign 20.5.5.119's own exports (`next/oracle/measured/notes3/split*-{setup,read}.idml`,
//! made by artifacts/smallmarks-c1-20261023/tools/n6_split.py: the Notes panel's caret put in the note with real input, then the
//! Split Note action). Rules: `next/oracle/rules/notes3-split.ko.md`.
use folio_edit::{commands::RegisteredCommand, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

fn load(name: &str) -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/notes3").join(name);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len()).filter_map(|i| {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() { return None; }
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        Some((name, text))
    }).collect()
}

/// Whitespace between tags removed.
fn squeeze(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut k = 0;
    while k < chars.len() {
        out.push(chars[k]);
        if chars[k] == '>' {
            let mut m = k + 1;
            while m < chars.len() && chars[m].is_whitespace() { m += 1; }
            if m < chars.len() && chars[m] == '<' { k = m; continue; }
        }
        k += 1;
    }
    out
}

const STORY: &str = "Stories/Story_u116.xml";

fn split_cmd(index: usize, at: usize, user: &str, date: &str) -> folio_edit::Command {
    RegisteredCommand::SplitNote { story: "u116".into(), index, at, user: user.into(), date: date.into() }.into()
}

/// `user` and `date` are what InDesign wrote into the new note (the split is made by the app's user at that moment).
fn check(case: &str, at: usize, user: &str, date: &str) {
    let setup = load(&format!("{case}-setup.idml"));
    let read = load(&format!("{case}-read.idml"));
    let mut ed = Editor::new(setup.clone());
    let out = ed.apply("split note", &[split_cmd(0, at, user, date)]).unwrap();
    assert_eq!(squeeze(ed.part(STORY).unwrap()), squeeze(&read[STORY]), "{case}: the story");
    assert_eq!(out.created, vec!["u116#note1".to_string()], "{case}: the new note is reported");
    // the author table: the new note's author is in it, under the id InDesign gave
    let user_ids = |m: &BTreeMap<String, String>| m["designmap.xml"].match_indices("<DocumentUser Self=\"").map(|(i, p)| {
        let rest = &m["designmap.xml"][i + p.len()..];
        let id = rest.split('"').next().unwrap().to_owned();
        let name = rest.split("UserName=\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
        (id, name)
    }).collect::<Vec<_>>();
    assert_eq!(user_ids(&ed.parts().clone()), user_ids(&read), "{case}: DocumentUser entries");
    for (name, source) in &setup {
        if name != STORY && name != "designmap.xml" { assert_eq!(ed.part(name), Some(source.as_str()), "{case}: {name} must not change"); }
    }
    let after = ed.parts().clone();
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &setup, "{case}: one undo gives the document back");
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after, "{case}: redo");
}

#[test]
fn split_in_the_middle_of_a_line_matches_indesign() { check("split4", 4, "Carol", "2026-11-03T08:38:17"); }

#[test]
fn split_after_a_return_matches_indesign() { check("splitcr", 8, "Carol", "2026-11-03T08:38:22"); }

#[test]
fn the_new_note_belongs_to_the_user_who_splits_not_to_the_original_author() { check("splituser", 4, "Bob", "2026-11-03T08:38:27"); }

#[test]
fn a_caret_at_the_start_or_the_end_or_a_missing_note_is_refused() {
    // InDesign leaves Split Note off at both ends (measured), so there is nothing to split into
    let setup = load("split4-setup.idml");
    let mut ed = Editor::new(setup.clone());
    for (index, at) in [(0, 0), (0, 18), (0, 99), (1, 4)] {
        assert!(ed.apply("split note", &[split_cmd(index, at, "Carol", "2026-11-03T09:00:00")]).is_err(), "index {index} at {at}");
        assert_eq!(ed.parts(), &setup);
    }
}

#[test]
fn splitting_twice_keeps_every_character_in_order() {
    let setup = load("split4-setup.idml");
    let mut ed = Editor::new(setup);
    ed.apply("split note", &[split_cmd(0, 4, "Carol", "2026-11-03T09:00:00")]).unwrap();
    ed.apply("split note", &[split_cmd(1, 3, "Carol", "2026-11-03T09:00:01")]).unwrap();
    let story = ed.part(STORY).unwrap();
    let texts: Vec<String> = story.split("<Note ").skip(1).map(|n| n.split("</Note>").next().unwrap().split("<Content>").skip(1).map(|c| c.split("</Content>").next().unwrap()).collect::<Vec<_>>().join("|")).collect();
    assert_eq!(texts, vec!["One ", "two", "Three four"]);
}
