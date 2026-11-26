use folio_text::layout::listconvert;
use std::path::Path;

fn document(case: &str) -> folio_model::Document {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../oracle/fixtures/bullets2-{case}.idml"));
    folio_idml::read_package(&path, None).unwrap()
}

#[test]
fn measured_story_order() {
    for (case, expected) in [
        ("duplicate", vec![("A1", vec![1, 2]), ("B1", vec![3, 4]), ("A1", vec![5, 6])]),
        ("anchor", vec![("A1", vec![2, 3]), ("B1", vec![1, 4])]),
        ("parent", vec![("A1", vec![1, 2]), ("B1", vec![3, 4]), ("M1", vec![1, 2])]),
        ("page-order", vec![("A1", vec![3, 4]), ("B1", vec![1, 2])]),
    ] {
        let doc = document(case);
        let mut actual = Vec::new();
        for id in &doc.story_order {
            let Some(story) = doc.stories.get(id) else {continue};
            let text: String = story.paragraphs.iter().flat_map(|p| &p.runs).map(|r|r.text.as_str()).collect();
            if let Some((name, _)) = expected.iter().find(|(name, _)|text.contains(name)) {
                let labels = listconvert::labels(&doc, id).unwrap();
                actual.push((*name, labels.values().map(|s| s.split('.').next().unwrap().parse::<u32>().unwrap()).collect::<Vec<_>>()));
            }
        }
        assert_eq!(actual, expected, "{case}");
    }
}

#[test]
fn measured_cells_share_story_counter_without_rotation_double_count() {
    for case in ["cells-default", "cells-rotated"] {
        let doc = document(case);
        let story = doc.stories.values().find(|s|s.all_runs().into_iter().any(|r|!r.tables.is_empty())).unwrap();
        let labels = listconvert::labels(&doc, &story.id).unwrap();
        assert_eq!(labels.values().collect::<Vec<_>>(), vec!["1.\t"], "{case}");
        let table = story.all_runs().into_iter().flat_map(|r|&r.tables).next().unwrap();
        let mut cells: Vec<_> = table.cells.iter().collect();
        cells.sort_by_key(|c|(c.row,c.column));
        for (n,cell) in cells.iter().enumerate() {
            assert_eq!(listconvert::labels(&doc,&cell.id).unwrap().values().cloned().collect::<Vec<_>>(),
                vec![format!("{}.\t", n*2+2),format!("{}.\t",n*2+3)], "{case} cell {n}");
        }
    }
}

#[test]
fn measured_text_before_table_consumes_paragraph_number() {
    let mut doc = document("cells-default");
    let story = doc.stories.values_mut().find(|s|s.all_runs().into_iter().any(|r|!r.tables.is_empty())).unwrap();
    let id = story.id.clone();
    let paragraph = story.paragraphs.iter_mut().find(|p|p.runs.iter().any(|r|!r.tables.is_empty())).unwrap();
    paragraph.runs.first_mut().unwrap().text.insert_str(0, "Two");
    let labels = listconvert::labels(&doc, &id).unwrap();
    assert_eq!(labels.values().collect::<Vec<_>>(), vec!["1.\t", "2.\t"]);
    let table = doc.stories[&id].all_runs().into_iter().flat_map(|r|&r.tables).next().unwrap();
    let cell = table.cells.iter().min_by_key(|c|(c.row,c.column)).unwrap();
    assert_eq!(listconvert::labels(&doc,&cell.id).unwrap().values().cloned().collect::<Vec<_>>(), vec!["3.\t", "4.\t"]);
}
