//! Type ▸ Convert Footnotes and Endnotes (whole document) and Type ▸ Insert Endnote.
//!
//! The conversion answers are InDesign 20.5.5.119's own: `next/oracle/measured/notes3/convert-*-{setup,read}.idml`
//! are the document before and after the dialog's Convert, exported from InDesign by the notes3 measurement
//! (artifacts/notes3-20261023, rules `next/oracle/rules/notes3-convert.ko.md`). The dialog cannot be driven
//! without desktop input, so these two pairs are the evidence; the selection scope was not measured.
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

/// Whitespace between tags gone, object ids that are not in `known` numbered by first appearance.
fn shape(text: &str, known: &std::collections::BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut fresh: Vec<String> = Vec::new();
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == 'u' && i > 0 && (b[i - 1] == '"') {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_hexdigit() { j += 1; }
            if j > i + 1 && j < b.len() && b[j] == '"' {
                let id: String = b[i..j].iter().collect();
                if known.contains(&id) { out.push_str(&id); } else {
                    let k = fresh.iter().position(|f| *f == id).unwrap_or_else(|| { fresh.push(id.clone()); fresh.len() - 1 });
                    out.push_str(&format!("new{k}"));
                }
                i = j;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    let chars: Vec<char> = out.chars().collect();
    let mut squeezed = String::new();
    let mut k = 0;
    while k < chars.len() {
        squeezed.push(chars[k]);
        if chars[k] == '>' {
            let mut m = k + 1;
            while m < chars.len() && chars[m].is_whitespace() { m += 1; }
            if m < chars.len() && chars[m] == '<' { k = m; continue; }
        }
        k += 1;
    }
    squeezed
}

fn ids(parts: &BTreeMap<String, String>) -> std::collections::BTreeSet<String> {
    let mut known = std::collections::BTreeSet::new();
    for t in parts.values() {
        for piece in t.split('"') {
            if piece.starts_with('u') && piece.len() > 1 && piece[1..].chars().all(|c| c.is_ascii_hexdigit()) { known.insert(piece.to_owned()); }
        }
    }
    known
}

fn convert(pair: &str, direction: &str) {
    let setup = load(&format!("convert-{pair}-setup.idml"));
    let read = load(&format!("convert-{pair}-read.idml"));
    let mut ed = Editor::new(setup.clone());
    let out = ed.apply("convert", &[RegisteredCommand::ConvertNotes { direction: direction.into(), scope: "document".into() }.into()]).unwrap();
    let known = ids(&setup);
    let mut checked = 0;
    for (name, expected) in &read {
        if !name.starts_with("Stories/") { continue; }
        let got = ed.part(name).unwrap_or_else(|| panic!("{name} missing"));
        // `known` for the expected side is the setup's too: ids InDesign made are fresh on both sides
        assert_eq!(shape(got, &known), shape(expected, &known), "{name}");
        checked += 1;
    }
    assert!(checked >= 2);
    for (name, source) in &setup {
        if !name.starts_with("Stories/") { assert_eq!(ed.part(name), Some(source.as_str()), "{name} must not change"); }
    }
    assert!(!out.created.is_empty(), "the ids made are reported");
    let after = ed.parts().clone();
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &setup, "one undo gives the document back");
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

#[test]
fn footnotes_to_endnotes_match_indesign_for_the_whole_document() { convert("forward", "footnotesToEndnotes"); }

#[test]
fn endnotes_to_footnotes_match_indesign_for_the_whole_document() { convert("reverse", "endnotesToFootnotes"); }

#[test]
fn refused_conversions_leave_the_document_alone() {
    let setup = load("convert-forward-setup.idml");
    let mut ed = Editor::new(setup.clone());
    for (direction, scope) in [("footnotesToEndnotes", "selection"), ("sideways", "document")] {
        assert!(ed.apply("convert", &[RegisteredCommand::ConvertNotes { direction: direction.into(), scope: scope.into() }.into()]).is_err());
        assert_eq!(ed.parts(), &setup);
    }
    // converting twice: nothing is left to convert
    ed.apply("convert", &[RegisteredCommand::ConvertNotes { direction: "footnotesToEndnotes".into(), scope: "document".into() }.into()]).unwrap();
    let once = ed.parts().clone();
    assert!(ed.apply("convert", &[RegisteredCommand::ConvertNotes { direction: "footnotesToEndnotes".into(), scope: "document".into() }.into()]).is_err());
    assert_eq!(ed.parts(), &once);
}

fn footnotes_fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/footnotes.idml");
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

#[test]
fn insert_endnote_makes_page_frame_story_and_undoes_in_one_step() {
    let parts = footnotes_fixture();
    let mut ed = Editor::new(parts.clone());
    let out = ed.apply("insert endnote", &[RegisteredCommand::InsertEndnote { story: "u12e".into(), at: 30 }.into()]).unwrap();
    assert!(out.created.len() >= 3, "frame, reference and body ids: {:?}", out.created);
    let doc = folio_idml::read_parts(ed.parts(), Path::new("."), &Default::default()).unwrap();
    let last = doc.spreads.last().unwrap();
    assert_eq!(last.items.len(), 1, "{:?}", last.items.iter().map(|i| &i.id).collect::<Vec<_>>());
    let after = ed.parts().clone();
    assert_eq!(after.keys().filter(|k| k.starts_with("Spreads/")).count(), parts.keys().filter(|k| k.starts_with("Spreads/")).count() + 1);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &parts, "one undo takes the page, the frame, the story and the reference away");
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

fn endnotes_fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/footnotes2-endnotes.idml");
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

/// The endnote bodies in story order, read as the order of their references: before the first, between, after the last.
#[test]
fn insert_endnote_into_an_existing_story_keeps_bodies_in_reference_order() {
    for at in [10, 45, 90] {
        let parts = endnotes_fixture();
        let mut ed = Editor::new(parts.clone());
        ed.apply("insert endnote", &[RegisteredCommand::InsertEndnote { story: "u12e".into(), at }.into()]).unwrap();
        let story = ed.parts().iter().find(|(_, t)| t.contains("IsEndnoteStory=\"true\"")).unwrap().1.clone();
        let body = &ed.parts()["Stories/Story_u12e.xml"];
        // references in text order and the ranges they name, in story order
        let refs: Vec<&str> = body.match_indices("EndnoteTextRange=\"").map(|(i, m)| body[i + m.len()..].split('"').next().unwrap()).collect();
        let ranges: Vec<&str> = story.match_indices("<EndnoteRange Self=\"").map(|(i, m)| story[i + m.len()..].split('"').next().unwrap()).collect();
        assert_eq!(refs.len(), 3, "at {at}");
        assert_eq!(refs, ranges, "at {at}: the bodies follow the references");
        assert_eq!(story.matches("<Br />").count(), 4, "at {at}: the title Br, the first body own paragraph break and one between each pair of bodies");
        let doc = folio_idml::read_parts(ed.parts(), Path::new("."), &Default::default()).unwrap();
        let mut numbers: Vec<_> = doc.stories.values().flat_map(|s| &s.paragraphs).flat_map(|p| &p.runs).filter_map(|r| r.note.as_ref().and_then(|n| n.number)).collect();
        numbers.sort();
        numbers.dedup();
        assert_eq!(numbers, vec![1, 2, 3], "at {at}");
        ed.undo().unwrap();
        assert_eq!(ed.parts(), &parts);
    }
}

#[test]
fn converting_footnotes_makes_the_endnote_story_when_there_is_none() {
    let parts = footnotes_fixture();
    let mut ed = Editor::new(parts.clone());
    ed.apply("footnote", &[RegisteredCommand::InsertFootnote { story: "u12e".into(), at: 30 }.into()]).unwrap();
    let with_footnote = ed.parts().clone();
    ed.apply("convert", &[RegisteredCommand::ConvertNotes { direction: "footnotesToEndnotes".into(), scope: "document".into() }.into()]).unwrap();
    assert!(!ed.parts()["Stories/Story_u12e.xml"].contains("<Footnote"));
    assert!(ed.parts().values().any(|t| t.contains("IsEndnoteStory=\"true\"") && t.contains("<EndnoteRange")));
    assert!(ed.parts().keys().any(|k| k.starts_with("Spreads/")) && ed.parts().values().any(|t| t.contains("<EndnoteTextFrame")));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &with_footnote);
}
