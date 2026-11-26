//! Read-only PDF annotation catalogue. No document edits or PDF actions are executed.
//! What counts as a comment, how replies nest and which InDesign type each annotation gets follows `Document.importPdfComments`
//! on InDesign 2025 (rules and measurements: `next/oracle/rules/pdfcomments.ko.md`).
use lopdf::{Dictionary, Document, Object};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

/// A reply is its own small record (InDesign: `Replies`; no status, no replies of its own).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub id: String,
    pub author: String,
    pub content: String,
    /// Local time without zone, `YYYY-MM-DDTHH:MM:SS` (empty when the PDF has no usable date).
    pub date: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    /// One-based PDF page number, independent of printed page labels.
    pub page: u32,
    /// The PDF annotation subtype as written in the file.
    pub kind: String,
    /// InDesign's comment type (`CommentTypeEnum`): sticky, highlight, underline, squiggly, strike, insert, typewriter, textbox,
    /// callout, pencil, line, oval, rect, polygon, connected, stamp.
    #[serde(rename = "type")]
    pub comment_type: String,
    pub author: String,
    pub content: String,
    /// The annotation's `/Subj`; InDesign shows a stamp's content as `<subject> : <content>` when there is one (measured 2026-11-12).
    pub subject: String,
    /// Raw PDF `M` / `CreationDate` strings.
    pub modified: String,
    pub created: String,
    /// InDesign's `commentDate`: the PDF `M` date read as local time (the `Z` is ignored); empty when absent.
    pub date: String,
    /// The PDF's own review state. InDesign ignores it (its review mark is Open/Resolved inside the document).
    pub state: String,
    pub state_model: String,
    /// The annotation's `Rect` in PDF user space `[x0, y0, x1, y1]` (normalised so x0 <= x1 and y0 <= y1); `None` when it has none.
    pub rect: Option<[f64; 4]>,
    /// `QuadPoints` of a text markup (eight numbers per quad, PDF user space).
    pub quads: Vec<[f64; 8]>,
    /// Where a comment points (InDesign's `TargetPoint`, measured 2026-11-12): an arrow's end point (`/L` x2 y2, whichever end has the
    /// arrowhead) and a callout's `/CL` start; PDF user space. `None` for the other kinds.
    pub target: Option<[f64; 2]>,
    pub replies: Vec<Reply>,
}

/// What a PDF page says about the document it was made from (`PieceInfo/InDesign`, measured: only `OriginalDocumentID` counts).
#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageOrigin {
    /// The page has `PieceInfo/InDesign`.
    pub indesign: bool,
    /// Its `OriginalDocumentID`.
    pub document: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub sha256: String,
    pub page_count: usize,
    /// The box each PDF page's user space is laid out in `[x0, y0, x1, y1]` (TrimBox, else CropBox, else MediaBox), in page order.
    pub boxes: Vec<[f64; 4]>,
    /// Where each page says it came from, in page order.
    pub origins: Vec<PageOrigin>,
    /// Top-level comments only; replies are inside `replies` and are not counted (InDesign's `pdfComments.length`).
    pub comments: Vec<Comment>,
}

fn value<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    doc.dereference(dict.get(key).ok()?).ok().map(|(_, o)| o)
}

fn text(doc: &Document, dict: &Dictionary, key: &[u8]) -> Result<String, String> {
    match value(doc, dict, key) {
        None => Ok(String::new()),
        Some(o) => lopdf::decode_text_string(o).map_err(|_| format!("Invalid PDF comment field {}", String::from_utf8_lossy(key))),
    }
}

