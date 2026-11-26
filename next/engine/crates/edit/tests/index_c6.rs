//! Index cross references (index-c6): commands, undo, the IDML Folio reads back, what deleting or renaming a topic does to them.
//! InDesign's own answers (the printed "See also" lines, the order, the refusals) are checked by the oracle (`index6-see-*` scenarios).
use folio_edit::commands::RegisteredCommand as R;
use folio_edit::Editor;
use std::collections::BTreeMap;
use std::path::Path;

fn fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/panels-index.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    out
}

fn read(ed: &Editor) -> folio_model::Document {
    folio_idml::read_parts(ed.parts(), Path::new("index.idml"), &folio_idml::ReaderData::default()).unwrap()
}

fn topic(parent: &str, name: &str) -> folio_edit::Command {
    R::AddIndexTopic { parent: parent.into(), name: name.into(), sort_order: String::new() }.into()
}

fn cross(topic: &str, referenced: &str, kind: &str, custom: &str) -> folio_edit::Command {
    R::AddIndexCrossReference { topic: topic.into(), referenced: referenced.into(), kind: kind.into(), custom: custom.into() }.into()
}

/// (topic name, kind, custom, referenced topic id) of every cross reference.
fn crosses(ed: &Editor) -> Vec<(String, String, String, String)> {
    fn walk(t: &folio_model::indexes::Topic, out: &mut Vec<(String, String, String, String)>) {
        for c in &t.cross_references {
            out.push((t.name.clone(), c.kind.clone(), c.custom.clone(), c.referenced.clone()));
        }
        for s in &t.topics {
            walk(s, out);
        }
    }
    let mut out = Vec::new();
    for i in &read(ed).indexes {
        for t in &i.topics {
            walk(t, &mut out);
        }
    }
    out
}

fn ids(ed: &mut Editor) -> (String, String, String) {
    let mut id = |c: folio_edit::Command| ed.apply("topic", &[c]).unwrap().created.last().cloned().unwrap();
    (id(topic("", "Alpha")), id(topic("", "Beta")), id(topic("", "Gamma")))
}

#[test]
fn a_cross_reference_is_added_read_back_and_undone() {
    let mut ed = Editor::new(fixture());
    let before = ed.parts().clone();
    let (alpha, beta, _) = ids(&mut ed);
    let after_topics = ed.parts().clone();
    let r = ed.apply("see", &[cross(&alpha, &beta, "SeeAlso", "")]).unwrap();
    assert_eq!(r.created.len(), 1);
    assert_eq!(crosses(&ed), [("Alpha".into(), "SeeAlso".into(), String::new(), beta.clone())]);
    // the custom kinds carry their text (CustomCrossReference is stored as Before), the others none
    ed.apply("see", &[cross(&alpha, &beta, "CustomCrossReference", "Compare")]).unwrap();
    ed.apply("see", &[cross(&alpha, &beta, "SeeHerein", "ignored")]).unwrap();
    let all = crosses(&ed);
    assert_eq!(all[1].1, "CustomCrossReferenceBefore");
    assert_eq!(all[1].2, "Compare");
    assert_eq!(all[2].2, "");
    ed.undo();
    ed.undo();
    ed.undo();
    assert_eq!(ed.parts(), &after_topics);
    ed.undo();
    ed.undo();
    ed.undo();
    assert_eq!(ed.parts(), &before);
}

#[test]
fn what_cannot_be_added_is_refused_and_leaves_the_document_alone() {
    let mut ed = Editor::new(fixture());
    let (alpha, beta, _) = ids(&mut ed);
    ed.apply("see", &[cross(&alpha, &beta, "See", "")]).unwrap();
    let kept = ed.parts().clone();
    for (label, command) in [
        ("the same kind to the same topic again (InDesign refuses it)", cross(&alpha, &beta, "See", "")),
        ("itself", cross(&alpha, &alpha, "See", "")),
        ("an unknown kind", cross(&alpha, &beta, "Elsewhere", "")),
        ("a custom kind without its text", cross(&alpha, &beta, "CustomCrossReferenceAfter", " ")),
        ("a missing topic", cross(&alpha, "nothing", "See", "")),
    ] {
        assert!(ed.apply("see", &[command]).is_err(), "{label}");
        assert_eq!(ed.parts(), &kept, "{label}");
    }
    // the same custom words twice are refused, other words are another cross reference
    ed.apply("see", &[cross(&alpha, &beta, "CustomCrossReferenceAfter", "x")]).unwrap();
    assert!(ed.apply("see", &[cross(&alpha, &beta, "CustomCrossReferenceAfter", "x")]).is_err());
    ed.apply("see", &[cross(&alpha, &beta, "CustomCrossReferenceAfter", "y")]).unwrap();
}

