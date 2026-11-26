use folio_edit::xml::{Index, escape};
use folio_edit::{Command, Editor};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

const PREFERENCES: &str = "Resources/Preferences.xml";
#[cfg(has_reference_vectors)]
const VECTORS: &str = include_str!("../../../../../cleanroom/vectors/A8/old.json");
const FIXTURE: &[u8] = include_bytes!("../../../../oracle/fixtures/basic-create.idml");

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn text_frame_creation_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn text_frame_creation_vectors() {
    let cases: Vec<Value> = serde_json::from_str(VECTORS).unwrap();
    for case in cases {
        assert_eq!(case["source"], "old-impl");
        let output = run_case(&case["input"]);
        assert_eq!(output, case["expected"], "{}", case["id"]);
    }
}

#[test]
fn fit_preserves_inherited_column_width_and_saves_column_count() {
    for local_width in [false, true] {
        let parts = fixture_parts();
        let (page, _) = first_page(&parts);
        let mut editor = Editor::new(parts);
        let frame = editor
            .apply(
                "create frame",
                &[command(json!({
                    "op": "createItem", "kind": "textFrame", "page": page,
                    "rect": [50, 50, 138, 120]
                }))],
            )
            .unwrap()
            .created
            .remove(0);
        let mut parts = editor.parts().clone();
        let styles = parts.get_mut("Resources/Styles.xml").unwrap();
        let index = Index::scan(styles).unwrap();
        let group = find(&index, styles, "RootObjectStyleGroup");
        let style = concat!(
            r#"<ObjectStyle Self="ObjectStyle/TestColumns" Name="TestColumns">"#,
            r#"<TextFramePreference AutoSizingType="WidthOnly" UseFixedColumnWidth="true" "#,
            r#"TextColumnCount="2" TextColumnFixedWidth="35" TextColumnGutter="12">"#,
            r#"<Properties><InsetSpacing type="unit">3</InsetSpacing></Properties>"#,
            r#"</TextFramePreference></ObjectStyle>"#
        );
        styles.insert_str(index.nodes[group].close_start, style);
        let (part, xml) = parts
            .iter_mut()
            .find(|(name, xml)| {
                name.starts_with("Spreads/") && xml.contains(&format!("Self=\"{frame}\""))
            })
            .unwrap();
        let part = part.clone();
        let index = Index::scan(xml).unwrap();
        let node = index.by_self(xml, &frame).unwrap();
        let pref = index.path(xml, node, &["TextFramePreference"]).unwrap();
        let style_attr = index
            .attrs(xml, node)
            .into_iter()
            .find(|attr| &xml[attr.name.0..attr.name.1] == "AppliedObjectStyle")
            .unwrap();
        let pref = &index.nodes[pref];
        let replacement = if local_width {
            r#"<TextFramePreference TextColumnFixedWidth="35"/>"#
        } else {
            ""
        };
        xml.replace_range(pref.start..pref.end, replacement);
        xml.replace_range(
            style_attr.value.0..style_attr.value.1,
            "ObjectStyle/TestColumns",
        );

        let mut editor = Editor::new(parts);
        editor
            .apply(
                "fit frame",
                &[Command::FitFrame {
                    id: frame.clone(),
                    size: [41.0, 70.0],
                    point: [0.0, 0.0],
                }],
            )
            .unwrap();
        let xml = editor.part(&part).unwrap();
        let index = Index::scan(xml).unwrap();
        let node = index.by_self(xml, &frame).unwrap();
        let pref = index.path(xml, node, &["TextFramePreference"]).unwrap();
        // R6
        assert_eq!(index.attr(xml, pref, "TextColumnCount"), Some("1"));
        assert_eq!(
            index.attr(xml, pref, "TextColumnFixedWidth"),
            local_width.then_some("35")
        );
    }
}

fn run_case(input: &Value) -> Value {
    assert_eq!(input["fixture"], "basic-create");
    let mut parts = fixture_parts();
    customize_preferences(parts.get_mut(PREFERENCES).unwrap(), input);
    let (page, origin) = first_page(&parts);
    let mut editor = Editor::new(parts);
    let [top, left, bottom, right] =
        serde_json::from_value::<[f64; 4]>(input["bounds"].clone()).unwrap();
    let rect = [left, top, right, bottom];
    let frame = if input["op"] == "convert" {
        let create =
            json!({"op": "createItem", "kind": input["shape"], "page": page, "rect": rect});
        editor
            .apply("create shape", &[command(create)])
            .unwrap()
            .created
            .into_iter()
            .next()
    } else {
        None
    };
    let edit = match &frame {
        Some(frame) => json!({"op": "setContentType", "ids": [frame], "kind": "text"}),
        None => json!({"op": "createItem", "kind": "textFrame", "page": page, "rect": rect}),
    };
    let before = editor.parts().clone();
    match editor.apply("make text frame", &[command(edit)]) {
        Ok(outcome) => {
            let frame = frame.unwrap_or_else(|| outcome.created[0].clone());
            frame_output(&mut editor, &frame, origin)
        }
        Err(error) => {
            json!({"ok": false, "code": error.code, "unchanged": editor.parts() == &before})
        }
    }
}

