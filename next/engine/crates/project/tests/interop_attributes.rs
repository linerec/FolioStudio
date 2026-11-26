#[path = "support/interop.rs"]
mod support;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    fixture: String,
    part: String,
    tag: String,
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    key: String,
    value: String,
    pointer: String,
    expected: Value,
    items: Vec<String>,
    golden: String,
    #[serde(default)]
    setup: Vec<[String; 3]>,
    #[serde(default)]
    overlay: bool,
}

#[test]
fn attribute_mutations_survive_read_export_read() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("support/interop_attributes.json")).unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        println!("{} {} {}={} {:?}", case.fixture, case.tag, case.key, case.value, case.items);
        assert!(support::root().join("oracle/goldens").join(&case.golden).is_dir());
        let mut parts = support::fixture(&case.fixture);
        if case.overlay {
            let path = support::root().join("oracle/goldens").join(&case.golden).join("after").join(&case.part);
            parts.insert(case.part.clone(), std::fs::read_to_string(path).unwrap());
        }
        for [tag, key, value] in &case.setup {
            support::attr(parts.get_mut(&case.part).unwrap(), tag, 0, key, value);
        }
        support::attr(parts.get_mut(&case.part).unwrap(), &case.tag, case.index, &case.key, &case.value);
        let (before, after, written) = support::roundtrip(&case.fixture, &parts);
        let left = case.id.as_ref().map_or(&before, |id| support::by_id(&before, id));
        let right = case.id.as_ref().map_or(&after, |id| support::by_id(&after, id));
        support::check(left, right, &case.pointer, case.expected);
        assert!(written[&case.part].contains(&format!("{}=\"{}\"", case.key, case.value)));
    }
}
