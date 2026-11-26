use folio_edit::{Editor, Command, commands::RegisteredCommand as Toc};
use folio_model::toc::{TocStyle, TocEntry, TocSource};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> BTreeMap<String,String> {
    let file=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/text.idml");
    let mut zip=zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
    let mut out=BTreeMap::new();
    for n in 0..zip.len() {let mut p=zip.by_index(n).unwrap();if p.is_dir(){continue}let mut t=String::new();p.read_to_string(&mut t).unwrap();out.insert(p.name().unwrap().into_owned(),t);}
    out
}
fn doc(ed:&Editor)->folio_model::Document {folio_idml::read_parts(ed.parts(),Path::new("toc.idml"),&Default::default()).unwrap()}
fn setup()->(Editor,String,TocSource) {
    let mut ed=Editor::new(fixture());
    let d=doc(&ed);
    let story=d.stories.values().find(|s|s.paragraphs.iter().any(|p|p.runs.iter().any(|r|!r.text.is_empty()))).unwrap();
    let style=TocStyle {name:"Contents".into(),title:"Contents & index".into(),
        entries:vec![TocEntry {name:story.paragraphs[0].style.clone(),..Default::default()}],..Default::default()};
    let source=TocSource {story:story.id.clone(),paragraph:0,page:"iv".into(),page_index:0,hidden:false,number:String::new(),book:None};
    ed.apply("style",&[Toc::SaveTOCStyle{style}.into()]).unwrap();
    let id=ed.toc_styles().unwrap().into_iter().find(|s|s.name=="Contents").unwrap().id;
    (ed,id,source)
}

#[test]
fn editing_empty_default_style_keeps_entries_inside_style() {
    let mut ed=Editor::new(fixture());
    let mut style=ed.toc_styles().unwrap().remove(0);
    style.title_style="ParagraphStyle/Text Body".into();
    style.entries.push(TocEntry{name:"Text Body".into(),..Default::default()});
    ed.apply("default style",&[Toc::SaveTOCStyle{style:style.clone()}.into()]).unwrap();
    let read=ed.toc_styles().unwrap().remove(0);
    assert_eq!(read,style);
    assert_eq!(doc(&ed).toc_styles[0],style);
}

#[test]
fn create_update_reopen_undo_and_redo() {
    let (mut ed,id,source)=setup();
    let before=ed.parts().clone();
    let made=ed.apply("create",&[Toc::CreateTOC{style:id.clone(),frame:String::new(),replace:false,sources:vec![source.clone()]}.into()]).unwrap();
    assert_eq!(made.created.len(),1);
    let d=doc(&ed);
    let story=d.toc_stories.keys().next().unwrap().clone();
    let text=|d:&folio_model::Document|d.stories[&story].paragraphs.iter().flat_map(|p|&p.runs).map(|r|r.text.as_str()).collect::<String>();
    assert!(text(&d).starts_with("Contents & index"));assert!(text(&d).ends_with("\tiv"));
    let created=ed.parts().clone();ed.undo().unwrap();assert_eq!(ed.parts(),&before);ed.redo().unwrap();assert_eq!(ed.parts(),&created);
    ed.apply("heading",&[Command::InsertText{story:source.story.clone(),at:0,text:"Changed ".into()}]).unwrap();
    let mut moved=source;moved.page="12".into();
    let before_update=ed.parts().clone();
    ed.apply("update",&[Toc::UpdateTOC{story:story.clone(),sources:vec![moved]}.into()]).unwrap();
    let reopened=Editor::new(ed.parts().clone());
    let d=doc(&reopened);assert_eq!(d.toc_stories.len(),1);assert!(text(&d).contains("Changed "));assert!(text(&d).ends_with("\t12"));
    ed.undo().unwrap();assert_eq!(ed.parts(),&before_update);
    assert!(ed.apply("delete in use",&[Toc::DeleteTOCStyle{id}.into()]).is_err());
}

