//! Data Merge QR code fields against InDesign 2025 (probes `datamerge-c4-w`, `-x`; fixtures `oracle/fixtures/datamerge-qr`: InDesign's own
//! template exports and the merged documents it made of them). Every record's frame gets the EPS InDesign drew, embedded, fitted to the
//! frame's smaller side and centred; a record without a value leaves its frame empty. The EPS text and the transform are InDesign's.
use folio_edit::xml::Index;
use folio_edit::{Command, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/datamerge-qr")
}

fn package(file: &str) -> BTreeMap<String, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(dir().join(file)).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() { continue; }
        let name = e.name().unwrap().into_owned();
        if name.ends_with(".xml") || name == "mimetype" {
            let mut text = String::new();
            e.read_to_string(&mut text).unwrap();
            parts.insert(name, text);
        }
    }
    parts
}

/// Per spread, in document order: the QR code of its frame (the EPS bytes, the transform, the graphic bounds, the text) or none.
fn codes(parts: &BTreeMap<String, String>) -> Vec<Option<(Vec<u8>, Vec<f64>, Vec<f64>, String)>> {
    let dm = &parts["designmap.xml"];
    let ix = Index::scan(dm).unwrap();
    let root = ix.roots().next().unwrap();
    let mut out = Vec::new();
    for &c in ix.children(root) {
        if ix.local(dm, c) != "Spread" { continue; }
        let t = &parts[ix.attr(dm, c, "src").unwrap()];
        let sx = Index::scan(t).unwrap();
        let eps = (0..sx.nodes.len()).find(|&k| sx.local(t, k) == "EPS");
        out.push(eps.map(|k| {
            let nums = |s: &str| s.split_whitespace().map(|v| v.parse::<f64>().unwrap()).collect::<Vec<_>>();
            let contents = sx.path(t, k, &["Properties", "Contents"]).unwrap();
            let body = t[sx.nodes[contents].open_end..sx.nodes[contents].close_start].trim();
            let body = body.strip_prefix("<![CDATA[").and_then(|b| b.strip_suffix("]]>")).unwrap();
            let bounds = sx.path(t, k, &["Properties", "GraphicBounds"]).unwrap();
            let code = sx.named(t, k, "PlainTextQRCode")[0];
            assert!(t.contains("StoredState=\"Embedded\""), "the code is embedded");
            (decode(body), nums(sx.attr(t, k, "ItemTransform").unwrap()), nums(&format!("{} {}", sx.attr(t, bounds, "Right").unwrap(), sx.attr(t, bounds, "Bottom").unwrap())),
                folio_edit::xml::unescape(sx.attr(t, code, "PlainText").unwrap()))
        }));
    }
    out
}

fn decode(b64: &str) -> Vec<u8> {
    let value = |c: u8| match c { b'A'..=b'Z' => c - b'A', b'a'..=b'z' => c - b'a' + 26, b'0'..=b'9' => c - b'0' + 52, b'+' => 62, b'/' => 63, _ => 255 };
    let chars: Vec<u8> = b64.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').map(value).collect();
    let mut out = Vec::new();
    for g in chars.chunks(4) {
        let n = g.iter().fold(0u32, |n, &v| n << 6 | v as u32) << (6 * (4 - g.len()));
        out.extend_from_slice(&n.to_be_bytes()[1..g.len()]);
    }
    out
}

#[test]
fn qr_fields_match_indesign() {
    for (case, csv) in [("qr1", "qr.csv"), ("qrwide", "qr2.csv"), ("qrtall", "qr2.csv"), ("qrsmall", "qr2.csv")] {
        let source = folio_idml::datamerge::parse(&std::fs::read(dir().join(csv)).unwrap()).unwrap();
        let mut ed = Editor::new(package(&format!("{case}-template.idml")));
        let original = ed.parts().clone();
        let command: Command = folio_edit::commands::RegisteredCommand::MergeRecords {
            fields: source.fields.iter().map(|f| f.name.clone()).collect(), records: source.records.clone(), range: String::new(), multiple: false, arrange: "rows".into(),
            row_spacing: 0.0, column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false, fitting: "proportional".into(), center_image: false,
            graphics: BTreeMap::new(), embed: None, link: None,
        }.into();
        ed.apply("merge", &[command]).unwrap_or_else(|e| panic!("{case}: {}", e.message));
        let merged = ed.parts().clone();
        let (ours, theirs) = (codes(&merged), codes(&package(&format!("{case}-merged.idml"))));
        assert_eq!(ours.len(), theirs.len(), "{case}: pages");
        for (n, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            match (a, b) {
                (None, None) => {}
                (Some((eps, matrix, bounds, text)), Some((eps2, matrix2, bounds2, text2))) => {
                    let (ours_text, theirs_text) = (String::from_utf8_lossy(eps), String::from_utf8_lossy(eps2));
                    if let Some((k, (x, y))) = ours_text.lines().zip(theirs_text.lines()).enumerate().find(|(_, (x, y))| x != y) { panic!("{case} record {n}: the EPS differs at line {k}: {x:?} vs {y:?}"); }
                    assert_eq!(ours_text.len(), theirs_text.len(), "{case} record {n}: the EPS length");
                    assert_eq!(text, text2, "{case} record {n}: the text");
                    for (x, y) in matrix.iter().zip(matrix2).chain(bounds.iter().zip(bounds2)) { assert!((x - y).abs() < 1e-9, "{case} record {n}: {matrix:?} vs {matrix2:?}, {bounds:?} vs {bounds2:?}"); }
                    assert_eq!(matrix.len(), matrix2.len());
                }
                _ => panic!("{case} record {n}: a code in one document only"),
            }
        }
        let designmap = &merged["designmap.xml"];
        assert!(!designmap.contains("DataMergeQrcodePlaceholder") && !designmap.contains("QRC_") && !designmap.contains("<Hyperlink"), "{case}: QR placeholder left");
        // one undo step brings the template back, redo the merge
        assert!(ed.undo().is_some());
        assert_eq!(ed.parts(), &original, "{case}: undo");
        assert!(ed.redo().is_some());
        assert_eq!(ed.parts(), &merged, "{case}: redo");
    }
}