/// InDesign 2025 comment type of a PDF annotation, or `None` when InDesign does not import it (FileAttachment, Popup, Link,
/// Widget, and subtypes that were not measured). The intent `/IT` decides the kind of a Line (`LineArrow` → arrow), a Polygon
/// (`PolygonCloud` → cloud) and a FreeText (`FreeTextTypewriter`, `FreeTextCallout`); for those three any other intent than the
/// subtype's own name makes InDesign skip the annotation (measured 2026-11-12: `LineDimension`, `PolygonDimension`, an unknown name);
/// PolyLine, Ink, Square and Circle ignore the intent.
fn classify(subtype: &str, intent: &str) -> Option<&'static str> {
    Some(match (subtype, intent) {
        ("Text", _) => "sticky",
        ("Highlight", _) => "highlight",
        ("Underline", _) => "underline",
        ("Squiggly", _) => "squiggly",
        ("StrikeOut", _) => "strike",
        ("Caret", _) => "insert",
        ("FreeText", "" | "FreeText") => "textbox",
        ("FreeText", "FreeTextTypewriter") => "typewriter",
        // Only the intent decides: a plain text box with a (degenerate) /CL array stays a text box in InDesign (measured).
        ("FreeText", "FreeTextCallout") => "callout",
        ("Line", "" | "Line") => "line",
        ("Line", "LineArrow") => "arrow",
        ("Square", _) => "rect",
        ("Circle", _) => "oval",
        ("Polygon", "" | "Polygon") => "polygon",
        ("Polygon", "PolygonCloud") => "cloud",
        ("PolyLine", _) => "connected",
        ("Ink", _) => "pencil",
        ("Stamp", _) => "stamp",
        _ => return None,
    })
}

/// `D:YYYYMMDDHHmmSS...` as `YYYY-MM-DDTHH:MM:SS`, ignoring the zone suffix like InDesign does. Empty when there is no year.
fn local_date(raw: &str) -> String {
    let digits: String = raw.strip_prefix("D:").unwrap_or(raw).chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 { return String::new(); }
    let part = |from: usize, len: usize, default: &str| -> String { digits.get(from..from + len).map(str::to_owned).unwrap_or_else(|| default.to_owned()) };
    format!("{}-{}-{}T{}:{}:{}", &digits[..4], part(4, 2, "01"), part(6, 2, "01"), part(8, 2, "00"), part(10, 2, "00"), part(12, 2, "00"))
}

pub fn read(path: &Path) -> Result<Catalogue, String> {
    let metadata = path.metadata().map_err(|_| "Cannot read project PDF")?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err("PDF comments require a file no larger than 128 MiB".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "Cannot read project PDF")?;
    catalogue(&bytes)
}

fn numbers(doc: &Document, dict: &Dictionary, key: &[u8]) -> Vec<f64> {
    let Some(array) = value(doc, dict, key).and_then(|o| o.as_array().ok()) else { return Vec::new() };
    array.iter().filter_map(|o| doc.dereference(o).ok().and_then(|(_, o)| o.as_float().ok()).map(f64::from)).collect()
}

