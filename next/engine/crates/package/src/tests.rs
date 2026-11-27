//! Whole-package tests on a synthetic document: names, collisions, the atomic folder,
//! cancel and failure cleanup, protected folders, both copy methods, the PDF slot.

use super::*;
use folio_links::{How, LinkEntry, LinkSearch, LinkStatus};
use folio_model::{LinkInfo, Page, Rect, Spread};
use std::sync::atomic::AtomicBool;

struct Fixture {
    base: PathBuf,
    doc: Document,
    layout: DocumentLayout,
    fonts: FontStore,
    links: LinkReport,
    preflight: folio_preflight::Report,
    locations: HashMap<String, folio_preflight::Location>,
    source: PathBuf,
    out: PathBuf,
}

fn entry(owner: &str, uri: &str, status: Option<LinkStatus>, embedded: bool) -> LinkEntry {
    LinkEntry {
        owner: owner.into(),
        element: "Image".into(),
        recorded: LinkInfo { uri: uri.into(), stored_state: if embedded { "Embedded".into() } else { "Normal".into() }, ..Default::default() },
        embedded,
        status,
        modified: None,
        identity_matches: Vec::new(),
        info: None,
    }
}

fn found(p: &Path) -> Option<LinkStatus> {
    Some(LinkStatus::Found { path: p.to_path_buf(), how: How::Search { score: 1 } })
}

/// issue/issue.indd with links in three folders: two different "photo.jpg", a third
/// "PHOTO.jpg" with the second one's bytes, an NFD-named PDF, one file placed twice, a
/// missing, an embedded and an ambiguous link.
fn fixture(name: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!("folio-package-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let issue = base.join("issue");
    for d in ["issue", "a", "b", "c", "out"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
    }
    let source = issue.join("issue.indd");
    std::fs::write(&source, vec![7u8; 300_000]).unwrap();
    let a = base.join("a").join("photo.jpg");
    let b = base.join("b").join("photo.jpg");
    let c = base.join("c").join("PHOTO.jpg");
    std::fs::write(&a, vec![1u8; 5000]).unwrap();
    std::fs::write(&b, vec![2u8; 5000]).unwrap();
    std::fs::write(&c, vec![2u8; 5000]).unwrap();
    let nfd = base.join("a").join(folio_links::uri::nfd("광고_2024.pdf"));
    std::fs::write(&nfd, b"%PDF-1.4 fake").unwrap();
    let links = LinkReport {
        search: LinkSearch { document_dir: Some(issue.clone()), roots: vec![base.join("a"), base.join("b"), base.join("c")] },
        links: vec![
            entry("g1", "file:/Volumes/x/a/photo.jpg", found(&a), false),
            entry("g2", "file:/Volumes/x/b/photo.jpg", found(&b), false),
            entry("g3", "file:/Volumes/x/c/PHOTO.jpg", found(&c), false),
            entry("g4", "file:/Volumes/x/a/%E1%84%80%E1%85%AA%E1%86%BC%E1%84%80%E1%85%A9_2024.pdf", found(&nfd), false),
            entry("g5", "file:/Volumes/x/a/photo.jpg", found(&a), false),
            entry("g6", "file:/Volumes/x/gone/lost.tif", Some(LinkStatus::Missing), false),
            entry("g7", "file:/Volumes/x/inside.png", None, true),
            entry("g8", "file:/Volumes/x/twin.jpg", Some(LinkStatus::Ambiguous(vec![base.join("a/twin.jpg"), base.join("b/twin.jpg")])), false),
        ],
        summary: Default::default(),
    };
    let page = |i: usize, name: &str| Page { id: format!("p{i}"), name: name.into(), bounds: Rect { left: 0.0, top: 0.0, right: 595.276, bottom: 841.89 }, index: i, ..Default::default() };
    let doc = Document {
        spreads: vec![Spread { id: "s1".into(), pages: vec![page(0, "1"), page(1, "2")], ..Default::default() }],
        fonts_used: vec![("Minion Pro".into(), "Regular".into())],
        ..Default::default()
    };
    let mut locations = HashMap::new();
    locations.insert("g2".to_string(), folio_preflight::Location { page: 1, bounds: [0.0; 4] });
    Fixture {
        out: base.join("out"),
        base,
        doc,
        layout: DocumentLayout::default(),
        fonts: FontStore::empty(),
        links,
        preflight: folio_preflight::Report::default(),
        locations,
        source,
    }
}

impl Fixture {
    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            doc: &self.doc,
            layout: &self.layout,
            fonts: &self.fonts,
            links: &self.links,
            preflight: &self.preflight,
            locations: &self.locations,
            source: &self.source,
            inks: InkFacts::default(),
            profiles: ProfileFacts::default(),
        }
    }

    /// Nothing but the finished package (if any) in the output folder.
    fn leftovers(&self) -> Vec<String> {
        std::fs::read_dir(&self.out).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.contains("folio-partial")).collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn opts() -> Options {
    Options { protected_roots: Vec::new(), app_version: "test".into(), engine_version: "0".into(), utc_offset_minutes: Some(540), ..Default::default() }
}

