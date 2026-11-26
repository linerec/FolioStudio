use super::*;

#[test]
fn plan_keeps_legacy_warnings_and_structured_messages_in_order() {
    let base = std::env::var_os("FOLIO_USER_DATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("import-messages-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let project = crate::manifest::create(&dir, "Import messages").unwrap();
    let source = dir.join("unreadable.idml");
    std::fs::write(&source, b"not a zip").unwrap();
    let result = json!({"embedded":{"file":"embedded.json"}});
    let doc = Document::default();
    let fonts = FontStore::empty();
    let links = LinkReport::default();
    let prepared = Prepared {
        source: &source, kind: "indd", source_sha256: "test", source_bytes: 9,
        idml: &source, reader_dir: Some(&dir), reader_result: Some(&result),
        doc: &doc, fonts: &fonts, links: &links,
        resolve_uri: &|_| panic!("empty document has no links"),
    };
    for (embedded, expected) in [(None, "import.embedded.unreadable"), (Some("{}"), "import.embedded.unsupported")] {
        if let Some(value) = embedded { std::fs::write(dir.join("embedded.json"), value).unwrap(); }
        let plan = plan(&project, &RelPath::parse("test.folio").unwrap(), &prepared);
        let wire = serde_json::to_value(&plan).unwrap();
        assert_eq!(plan.warnings.len(), 2);
        assert_eq!(wire["warningMessages"][0]["messageId"], expected);
        assert_eq!(wire["warningMessages"][1]["messageId"], "import.idml.parts");
        for (legacy, message) in plan.warnings.iter().zip(wire["warningMessages"].as_array().unwrap()) {
            assert_eq!(message["message"], *legacy);
            assert!(message["messageArgs"].is_object());
        }
        assert!(wire["warningMessages"][1]["messageArgs"]["error"].is_string());
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
