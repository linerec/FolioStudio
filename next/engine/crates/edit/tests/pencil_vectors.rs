use folio_edit::{Command, Editor, xml::Index};
use folio_model::{Affine, Point, Rect};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::{fs, io::Read, path::Path};

fn fixture(name: &str) -> BTreeMap<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let file = fs::File::open(manifest.join("../../../oracle/fixtures").join(name)).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml).unwrap();
        parts.insert(entry.name().unwrap().to_string(), xml);
    }
    parts
}

fn first_page(parts: &BTreeMap<String, String>) -> String {
    for (name, xml) in parts {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let index = Index::scan(xml).unwrap();
        if let Some(page) = (0..index.nodes.len()).find(|&i| index.local(xml, i) == "Page") {
            return index.attr(xml, page, "Self").unwrap().to_owned();
        }
    }
    panic!("fixture has no page");
}

fn find_on_spreads<'a>(editor: &'a Editor, id: &str) -> Option<(&'a str, Index, usize)> {
    editor.parts().iter().find_map(|(name, xml)| {
        if !name.starts_with("Spreads/") {
            return None;
        }
        let index = Index::scan(xml).unwrap();
        let node = index.by_self(xml, id)?;
        Some((xml.as_str(), index, node))
    })
}

fn numbers<const N: usize>(text: &str) -> [f64; N] {
    let values: Vec<f64> = text.split_whitespace().map(|s| s.parse().unwrap()).collect();
    values.try_into().unwrap()
}

fn to_spread(index: &Index, xml: &str, node: usize) -> Affine {
    std::iter::once(node)
        .chain(index.ancestors(node))
        .take_while(|&i| index.local(xml, i) != "Spread")
        .filter_map(|i| index.attr(xml, i, "ItemTransform"))
        .fold(Affine::IDENTITY, |transform, attr| {
            let [a, b, c, d, tx, ty] = numbers(attr);
            Affine::new(a, b, c, d, tx, ty).then_inner(&transform)
        })
}

fn page_origin(editor: &Editor, page: &str) -> Point {
    let (xml, index, node) = find_on_spreads(editor, page).expect("fixture page");
    let bounds = Rect::from_idml_bounds(numbers(index.attr(xml, node, "GeometricBounds").unwrap()));
    let bounds = to_spread(&index, xml, node).apply_rect(&bounds);
    Point::new(bounds.left, bounds.top)
}

fn read_object(editor: &Editor, id: &str, origin: Point) -> Value {
    let Some((xml, index, node)) = find_on_spreads(editor, id) else {
        return Value::Null;
    };
    let transform = to_spread(&index, xml, node);
    let mut paths = Vec::new();
    for path in index.own_descendants(xml, node, "GeometryPathType") {
        let mut points = Vec::new();
        for point in index.own_descendants(xml, path, "PathPointType") {
            let mut coordinates = Vec::with_capacity(6);
            for name in ["Anchor", "LeftDirection", "RightDirection"] {
                let [x, y] = numbers(index.attr(xml, point, name).unwrap());
                let position = transform.apply(Point::new(x, y));
                coordinates.extend([position.x - origin.x, position.y - origin.y]);
            }
            points.push(coordinates);
        }
        let open = index.attr(xml, path, "PathOpen") != Some("false");
        paths.push(json!({"open": open, "points": points}));
    }
    json!({"type": index.local(xml, node), "paths": paths})
}

fn resolve_references(value: &mut Value, page: &str, created: &[String]) {
    match value {
        Value::String(reference) if reference == "$page" => *reference = page.to_owned(),
        Value::String(reference) if reference.starts_with("$s") => {
            let index: usize = reference[2..].parse().expect("setup reference");
            reference.clone_from(&created[index]);
        }
        Value::Array(items) => {
            for item in items {
                resolve_references(item, page, created);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                resolve_references(item, page, created);
            }
        }
        _ => {}
    }
}

fn run_case(parts: &BTreeMap<String, String>, page: &str, input: &Value) -> Value {
    let mut editor = Editor::new(parts.clone());
    let mut created = Vec::new();
    let command = |mut value, ids: &[String]| {
        resolve_references(&mut value, page, ids);
        serde_json::from_value::<Command>(value).expect("test command")
    };
    for setup in input["setup"].as_array().expect("test setup array") {
        let outcome = editor.apply("setup", &[command(setup.clone(), &created)]).unwrap();
        created.extend(outcome.created);
    }
    match editor.apply("A4 test", &[command(input["command"].clone(), &created)]) {
        Err(error) => json!({"ok": false, "code": error.code}),
        Ok(outcome) => {
            created.extend(outcome.created);
            let origin = page_origin(&editor, page);
            let objects: Vec<_> = created.iter().map(|id| read_object(&editor, id, origin)).collect();
            json!({"ok": true, "objects": objects})
        }
    }
}

