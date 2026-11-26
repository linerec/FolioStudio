use folio_edit::{Editor, commands::RegisteredCommand};
use std::{collections::BTreeMap, path::Path};

fn measured(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/bookx-c2/chapter").join(name)).unwrap()
}

#[test]
fn measured_book_chain_and_idml_undo() {
    let mut previous = None;
    for i in 0..3 {
        let before = measured(&format!("before-{i}.xml"));
        let expected = folio_idml::chapter::read(&measured(&format!("after-{i}.xml"))).unwrap();
        let mut ed = Editor::new(BTreeMap::from([(folio_idml::chapter::PART.into(), before.clone())]));
        let number = ed.update_book_chapter(previous).unwrap();
        assert_eq!(ed.chapter_number().unwrap(), expected);
        assert_eq!(number, [5, 6, 2][i]);
        if i == 1 {
            assert!(ed.undo().is_some());
            assert_eq!(ed.part(folio_idml::chapter::PART), Some(before.as_str()));
            ed.redo().unwrap();
            assert_eq!(ed.chapter_number().unwrap(), expected);
        }
        previous = Some(number);
    }
}

#[test]
fn chapter_command_validates_without_partial_changes() {
    let before = measured("before-0.xml");
    let mut ed = Editor::new(BTreeMap::from([(folio_idml::chapter::PART.into(), before.clone())]));
    for (number, source) in [(0, "UserDefined"), (1, "unknown")] {
        assert!(ed.apply("invalid", &[RegisteredCommand::SetChapterNumber { number, source: source.into() }.into()]).is_err());
        assert_eq!(ed.part(folio_idml::chapter::PART), Some(before.as_str()));
    }
}
