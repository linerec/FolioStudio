use folio_edit::{Editor, commands::RegisteredCommand, xml::Index};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const SPREAD: &str = "Spreads/Spread_s.xml";
const STORY: &str = "Stories/Story_story.xml";

fn links(parts: &BTreeMap<String, String>) -> Value {
    let xml = &parts[SPREAD];
    let ix = Index::scan(xml).unwrap();
    let frames: Vec<_> = (0..ix.nodes.len())
        .filter(|&i| matches!(ix.local(xml, i), "TextFrame" | "TextPath"))
        .collect();
    let owner = |raw: &str| raw.strip_prefix("text-on-").unwrap_or(raw).to_owned();
    let edge = |i, key| ix.attr(xml, i, key).filter(|&id| id != "n").map(owner);
    let mut neighbors = BTreeMap::new();
    for i in frames {
        let id = owner(ix.attr(xml, i, "Self").unwrap());
        let previous = edge(i, "PreviousTextFrame");
        let next = edge(i, "NextTextFrame");
        neighbors.insert(id, (previous, next));
    }
    let mut order = Vec::new();
    let mut current = neighbors
        .iter()
        .find(|(_, (previous, _))| previous.is_none())
        .map(|(id, _)| id);
    while let Some(id) = current {
        assert!(order.len() < neighbors.len(), "cycle at {id}");
        order.push(id);
        current = neighbors[id].1.as_ref();
    }
    assert_eq!(order.len(), neighbors.len());
    let edges: BTreeMap<_, _> = neighbors
        .iter()
        .map(|(id, (previous, next))| (id, json!({ "previous": previous, "next": next })))
        .collect();
    json!({ "order": order, "links": edges })
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn removal_keeps_the_measured_chain_and_story() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn removal_keeps_the_measured_chain_and_story() {
    let vectors: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../../cleanroom/vectors/B8/cr-02.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 15);
    for case in vectors {
        let input = &case["input"];
        let expected = &case["output"];
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["source"], "measured");
        let removed = input["removeFromPath"].as_str().unwrap();
        let chain = &input["chain"];
        let order = chain["order"].as_array().unwrap();
        let path_ids = [
            removed,
            order
                .iter()
                .find_map(|v| v.as_str().filter(|&id| id != removed))
                .unwrap(),
        ];
        let raw_id = |id: &str| {
            if path_ids.contains(&id) {
                format!("text-on-{id}")
            } else {
                id.to_owned()
            }
        };
        let mut spread = String::from("<Spread Self=\"s\">");
        for id in order.iter().rev().map(|value| value.as_str().unwrap()) {
            let edge = |key| {
                chain["links"][id][key]
                    .as_str()
                    .map_or_else(|| "n".into(), raw_id)
            };
            let previous = edge("previous");
            let next = edge("next");
            let raw = raw_id(id);
            let attributes = format!(
                "Self=\"{raw}\" ParentStory=\"story\" PreviousTextFrame=\"{previous}\" NextTextFrame=\"{next}\""
            );
            if path_ids.contains(&id) {
                write!(
                    spread,
                    "<GraphicLine Self=\"{id}\"><TextPath {attributes}/></GraphicLine>"
                )
                .unwrap();
            } else {
                write!(spread, "<TextFrame {attributes}/>").unwrap();
            }
        }
        spread.push_str("</Spread>");
        let story = format!(
            "<Story Self=\"story\"><ParagraphStyleRange><CharacterStyleRange><Content>{}</Content></CharacterStyleRange></ParagraphStyleRange></Story>",
            folio_edit::xml::escape(input["storyText"].as_str().unwrap()),
        );
        let mut editor = Editor::new(BTreeMap::from([
            ("designmap.xml".into(), "<Document/>".into()),
            (SPREAD.into(), spread),
            (STORY.into(), story.clone()),
        ]));
        let shape_count = |xml: &str| xml.matches("<GraphicLine ").count();
        let before = shape_count(&editor.parts()[SPREAD]);
        assert_eq!(links(editor.parts()), *chain, "{name}: initial chain");
        editor
            .apply(
                "remove",
                &[RegisteredCommand::RemoveTextFromPath {
                    ids: vec![removed.into()],
                }
                .into()],
            )
            .unwrap();
        let parts = editor.parts();
        let spread = &parts[SPREAD];
        let index = Index::scan(spread).unwrap();
        assert_eq!(links(parts), expected["chain"], "{name}: remaining chain");
        assert_eq!(
            index.by_self(spread, removed).is_some(),
            expected["shapeRemains"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            shape_count(spread) == before,
            expected["shapeCountUnchanged"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(parts[STORY], story, "{name}: story bytes");
        let ix = Index::scan(&parts[STORY]).unwrap();
        let node = (0..ix.nodes.len())
            .find(|&i| ix.local(&parts[STORY], i) == "Content")
            .unwrap();
        let content = &parts[STORY][ix.nodes[node].open_end..ix.nodes[node].close_start];
        assert_eq!(
            folio_edit::xml::unescape(content),
            expected["storyText"].as_str().unwrap(),
            "{name}"
        );
    }
}