fn compare_list<'a>(id: &str, expected: &'a Value, actual: &'a Value) -> impl Iterator<Item = (&'a Value, &'a Value)> {
    let expected = expected.as_array().unwrap();
    let actual = actual.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{id}");
    expected.iter().zip(actual)
}

fn compare(id: &str, expected: &Value, actual: &Value, tolerance: &Value) {
    assert_eq!(actual["ok"], expected["ok"], "{id}: {actual}");
    if expected["ok"] == false {
        assert_eq!(actual["code"], expected["code"], "{id}");
        return;
    }
    for (object, got) in compare_list(id, &expected["objects"], &actual["objects"]) {
        assert_eq!(got.is_null(), object.is_null(), "{id}");
        if object.is_null() {
            continue;
        }
        if let Some(kind) = object.get("type") {
            assert_eq!(got["type"], *kind, "{id}");
        }
        for (path, actual_path) in compare_list(id, &object["paths"], &got["paths"]) {
            assert_eq!(actual_path["open"], path["open"], "{id}");
            for (i, (point, actual_point)) in compare_list(id, &path["points"], &actual_path["points"]).enumerate() {
                assert_eq!(actual_point.as_array().unwrap().len(), 6, "{id} point {i}");
                for (axis, kind) in [(0, "anchor"), (2, "handle"), (4, "handle")] {
                    let diff = |k: usize| actual_point[k].as_f64().unwrap() - point[k].as_f64().unwrap();
                    let distance = diff(axis).hypot(diff(axis + 1));
                    assert!(
                        distance <= tolerance[kind].as_f64().unwrap(),
                        "{id} point {i} {kind}: {distance} pt"
                    );
                }
            }
        }
    }
}

#[cfg_attr(not(has_reference_vectors), ignore = "optional reference vectors are missing")]
#[test]
fn public_commands_match_the_a4_vectors() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let parts = fixture("basic.idml");
    let page = first_page(&parts);
    let mut ids = BTreeSet::new();
    for file in fs::read_dir(manifest.join("../../../../cleanroom/vectors/A4")).unwrap() {
        let file = file.unwrap().path();
        if file.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let cases: Vec<Value> = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        for case in cases {
            let id = case["id"].as_str().unwrap();
            assert!(ids.insert(id.to_owned()), "duplicate {id}");
            assert!(
                matches!(case["source"].as_str(), Some("measured" | "old-impl" | "oracle")),
                "{id}"
            );
            let actual = run_case(&parts, &page, &case["input"]);
            compare(id, &case["expected"], &actual, &case["tolerance"]);
        }
    }
    assert!(ids.len() >= 110, "read {} A4 cases", ids.len());
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn smoothing_a_compound_path_opens_only_the_edited_subpath() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn smoothing_a_compound_path_opens_only_the_edited_subpath() {
    let parts = fixture("basic.idml");
    let page = first_page(&parts);
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../../../../cleanroom/vectors/A4/erase.json")).unwrap();
    let mut input = cases.iter().find(|c| c["id"] == "erase-compound-inner").unwrap()["input"].clone();
    input["command"]["points"] = json!([[180, 420], [181, 420]]);
    let before = run_case(&parts, &page, &input);
    assert_eq!(before["ok"], true, "{before}");
    input["command"]["op"] = json!("smoothPath");
    input["command"]["points"] = json!([[175, 450], [210, 450], [210, 475]]);
    let after = run_case(&parts, &page, &input);
    assert_eq!(after["ok"], true, "{after}");
    let paths = after["objects"][0]["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], before["objects"][0]["paths"][0]);
    assert_eq!(before["objects"][0]["paths"][1]["open"], false);
    assert_eq!(paths[1]["open"], true);
}

#[test]
fn pen_continuation_uses_the_turned_page_origin() {
    let parts = fixture("page-rotated.idml");
    let case = json!({
        "setup": [{"op": "createPath", "page": "$page", "closed": false,
                   "points": [{"anchor": [30, 40]}, {"anchor": [70, 60]}]}],
        "command": {"op": "penContinue", "id": "$s0", "page": "$page", "at_start": false,
                    "anchors": [[80, 95]], "lefts": [[75, 90]], "rights": [[85, 100]],
                    "join": "", "join_at_start": false}
    });
    for page in ["ufc", "u116"] {
        let result = run_case(&parts, page, &case);
        assert_eq!(result["ok"], true, "{page}: {result}");
        let points = result["objects"][0]["paths"][0]["points"].as_array().unwrap();
        assert_eq!(points.len(), 3, "{page}");
        for (got, want) in points[2]
            .as_array()
            .unwrap()
            .iter()
            .zip([80., 95., 75., 90., 85., 100.])
        {
            assert!(
                (got.as_f64().unwrap() - want).abs() < 1e-6,
                "{page}: {got} expected {want}"
            );
        }
    }
}
