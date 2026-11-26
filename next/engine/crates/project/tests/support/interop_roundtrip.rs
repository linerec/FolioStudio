//! Field evidence uses the real .folio writer, resolver and IDML exporter. No composition,
//! font loading or installed InDesign is involved. Mutations use only existing XML elements.
#![allow(dead_code)] // Shared by independent integration test binaries.
use folio_project::{format::*, manifest, open};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn oracle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

pub fn golden_part(case: &str, part: &str) -> String {
    std::fs::read_to_string(oracle().join("goldens").join(case).join("after").join(part)).unwrap()
}

pub fn attribute(xml: &str, selector: &str, name: &str, value: &str) -> String {
    assert!(!value.contains(['"', '<', '&']));
    let start = xml.find(selector).unwrap_or_else(|| panic!("missing {selector}"));
    let end = start + xml[start..].find('>').unwrap();
    let opening = &xml[start..end];
    let needle = format!(" {name}=\"");
    let mut out = xml.to_owned();
    if let Some(offset) = opening.find(&needle) {
        let a = start + offset + needle.len();
        let b = a + xml[a..].find('"').unwrap();
        out.replace_range(a..b, value);
    } else {
        let at = if opening.ends_with('/') { end - 1 } else { end };
        out.insert_str(at, &format!(" {name}=\"{value}\""));
    }
    out
}

pub fn property(xml: &str, selector: &str, name: &str, value: &str) -> String {
    let start = xml.find(selector).unwrap();
    let a = start + xml[start..].find(&format!("<{name} ")).unwrap();
    let a = a + xml[a..].find('>').unwrap() + 1;
    let b = a + xml[a..].find(&format!("</{name}>")).unwrap();
    let mut out = xml.to_owned();
    out.replace_range(a..b, value);
    out
}

pub fn check(
    label: &str,
    source: &Path,
    part: &str,
    mutate: impl FnOnce(&str) -> String,
    select: impl Fn(&folio_model::Document) -> Value,
    expected: Value,
) {
    let base = oracle().join("../../artifacts/interop-a-c1-20261023/userdata/roundtrip")
        .join(folio_project::util::uuid_v4());
    let project = manifest::create(&base, "Interop field roundtrip").unwrap();
    let m = DocManifest {
        format: FORMAT.into(), version: VERSION, id: "interop-field".into(), project: project.file.id,
        created: String::new(), saved: String::new(), engine: String::new(), source: None,
        link_uris: "folio-ref".into(), references: vec![], graphics: vec![], pins: vec![],
        saved_lines: None, import_report: None, extra: Default::default(),
    };
    let original = read_part(source, part).unwrap().unwrap();
    let changed = mutate(std::str::from_utf8(&original).unwrap());
    let folio = base.join("field.folio");
    write_folio(&folio, source, &mut |name, _| {
        Ok(if name == part { PartAction::Replace(changed.clone()) } else { PartAction::Keep })
    }, &m, None, None).unwrap();
    let opened = open::resolve(&folio).unwrap();
    let first = folio_idml::read_package_with(&folio, opened.reader.clone()).unwrap();
    assert_eq!(select(&first), expected, "{label}: mutation must be understood by the model");
    let exported = base.join("roundtrip.idml");
    folio_project::export::export_idml(&opened, &exported, 0).unwrap();
    assert_eq!(read_part(&exported, part).unwrap().unwrap(), changed.as_bytes(), "{label}: XML preservation");
    let second = folio_idml::read_package(&exported, None).unwrap();
    assert_eq!(select(&second), expected, "{label}: model after IDML export and reread");
    // Only this test's freshly generated directory inside this worktree is removed.
    std::fs::remove_dir_all(base).unwrap();
}
