//! Create Merged Document against InDesign 2025's `mergeRecords` (probe results in
//! `oracle/goldens/datamerge-c3/merged.json`, the templates are InDesign's own exports).
//! The Folio result must have the same pages, margins, objects (type, bounds on the page, text)
//! and stacking order. Rebuild the goldens: `python -I next/oracle/datamerge-merge-golden.py`.
use folio_edit::xml::Index;
use folio_edit::{Command, Editor};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn oracle() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

fn package(name: &str) -> BTreeMap<String, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(oracle().join("fixtures/datamerge-merge").join(format!("{name}.idml"))).unwrap()).unwrap();
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

fn nums(s: &str) -> Vec<f64> {
    s.split_whitespace().map(|v| v.parse().unwrap()).collect()
}

/// Pages in order: margins, size and page items back to front (type, bounds on the page, text).
fn summary(ed: &mut Editor) -> Value {
    let parts = ed.parts().clone();
    let dm = &parts["designmap.xml"];
    let ix = Index::scan(dm).unwrap();
    let root = ix.roots().next().unwrap();
    let mut pages = Vec::new();
    for &c in ix.children(root) {
        if ix.local(dm, c) != "Spread" { continue; }
        let t = &parts[ix.attr(dm, c, "src").unwrap()];
        let sx = Index::scan(t).unwrap();
        let top = sx.roots().next().unwrap();
        let spread = sx.named(t, top, "Spread")[0];
        let mut origin = [0.0, 0.0];
        let mut margins = json!(null);
        let mut size = json!(null);
        let mut items = Vec::new();
        for &k in sx.children(spread) {
            match sx.local(t, k) {
                "Page" => {
                    let b = nums(sx.attr(t, k, "GeometricBounds").unwrap());
                    let m = nums(sx.attr(t, k, "ItemTransform").unwrap());
                    origin = [m[4] + b[1], m[5] + b[0]];
                    size = json!([b[2] - b[0], b[3] - b[1]]);
                    let mp = sx.named(t, k, "MarginPreference")[0];
                    let g = |n: &str| sx.attr(t, mp, n).unwrap().parse::<f64>().unwrap();
                    margins = json!([g("Top"), g("Left"), g("Bottom"), g("Right")]);
                }
                kind @ ("TextFrame" | "Rectangle" | "Polygon" | "Oval") => {
                    let id = sx.attr(t, k, "Self").unwrap().to_owned();
                    if kind != "TextFrame" && let Some(image) = sx.named(t, k, "Image").first().or(sx.named(t, k, "PDF").first()).copied() {
                        // the picture's own bounds on the page (the frame is only translated)
                        let r = nums(sx.attr(t, k, "ItemTransform").unwrap());
                        let m = nums(sx.attr(t, image, "ItemTransform").unwrap());
                        let g = sx.path(t, image, &["Properties", "GraphicBounds"]).unwrap();
                        let v = |n: &str| sx.attr(t, g, n).unwrap().parse::<f64>().unwrap();
                        let (x0, y0, x1, y1) = (m[0] * v("Left") + m[4] + r[4] - origin[0], m[3] * v("Top") + m[5] + r[5] - origin[1], m[0] * v("Right") + m[4] + r[4] - origin[0], m[3] * v("Bottom") + m[5] + r[5] - origin[1]);
                        items.push(json!({"type": sx.local(t, image), "bounds": [y0, x0, y1, x1]}));
                    }
                    let b = ed.bounds(&id).unwrap();
                    let mut item = json!({"type": kind, "bounds": [b[1] - origin[1], b[0] - origin[0], b[3] - origin[1], b[2] - origin[0]]});
                    if kind == "TextFrame" {
                        let story = sx.attr(t, k, "ParentStory").unwrap();
                        let text = ed.part(&format!("Stories/Story_{story}.xml")).unwrap();
                        item["contents"] = json!(folio_edit::text::contents(text, story).unwrap());
                    }
                    items.push(item);
                }
                _ => {}
            }
        }
        // allPageItems lists the front one first
        items.reverse();
        pages.push(json!({"margins": margins, "size": size, "items": items}));
    }
    json!(pages)
}