fn quiet(_: &Progress) {}

fn malformed_fixture(name: &str, bytes: &[u8]) -> Fixture {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../artifacts/robust-a-c1-20261023/package-inputs").join(name);
    let out = base.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let source = base.join("input.idml");
    std::fs::write(&source, bytes).unwrap();
    Fixture {
        base, out, source,
        doc: Document::default(),
        layout: DocumentLayout::default(),
        fonts: FontStore::empty(),
        links: LinkReport { search: LinkSearch::default(), links: Vec::new(), summary: Default::default() },
        preflight: folio_preflight::Report::default(),
        locations: HashMap::new(),
    }
}

#[test]
fn empty_and_malformed_documents_remain_opaque_byte_copies() {
    for (name, bytes) in [("empty", &b""[..]), ("truncated", &b"PK\x03\x04<broken"[..])] {
        let f = malformed_fixture(name, bytes);
        let inp = f.inputs();
        let o = Options { jobs: 0, copy: CopyMethod::Stream, ..opts() };
        let plan = plan(&inp, &o, true).unwrap();
        let dest = f.out.join("pkg");
        let out = run(&plan, &inp, &o, &dest, None, &quiet, &AtomicBool::new(false)).unwrap();
        assert!(out.failures.is_empty());
        assert_eq!(std::fs::read(dest.join("input.idml")).unwrap(), bytes);
        assert!(f.leftovers().is_empty());
    }
}

#[test]
fn missing_source_is_rejected_during_planning() {
    let f = malformed_fixture("missing", b"");
    std::fs::remove_file(&f.source).unwrap();
    let error = plan(&f.inputs(), &opts(), true).unwrap_err();
    assert_eq!(error.code, "source");
    assert!(f.leftovers().is_empty());
}

#[test]
fn preview_and_copy_plans_assign_colliding_links_in_placement_order() {
    let f = fixture("plan-collisions");
    let inp = f.inputs();
    let options = opts();
    let preview = plan(&inp, &options, false).unwrap();
    let copy = plan(&inp, &options, true).unwrap();
    let indexes = |p: &Plan| p.link_rows.iter().map(|row| row.file).collect::<Vec<_>>();

    assert_eq!(indexes(&preview), [Some(0), Some(1), Some(2), Some(3), Some(0), None, None, None]);
    assert_eq!(indexes(&copy), [Some(0), Some(1), Some(1), Some(2), Some(0), None, None, None]);
    assert_eq!(preview.links[2].copy.dest, "Links/PHOTO (3).jpg");
    assert_eq!(copy.links[1].owners, ["g2", "g3"]);
    assert_eq!(preview.bytes, copy.bytes + 5000);
}