#[test]
fn style_roundtrip_preserves_unknown_fields_and_validation_is_atomic() {
    let (ed,id,_)=setup();let mut parts=ed.parts().clone();
    for t in parts.values_mut(){*t=t.replace("<TOCStyle ","<TOCStyle FutureSetting=\"keep\" ").replace("<TOCStyleEntry ","<TOCStyleEntry FutureEntry=\"keep\" ");}
    let mut ed=Editor::new(parts);let mut style=ed.toc_styles().unwrap().into_iter().find(|s|s.id==id).unwrap();
    style.title="New title".into();ed.apply("change",&[Toc::SaveTOCStyle{style:style.clone()}.into()]).unwrap();
    assert!(ed.parts().values().any(|t|t.contains("FutureSetting=\"keep\"")&&t.contains("FutureEntry=\"keep\"")));
    let before=ed.parts().clone();style.entries[0].level=11;
    assert!(ed.apply("invalid",&[Toc::SaveTOCStyle{style}.into()]).is_err());assert_eq!(ed.parts(),&before);
    ed.apply("delete",&[Toc::DeleteTOCStyle{id}.into()]).unwrap();assert!(!ed.toc_styles().unwrap().iter().any(|s|s.name=="Contents"));
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);
}

#[test]
fn import_preserves_extensions_dependencies_and_unique_names() {
    let (source,id,_)=setup();
    let included=source.toc_styles().unwrap().into_iter().find(|s|s.id==id).unwrap().entries[0].name.clone();
    let mut parts=source.parts().clone();
    let styles=parts.get_mut("Resources/Styles.xml").unwrap();
    *styles=styles.replace(&included,"Imported Body").replace("<TOCStyle ","<TOCStyle FutureSetting=\"keep\" ");
    let (mut ed,_,_)=setup();
    let before=ed.parts().clone();
    ed.apply("import",&[Toc::ImportTOCStyles{parts}.into()]).unwrap();
    let imported=ed.toc_styles().unwrap().into_iter().find(|s|s.name=="Contents 2").unwrap();
    assert_eq!(imported.entries[0].name,"Imported Body");
    assert!(doc(&ed).styles.paragraph.iter().any(|s|s.name=="Imported Body"));
    assert!(ed.parts()["Resources/Styles.xml"].contains("FutureSetting=\"keep\""));
    let after=ed.parts().clone();ed.undo().unwrap();assert_eq!(ed.parts(),&before);
    ed.redo().unwrap();assert_eq!(ed.parts(),&after);
    assert!(ed.apply("empty import",&[Toc::ImportTOCStyles{parts:BTreeMap::new()}.into()]).is_err());
    assert_eq!(ed.parts(),&after);
}