#[test]
fn a_cross_reference_follows_a_rename_and_goes_with_the_topic_it_points_to() {
    let mut ed = Editor::new(fixture());
    let (alpha, beta, gamma) = ids(&mut ed);
    ed.apply("see", &[cross(&alpha, &beta, "SeeAlso", ""), cross(&alpha, &gamma, "See", ""), cross(&gamma, &beta, "See", "")]).unwrap();
    ed.apply("rename", &[R::SetIndexTopic { id: beta.clone(), name: "Beta2".into(), sort_order: String::new() }.into()]).unwrap();
    let renamed = crosses(&ed);
    assert!(renamed.iter().filter(|c| c.3.ends_with("Beta2")).count() == 2, "{renamed:?}");
    // deleting the topic pointed to deletes the cross references to it (measured: index6-see-more); the others stay
    let beta2 = renamed.iter().find(|c| c.3.ends_with("Beta2")).unwrap().3.clone();
    let before = ed.parts().clone();
    ed.apply("delete", &[R::DeleteIndexTopic { id: beta2 }.into()]).unwrap();
    assert_eq!(crosses(&ed), [("Alpha".into(), "See".into(), String::new(), gamma.clone())]);
    ed.undo();
    assert_eq!(ed.parts(), &before);
}

#[test]
fn a_cross_reference_can_be_deleted_by_its_id_and_a_subtopic_goes_before_the_cross_references() {
    let mut ed = Editor::new(fixture());
    let (alpha, beta, _) = ids(&mut ed);
    let see = ed.apply("see", &[cross(&alpha, &beta, "See", "")]).unwrap().created[0].clone();
    // a subtopic added after a cross reference is written before it (measured: index6-see-more)
    ed.apply("child", &[topic(&alpha, "Child")]).unwrap();
    let designmap = &ed.parts()["designmap.xml"];
    assert!(designmap.find("Name=\"Child\"").unwrap() < designmap.find("<CrossReference ").unwrap(), "{designmap}");
    assert!(ed.apply("delete", &[R::DeleteIndexCrossReference { id: beta.clone() }.into()]).is_err());
    ed.apply("delete", &[R::DeleteIndexCrossReference { id: see }.into()]).unwrap();
    assert!(crosses(&ed).is_empty());
}

fn index_c3() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/index-c3.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    out
}

fn generated(ed: &mut Editor) -> Result<String, String> {
    static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
    let doc = read(ed);
    let layout = folio_text::layout_document(&doc, FONTS.get_or_init(|| folio_text::FontStore::discover(&[])));
    let locations = folio_text::layout::toc::locations(&doc, &layout);
    let make = R::GenerateIndex { page: String::new(), frame: String::new(), replace: true, title: "Index".into(), headings: true, font: "Malgun Gothic".into(), locations, include_book_documents: None, book_documents: None };
    let result = ed.apply("generate", &[make.into()]).map_err(|e| e.to_string())?;
    let story = result.created.last().cloned().ok_or("no story")?;
    let doc = read(ed);
    Ok(doc.stories[&story].paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\r"))
}

/// A marker in a footnote is on the page of the place the footnote is anchored at (index-c6: approximate, not measured in InDesign).
#[test]
fn a_marker_in_a_footnote_is_on_the_page_of_the_footnote() {
    let mut parts = index_c3();
    let designmap = parts.get_mut("designmap.xml").unwrap();
    *designmap = designmap.replace("<Index Self=\"u1e7\" />", "<Index Self=\"u1e7\"><Topic Self=\"u1e7TopicnFoot\" SortOrder=\"\" Name=\"Foot\" /></Index>");
    let story = parts.get_mut("Stories/Story_uf4.xml").unwrap();
    let note = "<Footnote Self=\"ufn1\"><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>note</Content><PageReference Self=\"ufnref\" PageReferenceType=\"CurrentPage\" ReferencedTopic=\"u1e7TopicnFoot\" Id=\"1\" /></CharacterStyleRange></ParagraphStyleRange></Footnote>";
    assert!(story.contains("<Content>Gamma three.</Content>"));
    *story = story.replace("<Content>Gamma three.</Content>", &format!("{note}<Content>Gamma three.</Content>"));
    let mut ed = Editor::new(parts);
    let doc = read(&ed);
    let r = &doc.indexes[0].topics[0].references[0];
    assert!(r.nested && r.anchor == 21, "{r:?}");
    // the paragraph "Gamma three." starts page 2 (measured in the oracle: a reference at 21 is on page 2)
    let text = generated(&mut ed).unwrap();
    assert!(text.contains("Foot  2"), "{text:?}");
    // a range from inside a footnote is refused, not guessed
    let ranged = ed.parts()["Stories/Story_uf4.xml"].replace("PageReferenceType=\"CurrentPage\" ReferencedTopic=\"u1e7TopicnFoot\" Id=\"1\"", "PageReferenceType=\"ToEndOfStory\" ReferencedTopic=\"u1e7TopicnFoot\" Id=\"1\"");
    let mut parts = ed.parts().clone();
    parts.insert("Stories/Story_uf4.xml".into(), ranged);
    assert!(generated(&mut Editor::new(parts)).unwrap_err().contains("현재 쪽"));
}
