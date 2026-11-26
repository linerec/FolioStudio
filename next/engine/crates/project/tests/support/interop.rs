use folio_project::{format::*, manifest, open};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::{Path, PathBuf}};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn fixture(name: &str) -> BTreeMap<String, String> {
    let file = std::fs::File::open(root().join("oracle/fixtures").join(format!("{name}.idml"))).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        if name.ends_with(".xml") {
            let mut text = String::new();
            f.read_to_string(&mut text).unwrap();
            parts.insert(name, text);
        }
    }
    parts
}

pub fn attr(xml: &mut String, tag: &str, nth: usize, key: &str, value: &str) {
    let start = xml.match_indices(&format!("<{tag} ")).nth(nth).unwrap().0;
    let end = start + xml[start..].find('>').unwrap();
    let needle = format!(" {key}=\"");
    if let Some(at) = xml[start..end].find(&needle) {
        let at = start + at + needle.len();
        let stop = at + xml[at..end].find('"').unwrap();
        xml.replace_range(at..stop, value);
    } else {
        let at = if xml.as_bytes()[end - 1] == b'/' { end - 1 } else { end };
        xml.insert_str(at, &format!(" {key}=\"{value}\""));
    }
}

/// Uses the same .folio writer and IDML exporter as the application, including its XML rewrite pass.
pub fn roundtrip(name: &str, parts: &BTreeMap<String, String>) -> (Value, Value, BTreeMap<String, String>) {
    let base = root().join("../artifacts/interop-b-c1-20261023/roundtrip").join(folio_project::util::uuid_v4());
    let project = manifest::create(&base, "상호운용 시험").unwrap();
    let source = root().join("oracle/fixtures").join(format!("{name}.idml"));
    let before = folio_idml::read_parts(parts, &source, &Default::default()).unwrap();
    let m = DocManifest {
        format: FORMAT.into(), version: VERSION, id: "interop-test".into(), project: project.file.id.clone(),
        created: String::new(), saved: String::new(), engine: String::new(), source: None,
        link_uris: "folio-ref".into(), references: vec![], graphics: vec![], pins: vec![],
        saved_lines: None, import_report: None, extra: Default::default(),
    };
    let folio = base.join("test.folio");
    write_folio(&folio, &source, &mut |name, _| {
        Ok(parts.get(name).map_or(PartAction::Keep, |s| PartAction::Replace(s.clone())))
    }, &m, None, None).unwrap();
    let opened = open::resolve(&folio).unwrap();
    let exported = base.join("export.idml");
    folio_project::export::export_idml(&opened, &exported, 0).unwrap();
    let after = folio_idml::read_package(&exported, None).unwrap();
    let mut written = BTreeMap::new();
    for name in parts.keys() {
        if let Some(bytes) = read_part(&exported, name).unwrap() {
            written.insert(name.clone(), String::from_utf8(bytes).unwrap());
        }
    }
    assert!(base.starts_with(root().join("../artifacts/interop-b-c1-20261023/roundtrip")));
    std::fs::remove_dir_all(base).unwrap();
    (serde_json::to_value(before).unwrap(), serde_json::to_value(after).unwrap(), written)
}

pub fn check(before: &Value, after: &Value, pointer: &str, expected: Value) {
    assert_eq!(before.pointer(pointer), Some(&expected), "mutation was not read: {pointer}");
    assert_eq!(after.pointer(pointer), Some(&expected), "mutation was lost on export: {pointer}");
}

pub fn by_id<'a>(value: &'a Value, id: &str) -> &'a Value {
    fn find<'a>(value: &'a Value, id: &str, result: &mut Vec<&'a Value>) {
        match value {
            Value::Object(map) => {
                if map.get("id").and_then(Value::as_str) == Some(id) { result.push(value); }
                for child in map.values() { find(child, id, result); }
            }
            Value::Array(values) => for child in values { find(child, id, result); },
            _ => (),
        }
    }
    let mut result = Vec::new();
    find(value, id, &mut result);
    assert_eq!(result.len(), 1, "expected one model object with id {id}");
    result[0]
}