#[test]
fn package_collects_names_and_reports() {
    let f = fixture("full");
    let inp = f.inputs();
    let o = opts();
    let plan = plan(&inp, &o, true).unwrap();
    // first "photo.jpg" keeps its name, the different one is suffixed, the identical
    // "PHOTO.jpg" shares the suffixed copy, the twice-placed file is copied once
    let dests: Vec<&str> = plan.links.iter().map(|l| l.copy.dest.as_str()).collect();
    let nfd_name = format!("Links/{}", folio_links::uri::nfd("광고_2024.pdf"));
    assert_eq!(dests, ["Links/photo.jpg", "Links/photo (2).jpg", nfd_name.as_str()]);
    assert!(plan.links[1].renamed && !plan.links[0].renamed);
    assert_eq!(plan.links[1].same_as.len(), 1);
    assert_eq!(plan.links[0].owners, ["g1", "g5"]);
    assert_eq!(plan.link_rows.iter().filter(|r| r.status == "missing").count(), 1);
    assert!(plan.link_rows.iter().find(|r| r.owner == "g4").unwrap().nfd_name);
    assert_eq!(plan.font_rows[0].status, "missing");

    let dest = f.out.join("issue 폴더");
    let events = Mutex::new(Vec::new());
    let progress = |p: &Progress| events.lock().unwrap().push((p.stage, p.done, p.total));
    let out = run(&plan, &inp, &o, &dest, None, &progress, &AtomicBool::new(false)).unwrap();
    assert_eq!(out.dir, dest);
    assert!(f.leftovers().is_empty());
    // the document is a byte copy under its own name, hashed
    let copy = std::fs::read(dest.join("issue.indd")).unwrap();
    assert_eq!(copy, std::fs::read(&f.source).unwrap());
    assert_eq!(out.document_sha256.as_deref(), Some(fsops::hex(&<sha2::Sha256 as sha2::Digest>::digest(&copy)).as_str()));
    assert_eq!(std::fs::read(dest.join("Links/photo.jpg")).unwrap(), vec![1u8; 5000]);
    assert_eq!(std::fs::read(dest.join("Links/photo (2).jpg")).unwrap(), vec![2u8; 5000]);
    assert!(dest.join(&nfd_name).is_file(), "NFD name kept as spelled on disk");
    assert_eq!(std::fs::read_dir(dest.join("Links")).unwrap().count(), 3);
    assert!(!dest.join("Document fonts").exists(), "no font folder without fonts");

    // reports
    let mut json: Value = serde_json::from_slice(&std::fs::read(dest.join(REPORT_JSON)).unwrap()).unwrap();
    // the file is what the run returned (timings are floats: compared apart from the rest)
    let mut returned = out.report.clone();
    assert!(json["timings"]["copyMs"].as_f64().unwrap() >= 0.0);
    json["timings"] = Value::Null;
    returned["timings"] = Value::Null;
    assert_eq!(json, returned);
    assert_eq!(json["links"]["summary"]["missing"], 1);
    assert_eq!(json["links"]["summary"]["ambiguous"], 1);
    assert_eq!(json["links"]["summary"]["embedded"], 1);
    assert_eq!(json["links"]["summary"]["renamed"], 1);
    assert_eq!(json["links"]["summary"]["sameContent"], 1);
    assert_eq!(json["links"]["missing"][0]["name"], "lost.tif");
    assert_eq!(json["fonts"]["summary"]["missing"], 1);
    assert_eq!(json["document"]["pages"], 2);
    assert_eq!(json["document"]["pageSizes"][0]["widthMm"], 210.0);
    assert!(json["created"].as_str().unwrap().ends_with("+09:00"));
    let text = std::fs::read(dest.join(INSTRUCTIONS)).unwrap();
    assert!(text.starts_with(b"\xEF\xBB\xBF"));
    let text = String::from_utf8(text[3..].to_vec()).unwrap();
    assert!(text.contains("없는 링크 파일 1개(배치 1곳)"), "{text}");
    assert!(text.contains("lost.tif"));
    assert!(text.contains("photo.jpg → Links/photo (2).jpg"));
    assert!(text.contains("\r\n") && !text.contains("\r\r"));
    assert!(text.contains("크기: 210 × 297 mm — 2쪽 (1–2쪽)"), "{text}");
    // progress: copy events counted up to the total, then report and finish
    let ev = events.into_inner().unwrap();
    assert!(ev.iter().any(|e| e.0 == "copy" || e.0 == "pdf"));
    assert_eq!(ev.last().unwrap().0, "finish");
    assert!(ev.iter().all(|e| e.1 <= e.2));
    let copied: u64 = 300_000 + 5000 + 5000 + 13;
    assert_eq!(plan.bytes, copied);
}

