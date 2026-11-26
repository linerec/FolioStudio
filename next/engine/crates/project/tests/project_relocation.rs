//! 외부 INDD·글꼴 없이 .folio 전체 복사→열기→내보내기를 검사한다.
use folio_project::{format::*, hash, manifest, open, paths::RelPath};
use std::io::Write;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().map(Result::unwrap) {
        if e.file_name() == ".folio" { continue; } // 해시 캐시 없이도 재연결돼야 한다.
        if e.file_type().unwrap().is_dir() { copy_tree(&e.path(), &to.join(e.file_name())); }
        else { std::fs::copy(e.path(), to.join(e.file_name())).unwrap(); }
    }
}

#[test]
fn synthetic_project_all_links_survive_copy_and_original_removal() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/infra-path-tests")
        .join(folio_project::util::uuid_v4());
    let original = base.join("drive-c/책");
    let project = manifest::create(&original, "합성 문서").unwrap();
    let idml = base.join("source.idml");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&idml).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    z.start_file("mimetype", options).unwrap();
    z.write_all(MIMETYPE.as_bytes()).unwrap();
    z.start_file("designmap.xml", options).unwrap();
    z.write_all(br#"<Document Self="d" DOMVersion="20.5"><idPkg:Spread src="Spreads/Spread_s.xml"/></Document>"#).unwrap();
    let mut spread = String::from("<idPkg:Spread><Spread Self=\"s\">");
    let mut references = Vec::new();
    let mut graphics = Vec::new();
    for (i, name) in ["사진.jpg", "École.png", "Other.tif"].iter().enumerate() {
        let bytes = format!("synthetic image {i}").into_bytes();
        let rel = RelPath::parse(&format!("assets/그림/{name}")).unwrap();
        let disk = original.join("assets/그림").join(name.nfd().collect::<String>());
        std::fs::create_dir_all(disk.parent().unwrap()).unwrap();
        std::fs::write(disk, &bytes).unwrap();
        references.push(Reference { id: format!("r{i}"), kind: RefKind::Link, path: Some(rel),
            sha256: Some(hash::sha256_bytes(&bytes)), bytes: Some(bytes.len() as u64),
            hint: None, original_name: None, status: None, font: None, extra: Default::default() });
        graphics.push(GraphicRefs { self_id: format!("g{i}"), folio_id: format!("f{i}"),
            link_self: Some(format!("l{i}")), link: Some(format!("r{i}")), ..Default::default() });
        spread.push_str(&format!("<Rectangle Self=\"frame{i}\"><Image Self=\"g{i}\"><Link Self=\"l{i}\" LinkResourceURI=\"folio:ref/r{i}\"/></Image></Rectangle>"));
    }
    spread.push_str("</Spread></idPkg:Spread>");
    z.start_file("Spreads/Spread_s.xml", options).unwrap();
    z.write_all(spread.as_bytes()).unwrap();
    z.finish().unwrap();
    let m = DocManifest { format: FORMAT.into(), version: VERSION, id: "document".into(), project: project.file.id.clone(),
        created: String::new(), saved: String::new(), engine: String::new(), source: None, link_uris: "folio-ref".into(),
        references, graphics, pins: vec![], saved_lines: None, import_report: None, extra: Default::default() };
    write_folio(&original.join("책.folio"), &idml, &mut |_, _| Ok(PartAction::Keep), &m, None, None).unwrap();
    let roots = [base.join("drive-d/deep/a/b/바뀐 이름"), base.join("mac/Volumes/Book"), base.join("linux/home/editor/book")];
    for root in &roots { copy_tree(&original, root); }
    // 전부 시험 전용 base 아래임을 확인하고 원본을 제거한다. 옛 위치 의존을 잡는다.
    assert!(original.starts_with(&base));
    std::fs::remove_dir_all(&original).unwrap();
    for root in roots {
        let doc = root.join("책.folio");
        let opened = open::resolve(&doc).unwrap();
        assert_eq!(opened.project.file.id, project.file.id);
        assert_eq!(opened.summary.links_in_project, 3);
        assert_eq!(opened.summary.links_resolved, 3);
        assert_eq!(opened.summary.ok, 3);
        assert_eq!(opened.summary.missing + opened.summary.changed, 0);
        for r in &opened.refs {
            let file = r.file.as_ref().unwrap();
            assert!(file.canonicalize().unwrap().starts_with(root.canonicalize().unwrap()));
            assert_eq!(hash::sha256_bytes(&std::fs::read(file).unwrap()), m.reference(&r.id).unwrap().sha256.clone().unwrap());
        }
        let exported = root.join("roundtrip.idml");
        folio_project::export::export_idml(&opened, &exported, 0).unwrap();
        let xml = String::from_utf8(read_part(&exported, "Spreads/Spread_s.xml").unwrap().unwrap()).unwrap();
        assert!(!xml.contains("folio:ref/"));
        assert_eq!(xml.matches("LinkResourceURI=\"file:").count(), 3);
        let (persisted, _) = read_manifest(&doc).unwrap();
        assert_eq!(persisted, m);
        assert!(!serde_json::to_string(&persisted).unwrap().contains(&base.to_string_lossy().to_string()));
    }
    std::fs::remove_dir_all(&base).unwrap();
}
