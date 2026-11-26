//! PDF comments in the document designmap: `PDFComment` (with `Reply` children) and the destination elements it names by
//! `CommentDestination`. Measured with InDesign 2025 (`next/oracle/rules/pdfcomments.ko.md`, `artifacts/pdfcomments-c2-20261023`).
//! The reader gives the model; the writers give the XML a new comment is spliced in as (InDesign's own spelling and order: the
//! destinations first, then the comments, tab indented).
use crate::xml::{self, Element};
use folio_model::pdfcomments::{PdfComment, PdfDestination, PdfReply};
use std::collections::BTreeMap;

pub const OPEN: &str = "OpenStatus";
pub const RESOLVED: &str = "ResolvedStatus";
pub const PAGE: &str = "page";
pub const PAGE_ITEM: &str = "pageItem";
pub const TEXT: &str = "text";

/// The kinds of comment a text edit can be accepted for (InDesign shows Accept on these; measured: a strikethrough comment).
pub fn deletes_text(kind: &str) -> bool { kind == "StrikethroughTextType" }

/// IDML `CommentType` of the type name the PDF reader (`folio_links::pdfcomments`) gives, spelled as InDesign's `CommentTypeEnum`
/// (`STICKY_NOTE_TYPE` → `StickyNoteType`).
pub fn kind_of(comment_type: &str) -> Option<&'static str> {
    Some(match comment_type {
        "sticky" => "StickyNoteType", "highlight" => "HighlightTextType", "underline" => "UnderlineTextType",
        "squiggly" => "SquigglyTextType", "strike" => "StrikethroughTextType", "replace" => "ReplaceTextType",
        "insert" => "InsertTextType", "typewriter" => "TextTypewriterType", "textbox" => "TextBoxType",
        "pencil" => "FreeformDrawingType", "line" => "LineSegmentType", "oval" => "OvalType", "rect" => "RectangleType",
        "polygon" => "PolygonType", "arrow" => "ArrowType", "callout" => "TextCalloutType", "stamp" => "StampType",
        "connected" => "ConnectedLinesType", "cloud" => "CloudType",
        _ => return None,
    })
}

fn numbers<const N: usize>(text: &str) -> [f64; N] {
    let mut out = [0.0; N];
    for (slot, word) in out.iter_mut().zip(text.split_whitespace()) { *slot = word.parse().unwrap_or(0.0); }
    out
}

fn destination(e: &Element<'_>) -> Option<PdfDestination> {
    let value = |key: &str| e.attr(key).unwrap_or_default().to_owned();
    let id = value("Self");
    Some(match e.name {
        "PDFCommentPageDestination" => PdfDestination {
            id, kind: PAGE.into(), target: value("CommentPageDestinationPage"),
            point: numbers(e.attr("CommentPageDestinationTargetPoint").unwrap_or_default()),
            rect: rect(e, "CommentPageDestinationHighlightRect"), ..Default::default()
        },
        "PDFCommentPageItemDestination" => PdfDestination {
            id, kind: PAGE_ITEM.into(), target: value("CommentPageItemDestinationPageItem"),
            point: numbers(e.attr("CommentPageItemDestinationTargetPoint").unwrap_or_default()),
            rect: rect(e, "CommentPageItemDestinationHighlightRect"), ..Default::default()
        },
        "PDFCommentTextDestination" => PdfDestination {
            id, kind: TEXT.into(), target: value("CommentTextDestinationStory"),
            start: e.attr("CommentTextDestinationStartIndex").and_then(|v| v.parse().ok()).unwrap_or(0),
            end: e.attr("CommentTextDestinationEndIndex").and_then(|v| v.parse().ok()).unwrap_or(0), ..Default::default()
        },
        _ => return None,
    })
}

fn rect(e: &Element<'_>, name: &str) -> [f64; 4] {
    let Some(r) = e.child("Properties").and_then(|p| p.child(name)) else { return [0.0; 4] };
    let side = |key: &str| r.attr(key).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    [side("Left"), side("Top"), side("Right"), side("Bottom")]
}