/// Update Content in Data Fields (rest-c1): a merged document made with a link remembers each record's QR code; an update draws the
/// code of the new value in a frame that still holds the merge's code, leaves a frame a person emptied alone (logged), and is one undo.
#[test]
fn update_redraws_qr_codes_the_merge_made() {
    let source = folio_idml::datamerge::parse(&std::fs::read(dir().join("qr.csv")).unwrap()).unwrap();
    let fields: Vec<String> = source.fields.iter().map(|f| f.name.clone()).collect();
    let mut ed = Editor::new(package("qr1-template.idml"));
    let link = serde_json::json!({"template": {"path": "qr1.folio", "sha256": "t"}, "source": {"path": "qr.csv", "sha256": "a"}});
    let command: Command = folio_edit::commands::RegisteredCommand::MergeRecords {
        fields: fields.clone(), records: source.records.clone(), range: String::new(), multiple: false, arrange: "rows".into(),
        row_spacing: 0.0, column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false, fitting: "proportional".into(), center_image: false,
        graphics: BTreeMap::new(), embed: None, link: Some(link.to_string()),
    }.into();
    ed.apply("merge", &[command]).unwrap();
    let remembered = ed.merged_document().unwrap()["codes"].as_array().cloned().unwrap_or_default();
    let qr = fields.iter().position(|f| f.starts_with('#') || f == "Code").expect("a QR code field");
    let with_code: Vec<usize> = (0..source.records.len()).filter(|&r| !source.records[r][qr].is_empty()).collect();
    assert!(with_code.len() >= 2, "two records with a code: {:?}", source.records);
    assert_eq!(remembered.len(), source.records.len(), "{remembered:?}");
    // a person empties the second coded record's frame (the code is removed by hand)
    let second = remembered.iter().find(|c| c["record"] == with_code[1]).unwrap()["frame"].as_str().unwrap().to_owned();
    let mut parts = ed.parts().clone();
    let part = parts.iter().find(|(k, t)| k.starts_with("Spreads/") && t.contains(&format!("Self=\"{second}\""))).unwrap().0.clone();
    let t = parts[&part].clone();
    let ix = Index::scan(&t).unwrap();
    let frame = ix.by_self(&t, &second).unwrap();
    let eps = ix.children(frame).iter().copied().find(|&n| ix.local(&t, n) == "EPS").unwrap();
    parts.insert(part, format!("{}{}", &t[..ix.nodes[eps].start], &t[ix.nodes[eps].end..]));
    let mut ed = Editor::new(parts);
    let before = ed.parts().clone();
    let mut records = source.records.clone();
    records[with_code[0]][qr] = "https://example.org/updated".into();
    records[with_code[1]][qr] = "https://example.org/other".into();
    let command: Command = folio_edit::commands::RegisteredCommand::UpdateMergedFields { fields: fields.clone(), records, sha256: "b".into(), at: "2026-11-21T20:00:00Z".into() }.into();
    ed.apply("update", &[command]).unwrap();
    let texts: Vec<Option<String>> = codes(ed.parts()).into_iter().map(|c| c.map(|c| c.3)).collect();
    assert!(texts.contains(&Some("https://example.org/updated".into())), "{texts:?}");
    assert!(!texts.contains(&Some("https://example.org/other".into())), "the frame a person emptied stays empty: {texts:?}");
    let log = ed.merged_document().unwrap()["log"].as_array().unwrap().clone();
    assert!(log.iter().any(|e| e["result"] == "updated" && e["to"] == "https://example.org/updated"), "{log:?}");
    assert!(log.iter().any(|e| e["result"] == "edited" && e["frame"] == second.as_str()), "{log:?}");
    assert!(ed.undo().is_some());
    assert_eq!(ed.parts(), &before, "one undo step");
}
