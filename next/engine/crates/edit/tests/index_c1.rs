//! Index topics and page references (index-c1): commands, undo/redo, positions through text edits,
//! and the IDML Folio reads back. InDesign's own answers are checked by the oracle (`index-*` scenarios).
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

fn reference(at: usize, topic: &str, kind: &str, limit: usize) -> folio_edit::Command {
    R::AddIndexReference { story: "uf4".into(), at, topic: topic.into(), kind: kind.into(), limit }.into()
}

fn refs(doc: &folio_model::Document) -> Vec<(String, String, usize)> {
    fn walk(t: &folio_model::indexes::Topic, out: &mut Vec<(String, String, usize)>) {
        for r in &t.references {
            out.push((t.name.clone(), r.kind.clone(), r.at));
        }
        for c in &t.topics {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    for i in &doc.indexes {
        for t in &i.topics {
            walk(t, &mut out);
        }
    }
    out.sort();
    out
}

#[test]
fn marker_is_one_character_and_the_fixture_reads_with_its_position() {
    let ed = Editor::new(fixture());
    let doc = read(&ed);
    // measured in InDesign: "Alpha bold " is 11 characters, the marker is the 12th
    assert_eq!(refs(&doc), [("글꼴".to_string(), "ToEndOfStory".to_string(), 11)]);
    let story = ed.part("Stories/Story_uf4.xml").unwrap();
    assert_eq!(folio_edit::text::contents(story, "uf4").unwrap().chars().nth(11), Some('\u{FEFF}'));
}

#[test]
fn topics_and_references_undo_redo_and_read_back() {
    let mut ed = Editor::new(fixture());
    let before = ed.parts().clone();
    let out = ed.apply("topic", &[topic("", "Alpha")]).unwrap();
    assert_eq!(out.created, ["u13eTopicnAlpha"]);
    // a taken name returns the topic that is there
    let one = ed.parts().clone();
    ed.apply("topic again", &[topic("", "Alpha")]).unwrap();
    assert_eq!(ed.parts(), &one, "adding a taken name changes nothing");
    let out = ed.apply("reference", &[reference(3, "u13eTopicnAlpha", "CurrentPage", 0)]).unwrap();
    assert_eq!(out.created.len(), 1, "the new marker's id is returned");
    let story = ed.part("Stories/Story_uf4.xml").unwrap();
    // inside "Alpha": the marker goes to the start of the word
    assert_eq!(folio_edit::text::contents(story, "uf4").unwrap().chars().next(), Some('\u{FEFF}'));
    assert!(story.contains("PageReferenceType=\"CurrentPage\" ReferencedTopic=\"u13eTopicnAlpha\" Id=\"2\""));
    assert_eq!(refs(&read(&ed)), [("Alpha".into(), "CurrentPage".into(), 0), ("글꼴".into(), "ToEndOfStory".into(), 12)]);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &one);
    ed.redo().unwrap();
    assert_eq!(refs(&read(&ed)).len(), 2);
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
}

#[test]
fn counted_references_keep_their_count_as_a_property_and_read_it() {
    let mut ed = Editor::new(fixture());
    ed.apply("t", &[topic("", "Alpha"), reference(47, "u13eTopicnAlpha", "ForNextNParagraphs", 2)]).unwrap();
    let story = ed.part("Stories/Story_uf4.xml").unwrap();
    assert!(story.contains("<PageReferenceLimit type=\"long\">2</PageReferenceLimit>"));
    let doc = read(&ed);
    let alpha = doc.indexes[0].topics.iter().find(|t| t.name == "Alpha").unwrap();
    assert_eq!((alpha.references[0].kind.as_str(), alpha.references[0].limit.as_str()), ("ForNextNParagraphs", "2"));
    assert!(ed.apply("zero", &[reference(0, "u13eTopicnAlpha", "ForNextNPages", 0)]).is_err());
    assert!(ed.apply("unmeasured", &[reference(0, "u13eTopicnAlpha", "ToNextUseOfStyle", 0)]).is_err());
}

#[test]
fn markers_follow_text_edits_and_leave_with_deleted_text() {
    let mut ed = Editor::new(fixture());
    ed.apply("type", &[folio_edit::Command::InsertText { story: "uf4".into(), at: 3, text: "XY".into() }]).unwrap();
    assert_eq!(refs(&read(&ed))[0].2, 13);
    ed.apply("cut", &[folio_edit::Command::DeleteText { story: "uf4".into(), start: 8, end: 14 }]).unwrap();
    assert!(refs(&read(&ed)).is_empty(), "a marker inside deleted text is deleted with it");
    ed.undo().unwrap();
    assert_eq!(refs(&read(&ed))[0].2, 13);
}

#[test]
fn rename_rewrites_ids_and_references_and_delete_takes_the_markers() {
    let mut ed = Editor::new(fixture());
    ed.apply("rename", &[R::SetIndexTopic { id: "u13eTopicn출판".into(), name: "Publishing".into(), sort_order: String::new() }.into()]).unwrap();
    assert!(ed.part("designmap.xml").unwrap().contains("Self=\"u13eTopicnPublishingTopicn글꼴\""));
    assert!(ed.part("Stories/Story_uf4.xml").unwrap().contains("ReferencedTopic=\"u13eTopicnPublishingTopicn글꼴\""));
    let renamed = ed.parts().clone();
    ed.apply("delete", &[R::DeleteIndexTopic { id: "u13eTopicnPublishing".into() }.into()]).unwrap();
    assert!(refs(&read(&ed)).is_empty());
    assert!(!ed.part("Stories/Story_uf4.xml").unwrap().contains("<PageReference"));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &renamed);
    // move: the reference now belongs to another topic
    ed.apply("topic", &[topic("", "Alpha")]).unwrap();
    let marker = read(&ed).indexes[0].topics.iter().flat_map(|t| t.topics.iter()).flat_map(|t| t.references.iter()).next().unwrap().id.clone();
    ed.apply("move", &[R::MoveIndexReference { id: marker, topic: "u13eTopicnAlpha".into() }.into()]).unwrap();
    assert_eq!(refs(&read(&ed)), [("Alpha".to_string(), "ToEndOfStory".to_string(), 11)]);
}

#[test]
fn read_only_odd_input_is_refused_without_changing_anything() {
    let mut ed = Editor::new(fixture());
    let before = ed.parts().clone();
    assert!(ed.apply("empty", &[topic("", "  ")]).is_err());
    assert!(ed.apply("no topic", &[reference(0, "u13eTopicnNope", "CurrentPage", 0)]).is_err());
    assert!(ed.apply("outside", &[reference(9999, "u13eTopicn출판", "CurrentPage", 0)]).is_err());
    assert!(ed.apply("not a topic", &[R::DeleteIndexTopic { id: "uf4".into() }.into()]).is_err());
    assert_eq!(ed.parts(), &before);
}
