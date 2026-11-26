//! Invalid requests must be atomic; selection normalization must not move a child twice.
use folio_edit::{AlignBounds, AlignMode, Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, BTreeMap<String, String>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut labels = BTreeMap::new();
    for n in 0..z.len() {
        let mut entry = z.by_index(n).unwrap();
        if entry.is_dir() {
            continue;
        }
        let mut t = String::new();
        entry.read_to_string(&mut t).unwrap();
        if entry.name().unwrap().starts_with("Spreads/") {
            let ix = Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") {
                    let owner = ix
                        .ancestors(i)
                        .find(|&a| ix.attr(&t, a, "Self").is_some())
                        .unwrap();
                    labels.insert(
                        ix.value(&t, i, "Value").unwrap(),
                        ix.attr(&t, owner, "Self").unwrap().to_owned(),
                    );
                }
            }
        }
        parts.insert(entry.name().unwrap().into_owned(), t);
    }
    (Editor::new(parts), labels)
}

#[test]
fn invalid_alignment_rolls_back_the_whole_transaction() {
    let (mut ed, ids) = fixture();
    let before = ed.parts().clone();
    let c = Command::Align {
        ids: vec![ids["r-plain"].clone(), ids["tf"].clone()],
        how: AlignMode::Left,
        bounds: AlignBounds::Page,
        key: None,
    };
    let r = ed.apply(
        "invalid",
        &[
            Command::Move {
                ids: vec![ids["r-plain"].clone()],
                by: [20.0, 10.0],
            },
            c,
        ],
    );
    assert_eq!(r.unwrap_err().code, "invalid");
    assert_eq!(ed.parts(), &before);
    assert_eq!(ed.rev(), 0);
    assert_eq!(ed.undo_label(), None);
}

#[test]
fn a_missing_key_or_nonfinite_distance_does_not_write_parts() {
    let (mut ed, ids) = fixture();
    let before = ed.parts().clone();
    for c in [
        Command::Align {
            ids: vec![ids["r-plain"].clone()],
            how: AlignMode::Right,
            bounds: AlignBounds::Key,
            key: Some(ids["r-fill"].clone()),
        },
        Command::Distribute {
            ids: vec![ids["r-plain"].clone(), ids["r-fill"].clone()],
            how: AlignMode::HSpace,
            bounds: AlignBounds::Selection,
            key: None,
            gap: Some(f64::NAN),
        },
    ] {
        assert!(ed.apply("invalid", &[c]).is_err());
        assert_eq!(ed.parts(), &before);
    }
}

#[test]
fn duplicates_and_selected_group_children_do_not_move_twice() {
    let (mut a, ids) = fixture();
    let mut b = Editor::new(a.parts().clone());
    for (ed, selection) in [
        (&mut a, vec![ids["grp"].clone(), ids["r-plain"].clone()]),
        (
            &mut b,
            vec![
                ids["grp"].clone(),
                ids["grp-a"].clone(),
                ids["r-plain"].clone(),
                ids["r-plain"].clone(),
            ],
        ),
    ] {
        ed.apply(
            "align",
            &[Command::Align {
                ids: selection,
                how: AlignMode::Right,
                bounds: AlignBounds::Page,
                key: None,
            }],
        )
        .unwrap();
    }
    assert_eq!(a.parts(), b.parts());
}

#[test]
fn key_distribution_keeps_the_reference_and_roundtrips_exactly() {
    let (mut ed, ids) = fixture();
    let original = ed.parts().clone();
    let key_before = ed.bounds(&ids["r-rot"]).unwrap();
    ed.apply(
        "spacing",
        &[Command::Distribute {
            ids: ["r-shear", "r-rot", "r-plain"]
                .map(|id| ids[id].clone())
                .into(),
            how: AlignMode::HSpace,
            bounds: AlignBounds::Key,
            key: Some(ids["r-rot"].clone()),
            gap: Some(18.0),
        }],
    )
    .unwrap();
    let after = ed.parts().clone();
    assert_ne!(after, original);
    assert_eq!(ed.bounds(&ids["r-rot"]).unwrap(), key_before);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &original);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}
