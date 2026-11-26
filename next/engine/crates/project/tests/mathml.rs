use folio_project::{format::*, manifest, open};
use std::path::Path;

#[test]
fn mathml_folio_export_preserves_spread_bytes_and_unknown_attributes() {
    roundtrip("storage");
}

#[test]
fn inline_mathml_folio_export_preserves_story_bytes_and_unknown_attributes() {
    roundtrip("inline");
}

fn roundtrip(fixture: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("../../../oracle/goldens/mathml-c1").join(fixture).join("after.idml");
    let base = root.join("../../../../artifacts/mathml-c1-tests").join(folio_project::util::uuid_v4());
    let project = manifest::create(&base, "MathML roundtrip").unwrap();
    let m = DocManifest {
        format: FORMAT.into(), version: VERSION, id: "mathml-test".into(), project: project.file.id,
        created: String::new(), saved: String::new(), engine: String::new(), source: None, link_uris: "folio-ref".into(),
        references: vec![], graphics: vec![], pins: vec![], saved_lines: None, import_report: None, extra: Default::default(),
    };
    let folio = base.join("math.folio");
    let mut expected = Vec::new();
    write_folio(&folio, &source, &mut |name, read| {
        if name.starts_with("Spreads/") || name.starts_with("Stories/") {
            let text = read()?.replace("<MathObject ", "<MathObject FutureMathSetting=\"keep &amp; safe\" ")
                .replace("</MathObject>", "<FutureMathChild Version=\"2\">keep &amp; safe</FutureMathChild></MathObject>");
            expected.push((name.to_owned(), text.clone()));
            Ok(PartAction::Replace(text))
        } else { Ok(PartAction::Keep) }
    }, &m, None, None).unwrap();
    let opened = open::resolve(&folio).unwrap();
    let exported = base.join("roundtrip.idml");
    folio_project::export::export_idml(&opened, &exported, 0).unwrap();
    assert!(!expected.is_empty());
    for (name, text) in expected {
        assert_eq!(read_part(&exported, &name).unwrap().unwrap(), text.as_bytes());
    }
    let original = folio_idml::read_package(&source, None).unwrap();
    let reopened = folio_idml::read_package(&exported, None).unwrap();
    let collect = |doc: &folio_model::Document| {
        let mut result = Vec::new();
        folio_idml::for_each_item(doc, &mut |item| {
            if let Some(math) = &item.math { result.push((item.id.clone(), item.paths.clone(), math.clone())); }
        });
        result
    };
    let math = collect(&original);
    assert_eq!(math.len(), 4);
    assert_eq!(math, collect(&reopened));
    assert!(base.starts_with(root.join("../../../../artifacts/mathml-c1-tests")));
    std::fs::remove_dir_all(base).unwrap();
}
