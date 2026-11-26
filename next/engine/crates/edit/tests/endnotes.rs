use folio_edit::{Editor, commands::RegisteredCommand};
use std::{collections::BTreeMap,io::Read,path::Path};

fn fixture() -> BTreeMap<String,String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/footnotes2-endnotes.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len()).filter_map(|i| {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {return None;}
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new(); entry.read_to_string(&mut text).unwrap();
        Some((name,text))
    }).collect()
}

#[test]
fn options_preserve_styled_ranges_and_undo_and_number_existing_references() {
    let mut parts = fixture();
    let dm = parts.get_mut("designmap.xml").unwrap();
    *dm = dm.replace("<EndnoteOption ","<EndnoteOption FutureOption=\"keep &amp; preserve\" ");
    let mut ed = Editor::new(parts.clone());
    let props = BTreeMap::from([
        ("StartEndnoteNumberAt".into(),serde_json::json!(4)),
        ("EndnoteTitle".into(),serde_json::json!("Notes & references")),
        ("EndnoteSeparatorText".into(),serde_json::json!(": ")),
    ]);
    ed.apply("options",&[RegisteredCommand::SetEndnoteOptions{props}.into()]).unwrap();
    assert_eq!(ed.endnote_options().unwrap()["FutureOption"],"keep & preserve");
    for (name,source) in &parts {
        if name.starts_with("Stories/") { assert_eq!(ed.part(name),Some(source.as_str())); }
    }
    let doc = folio_idml::read_parts(ed.parts(),Path::new("."),&Default::default()).unwrap();
    let mut numbers: Vec<_> = doc.stories.values().flat_map(|s|&s.paragraphs).flat_map(|p|&p.runs)
        .filter_map(|r|r.note.as_ref().and_then(|n|n.number)).collect();
    numbers.sort();numbers.dedup();assert_eq!(numbers,vec![4,5]);
    let after = ed.parts().clone();
    ed.undo().unwrap();assert_eq!(ed.parts(),&parts);
    ed.redo().unwrap();assert_eq!(ed.parts(),&after);
    for value in [serde_json::json!(0),serde_json::json!(100001),serde_json::json!(1.5)] {
        let props = BTreeMap::from([("StartEndnoteNumberAt".into(),value)]);
        assert!(ed.apply("invalid",&[RegisteredCommand::SetEndnoteOptions{props}.into()]).is_err());
        assert_eq!(ed.parts(),&after);
    }
}

#[test]
fn navigation_reads_styled_ranges_without_making_them_mutable() {
    let parts = fixture();
    let mut ed = Editor::new(parts.clone());
    let body = &parts["Stories/Story_u12e.xml"];
    let mut targets = Vec::new();
    for at in [30,60] {
        let context = folio_edit::text::note_context(body,"u12e",at).unwrap();
        let range = context["endnoteRange"].as_str().unwrap();
        let target = parts.values().find_map(|p|folio_edit::text::endnote_target(p,range).ok()).unwrap();
        let id = target["story"].as_str().unwrap();
        let at = target["at"].as_u64().unwrap() as usize;
        let text = parts.values().find_map(|p|folio_edit::text::contents(p,id).ok()).unwrap();
        assert_eq!(text.chars().nth(at),Some('\u{fffc}'));
        assert_eq!(text.chars().nth(at-1),Some('\u{feff}'));
        assert!(ed.text_format(id,at,at).is_ok());
        assert!(ed.apply("unsupported edit",&[folio_edit::Command::InsertText{story:id.into(),at,text:"unsafe".into()}]).is_err());
        targets.push((id.to_owned(),at));
    }
    assert_eq!(targets[0].0,targets[1].0);
    assert_eq!(targets[0].1,10);
    assert!(targets[1].1>targets[0].1);
    assert_eq!(ed.parts(),&parts,"queries and rejected edits preserve every original byte");
    assert!(folio_edit::text::note_context(body,"u12e",40).unwrap()["endnoteRange"].is_null());
}