fn close(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => assert!((x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 0.01, "{path}: {x} vs {y}"),
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{path}: length");
            for (i, (p, q)) in x.iter().zip(y).enumerate() { close(p, q, &format!("{path}/{i}")); }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in y { close(x.get(k).unwrap_or(&Value::Null), v, &format!("{path}/{k}")); }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

/// The pictures the records name that the fixtures hold (a PDF as InDesign's merge places it: page 1, `CropContentVisibleLayers`, measured
/// in `datamerge-c4-u`); the others are missing files, which leave their frames empty.
fn pictures(source: &folio_idml::datamerge::Source) -> BTreeMap<String, (folio_edit::GraphicSource, Vec<u8>)> {
    let dir = oracle().join("fixtures/datamerge-merge");
    let mut out = BTreeMap::new();
    for (column, _) in source.fields.iter().enumerate().filter(|(_, f)| f.kind == "image") {
        for record in &source.records {
            let value = &record[column];
            let path = dir.join(value);
            if value.is_empty() || out.contains_key(value) || !path.is_file() { continue; }
            let uri = format!("folio:ref/{value}");
            let options = value.ends_with(".pdf").then_some(folio_links::pdfplace::Options { page_number: 1, crop: folio_links::pdfplace::Crop::CropContentVisibleLayers, transparent_background: true });
            out.insert(value.clone(), (folio_edit::GraphicSource::read_with_pdf(&path, uri, options).unwrap(), std::fs::read(&path).unwrap()));
        }
    }
    out
}

fn command(spec: &Value, source: &folio_idml::datamerge::Source) -> Command {
    let pictures = pictures(source);
    let graphics: BTreeMap<String, folio_edit::GraphicSource> = pictures.iter().map(|(k, (g, _))| (k.clone(), g.clone())).collect();
    let text = |k: &str, d: &str| spec[k].as_str().unwrap_or(d).to_owned();
    let margins = spec["margins"].as_array().map_or([36.0; 4], |m| [m[0].as_f64().unwrap(), m[1].as_f64().unwrap(), m[2].as_f64().unwrap(), m[3].as_f64().unwrap()]);
    folio_edit::commands::RegisteredCommand::MergeRecords {
        fields: source.fields.iter().map(|f| f.name.clone()).collect(),
        records: source.records.clone(),
        range: text("range", ""),
        multiple: spec["multiple"].as_bool().unwrap_or(false),
        arrange: if spec["arrange"] == "COLUMNS_FIRST" { "columns".into() } else { "rows".into() },
        row_spacing: spec["rowSpacing"].as_f64().unwrap_or(0.0),
        column_spacing: spec["columnSpacing"].as_f64().unwrap_or(0.0),
        margins,
        remove_blank_lines: spec["removeBlankLines"].as_bool().unwrap_or(false),
        fitting: match (spec["fill"].as_bool().unwrap_or(false), spec["fit"].as_str().unwrap_or("")) {
            (true, _) | (_, "FILL_PROPORTIONAL") => "fill",
            (_, "FIT_CONTENT_TO_FRAME") => "contentToFrame",
            (_, "FIT_FRAME_TO_CONTENT") => "frameToContent",
            (_, "PRESERVE_SIZES") => "none",
            _ => "proportional",
        }.into(),
        center_image: spec["center"].as_bool().unwrap_or(spec["fill"].as_bool().unwrap_or(false)),
        graphics,
        embed: (spec["link"] == false).then(|| pictures.iter().map(|(k, (_, bytes))| (k.clone(), base64_lines(bytes))).collect()),
        link: None,
    }.into()
}

/// The file's bytes as InDesign writes embedded data (76 characters a line).
fn base64_lines(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut col = 0;
    for g in data.chunks(3) {
        if col == 76 { out.push(char::from(10)); col = 0; }
        let n = (g[0] as u32) << 16 | (*g.get(1).unwrap_or(&0) as u32) << 8 | *g.get(2).unwrap_or(&0) as u32;
        for i in 0..4 { out.push(if i <= g.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' }); }
        col += 4;
    }
    out
}

#[test]
fn merged_documents_match_indesign() {
    let golden: Value = serde_json::from_str(&std::fs::read_to_string(oracle().join("goldens/datamerge-c3/merged.json")).unwrap()).unwrap();
    let only = std::env::var("FOLIO_MERGE_ONLY").ok();
    let mut checked = 0;
    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| !o.split(',').any(|n| n == name)) { continue; }
        let spec = &case["spec"];
        let bytes = std::fs::read(oracle().join("fixtures/datamerge-merge").join(spec["file"].as_str().unwrap())).unwrap();
        let source = folio_idml::datamerge::parse(&bytes).unwrap();
        let mut ed = Editor::new(package(name));
        let original = ed.parts().clone();
        let result = ed.apply("merge", &[command(spec, &source)]);
        if let Some(error) = case["error"].as_str() {
            let message = result.expect_err(name).message;
            assert!(error.contains(&message), "{name}: {message} vs {error}");
            continue;
        }
        result.unwrap_or_else(|e| panic!("{name}: {}", e.message));
        let merged = ed.parts().clone();
        let designmap = &merged["designmap.xml"];
        // the merge fields' hyperlink objects are gone, the section covers every page
        assert!(!designmap.contains("<Hyperlink") && !designmap.contains("DataMergeImagePlaceholder"), "{name}: merge links left");
        // the merged document has no data source (measured: no `DataMerge` element in the preferences, the options stay)
        let prefs = &merged["Resources/Preferences.xml"];
        assert!(!prefs.contains("<DataMerge ") && prefs.contains("<DataMergeOption "), "{name}: DataMerge element left");
        // "Link Images" off: the pictures are embedded (measured, `datamerge-c4-s`: `StoredState="Embedded"` and the bytes as `Contents`)
        let spreads: String = merged.iter().filter(|(n, _)| n.starts_with("Spreads/")).map(|(_, t)| t.as_str()).collect();
        if spec["link"] == false {
            assert!(spreads.contains("StoredState=\"Embedded\"") && spreads.contains("<Contents><![CDATA["), "{name}: not embedded");
        } else {
            assert!(!spreads.contains("StoredState=\"Embedded\""), "{name}: embedded without asking");
        }
        // a PDF picture: page 1, cropped to the content of the visible layers, transparent (measured, `datamerge-c4-u`; the same for a PDF of many pages)
        if spec["file"] == "pdf.csv" {
            assert_eq!(spreads.matches("<PDFAttribute PageNumber=\"1\" PDFCrop=\"CropContentVisibleLayers\" TransparentBackground=\"true\"").count(), 2, "{name}: PDF attributes");
        }
        let pages = case["pages"].as_array().unwrap().len();
        if !spec["multiple"].as_bool().unwrap_or(false) {
            let section = designmap.split("<Section ").nth(1).and_then(|t| t.split('>').next()).unwrap_or("");
            assert!(section.contains(&format!("Length=\"{pages}\"")), "{name}: section length: {section}");
        }
        close(&summary(&mut ed), &json!(case["pages"].as_array().unwrap().iter().map(|p| json!({
            "margins": p["margins"], "size": p["size"],
            "items": p["items"].as_array().unwrap().iter().map(|i| {
                let mut o = json!({"type": i["type"], "bounds": i["bounds"]});
                if let Some(c) = i.get("contents") { o["contents"] = c.clone(); }
                o
            }).collect::<Vec<_>>()
        })).collect::<Vec<_>>()), name);
        // one undo step restores the template byte for byte, redo brings the merged document back
        assert!(ed.undo().is_some(), "{name}: undo");
        assert_eq!(ed.parts(), &original, "{name}: undo");
        assert!(ed.redo().is_some(), "{name}: redo");
        assert_eq!(ed.parts(), &merged, "{name}: redo");
        eprintln!("merged document matches InDesign: {name}");
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} cases checked");
}

/// A placeholder of a field the data source does not have (measured, `datamerge-c4-t`): InDesign's error, nothing merged, the template untouched.
#[test]
fn a_placeholder_without_its_field_is_refused() {
    let bytes = std::fs::read(oracle().join("fixtures/datamerge-merge/people.csv")).unwrap();
    let source = folio_idml::datamerge::parse(&bytes).unwrap();
    for (keep, name) in [("City", "some fields still match"), ("Other", "none does")] {
        let mut ed = Editor::new(package("single"));
        let original = ed.parts().clone();
        // the template's fields are Name and City: the data source has `keep` instead of one of them
        let mut fields: Vec<String> = source.fields.iter().map(|f| f.name.clone()).filter(|f| f == keep).collect();
        if keep == "Other" { fields = vec!["Other".into()]; }
        let command = folio_edit::commands::RegisteredCommand::MergeRecords {
            fields: fields.clone(), records: vec![vec!["x".into(); fields.len()]], range: String::new(), multiple: false, arrange: "rows".into(), row_spacing: 0.0,
            column_spacing: 0.0, margins: [36.0; 4], remove_blank_lines: false, fitting: "proportional".into(), center_image: false, graphics: BTreeMap::new(), embed: None, link: None,
        }.into();
        let error = ed.apply("merge", &[command]).expect_err(name);
        assert_eq!(error.message, "There is at least one data placeholder that cannot be found in the data source. Make sure all the placeholders correspond to the fields in the data source.", "{name}");
        assert_eq!(ed.parts(), &original, "{name}: the template is untouched");
    }
}