#[test]
fn existing_folder_is_never_touched() {
    let f = fixture("exists");
    let inp = f.inputs();
    let dest = f.out.join("pkg");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("keep.txt"), b"mine").unwrap();
    let plan = plan(&inp, &opts(), false).unwrap();
    let e = run(&plan, &inp, &opts(), &dest, None, &quiet, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(e.code, "exists");
    assert_eq!(e.suggestion.as_deref(), Some("pkg 2"));
    assert_eq!(std::fs::read(dest.join("keep.txt")).unwrap(), b"mine");
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1);
    assert!(f.leftovers().is_empty());
    // a file of that name counts too
    std::fs::write(f.out.join("pkg 2"), b"x").unwrap();
    let e = run(&plan, &inp, &opts(), &f.out.join("pkg 2"), None, &quiet, &AtomicBool::new(false)).unwrap_err();
    assert_eq!((e.code, e.suggestion.as_deref()), ("exists", Some("pkg 2 2")));
}

#[test]
fn cancel_leaves_no_folder() {
    let f = fixture("cancel");
    let inp = f.inputs();
    let plan = plan(&inp, &opts(), true).unwrap();
    let dest = f.out.join("pkg");
    // cancelled from the first progress event (copies already under way)
    let cancel = AtomicBool::new(false);
    let progress = |_: &Progress| cancel.store(true, Ordering::Relaxed);
    let e = run(&plan, &inp, &opts(), &dest, None, &progress, &cancel).unwrap_err();
    assert_eq!(e.code, "cancelled");
    assert!(!dest.exists());
    assert!(f.leftovers().is_empty());
    assert!(e.leftover.is_none());
}

