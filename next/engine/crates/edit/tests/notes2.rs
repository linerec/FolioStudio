use folio_edit::{Editor, commands::RegisteredCommand};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/notes2.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len()).filter_map(|i| {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() { return None; }
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new(); entry.read_to_string(&mut text).unwrap();
        Some((name, text))
    }).collect()
}

fn resolve(accept: bool, index: i32) -> folio_edit::Command {
    RegisteredCommand::ResolveChanges { story: "u12d".into(), document: false, index, user: String::new(), accept }.into()
}

#[test]
fn endnote_revisions_keep_identity_unknown_data_and_atomic_undo() {
    for accept in [true, false] {
        let mut before = fixture();
        let body = before.get_mut("Stories/Story_u149.xml").unwrap();
        *body = body.replace("<EndnoteRange Self=\"u16b\"", "<EndnoteRange Future=\"keep &amp; retain\" Self=\"u16b\"");
        let mut ed = Editor::new(before.clone());
        let text = folio_edit::text::contents(&before["Stories/Story_u12d.xml"], "u12d").unwrap();
        assert_eq!(text.chars().filter(|&c| c == '\u{4}').count(), 3);
        let context = folio_edit::text::note_context(&before["Stories/Story_u12d.xml"], "u12d", 5).unwrap();
        assert_eq!(context["endnoteRange"], "u167");
        ed.apply("resolve middle", &[resolve(accept, 1)]).unwrap();
        let after = ed.parts().clone();
        let body = ed.part("Stories/Story_u149.xml").unwrap();
        assert!(body.contains("Future=\"keep &amp; retain\""));
        assert_eq!(body.contains("Self=\"u16a\""), accept);
        assert!(body.contains("Self=\"u167\"") && body.contains("Self=\"u16b\""));
        assert!(body.contains("FontStyle=\"Bold\""));
        if accept { assert_eq!(body, before["Stories/Story_u149.xml"]); }
        for (part, xml) in &before {
            if !part.starts_with("Stories/") { assert_eq!(ed.part(part), Some(xml.as_str())); }
        }
        ed.undo().unwrap(); assert_eq!(ed.parts(), &before);
        ed.redo().unwrap(); assert_eq!(ed.parts(), &after);
    }
}

#[test]
fn mismatched_or_unmeasured_endnote_ranges_leave_all_parts_untouched() {
    for replacement in ["SourceEndnote=\"unknown\"", "SourceEndnote=\"u16d\" unmeasured=\"yes\""] {
        let mut before = fixture();
        let body = before.get_mut("Stories/Story_u149.xml").unwrap();
        *body = body.replace("SourceEndnote=\"u16d\"", replacement);
        if replacement.contains("unmeasured") {
            *body = body.replace("</Story>", "<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>Extra</Content></CharacterStyleRange></ParagraphStyleRange></Story>");
        }
        let mut ed = Editor::new(before.clone());
        assert!(ed.apply("reject", &[resolve(false, 1)]).is_err());
        assert_eq!(ed.parts(), &before);
    }
}

#[test]
fn deleting_a_pending_endnote_cannot_orphan_its_body() {
    for separator in [" ", "\n"] {
        let mut before = fixture();
        let source = before.get_mut("Stories/Story_u12d.xml").unwrap();
        *source = source.replace("<Endnote ", &format!("<Endnote{separator}"));
        let mut ed = Editor::new(before.clone());
        let cmd = RegisteredCommand::DeleteTextAs { story: "u12d".into(), start: 5, end: 6, user: "Carol".into(), date: "2026-10-26T01:00:00".into() };
        assert!(ed.apply("delete pending reference", &[cmd.into()]).is_err());
        assert_eq!(ed.parts(), &before);
    }
}