fn rect_of(values: &[f64]) -> Option<[f64; 4]> {
    let [a, b, c, d] = *values.get(..4)?.first_chunk::<4>()?;
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

/// The page's box (boxes and resources are inherited from the page tree).
fn page_box(doc: &Document, page: &Dictionary) -> Option<[f64; 4]> {
    let mut at = page;
    for _ in 0..16 {
        for key in [&b"TrimBox"[..], b"CropBox", b"MediaBox"] {
            if let Some(r) = rect_of(&numbers(doc, at, key)) { return Some(r); }
        }
        at = value(doc, at, b"Parent")?.as_dict().ok()?;
    }
    None
}

fn origin_of(doc: &Document, page: &Dictionary) -> PageOrigin {
    let app = value(doc, page, b"PieceInfo").and_then(|o| o.as_dict().ok()).and_then(|d| value(doc, d, b"InDesign")).and_then(|o| o.as_dict().ok());
    match app {
        None => PageOrigin::default(),
        Some(app) => PageOrigin { indesign: true, document: text(doc, app, b"OriginalDocumentID").ok().filter(|t| !t.is_empty()) },
    }
}

struct Raw {
    id: String,
    parent: Option<String>,
    comment: Comment,
}

pub fn catalogue(bytes: &[u8]) -> Result<Catalogue, String> {
    let doc = Document::load_mem(bytes).map_err(|_| "Cannot parse PDF")?;
    if doc.is_encrypted() { return Err("Encrypted PDF comments are not supported".into()); }
    let pages = doc.get_pages();
    let mut raws: Vec<Raw> = Vec::new();
    let mut boxes: Vec<[f64; 4]> = Vec::new();
    let mut origins: Vec<PageOrigin> = Vec::new();
    for (page, page_id) in &pages {
        let dictionary = doc.get_dictionary(*page_id).map_err(|_| "Invalid PDF page")?;
        boxes.push(page_box(&doc, dictionary).unwrap_or([0.0, 0.0, 612.0, 792.0]));
        origins.push(origin_of(&doc, dictionary));
        let Some(annotations) = value(&doc, dictionary, b"Annots") else { continue };
        let annotations = annotations.as_array().map_err(|_| "Invalid PDF annotations")?;
        for (index, reference) in annotations.iter().enumerate() {
            if raws.len() >= 100_000 { return Err("Too many PDF comments".into()); }
            let (object_id, object) = doc.dereference(reference).map_err(|_| "Invalid PDF annotation reference")?;
            let annotation = object.as_dict().map_err(|_| "Invalid PDF annotation")?;
            let kind = value(&doc, annotation, b"Subtype").and_then(|o| o.as_name().ok()).and_then(|s| std::str::from_utf8(s).ok()).ok_or("Missing PDF annotation subtype")?;
            let intent = value(&doc, annotation, b"IT").and_then(|o| o.as_name().ok()).and_then(|s| std::str::from_utf8(s).ok()).unwrap_or("");
            let Some(comment_type) = classify(kind, intent) else { continue };
            let id = object_id.map(|(n,g)| format!("{n}:{g}")).unwrap_or_else(|| format!("page-{page}-{index}"));
            let modified = text(&doc, annotation, b"M")?;
            let date = local_date(&modified);
            raws.push(Raw {
                parent: annotation.get(b"IRT").ok().and_then(|o| o.as_reference().ok()).map(|(n,g)| format!("{n}:{g}")),
                id: id.clone(),
                comment: Comment {
                    id, page: *page, kind: kind.to_owned(), comment_type: comment_type.to_owned(),
                    author: text(&doc, annotation, b"T")?,
                    content: text(&doc, annotation, b"Contents")?,
                    subject: text(&doc, annotation, b"Subj")?,
                    modified, created: text(&doc, annotation, b"CreationDate")?, date,
                    state: text(&doc, annotation, b"State")?,
                    state_model: text(&doc, annotation, b"StateModel")?,
                    rect: rect_of(&numbers(&doc, annotation, b"Rect")),
                    quads: numbers(&doc, annotation, b"QuadPoints").chunks_exact(8).map(|q| <[f64; 8]>::try_from(q).unwrap_or_default()).collect(),
                    target: match comment_type {
                        "arrow" => numbers(&doc, annotation, b"L").get(2..4).and_then(|p| <[f64; 2]>::try_from(p).ok()),
                        "callout" => numbers(&doc, annotation, b"CL").get(..2).and_then(|p| <[f64; 2]>::try_from(p).ok()),
                        _ => None,
                    },
                    replies: Vec::new(),
                },
            });
        }
    }
    // A comment with `IRT` is a reply (measured: a Caret grouped with a StrikeOut shows up as the StrikeOut's reply). Replies of
    // replies are flattened under the top-level comment; a reply whose parent was not imported stays a comment of its own.
    let index: HashMap<&str, usize> = raws.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
    let root_of = |start: usize| -> Option<usize> {
        let mut at = start;
        for _ in 0..32 {
            let parent = raws[at].parent.as_ref()?;
            let next = *index.get(parent.as_str())?;
            if next == at { return None; }
            if raws[next].parent.is_none() { return Some(next); }
            at = next;
        }
        None
    };
    let roots: Vec<Option<usize>> = (0..raws.len()).map(|i| if raws[i].parent.is_some() { root_of(i) } else { None }).collect();
    let mut comments: Vec<Comment> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    let mut replies: Vec<(usize, Reply)> = Vec::new();
    for (i, raw) in raws.into_iter().enumerate() {
        match roots[i] {
            Some(root) => replies.push((root, Reply { id: raw.id, author: raw.comment.author, content: raw.comment.content, date: raw.comment.date })),
            None => { slot.insert(i, comments.len()); comments.push(raw.comment); }
        }
    }
    for (root, reply) in replies {
        if let Some(&at) = slot.get(&root) { comments[at].replies.push(reply); }
    }
    Ok(Catalogue { sha256: Sha256::digest(bytes).iter().map(|b|format!("{b:02x}")).collect(), page_count: pages.len(), boxes, origins, comments })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn unicode_replies_indirect_arrays_and_empty_pages() {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let unicode = Object::String(vec![0xfe,0xff,0xd5,0x5c,0xae,0x00], lopdf::StringFormat::Hexadecimal);
        let note = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Text", "T"=>unicode.clone(), "Contents"=>unicode});
        let reply = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Text", "T"=>Object::string_literal("Second"), "Contents"=>Object::string_literal("Re"), "IRT"=>note, "State"=>Object::string_literal("Accepted"), "StateModel"=>Object::string_literal("Review")});
        let popup = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Popup", "Parent"=>note});
        let annots = doc.add_object(vec![note.into(),reply.into(),popup.into()]);
        let first = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Annots"=>annots});
        let second = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages});
        doc.objects.insert(pages, dictionary! {"Type"=>"Pages", "Kids"=>vec![first.into(),second.into()], "Count"=>2}.into());
        let root = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
        doc.trailer.set("Root",root);
        let mut bytes=Vec::new();doc.save_to(&mut bytes).unwrap();
        let result=catalogue(&bytes).unwrap();
        assert_eq!(result.page_count,2);
        // InDesign counts top-level comments only: the reply nests under the note and the popup is not a comment.
        assert_eq!(result.comments.len(),1);
        assert_eq!(result.comments[0].author,"한글");
        assert_eq!(result.comments[0].content,"한글");
        assert_eq!(result.comments[0].comment_type,"sticky");
        assert_eq!(result.comments[0].replies.len(),1);
        assert_eq!(result.comments[0].replies[0].author,"Second");
        assert_eq!(result.comments[0].replies[0].content,"Re");
        assert!(result.comments[0].modified.is_empty() && result.comments[0].date.is_empty());
    }

    #[test]
    fn pdf_without_comments_is_an_empty_catalogue() {
        // InDesign: "PDF has no comments. Select another PDF to continue." (nothing is imported); a file attachment and a link do not count.
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let attachment = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"FileAttachment", "Contents"=>Object::string_literal("x")});
        let link = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Link"});
        let page = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Annots"=>vec![attachment.into(), link.into()]});
        doc.objects.insert(pages, dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into()], "Count"=>1}.into());
        let root = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
        doc.trailer.set("Root", root);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let result = catalogue(&bytes).unwrap();
        assert_eq!(result.page_count, 1);
        assert!(result.comments.is_empty());
    }

    #[test]
    fn dates_ignore_the_zone_like_indesign() {
        assert_eq!(local_date("D:20261107084500Z"), "2026-11-07T08:45:00");
        assert_eq!(local_date("D:20261107084500+09'00'"), "2026-11-07T08:45:00");
        assert_eq!(local_date("D:2026"), "2026-01-01T00:00:00");
        assert_eq!(local_date("D:20261107"), "2026-11-07T00:00:00");
        assert_eq!(local_date(""), "");
    }

    #[cfg(not(has_comment_sample))]
    #[test]
    #[ignore = "optional annotation PDF fixture is missing"]
    fn self_authored_pdf_comments() { panic!("optional annotation PDF fixture is missing"); }

    #[cfg(has_comment_sample)]
    #[test]
    fn self_authored_pdf_comments() {
        let bytes = include_bytes!("../../../../../artifacts/pdfcomments-20261023/measurement/sample.pdf");
        let result = catalogue(bytes).unwrap();
        assert_eq!(result.page_count, 2);
        assert_eq!(result.comments.len(), 10);
        for (i, row) in result.comments.iter().enumerate() {
            assert_eq!(row.page, (i / 5 + 1) as u32);
            assert_eq!(row.author, format!("Reviewer {}", row.page));
            assert_eq!(row.kind, ["Text", "Highlight", "StrikeOut", "Underline", "FreeText"][i % 5]);
            assert_eq!(row.comment_type, ["sticky", "highlight", "strike", "underline", "textbox"][i % 5]);
            assert_eq!(row.content, format!("{} comment {}", ["note", "highlight", "strikeout", "underline", "textbox"][i % 5], row.page));
            assert_eq!(row.modified, "D:20261023103000Z");
            assert_eq!(row.date, "2026-10-23T10:30:00");
            assert!(row.state.is_empty());
        }
        assert_eq!(result.sha256.len(), 64);
        assert!(catalogue(b"not a PDF").is_err());
    }

    /// Compares with what InDesign 2025 made of the same PDF (`Document.importPdfComments`, dumped by
    /// `artifacts/pdfcomments-c2-20261023/tools/p14_golden.py`). `annot.pdf` keeps the PDF order; for `rich.pdf` InDesign
    /// puts the drawing annotations after the text ones in an order that is not understood yet, so it is compared as a set.
    fn compare(pdf: &[u8], golden: &str, ordered: bool) {
        let result = catalogue(pdf).unwrap();
        let golden: Vec<serde_json::Value> = serde_json::from_str(golden).unwrap();
        let key = |r: &serde_json::Value| format!("{}|{}", r["type"].as_str().unwrap(), r["content"].as_str().unwrap());
        let mut ours: Vec<serde_json::Value> = result.comments.iter().map(|c| serde_json::json!({
            "type": c.comment_type,
            // InDesign prefixes a stamp's content with its subject: "<Subj> : <content>" (measured 2026-11-12; c2's "Stamp : " was the subject).
            "content": if c.comment_type == "stamp" && !c.subject.is_empty() { format!("{} : {}", c.subject, c.content) } else { c.content.clone() },
            "reviewer": c.author,
            // InDesign reports a missing date as the Windows epoch (1600-12-31); there it shows nothing.
            "date": c.date,
            "replies": c.replies.iter().map(|r| serde_json::json!({"reviewer": r.author, "content": r.content, "date": r.date})).collect::<Vec<_>>(),
        })).collect();
        let mut theirs: Vec<serde_json::Value> = golden.iter().map(|g| {
            let mut g = g.clone();
            if g["date"].as_str().unwrap().starts_with("1600-") { g["date"] = "".into(); }
            serde_json::json!({"type": g["type"], "content": g["content"], "reviewer": g["reviewer"], "date": g["date"], "replies": g["replies"]})
        }).collect();
        if !ordered { ours.sort_by_key(key); theirs.sort_by_key(key); }
        assert_eq!(ours.len(), theirs.len(), "comment count");
        for (a, b) in ours.iter().zip(&theirs) { assert_eq!(a, b); }
    }

    #[cfg(not(has_comment_annot))]
    #[test]
    #[ignore = "optional annotation PDF fixture is missing"]
    fn pages_say_which_document_they_came_from() { panic!("optional annotation PDF fixture is missing"); }

    #[cfg(has_comment_annot)]
    #[test]
    fn pages_say_which_document_they_came_from() {
        // InDesign's PDF of the measured document: each page names the document (the same id as the document's XMP OriginalDocumentID)
        let result = catalogue(include_bytes!("../../../../oracle/rules/pdfcomments-annot.pdf")).unwrap();
        assert_eq!(result.origins.len(), 2);
        assert!(result.origins.iter().all(|o| o.indesign && o.document.as_deref() == Some("xmp.did:8973be2d-7d02-504d-8f3f-902efe2883ee")));
        // a PDF made elsewhere has none
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages});
        doc.objects.insert(pages, dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into()], "Count"=>1}.into());
        let root = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
        doc.trailer.set("Root", root);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert_eq!(catalogue(&bytes).unwrap().origins, vec![PageOrigin::default()]);
    }

    /// 2026-11-12 (pdfcomments-c5): the intent `/IT` decides Line (arrow), Polygon (cloud) and FreeText (typewriter, callout) and any other
    /// intent skips the annotation; the arrow's end point and the callout's leader start are the destination's TargetPoint. The PDFs are
    /// made from the measured document; InDesign's import of each is the golden (`tools/g5.py`, `tools/make_rules.py` of `pdfcomments-c5`).
    #[cfg(not(has_comment_intents))]
    #[test]
    #[ignore = "optional annotation PDF fixture is missing"]
    fn intents_and_target_points_match_indesign_import() { panic!("optional annotation PDF fixture is missing"); }

    #[cfg(has_comment_intents)]
    #[test]
    fn intents_and_target_points_match_indesign_import() {
        let all = &include_bytes!("../../../../oracle/rules/pdfcomments-c5-all.pdf")[..];
        let intents = &include_bytes!("../../../../oracle/rules/pdfcomments-c5-intents.pdf")[..];
        let arrows = &include_bytes!("../../../../oracle/rules/pdfcomments-c5-arrows.pdf")[..];
        let callouts = &include_bytes!("../../../../oracle/rules/pdfcomments-c5-callouts.pdf")[..];
        compare(all, include_str!("../../../../oracle/rules/pdfcomments-c5-all.indesign.json"), false);
        compare(intents, include_str!("../../../../oracle/rules/pdfcomments-c5-intents.indesign.json"), false);
        compare(arrows, include_str!("../../../../oracle/rules/pdfcomments-c5-arrows.indesign.json"), false);
        compare(callouts, include_str!("../../../../oracle/rules/pdfcomments-c5-callouts.indesign.json"), false);
        compare(include_bytes!("../../../../oracle/rules/pdfcomments-c5-stamps.pdf"), include_str!("../../../../oracle/rules/pdfcomments-c5-stamps.indesign.json"), false);
        // 5 of the 20 intents are skipped by InDesign (the golden has the other 15)
        assert_eq!(catalogue(intents).unwrap().comments.len(), 15);
        let target: serde_json::Value = serde_json::from_str(include_str!("../../../../oracle/rules/pdfcomments-c5-target.indesign.json")).unwrap();
        for (pdf, key) in [(all, "all"), (arrows, "arrows"), (callouts, "callouts")] {
            let result = catalogue(pdf).unwrap();
            let entries = target[key].as_array().unwrap();
            assert!(entries.len() >= 5, "{key}");
            for g in entries {
                let content = g["content"].as_str().unwrap();
                let c = result.comments.iter().find(|c| c.content == content).unwrap_or_else(|| panic!("{key}: {content}"));
                let number = |v: &serde_json::Value, i: usize| v[i].as_f64().unwrap();
                // InDesign's HighlightRect is the PDF /Rect moved by a constant (the item's place); the TargetPoint moves with it
                let rect = c.rect.unwrap();
                let height = result.boxes[(c.page - 1) as usize][3];
                let (dx, dy) = (rect[0] - number(&g["rect"], 0), (height - rect[3]) - number(&g["rect"], 1));
                match c.target {
                    Some([x, y]) => {
                        assert!(matches!(c.comment_type.as_str(), "arrow" | "callout"), "{key}: {content}");
                        assert!((x - dx - number(&g["point"], 0)).abs() < 0.01 && ((height - y) - dy - number(&g["point"], 1)).abs() < 0.01, "{key}: {content}: target {:?} vs {}", c.target, g["point"]);
                    }
                    None => assert_eq!(g["point"], serde_json::json!([0.0, 0.0]), "{key}: {content}"),
                }
            }
        }
    }

    #[cfg(not(has_comment_pair))]
    #[test]
    #[ignore = "optional annotation PDF fixture is missing"]
    fn annotations_match_indesign_import() { panic!("optional annotation PDF fixture is missing"); }

    #[cfg(has_comment_pair)]
    #[test]
    fn annotations_match_indesign_import() {
        compare(include_bytes!("../../../../oracle/rules/pdfcomments-annot.pdf"), include_str!("../../../../oracle/rules/pdfcomments-annot.indesign.json"), true);
        compare(include_bytes!("../../../../oracle/rules/pdfcomments-rich.pdf"), include_str!("../../../../oracle/rules/pdfcomments-rich.indesign.json"), false);
    }
}