#[test]
fn failure_leaves_no_folder() {
    let f = fixture("fail");
    let inp = f.inputs();
    let plan = plan(&inp, &opts(), true).unwrap();
    // the document disappears after planning: the package cannot be made
    std::fs::remove_file(&f.source).unwrap();
    let dest = f.out.join("pkg");
    let e = run(&plan, &inp, &opts(), &dest, None, &quiet, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(e.code, "io", "{e:?}");
    assert!(!dest.exists());
    assert!(f.leftovers().is_empty());
}

#[test]
fn unreadable_link_is_listed_not_fatal() {
    let f = fixture("linkgone");
    let inp = f.inputs();
    let plan = plan(&inp, &opts(), true).unwrap();
    std::fs::remove_file(f.base.join("a").join("photo.jpg")).unwrap();
    let dest = f.out.join("pkg");
    let out = run(&plan, &inp, &opts(), &dest, None, &quiet, &AtomicBool::new(false)).unwrap();
    assert_eq!(out.failures.len(), 1);
    assert_eq!(out.failures[0].dest, "Links/photo.jpg");
    assert!(!dest.join("Links/photo.jpg").exists());
    assert_eq!(out.report["links"]["summary"]["failed"], 1);
    let text = String::from_utf8_lossy(&std::fs::read(dest.join(INSTRUCTIONS)).unwrap()).to_string();
    assert!(text.contains("복사하지 못한 링크 1개"));
}

#[test]
fn protected_roots_are_the_users_choice() {
    let f = fixture("protected");
    let inp = f.inputs();
    // nothing is protected by default: the document's own folder may hold its package
    let plan0 = plan(&inp, &opts(), false).unwrap();
    assert!(check_destination(&f.source.parent().unwrap().join("pkg"), &inp, &opts()).is_ok());
    // the source document itself is never a destination
    let e = check_destination(&f.source, &inp, &opts()).unwrap_err();
    assert_eq!(e.code, "exists");
    // configured roots refuse anything inside them
    let o = Options { protected_roots: vec![f.source.parent().unwrap().to_path_buf(), f.base.join("b")], ..opts() };
    let dest = f.source.parent().unwrap().join("pkg");
    let e = run(&plan0, &inp, &o, &dest, None, &quiet, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(e.code, "protected");
    assert!(!dest.exists());
    let e = run(&plan0, &inp, &o, &f.base.join("b").join("deeper").join("pkg"), None, &quiet, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(e.code, "noParent", "the folder does not exist yet: {e:?}");
    let e = check_destination(&f.base.join("b").join("pkg"), &inp, &o).unwrap_err();
    assert_eq!(e.code, "protected");
    // elsewhere is allowed
    assert!(run(&plan0, &inp, &o, &f.out.join("pkg"), None, &quiet, &AtomicBool::new(false)).is_ok());
}

#[test]
fn destination_errors_preserve_text_and_identify_user_paths() {
    let f = fixture("destination-messages");
    let inp = f.inputs();
    let e = check_destination(Path::new("pkg"), &inp, &opts()).unwrap_err();
    assert_eq!(e.message, "패키지 폴더 위치가 올바르지 않습니다");
    assert_eq!(e.message_id, Some("package.destination.invalid"));
    assert_eq!(e.message_args, serde_json::json!({}));

    let e = check_destination(&f.out.join(" "), &inp, &opts()).unwrap_err();
    assert_eq!(e.message, "패키지 폴더 이름이 없습니다");
    assert_eq!(e.message_id, Some("package.destination.unnamed"));

    let parent = f.out.join("한글 {path}");
    let e = check_destination(&parent.join("pkg"), &inp, &opts()).unwrap_err();
    assert_eq!(e.message, format!("저장할 폴더가 없습니다: {}", parent.display()));
    assert_eq!(e.message_id, Some("package.destination.missingParent"));
    assert_eq!(e.message_args["path"], parent.display().to_string());

    let e = check_destination(&f.source, &inp, &opts()).unwrap_err();
    assert_eq!(e.message_id, Some("package.destination.exists"));
    assert_eq!(e.message_args["path"], f.source.display().to_string());
    assert!(e.suggestion.is_some());

    let o = Options { protected_roots: vec![f.out.clone()], ..opts() };
    let e = check_destination(&f.out.join("pkg"), &inp, &o).unwrap_err();
    assert_eq!(e.message_id, Some("package.destination.protected"));
    assert_eq!(e.message_args["path"], f.out.display().to_string());
    assert_eq!(e.message, format!("보호된 위치({}) 안에는 패키지를 만들 수 없습니다(설정에서 지정한 보호 폴더). 다른 위치를 고르세요.", f.out.display()));
    std::fs::remove_file(&f.source).unwrap();
    let e = check_destination(&f.source, &inp, &opts()).unwrap_err();
    assert_eq!(e.message_id, Some("package.destination.source"));
    assert_eq!(e.message_args["path"], f.source.display().to_string());
}

#[test]
fn stream_copies_hash_every_file_and_pdf_slot_is_used() {
    let f = fixture("stream");
    let inp = f.inputs();
    let o = Options { copy: CopyMethod::Stream, jobs: 2, pdf: true, ..opts() };
    let plan = plan(&inp, &o, true).unwrap();
    let maker = |path: &Path| -> Result<PdfOutcome, String> {
        std::fs::write(path, b"%PDF-1.6 test").map_err(|e| e.to_string())?;
        Ok(PdfOutcome { bytes: 13, pages: 2, standard: "PDF/X-4".into(), ..Default::default() })
    };
    let dest = f.out.join("pkg");
    let out = run(&plan, &inp, &o, &dest, Some(&maker), &quiet, &AtomicBool::new(false)).unwrap();
    for w in out.files.iter().filter(|w| w.path.starts_with("Links/") || w.path == "issue.indd") {
        assert_eq!(w.sha256.as_ref().map(String::len), Some(64), "{}", w.path);
    }
    assert_eq!(std::fs::read(dest.join("issue.pdf")).unwrap(), b"%PDF-1.6 test");
    assert_eq!(out.report["pdf"]["file"], "issue.pdf");

    // a PDF that fails is reported; the package is still made, without a PDF file
    let failing = |_: &Path| -> Result<PdfOutcome, String> { Err("CMYK 프로파일 없음".into()) };
    let dest2 = f.out.join("pkg2");
    let out = run(&plan, &inp, &o, &dest2, Some(&failing), &quiet, &AtomicBool::new(false)).unwrap();
    assert_eq!(out.report["pdf"]["error"], "CMYK 프로파일 없음");
    assert!(!dest2.join("issue.pdf").exists());
    assert!(f.leftovers().is_empty());
}

#[test]
fn links_and_fonts_can_be_left_out() {
    let f = fixture("noLinks");
    let inp = f.inputs();
    let o = Options { links: false, fonts: false, ..opts() };
    let plan = plan(&inp, &o, true).unwrap();
    assert!(plan.links.is_empty());
    let out = run(&plan, &inp, &o, &f.out.join("pkg"), None, &quiet, &AtomicBool::new(false)).unwrap();
    assert!(!f.out.join("pkg").join("Links").exists());
    assert_eq!(out.report["links"]["summary"]["collected"], false);
    // missing links are still listed
    assert_eq!(out.report["links"]["summary"]["missing"], 1);
}
