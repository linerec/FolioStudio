//! Go To Destination's target resolution: text, page (with its zoom setting), URL and broken links, as structured values.
//! InDesign's own results for these (2026-10-28, `next/oracle/rules/linkgoto.ko.md`): a page destination shows its page at the destination's zoom
//! setting; Fixed N is N percent of actual size (25/60/100/200/400/1000 gave 15/36/60/120/240/600, a constant 0.6 of the number on that display).
use folio_edit::Editor;
use folio_idml::hyperlinks::{Target, target_in};
use std::{collections::BTreeMap, io::Read, path::Path};

const FIXED_100: &str = "ViewSetting=\"Fixed\" ViewPercentage=\"100\"";

fn fixture(view: Option<&str>) -> Editor { fixture_file("linkgoto.idml", view) }

fn fixture_file(file: &str, view: Option<&str>) -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(file);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut entry = zip.by_index(k).unwrap();
        if entry.is_dir() { continue; }
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        if name == "designmap.xml"
            && let Some(view) = view { assert!(text.contains(FIXED_100)); text = text.replace(FIXED_100, view); }
        parts.insert(name, text);
    }
    Editor::new(parts)
}

fn link(editor: &Editor, name: &str) -> String {
    folio_idml::hyperlinks::read(editor.parts()).unwrap().links.iter().find(|l| l.name == name).unwrap().id.clone()
}

fn json(editor: &Editor, name: &str) -> serde_json::Value {
    serde_json::to_value(editor.hyperlink_target(&link(editor, name)).unwrap()).unwrap()
}

#[test]
fn every_destination_kind_resolves_to_a_structured_target() {
    let editor = fixture(None);
    let text = json(&editor, "Internal");
    assert_eq!((text["kind"].as_str(), text["start"].as_u64(), text["end"].as_u64()), (Some("text"), Some(7), Some(7)));
    assert!(text["story"].as_str().is_some_and(|s| !s.is_empty()));
    let page = json(&editor, "Page");
    assert_eq!(page["kind"], "page");
    assert_eq!(page["view"], serde_json::json!({"setting":"fixed","percent":100.0}));
    let data = folio_idml::hyperlinks::read(editor.parts()).unwrap();
    let dest = data.destinations.iter().find(|d| d.kind == "HyperlinkPageDestination").unwrap();
    assert_eq!(page["page"], dest.value.as_str());
    assert_eq!(json(&editor, "External URL"), serde_json::json!({"kind":"url","url":"https://example.invalid/linkgoto"}));
    assert_eq!(json(&editor, "Broken")["kind"], "missingDestination");
    assert_eq!(editor.hyperlink_target("absent").unwrap(), Target::MissingLink);
}

#[test]
fn page_zoom_settings_follow_idml_view_setting() {
    for (idml, setting, percent) in [
        ("ViewSetting=\"Fixed\" ViewPercentage=\"25\"", "fixed", 25.0), ("ViewSetting=\"Fixed\" ViewPercentage=\"60\"", "fixed", 60.0),
        ("ViewSetting=\"Fixed\" ViewPercentage=\"1000\"", "fixed", 1000.0), ("ViewSetting=\"Fixed\"", "fixed", 100.0), ("", "fixed", 100.0),
        ("ViewSetting=\"FitView\" ViewPercentage=\"100\"", "fitView", 100.0), ("ViewSetting=\"FitWidth\" ViewPercentage=\"100\"", "fitWidth", 100.0),
        ("ViewSetting=\"FitHeight\" ViewPercentage=\"100\"", "fitHeight", 100.0), ("ViewSetting=\"FitVisible\" ViewPercentage=\"100\"", "fitVisible", 100.0),
        ("ViewSetting=\"InheritZoom\" ViewPercentage=\"0\"", "inheritZoom", 100.0), ("ViewSetting=\"SomethingNew\"", "inheritZoom", 100.0),
    ] {
        let editor = fixture(Some(idml));
        let page = json(&editor, "Page");
        assert_eq!(page["view"], serde_json::json!({"setting":setting,"percent":percent}), "{idml}");
    }
}

#[test]
fn page_item_sources_are_read_and_their_links_resolve() {
    // linkgoto-objects.idml: linkgoto.idml plus links that start from the placed graphic's frame (to the text destination) and from a small rectangle (to the page).
    let editor = fixture_file("linkgoto-objects.idml", None);
    let data = folio_idml::hyperlinks::read(editor.parts()).unwrap();
    assert_eq!(data.item_sources.len(), 2);
    let from_object = |name: &str| {
        let link = data.links.iter().find(|l| l.name == name).unwrap();
        let source = data.item_sources.iter().find(|s| s.id == link.source).unwrap();
        assert!(!source.item.is_empty());
        serde_json::to_value(editor.hyperlink_target(&link.id).unwrap()).unwrap()
    };
    assert_eq!(from_object("Object to text")["kind"], "text");
    assert_eq!(from_object("Object to text")["start"], 7);
    assert_eq!(from_object("Object to page")["kind"], "page");
    // the text links keep their text sources, and an object source is not a text source
    assert!(data.sources.iter().all(|s| data.item_sources.iter().all(|i| i.id != s.id)));
}

#[test]
fn a_page_destination_needs_its_page() {
    let editor = fixture(None);
    let data = folio_idml::hyperlinks::read(editor.parts()).unwrap();
    let id = link(&editor, "Page");
    assert!(matches!(target_in(&data, &id, &|_| true), Target::Page { .. }));
    assert_eq!(target_in(&data, &id, &|_| false), Target::MissingDestination);
}
