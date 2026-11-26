//! `Editor::rollback`: a change the caller could not use (the server failed to read the
//! document back) leaves no trace: same bytes, same revision, same history.
use folio_edit::{Command, Editor, Rollback, xml::Index};
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

fn moved(l: &BTreeMap<String, String>, dx: f64) -> Command {
    Command::Move { ids: vec![l["r-plain"].clone()], by: [dx, 0.0] }
}

#[test]
fn rolled_back_apply_leaves_no_trace() {
    let (mut e, l) = fixture();
    e.apply("first", &[moved(&l, 5.0)]).unwrap();
    let (parts, rev) = (e.parts().clone(), e.rev());
    let o = e.apply("second", &[moved(&l, 7.0)]).unwrap();
    e.rollback(o, Rollback::Apply);
    assert_eq!(e.parts(), &parts);
    assert_eq!(e.rev(), rev);
    assert_eq!(e.undo_label(), Some("first"));
    assert_eq!(e.redo_label(), None);
}

#[test]
fn rolled_back_undo_and_redo_restore_the_history() {
    let (mut e, l) = fixture();
    e.apply("move", &[moved(&l, 5.0)]).unwrap();
    let (after, rev) = (e.parts().clone(), e.rev());
    let o = e.undo().unwrap();
    e.rollback(o, Rollback::Undo);
    assert_eq!(e.parts(), &after);
    assert_eq!((e.rev(), e.undo_label(), e.redo_label()), (rev, Some("move"), None));

    let before_redo = {
        e.undo().unwrap();
        (e.parts().clone(), e.rev())
    };
    let o = e.redo().unwrap();
    e.rollback(o, Rollback::Redo);
    assert_eq!((e.parts(), e.rev()), (&before_redo.0, before_redo.1));
    assert_eq!((e.undo_label(), e.redo_label()), (None, Some("move")));
}

#[test]
fn a_new_edit_after_undo_discards_the_old_redo_branch() {
    let (mut editor, labels) = fixture();
    editor.apply("first", &[moved(&labels, 5.0)]).unwrap();
    let first = editor.parts().clone();
    editor.apply("abandoned", &[moved(&labels, 7.0)]).unwrap();
    editor.undo().unwrap();
    assert_eq!(editor.parts(), &first);
    assert_eq!(editor.redo_label(), Some("abandoned"));
    editor.apply("replacement", &[moved(&labels, 11.0)]).unwrap();
    let replacement = editor.parts().clone();
    assert_eq!(editor.redo_label(), None);
    assert!(editor.redo().is_none());
    assert_eq!(editor.parts(), &replacement);
    editor.undo().unwrap();
    assert_eq!(editor.parts(), &first);
    editor.redo().unwrap();
    assert_eq!(editor.parts(), &replacement);
}

#[test]
fn move_preserves_linear_transform_and_adds_signed_coordinates() {
    let (mut editor, labels) = fixture();
    let transform = |editor: &Editor| -> Vec<f64> {
        for (name, text) in editor.parts() {
            if !name.starts_with("Spreads/") { continue; }
            let ix = Index::scan(text).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.attr(text, i, "Self") == Some(labels["r-plain"].as_str()) {
                    return ix.attr(text, i, "ItemTransform").unwrap().split_whitespace()
                        .map(|v| v.parse().unwrap()).collect();
                }
            }
        }
        panic!("fixture rectangle must exist");
    };
    let original = transform(&editor);
    editor.apply("move", &[Command::Move { ids: vec![labels["r-plain"].clone()], by: [5.0, -7.0] }]).unwrap();
    let moved = transform(&editor);
    assert_eq!(&moved[..4], &original[..4]);
    assert!((moved[4] - original[4] - 5.0).abs() < 1e-9);
    assert!((moved[5] - original[5] + 7.0).abs() < 1e-9);
    editor.undo().unwrap();
    assert_eq!(transform(&editor), original);
    editor.redo().unwrap();
    assert_eq!(transform(&editor), moved);
}
#[test]
fn escaped_attribute_keeps_quotes_and_following_attributes_on_reparse() {
    use folio_edit::xml::escape;
    let value = "A \"quoted\" label & <tag>";
    let xml = format!("<Rectangle Label=\"{}\" Self=\"next\"/>", escape(value));
    let index = Index::scan(&xml).unwrap();
    assert_eq!(index.value(&xml, 0, "Label").as_deref(), Some(value));
    assert_eq!(index.value(&xml, 0, "Self").as_deref(), Some("next"));
    assert_eq!(index.attrs(&xml, 0).len(), 2);
}
