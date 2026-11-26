#[path = "support/interop.rs"]
mod support;
use folio_idml::ReaderData;
use folio_model::{SavedComposition, SavedLine, SavedSegment};
use folio_project::{format::*, manifest, open};
use serde_json::json;
use support::{attr, check, fixture, root, roundtrip};

#[test]
fn source_location_size_and_reader_are_import_metadata_but_dom_version_is_stored() {
    for version in ["18.0", "20.0"] {
        let mut parts = fixture("basic");
        attr(parts.get_mut("designmap.xml").unwrap(), "Document", 0, "DOMVersion", version);
        let (before, after, written) = roundtrip("basic", &parts);
        check(&before, &after, "/source/dom_version", json!(version));
        assert_eq!(written["designmap.xml"], parts["designmap.xml"]);
        assert_ne!(before["source"]["path"], after["source"]["path"]);
        assert_eq!(before["source"]["bytes"], json!(parts.values().map(String::len).sum::<usize>()));
        assert_ne!(before["source"]["bytes"], after["source"]["bytes"]);
        for name in ["reader-a", "reader-b"] {
            let source = root().join(format!("../artifacts/interop-c1-20261023/{name}.idml"));
            let data = ReaderData { parser: Some(name.into()), warnings: vec![name.into()], ..Default::default() };
            let doc = folio_idml::read_parts(&parts, &source, &data).unwrap();
            assert_eq!(doc.source.reader, format!("{name} + folio-idml"));
            assert_eq!(doc.source.path, std::path::absolute(source).unwrap().to_string_lossy());
            assert!(doc.warnings.contains(&name.to_owned()));
            assert!(!after["warnings"].as_array().unwrap().contains(&json!(name)));
        }
    }
}

#[test]
fn saved_composition_is_folio_sidecar_data_and_not_an_idml_line_record() {
    let mut parts = fixture("basic");
    parts.insert("Stories/Story_u118.xml".into(),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><idPkg:Story xmlns:idPkg=\"http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging\"><Story Self=\"u118\"><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>abcd</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>".into());
    let hash = b"abcd".iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3));
    for (start, frame, column, baseline, x, width) in [(0, 0, 1, 7.5, 10.0, 40.0), (1, -1, 2, 15.0, 20.0, 80.0)] {
        let side = json!({"format":"folio-saved-lines","version":1,"units":"idml-text-v1", "stories":[{
            "story":"u118","units":4,"digest":format!("{hash:016x}"),"frames":["frame-a"],
            "lines":[[start,frame,column,baseline,[[x,width],[x+100.0,width,2]]]]}],"skipped":[]});
        let source = root().join("oracle/fixtures/basic.idml");
        let data = ReaderData { saved_lines: Some(side.clone()), ..Default::default() };
        let doc = folio_idml::read_parts(&parts, &source, &data).unwrap();
        let expected = SavedComposition::Lines(vec![SavedLine {
            start, frame: (frame == 0).then(|| "frame-a".into()), column, baseline: Some(baseline),
            segments: vec![SavedSegment { x, width, start: Some(start) }, SavedSegment { x: x+100.0, width, start: Some(2) }],
        }]);
        assert_eq!(doc.stories["u118"].saved, expected);
        let base = root().join("../artifacts/interop-c1-20261023/sidecar").join(folio_project::util::uuid_v4());
        let project = manifest::create(&base, "상호운용 부속 자료 시험").unwrap();
        let m = DocManifest {
            format: FORMAT.into(), version: VERSION, id: "interop-sidecar".into(), project: project.file.id.clone(),
            created: String::new(), saved: String::new(), engine: String::new(), source: None,
            link_uris: "folio-ref".into(), references: vec![], graphics: vec![], pins: vec![],
            saved_lines: Some(PartRef { part: SAVED_LINES_PART.into(), format: "folio-saved-lines".into(), version: 1 }),
            import_report: None, extra: Default::default(),
        };
        let folio = base.join("source.folio");
        write_folio(&folio, &source, &mut |name, _| Ok(parts.get(name).map_or(PartAction::Keep, |v| PartAction::Replace(v.clone()))),
            &m, Some(&serde_json::to_vec(&side).unwrap()), None).unwrap();
        let opened = open::resolve(&folio).unwrap();
        let reopened = folio_idml::read_parts(&parts, &folio, &opened.reader).unwrap();
        assert_eq!(reopened.stories["u118"].saved, expected);
        let exported = base.join("export.idml");
        folio_project::export::export_idml(&opened, &exported, 1).unwrap();
        assert!(read_part(&exported, SAVED_LINES_PART).unwrap().is_none());
        let after = folio_idml::read_package(&exported, None).unwrap();
        assert_eq!(after.stories["u118"].saved, SavedComposition::None);
        let mut stale = side.clone();
        stale["stories"][0]["digest"] = json!("0");
        let bad = folio_idml::read_parts(&parts, &source, &ReaderData { saved_lines: Some(stale), ..Default::default() }).unwrap();
        assert_eq!(bad.stories["u118"].saved, SavedComposition::Unusable("text differs from the saved lines".into()));
        assert!(base.starts_with(root().join("../artifacts/interop-c1-20261023/sidecar")));
        std::fs::remove_dir_all(base).unwrap();
    }
}
