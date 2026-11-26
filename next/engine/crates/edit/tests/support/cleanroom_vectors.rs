use super::{Editor, Index, Value, labels, oracle, read_zip, run_step, same_value, unescape};
use serde_json::Map;
use std::collections::BTreeSet;

type Row = Map<String, Value>;

fn rows(table: &Value) -> impl Iterator<Item = &Row> {
    table.as_array().into_iter().flatten().map(|row| row.as_object().unwrap())
}

fn attribute(index: &Index, xml: &str, node: usize, key: &str) -> Option<String> {
    let text = |node: usize| {
        let element = &index.nodes[node];
        &xml[element.open_end..element.close_start]
    };
    match key {
        "value" => Some(unescape(text(node).trim())),
        "content" => Some(index.named(xml, node, "Content").iter().map(|&child| unescape(text(child))).collect()),
        _ => index.value(xml, node, key),
    }
}

fn same_field(index: &Index, xml: &str, node: usize, key: &str, expected: Option<&Value>) -> bool {
    match (attribute(index, xml, node, key), expected) {
        (Some(actual), Some(value)) => same_value(&actual, value.as_str().unwrap()),
        (None, None) => true,
        _ => false,
    }
}

fn check_part(before: &Value, after: &Value, old_xml: &str, new_xml: &str, context: &str) {
    let old_index = Index::scan(old_xml).unwrap();
    let new_index = Index::scan(new_xml).unwrap();
    let old_rows: Vec<_> = rows(before).collect();
    let new_rows: Vec<_> = rows(after).collect();
    let mut matched = BTreeSet::new();
    for row in new_rows.iter().copied().filter(|row| !old_rows.contains(row)) {
        let tag = row["element"].as_str().unwrap();
        let id = row.get("id").and_then(Value::as_str);
        let previous = id.and_then(|id| old_rows.iter().copied().find(|old| old.get("id").and_then(Value::as_str) == Some(id)));
        let fields: BTreeSet<_> = row.keys().chain(previous.into_iter().flat_map(Row::keys))
            .filter(|key| !matches!(key.as_str(), "element" | "id"))
            .filter(|key| previous.and_then(|old| old.get(*key)) != row.get(*key))
            .collect();
        let candidates: Vec<_> = (0..new_index.nodes.len()).filter(|&node| {
            if new_index.local(new_xml, node) != tag { return false; }
            if let Some(id) = id {
                let actual = new_index.value(new_xml, node, "Self");
                if tag == "Group" && previous.is_none() {
                    return actual.is_some_and(|actual| old_index.by_self(old_xml, &actual).is_none());
                }
                return actual.as_deref() == Some(id);
            }
            true
        }).collect();
        let node = candidates.iter().copied().find(|node| {
            !matched.contains(node) && fields.iter().all(|key| same_field(&new_index, new_xml, *node, key, row.get(*key)))
        });
        let node = node.unwrap_or_else(|| {
            let actual: Vec<_> = candidates.iter().map(|&node| fields.iter().map(|key| (key, attribute(&new_index, new_xml, node, key))).collect::<Vec<_>>()).collect();
            panic!("{context}: expected {row:?}, checked {fields:?}, found {actual:?}");
        });
        matched.insert(node);
    }
}

fn replay_vectors(json: &str) {
    let vectors: Value = serde_json::from_str(json).unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    let only = std::env::var("FOLIO_ORACLE_ONLY").ok();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let (_, name) = id.split_once('/').unwrap();
        if only.as_ref().is_some_and(|names| !names.split(',').any(|selected| selected == name)) {
            continue;
        }
        assert_eq!(case["source"], "oracle", "{id}");
        let input = &case["input"];
        let fixture = input["fixture"].as_str().unwrap();
        let base = read_zip(&oracle().join("fixtures").join(format!("{fixture}.idml")));
        let mut editor = Editor::new(base.clone());
        for step in input["steps"].as_array().unwrap() {
            let step_before = editor.parts().clone();
            let ids = labels(&step_before);
            run_step(&mut editor, step, &ids, &step_before)
                .unwrap_or_else(|error| panic!("{id}: {step}: {error}"));
        }
        for (part, after) in case["output"]["projectedAfter"].as_object().unwrap() {
            let original = base.get(part).map_or("", String::as_str);
            let edited = editor.parts().get(part).unwrap_or_else(|| panic!("{id}: missing {part}"));
            check_part(&input["projectedBefore"][part], after, original, edited, &format!("{id} {part}"));
        }
        let edited = editor.parts().clone();
        let undo_steps = editor.history().0.len();
        for _ in 0..undo_steps { editor.undo().unwrap(); }
        assert_eq!(editor.parts(), &base, "{id}: undo");
        for _ in 0..undo_steps { editor.redo().unwrap(); }
        assert_eq!(editor.parts(), &edited, "{id}: redo");
    }
}