fn command(value: Value) -> Command {
    serde_json::from_value(value).unwrap()
}

fn fixture_parts() -> BTreeMap<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(FIXTURE)).unwrap();
    (0..archive.len())
        .filter_map(|i| {
            let mut part = archive.by_index(i).unwrap();
            if part.is_dir() {
                return None;
            }
            let mut data = String::new();
            part.read_to_string(&mut data).unwrap();
            Some((part.name().unwrap().into_owned(), data))
        })
        .collect()
}

fn customize_preferences(xml: &mut String, input: &Value) {
    let index = Index::scan(xml).unwrap();
    let pref = find(&index, xml, "TextFramePreference");
    let node = &index.nodes[pref];
    if input["removePreferences"] == true {
        xml.replace_range(node.start..node.end, "");
        return;
    }
    let mut attributes = attributes(&index, xml, pref);
    if let Some(changes) = input["attributes"].as_object() {
        for (key, value) in changes {
            if value.is_null() {
                attributes.remove(key);
            } else {
                attributes.insert(key.clone(), value.clone());
            }
        }
    }
    if let Some(insets) = input.get("insets") {
        let inset = index
            .path(xml, pref, &["Properties", "InsetSpacing"])
            .unwrap();
        let range = &index.nodes[inset];
        let replacement = insets.as_array().map_or_else(String::new, |insets| {
            let items: String = insets
                .iter()
                .map(|n| format!("<ListItem type=\"unit\">{n}</ListItem>"))
                .collect();
            format!("<InsetSpacing type=\"list\">{items}</InsetSpacing>")
        });
        xml.replace_range(range.start..range.end, &replacement);
    }
    let mut tag = String::from("<TextFramePreference");
    for (name, value) in attributes {
        tag.push_str(&format!(" {name}=\"{}\"", escape(value.as_str().unwrap())));
    }
    tag.push('>');
    xml.replace_range(node.start..node.open_end, &tag);
}

fn first_page(parts: &BTreeMap<String, String>) -> (String, [f64; 2]) {
    let (_, spread) = parts
        .iter()
        .find(|(name, _)| name.starts_with("Spreads/"))
        .unwrap();
    let index = Index::scan(spread).unwrap();
    let page = find(&index, spread, "Page");
    let bounds: Vec<f64> = numbers(index.attr(spread, page, "GeometricBounds").unwrap());
    let transform: Vec<f64> = numbers(index.attr(spread, page, "ItemTransform").unwrap());
    let x = bounds[1] * transform[0] + bounds[0] * transform[2] + transform[4];
    let y = bounds[1] * transform[1] + bounds[0] * transform[3] + transform[5];
    (index.value(spread, page, "Self").unwrap(), [x, y])
}

fn frame_output(editor: &mut Editor, id: &str, [x, y]: [f64; 2]) -> Value {
    let [left, top, right, bottom] = editor.bounds(id).unwrap();
    for (name, xml) in editor.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let index = Index::scan(xml).unwrap();
        let Some(frame) = index.by_self(xml, id) else {
            continue;
        };
        assert_eq!(index.local(xml, frame), "TextFrame");
        let story = index.attr(xml, frame, "ParentStory").unwrap();
        let story = editor.part(&format!("Stories/Story_{story}.xml")).unwrap();
        let story_index = Index::scan(story).unwrap();
        let empty = !(0..story_index.nodes.len()).any(|i| {
            let node = &story_index.nodes[i];
            story_index.local(story, i) == "Content" && node.open_end < node.close_start
        });
        let pref = index.path(xml, frame, &["TextFramePreference"]).unwrap();
        let mut values = attributes(&index, xml, pref);
        let inset = index
            .path(xml, pref, &["Properties", "InsetSpacing"])
            .unwrap();
        let read = |i: usize| {
            let node = &index.nodes[i];
            xml[node.open_end..node.close_start].parse::<f64>().unwrap()
        };
        let inset = if index.attr(xml, inset, "type") == Some("list") {
            json!(
                index
                    .children(inset)
                    .iter()
                    .copied()
                    .map(read)
                    .collect::<Vec<_>>()
            )
        } else {
            json!(read(inset))
        };
        values.insert("InsetSpacing".into(), inset);
        // R8
        let bounds = [top - y, left - x, bottom - y, right - x].map(|v| (v * 1e9).round() / 1e9);
        return json!({"ok": true, "kind": "textFrame", "empty": empty, "bounds": bounds, "preferences": values});
    }
    panic!("frame {id} was missing from the edited spread")
}

fn attributes(index: &Index, xml: &str, node: usize) -> Map<String, Value> {
    index
        .attrs(xml, node)
        .iter()
        .map(|attr| {
            let name = &xml[attr.name.0..attr.name.1];
            (
                name.to_owned(),
                json!(index.value(xml, node, name).unwrap()),
            )
        })
        .collect()
}

fn find(index: &Index, xml: &str, tag: &str) -> usize {
    (0..index.nodes.len())
        .find(|&i| index.local(xml, i) == tag)
        .unwrap()
}

fn numbers(value: &str) -> Vec<f64> {
    value
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect()
}
