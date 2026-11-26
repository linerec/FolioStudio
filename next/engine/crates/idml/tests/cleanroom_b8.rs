use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use folio_idml::{ReadBase, ReaderData, read_parts};
use folio_model::Document;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
struct Frame {
    id: String,
    next: Option<String>,
    on_path: bool,
    previous: Option<String>,
    story: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    before: Vec<Frame>,
    new_read: Vec<Frame>,
    updated: bool,
    incremental: Vec<Frame>,
}

#[derive(Deserialize)]
struct Edit {
    part: String,
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct Input {
    fixture: String,
    edits: Vec<Edit>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    input: Input,
    output: Expected,
}

fn frame_links(doc: &Document) -> Vec<Frame> {
    let mut frames = Vec::new();
    for item in doc.spreads.iter().flat_map(|spread| &spread.items) {
        item.walk(&mut |item| {
            if let Some(text) = item.text_frame() {
                frames.push(Frame {
                    id: item.id.clone(),
                    next: text.next.clone(),
                    on_path: item.text_path.is_some(),
                    previous: text.previous.clone(),
                    story: text.story.clone(),
                });
            }
        });
    }
    frames.sort();
    frames
}

fn xml_parts(path: &Path) -> BTreeMap<String, String> {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        if name.ends_with(".xml") {
            let mut xml = String::new();
            entry.read_to_string(&mut xml).unwrap();
            parts.insert(name, xml.trim_start_matches('\u{feff}').to_owned());
        }
    }
    parts
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn full_and_incremental_text_path_reads_follow_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn full_and_incremental_text_path_reads_follow_vectors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let vectors = include_str!("../../../../../cleanroom/vectors/B8/thread-observed.json");
    #[derive(Deserialize)]
    struct Vectors {
        cases: Vec<Case>,
    }

    for case in serde_json::from_str::<Vectors>(vectors).unwrap().cases {
        let path = root.join(case.input.fixture);
        let mut parts = xml_parts(&path);
        let data = ReaderData::default();
        let mut base = ReadBase::read(&parts, &path, &data).unwrap();
        assert_eq!(frame_links(base.doc()), case.output.before, "{}", case.id);

        let mut changed = BTreeSet::new();
        for edit in case.input.edits {
            let xml = parts.get_mut(&edit.part).unwrap();
            assert_eq!(xml.matches(&edit.from).count(), 1, "{}", case.id);
            *xml = xml.replacen(&edit.from, &edit.to, 1);
            changed.insert(edit.part);
        }

        let full = read_parts(&parts, &path, &data).unwrap();
        assert_eq!(frame_links(&full), case.output.new_read, "{}", case.id);
        let updated = base.update(&parts, &path, &data, &changed).is_ok();
        assert_eq!(updated, case.output.updated, "{}", case.id);
        assert_eq!(
            frame_links(base.doc()),
            case.output.incremental,
            "{}",
            case.id
        );
    }
}