fn replay_previous_unit(relative: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let file = root.join("cleanroom/vectors").join(relative);
    match std::fs::read_to_string(&file) {
        Ok(json) => replay_vectors(&json),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
            && file.parent().unwrap().symlink_metadata()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            && std::env::var("FOLIO_ORACLE_ONLY").is_ok_and(|names| names.split(',').all(|name| {
                name.starts_with("cell2-direction-") || name.starts_with("toc-")
                    || name == "textpath-delete-threaded"
            })) => (),
        Err(error) => panic!("{}: {error}", file.display()),
    }
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn corner_attributes_match_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn corner_attributes_match_cleanroom_vectors() {
    replay_previous_unit("B1/corner-oracle.json");
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn object_styles_match_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn object_styles_match_cleanroom_vectors() {
    replay_previous_unit("B2/objectstyle-oracle.json");
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn character_gradients_match_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn character_gradients_match_cleanroom_vectors() {
    replay_previous_unit("B3/character-paint-oracle.json");
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn paragraph_rule_idml_matches_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn paragraph_rule_idml_matches_cleanroom_vectors() {
    replay_previous_unit("B4/paragraph-rules-oracle.json");
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn cell_direction_idml_matches_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn cell_direction_idml_matches_cleanroom_vectors() {
    replay_vectors(include_str!("../../../../../../cleanroom/vectors/B5/cell-direction-oracle.json"));
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn text_path_removal_idml_matches_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn text_path_removal_idml_matches_cleanroom_vectors() {
    replay_vectors(include_str!("../../../../../../cleanroom/vectors/B8/thread-edit-oracle.json"));
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn generated_toc_matches_cleanroom_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn generated_toc_matches_cleanroom_vectors() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../../../cleanroom/vectors/B6/toc-oracle.json"
    )).unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    let only = std::env::var("FOLIO_ORACLE_ONLY").ok();
    let root = oracle();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let (_, name) = id.split_once('/').unwrap();
        if only.as_ref().is_some_and(|names| !names.split(',').any(|selected| selected == name)) {
            continue;
        }
        assert_eq!(case["source"], "oracle", "{id}");
        let fixture = case["input"]["fixture"].as_str().unwrap();
        let base = read_zip(&root.join("fixtures").join(format!("{fixture}.idml")));
        let golden = root.join("goldens").join(id);
        let problems = super::toc_oracle::check_with_vector(&base, &golden, Some(case));
        assert!(problems.is_empty(), "{id}: {}", problems.join("\n"));
    }
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn merged_cell_keeps_measured_direction_and_rotation() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn merged_cell_keeps_measured_direction_and_rotation() {
    use folio_edit::{CellChange, Command};

    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../../../cleanroom/vectors/B5/cell-direction-measured.json"
    )).unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        let input = &case["input"];
        let expected = &case["output"];
        let mut ed = Editor::new(read_zip(&oracle().join("fixtures/tables.idml")));
        let frame = labels(ed.parts())["tbl"].clone();
        let story = ed.parts().values().find_map(|xml| {
            let index = Index::scan(xml).ok()?;
            index.value(xml, index.by_self(xml, &frame)?, "ParentStory")
        }).unwrap();
        ed.apply("insert", &[Command::InsertTable {
            story: story.clone(),
            at: Some(0),
            rows: input["rows"].as_u64().unwrap() as u32,
            columns: input["columns"].as_u64().unwrap() as u32,
            header: 0,
            footer: 0,
        }]).unwrap();
        let xml = &ed.parts()[&format!("Stories/Story_{story}.xml")];
        let index = Index::scan(xml).unwrap();
        let table = (0..index.nodes.len())
            .find(|&node| index.local(xml, node) == "Table")
            .and_then(|node| index.value(xml, node, "Self"))
            .unwrap();
        let coordinate = |value: &Value| [
            value[1].as_u64().unwrap() as usize,
            value[0].as_u64().unwrap() as usize,
        ];
        let from = coordinate(&input["merge"][0]);
        let to = coordinate(&input["merge"][1]);
        let neighbor = [to[0] + 1, to[1]];
        ed.apply("merge", &[Command::MergeCells { table: table.clone(), from, to }]).unwrap();

        let cell = |ed: &mut Editor, position: [usize; 2]| {
            ed.table(&table).unwrap()["cells"].as_array().unwrap().iter()
                .find(|cell| cell["cell"] == serde_json::json!(position))
                .unwrap().clone()
        };
        assert_eq!(cell(&mut ed, from)["writingDirection"], expected["mergedDefaultHorizontal"]);
        let directions = input["directionChanges"].as_array().unwrap();
        for (index, direction) in directions.iter().enumerate() {
            ed.apply("direction", &[Command::SetCells {
                table: table.clone(),
                cells: vec![from],
                change: CellChange {
                    writing_direction: Some(direction == "horizontal"),
                    rotation: (index == 0).then(|| input["rotationAfterFirstChange"].as_u64().unwrap() as u16),
                    ..Default::default()
                },
            }]).unwrap();
            ed = Editor::new(ed.parts().clone());
            let merged = cell(&mut ed, from);
            assert_eq!(merged["writingDirection"], direction == "horizontal");
            assert_eq!(merged["rotation"].as_f64(), expected[if index == 0 { "rotation" } else { "rotationAfterHorizontal" }].as_f64());
            assert_eq!(merged["span"][0], expected["columnSpan"]);
            assert_eq!(cell(&mut ed, neighbor)["writingDirection"], expected["neighborHorizontal"]);
            if index == 0 {
                assert_eq!(merged["writingDirection"] == false, expected["mergedVertical"]);
            }
        }
    }
}