#[test]
fn update_preserves_shared_destinations_and_unrelated_child_bookmarks() {
    let (mut ed, id, source) = setup();
    ed.apply("create", &[Toc::CreateTOC {style:id, frame:String::new(), replace:false, sources:vec![source.clone()]}.into()]).unwrap();
    let story = doc(&ed).toc_stories.keys().next().unwrap().clone();
    let mut parts = ed.parts().clone();
    let map = parts.get_mut("designmap.xml").unwrap();
    let parsed = folio_edit::xml::Index::scan(map).unwrap();
    let dest = (0..parsed.nodes.len()).find(|&i|parsed.local(map,i)=="HyperlinkPageDestination").unwrap();
    let destination = parsed.attr(map,dest,"Self").unwrap().to_owned();
    let bookmark = (0..parsed.nodes.len()).find(|&i|parsed.local(map,i)=="Bookmark").unwrap();
    let at = parsed.nodes[bookmark].open_end;
    map.insert_str(at, "<Bookmark Self=\"unrelated-bookmark\" Name=\"Keep child\" Destination=\"unrelated-destination\"/>");
    let at = map.rfind("</Document>").unwrap();
    map.insert_str(at, &format!("<Hyperlink Self=\"unrelated-link\" Source=\"unrelated-source\"><Properties><Destination type=\"object\">{destination}</Destination></Properties></Hyperlink>"));
    let mut ed = Editor::new(parts);
    ed.apply("update", &[Toc::UpdateTOC {story, sources:vec![source]}.into()]).unwrap();
    let map = &ed.parts()["designmap.xml"];
    let parsed = folio_edit::xml::Index::scan(map).unwrap();
    for id in [&destination, "unrelated-bookmark", "unrelated-link"] {
        assert!((0..parsed.nodes.len()).any(|i|parsed.attr(map,i,"Self")==Some(id)), "lost {id}");
    }
    assert_eq!((0..parsed.nodes.len()).filter(|&i|parsed.local(map,i)=="Hyperlink").count(), 2);
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn replace_two_tocs_keeps_measured_story_identity() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn replace_two_tocs_keeps_measured_story_identity() {
    use serde_json::Value;

    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../../cleanroom/vectors/B6/toc-replace-measured.json"
    )).unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        assert_eq!(case["source"], "measured");
        let input = &case["input"];
        let expected = &case["output"];
        let (mut ed, style, source) = setup();
        let change_heading = |ed: &mut Editor, heading: &str| {
            let count = doc(ed).stories[&source.story].paragraphs[0].runs.iter()
                .map(|run| run.text.chars().count()).sum::<usize>();
            ed.apply("heading", &[
                Command::InsertText { story: source.story.clone(), at: 0, text: heading.into() },
                Command::DeleteText {
                    story: source.story.clone(),
                    start: heading.chars().count(),
                    end: heading.chars().count() + count,
                },
            ]).unwrap();
        };
        let create = |ed: &mut Editor, frame: String, replace| ed.apply("create", &[Toc::CreateTOC {
            style: style.clone(), frame, replace, sources: vec![source.clone()],
        }.into()]);
        let contents = |d: &folio_model::Document, story: &str| {
            d.stories[story].paragraphs.iter().flat_map(|p| &p.runs)
                .map(|r| r.text.as_str()).collect::<String>()
        };
        let original = input["originalHeading"].as_str().unwrap();
        let revised = input["revisedHeading"].as_str().unwrap();
        change_heading(&mut ed, original);
        let mut targets = Vec::new();
        for _ in 0..input["existingTocsWithSameStyle"].as_u64().unwrap() {
            let outcome = create(&mut ed, String::new(), false).unwrap();
            let story = &outcome.recompose[0];
            let d = doc(&ed);
            assert!(contents(&d, story).contains(original));
            targets.push((outcome.created[0].clone(), story.clone()));
        }
        let count_before = doc(&ed).stories.len() as i64;
        change_heading(&mut ed, revised);
        let replace = input["replaceExisting"].as_bool().unwrap();
        let before = ed.parts().clone();
        assert!(create(&mut ed, String::new(), replace).is_err());
        assert_eq!(ed.parts(), &before);
        for (frame, story) in &targets {
            let result = create(&mut ed, frame.clone(), replace).unwrap();
            assert_eq!(result.recompose == [story.clone()], expected["returnedExistingStory"].as_bool().unwrap());
            assert!(result.created.is_empty());
        }
        let updated = doc(&ed);
        let expected_change = expected["storiesAfter"].as_i64().unwrap()
            - expected["storiesBefore"].as_i64().unwrap();
        assert_eq!(updated.stories.len() as i64 - count_before, expected_change);
        for ((frame, story), label) in targets.iter().zip(["first", "second"]) {
            let mut kept = false;
            for item in updated.spreads.iter().flat_map(|spread| &spread.items) {
                item.walk(&mut |item| {
                    if item.id == *frame && item.text_frame().is_some_and(|text| text.story == *story) {
                        kept = true;
                    }
                });
            }
            assert_eq!(kept, expected[format!("{label}Kept")].as_bool().unwrap());
            assert_eq!(contents(&updated, story).contains(revised), expected[format!("{label}Updated")].as_bool().unwrap());
        }
    }
}
