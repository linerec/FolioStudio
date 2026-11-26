//! Index topics and their inline page references. Original ordering is preserved.
use crate::xml::{Element, Node};
use folio_model::indexes::{CrossReference, Index, Topic, Reference};
use std::collections::BTreeMap;

fn attr(e: &Element<'_>, key: &str) -> String { e.attr(key).unwrap_or_default().to_owned() }

pub(crate) fn from_elements(dm: &Element<'_>, stories: &[&Element<'_>]) -> Vec<Index> {
    /// `at` counts the story's characters as `editorial::contents` does (Content characters, one for each
    /// break, object and index marker); text of footnotes and table cells is counted on its own.
    fn references(e: &Element<'_>, story: &str, at: &mut usize, nested: bool, anchor: usize, out: &mut BTreeMap<String, Vec<Reference>>) {
        if e.name == "HiddenText" || e.name == "Properties" || (e.name == "Change" && e.attr("ChangeType") == Some("DeletedText")) { return; }
        match e.name {
            "PageReference" => {
                let limit = e.attr("PageReferenceLimit").map(str::to_owned).or_else(|| e.child("Properties")
                    .and_then(|p| p.child("PageReferenceLimit")).map(|l| l.text())).unwrap_or_default();
                out.entry(attr(e, "ReferencedTopic")).or_default().push(Reference {
                    id: attr(e, "Self"), story: story.into(), kind: attr(e, "PageReferenceType"), limit, at: *at, nested, anchor,
                });
                if e.elements().all(|c| c.name == "Properties") { *at += 1; }
                return;
            }
            "Content" => { *at += e.children.iter().map(|n| match n { Node::Text(t) => t.chars().count(), Node::Pi(..) => 1, _ => 0 }).sum::<usize>(); return; }
            "Br" | "Note" | "TextVariableInstance" | "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "Group" | "TextFrame" => { *at += 1; return; }
            "Table" | "Footnote" | "Endnote" => {
                // (a footnote or table inside another keeps the outer one's place)
                let here = if nested { anchor } else { *at };
                *at += 1;
                let mut inner = 0;
                for child in e.elements() { references(child, story, &mut inner, true, here, out); }
                return;
            }
            _ => {}
        }
        for child in e.elements() { references(child, story, at, nested, anchor, out); }
    }
    fn topic(e: &Element<'_>, refs: &BTreeMap<String, Vec<Reference>>) -> Topic {
        Topic { id: attr(e,"Self"), name: attr(e,"Name"), sort_order: attr(e,"SortOrder"),
            topics: e.children_named("Topic").map(|t|topic(t,refs)).collect(),
            references: refs.get(&attr(e,"Self")).cloned().unwrap_or_default(),
            cross_references: e.children_named("CrossReference").map(|c| CrossReference { id: attr(c,"Self"), kind: attr(c,"CrossReferenceType"),
                custom: attr(c,"CustomTypeString"), referenced: attr(c,"ReferencedTopic") }).collect() }
    }
    let mut refs = BTreeMap::new();
    for root in stories {
        if let Some(s) = root.child("Story") { references(s, &attr(s,"Self"), &mut 0, false, 0, &mut refs); }
    }
    dm.children_named("Index").map(|e| Index {id:attr(e,"Self"), topics:e.children_named("Topic").map(|t|topic(t,&refs)).collect()}).collect()
}
