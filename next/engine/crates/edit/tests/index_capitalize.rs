//! Index panel > Capitalize (build-b2): the four scopes as InDesign 2025 did them (DOM probe artifacts/build-b2-20261023/indexpanel/capitalize.json):
//! the first lower-case letter of a name becomes a capital; digits, spaces and Korean stay; one undo step.
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

/// Every topic as its path (`Parent: Child`), sorted.
fn names(ed: &Editor) -> Vec<String> {
    fn walk(t: &folio_model::indexes::Topic, prefix: &str, out: &mut Vec<String>) {
        let path = if prefix.is_empty() { t.name.clone() } else { format!("{prefix}: {}", t.name) };
        out.push(path.clone());
        for s in &t.topics {
            walk(s, &path, out);
        }
    }
    let doc = folio_idml::read_parts(ed.parts(), Path::new("index.idml"), &folio_idml::ReaderData::default()).unwrap();
    let mut out = Vec::new();
    for i in &doc.indexes {
        for t in &i.topics {
            walk(t, "", &mut out);
        }
    }
    out.sort();
    out
}

fn topic(parent: &str, name: &str) -> folio_edit::Command {
    R::AddIndexTopic { parent: parent.into(), name: name.into(), sort_order: String::new() }.into()
}

fn id_of(ed: &Editor, path: &str) -> String {
    fn walk(t: &folio_model::indexes::Topic, prefix: &str, want: &str) -> Option<String> {
        let p = if prefix.is_empty() { t.name.clone() } else { format!("{prefix}: {}", t.name) };
        if p == want { return Some(t.id.clone()); }
        t.topics.iter().find_map(|s| walk(s, &p, want))
    }
    let doc = folio_idml::read_parts(ed.parts(), Path::new("index.idml"), &folio_idml::ReaderData::default()).unwrap();
    doc.indexes.iter().flat_map(|i| &i.topics).find_map(|t| walk(t, "", path)).unwrap_or_else(|| panic!("no topic {path}"))
}

fn setup() -> Editor {
    let mut ed = Editor::new(fixture());
    ed.apply("topics", &[topic("", "apple pie"), topic("", "banana split"), topic("", "1st place"), topic("", "한글 주제")]).unwrap();
    let apple = id_of(&ed, "apple pie");
    ed.apply("sub", &[topic(&apple, "cherry tart")]).unwrap();
    let cherry = id_of(&ed, "apple pie: cherry tart");
    ed.apply("subsub", &[topic(&cherry, "éclair deep")]).unwrap();
    ed
}

fn has(ed: &Editor, want: &[&str], not: &[&str]) {
    let n = names(ed);
    for w in want { assert!(n.iter().any(|x| x == w), "{w} missing in {n:?}"); }
    for w in not { assert!(!n.iter().any(|x| x == w), "{w} should not be in {n:?}"); }
}

#[test]
fn capitalize_scopes_follow_indesign() {
    let mut ed = setup();
    let before = ed.parts().clone();
    let apple = id_of(&ed, "apple pie");
    ed.apply("cap", &[R::CapitalizeIndexTopics { scope: "selected".into(), topic: apple.clone() }.into()]).unwrap();
    has(&ed, &["Apple pie", "Apple pie: cherry tart", "banana split"], &["apple pie"]);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);

    ed.apply("cap", &[R::CapitalizeIndexTopics { scope: "subentries".into(), topic: apple }.into()]).unwrap();
    has(&ed, &["Apple pie: Cherry tart: Éclair deep", "banana split"], &[]);
    ed.undo().unwrap();

    ed.apply("cap", &[R::CapitalizeIndexTopics { scope: "level1".into(), topic: String::new() }.into()]).unwrap();
    has(&ed, &["Apple pie: cherry tart: éclair deep", "Banana split", "1st place", "한글 주제"], &[]);
    ed.undo().unwrap();

    ed.apply("cap", &[R::CapitalizeIndexTopics { scope: "all".into(), topic: String::new() }.into()]).unwrap();
    has(&ed, &["Apple pie: Cherry tart: Éclair deep", "Banana split", "1st place", "한글 주제"], &[]);
}