/// The comments of a designmap, in document order.
pub fn from_element(dm: &Element<'_>) -> Vec<PdfComment> {
    let destinations: BTreeMap<String, PdfDestination> = dm.elements().filter_map(destination).map(|d| (d.id.clone(), d)).collect();
    dm.children_named("PDFComment").map(|c| {
        let value = |key: &str| c.attr(key).unwrap_or_default().to_owned();
        PdfComment {
            id: value("Self"), reviewer: value("CommentReviewer"), content: value("CommentContent"), date: value("CommentDate"),
            kind: value("CommentType"), file: value("CommentFilePath"), status: value("CommentStatus"),
            orphan: c.attr("CommentIsOrphan") == Some("true"),
            applied: c.attr("CommentIsApplied").map(|v| v == "true"),
            destination: c.attr("CommentDestination").and_then(|d| destinations.get(d)).cloned(),
            replies: c.children_named("Reply").map(|r| {
                let value = |key: &str| r.attr(key).unwrap_or_default().to_owned();
                PdfReply { id: value("Self"), reviewer: value("ReplyReviewer"), content: value("ReplyContent"), date: value("ReplyDate") }
            }).collect(),
        }
    }).collect()
}

pub fn read(parts: &BTreeMap<String, String>) -> Result<Vec<PdfComment>, String> {
    let text = parts.get("designmap.xml").ok_or("missing designmap.xml")?;
    if !text.contains("<PDFComment") { return Ok(Vec::new()); }
    Ok(from_element(&xml::parse(text).map_err(|e| e.0)?))
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"), '\r' => out.push_str("&#xD;"), '\t' => out.push_str("&#x9;"),
            c => out.push(c),
        }
    }
    out
}

fn num(v: f64) -> String { if v == 0.0 { "0".into() } else { format!("{v}") } }

/// The destination element as InDesign writes it.
pub fn destination_xml(d: &PdfDestination) -> String {
    let id = escape(&d.id);
    let [x, y] = d.point;
    let [l, t, r, b] = d.rect;
    match d.kind.as_str() {
        PAGE => format!("<PDFCommentPageDestination Self=\"{id}\" CommentPageDestinationPage=\"{}\" CommentPageDestinationTargetPoint=\"{} {}\">\n\t\t<Properties>\n\t\t\t<CommentPageDestinationHighlightRect Left=\"{}\" Top=\"{}\" Right=\"{}\" Bottom=\"{}\" />\n\t\t</Properties>\n\t</PDFCommentPageDestination>",
            escape(&d.target), num(x), num(y), num(l), num(t), num(r), num(b)),
        PAGE_ITEM => format!("<PDFCommentPageItemDestination Self=\"{id}\" CommentPageItemDestinationPageItem=\"{}\" CommentPageItemDestinationTargetPoint=\"{} {}\">\n\t\t<Properties>\n\t\t\t<CommentPageItemDestinationHighlightRect Left=\"{}\" Top=\"{}\" Right=\"{}\" Bottom=\"{}\" />\n\t\t</Properties>\n\t</PDFCommentPageItemDestination>",
            escape(&d.target), num(x), num(y), num(l), num(t), num(r), num(b)),
        _ => format!("<PDFCommentTextDestination Self=\"{id}\" CommentTextDestinationStory=\"{}\" CommentTextDestinationStartIndex=\"{}\" CommentTextDestinationEndIndex=\"{}\" />",
            escape(&d.target), d.start, d.end),
    }
}

