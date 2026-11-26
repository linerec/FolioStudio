//! Story copies are independent, undoable parts, with no residue after failed batches.
use folio_edit::{Command, Editor};
use std::collections::BTreeMap;

fn editor() -> Editor {
    Editor::new(BTreeMap::from([
        ("designmap.xml".into(), r#"<Document Self="d" StoryList="u20"><idPkg:Story src="Stories/Story_u20.xml"/></Document>"#.into()),
        ("Stories/Story_u20.xml".into(), r#"<idPkg:Story><Story Self="u20"><ParagraphStyleRange><CharacterStyleRange><Content>Independent text</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"#.into()),
        ("Spreads/Spread_u1.xml".into(), r#"<idPkg:Spread><Spread Self="u1"><TextFrame Self="u10" ParentStory="u20" PreviousTextFrame="n" NextTextFrame="n" ItemTransform="1 0 0 1 0 0"/></Spread></idPkg:Spread>"#.into()),
    ]))
}
fn copy(id: &str) -> Command { Command::Duplicate { ids: vec![id.into()], by: [10.0, 20.0] } }

#[test]
fn independent_part_and_exact_undo_redo() {
    let mut e = editor(); let before = e.parts().clone();
    let o = e.apply("copy", &[copy("u10")]).unwrap();
    assert_eq!(o.created.len(), 1);
    assert_eq!(e.parts().len(), before.len() + 1);
    assert_eq!(e.part("Stories/Story_u20.xml"), before.get("Stories/Story_u20.xml").map(String::as_str));
    let after = e.parts().clone();
    e.undo().unwrap(); assert_eq!(e.parts(), &before);
    e.redo().unwrap(); assert_eq!(e.parts(), &after);
}

#[test]
fn failed_batch_removes_new_part_and_registration() {
    let mut e = editor(); let before = e.parts().clone();
    assert!(e.apply("copy", &[copy("u10"), copy("missing")]).is_err());
    assert_eq!(e.parts(), &before); assert_eq!(e.rev(), 0);
    assert!(e.undo().is_none());
}

#[test]
fn thread_boundary_requires_current_composition() {
    let e = editor();
    let mut parts = e.parts().clone();
    let p = parts.get_mut("Spreads/Spread_u1.xml").unwrap();
    *p = p.replace(r#"NextTextFrame="n""#, r#"NextTextFrame="u11""#);
    let mut e = Editor::new(parts.clone());
    assert!(e.apply("copy", &[copy("u10")]).is_err());
    assert_eq!(e.parts(), &parts);
}