/// The comment element as InDesign writes it (replies inside). `CommentIsApplied` only for comments that can be applied.
pub fn comment_xml(c: &PdfComment) -> String {
    let mut out = format!("<PDFComment Self=\"{}\" CommentReviewer=\"{}\" CommentContent=\"{}\" CommentDate=\"{}\" CommentType=\"{}\" CommentFilePath=\"{}\" CommentStatus=\"{}\" CommentIsOrphan=\"{}\"",
        escape(&c.id), escape(&c.reviewer), escape(&c.content), escape(&c.date), escape(&c.kind), escape(&c.file), escape(&c.status), c.orphan);
    if let Some(applied) = c.applied { out.push_str(&format!(" CommentIsApplied=\"{applied}\"")); }
    if let Some(d) = &c.destination { out.push_str(&format!(" CommentDestination=\"{}\"", escape(&d.id))); }
    if c.replies.is_empty() { out.push_str(" />"); return out; }
    out.push('>');
    for r in &c.replies {
        out.push_str(&format!("\n\t\t<Reply Self=\"{}\" ReplyReviewer=\"{}\" ReplyContent=\"{}\" ReplyDate=\"{}\" />", escape(&r.id), escape(&r.reviewer), escape(&r.content), escape(&r.date)));
    }
    out.push_str("\n\t</PDFComment>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../../oracle/rules/pdfcomments-persist.designmap.xml");

    #[test]
    fn reads_what_indesign_wrote() {
        let comments = read(&BTreeMap::from([("designmap.xml".to_owned(), SAMPLE.to_owned())])).unwrap();
        assert_eq!(comments.len(), 14);
        let note = &comments[0];
        assert_eq!((note.kind.as_str(), note.status.as_str(), note.orphan, note.applied), ("StickyNoteType", RESOLVED, false, None));
        assert_eq!(note.date, "2026-11-07T08:45:00");
        assert_eq!(note.replies.len(), 1);
        assert_eq!(note.replies[0].content, "Reply to note");
        let page = note.destination.as_ref().unwrap();
        assert_eq!((page.kind.as_str(), page.target.as_str(), page.rect), (PAGE, "ud6", [300.0, 166.0, 316.0, 182.0]));
        let strike = &comments[3];
        assert_eq!((strike.kind.as_str(), strike.applied), ("StrikethroughTextType", Some(false)));
        let text = strike.destination.as_ref().unwrap();
        assert_eq!((text.kind.as_str(), text.target.as_str(), text.start, text.end), (TEXT, "u13a", 12, 19));
        assert!(comments[6].orphan && comments[6].kind == "InsertTextType");
        assert_eq!(comments[5].destination.as_ref().unwrap().kind, PAGE_ITEM);
    }

    #[test]
    fn written_elements_read_back_the_same() {
        let comment = PdfComment {
            id: "u9".into(), reviewer: "A & B".into(), content: "say \"x\"\n<y>".into(), date: "2026-11-07T08:45:00".into(),
            kind: "StickyNoteType".into(), file: "$ID/folio:path-ref/r1".into(), status: OPEN.into(), orphan: false, applied: None,
            destination: Some(PdfDestination { id: "u8".into(), kind: PAGE.into(), target: "ud6".into(), point: [0.0, 0.0], rect: [300.0, 20.0, 316.0, 36.0], ..Default::default() }),
            replies: vec![PdfReply { id: "u10".into(), reviewer: "R".into(), content: "re".into(), date: "2026-11-07T20:45:00".into() }],
        };
        let text = format!("<Document>\n\t{}\n\t{}\n</Document>", destination_xml(comment.destination.as_ref().unwrap()), comment_xml(&comment));
        let back = read(&BTreeMap::from([("designmap.xml".to_owned(), text)])).unwrap();
        assert_eq!(back, vec![comment]);
    }

    #[test]
    fn writer_matches_indesign_spelling() {
        let comments = read(&BTreeMap::from([("designmap.xml".to_owned(), SAMPLE.to_owned())])).unwrap();
        // the strikethrough comment of the sample, spelled by the writer, is the line InDesign wrote (ids and paths aside)
        let line = SAMPLE.lines().find(|l| l.contains("Self=\"u17e\"")).unwrap().trim();
        assert_eq!(comment_xml(&comments[3]), line);
        let first = SAMPLE.split("<PDFCommentPageDestination Self=\"u174\"").nth(1).unwrap().split("</PDFCommentPageDestination>").next().unwrap();
        assert_eq!(destination_xml(comments[0].destination.as_ref().unwrap()), format!("<PDFCommentPageDestination Self=\"u174\"{first}</PDFCommentPageDestination>").replace("\r\n", "\n"));
    }
}
