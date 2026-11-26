//! PDF comments stored in the document: import, status, accept, delete, undo (rules: oracle/rules/pdfcomments.ko.md).
use folio_edit::{Editor, commands::RegisteredCommand};
use folio_model::pdfcomments::{PdfComment, PdfDestination, PdfReply};
use serde_json::json;
use std::collections::BTreeMap;

const STORY: &str = "Alpha bravo charlie delta echo.";

fn editor() -> Editor {
    Editor::new(BTreeMap::from([
        ("designmap.xml".into(), "<?xml version=\"1.0\"?>\n<Document Self=\"d\" StoryList=\"s\">\n\t<idPkg:Story src=\"Stories/Story_s.xml\" />\n\t<IndexingSortOption Self=\"x1\" Name=\"a\" />\n</Document>\n".into()),
        ("Stories/Story_s.xml".into(), format!("<Story Self=\"s\"><ParagraphStyleRange><CharacterStyleRange><Content>{STORY}</Content></CharacterStyleRange></ParagraphStyleRange></Story>")),
        ("Spreads/Spread_a.xml".into(), "<Spread Self=\"sp\"><Page Self=\"pg\" /><TextFrame Self=\"frame\" ParentStory=\"s\" /></Spread>".into()),
    ]))
}

fn text(start: usize, end: usize, kind: &str, content: &str) -> PdfComment {
    PdfComment {
        reviewer: "Reviewer 1".into(), content: content.into(), date: "2026-11-07T11:45:00".into(), kind: kind.into(), status: "OpenStatus".into(),
        applied: (kind == "StrikethroughTextType").then_some(false),
        destination: Some(PdfDestination { kind: "text".into(), target: "s".into(), start, end, ..Default::default() }),
        ..Default::default()
    }
}

fn note() -> PdfComment {
    PdfComment {
        reviewer: "Reviewer 1".into(), content: "Note".into(), date: "2026-11-07T08:45:00".into(), kind: "StickyNoteType".into(), status: "OpenStatus".into(),
        destination: Some(PdfDestination { kind: "page".into(), target: "pg".into(), rect: [300.0, 20.0, 316.0, 36.0], ..Default::default() }),
        replies: vec![PdfReply { reviewer: "Reviewer 2".into(), content: "Reply".into(), date: "2026-11-07T20:45:00".into(), ..Default::default() }],
        ..Default::default()
    }
}

fn import(ed: &mut Editor, comments: Vec<PdfComment>, again: bool) -> Result<folio_edit::Outcome, folio_edit::EditError> {
    ed.apply("Import PDF Comments", &[RegisteredCommand::ImportPdfComments {
        path: "reviews/a.pdf".into(), again, source: Some("$ID/folio:path-ref/r1".into()), comments: Some(comments),
    }.into()])
}

fn command(value: serde_json::Value) -> folio_edit::Command { serde_json::from_value::<RegisteredCommand>(value).unwrap().into() }

#[test]
fn import_writes_indesigns_elements_in_its_order_and_undoes() {
    let mut ed = editor();
    let original = ed.parts().clone();
    import(&mut ed, vec![note(), text(6, 11, "HighlightTextType", "Highlight"), text(12, 19, "StrikethroughTextType", "Strike")], false).unwrap();
    let designmap = ed.part("designmap.xml").unwrap();
    let at = |needle: &str| designmap.find(needle).unwrap_or_else(|| panic!("{needle}\n{designmap}"));
    // destinations by kind, then the comments, all in front of the IndexingSortOption
    assert!(at("<idPkg:Story") < at("<PDFCommentPageDestination"));
    assert!(at("<PDFCommentPageDestination") < at("<PDFCommentTextDestination"));
    assert!(at("<PDFCommentTextDestination") < at("<PDFComment Self"));
    assert!(at("</PDFComment>") < at("<IndexingSortOption"));
    let comments = ed.pdf_comments().unwrap();
    assert_eq!(comments.len(), 3);
    assert!(comments.iter().all(|c| c.file == "$ID/folio:path-ref/r1" && !c.id.is_empty() && c.destination.as_ref().is_some_and(|d| !d.id.is_empty())));
    assert_eq!((comments[0].replies.len(), comments[0].replies[0].id.is_empty()), (1, false));
    assert_eq!((comments[2].applied, comments[2].destination.as_ref().unwrap().start), (Some(false), 12));
    assert!(!designmap.contains("path-ref/r1\" CommentStatus=\"\""));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &original);
    ed.redo().unwrap();
    assert_eq!(ed.pdf_comments().unwrap().len(), 3);
}

#[test]
fn a_second_import_joins_each_group_and_a_repeat_is_refused_unless_asked() {
    let mut ed = editor();
    import(&mut ed, vec![note(), text(6, 11, "HighlightTextType", "Highlight")], false).unwrap();
    let before = ed.parts().clone();
    let error = import(&mut ed, vec![note()], false).unwrap_err();
    assert_eq!(error.code, "duplicate");
    assert_eq!(ed.parts(), &before);
    import(&mut ed, vec![note()], true).unwrap();
    let designmap = ed.part("designmap.xml").unwrap().to_owned();
    // groups stay together: both page destinations before the text destination, all destinations before the first comment
    let page_destinations: Vec<usize> = designmap.match_indices("<PDFCommentPageDestination ").map(|m| m.0).collect();
    assert_eq!(page_destinations.len(), 2);
    assert!(page_destinations[1] < designmap.find("<PDFCommentTextDestination").unwrap());
    assert!(designmap.rfind("<PDFCommentPageDestination ").unwrap() < designmap.find("<PDFComment Self").unwrap());
    assert_eq!(ed.pdf_comments().unwrap().len(), 3);
    assert_eq!(import(&mut ed, vec![], true).unwrap_err().code, "noComments");
    assert_eq!(import(&mut ed, vec![PdfComment { kind: String::new(), ..note() }], true).unwrap_err().code, "invalid");
}

#[test]
fn status_toggles_and_only_the_two_states_exist() {
    let mut ed = editor();
    import(&mut ed, vec![note(), text(6, 11, "HighlightTextType", "Highlight")], false).unwrap();
    let id = ed.pdf_comments().unwrap()[0].id.clone();
    let imported = ed.parts().clone();
    ed.apply("Update Comment", &[command(json!({"op": "setPdfCommentStatus", "ids": [id], "status": "resolved"}))]).unwrap();
    let comments = ed.pdf_comments().unwrap();
    assert_eq!((comments[0].status.as_str(), comments[1].status.as_str()), ("ResolvedStatus", "OpenStatus"));
    ed.apply("Update Comment", &[command(json!({"op": "setPdfCommentStatus", "ids": [id], "status": "open"}))]).unwrap();
    assert_eq!(ed.parts(), &imported);
    assert!(ed.apply("x", &[command(json!({"op": "setPdfCommentStatus", "ids": [id], "status": "accepted"}))]).is_err());
    assert_eq!(ed.apply("x", &[command(json!({"op": "setPdfCommentStatus", "ids": ["nope"], "status": "open"}))]).unwrap_err().code, "notFound");
}

#[test]
fn delete_takes_the_replies_and_the_destination_along() {
    let mut ed = editor();
    let original = ed.parts().clone();
    import(&mut ed, vec![note(), text(6, 11, "HighlightTextType", "Highlight")], false).unwrap();
    let ids: Vec<String> = ed.pdf_comments().unwrap().iter().map(|c| c.id.clone()).collect();
    ed.apply("Delete Comment", &[command(json!({"op": "removePdfComment", "ids": [ids[0]]}))]).unwrap();
    let designmap = ed.part("designmap.xml").unwrap();
    assert!(!designmap.contains("Reply") && !designmap.contains("PDFCommentPageDestination"));
    assert!(designmap.contains("PDFCommentTextDestination"));
    assert_eq!(ed.pdf_comments().unwrap().len(), 1);
    ed.apply("Delete Comment", &[command(json!({"op": "removePdfComment", "ids": [ids[1]]}))]).unwrap();
    assert_eq!(ed.part("designmap.xml"), original.get("designmap.xml").map(String::as_str));
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.pdf_comments().unwrap().len(), 2);
}

#[test]
fn accept_removes_the_struck_text_leaves_one_feff_and_applies_the_comment() {
    let mut ed = editor();
    import(&mut ed, vec![
        text(12, 19, "StrikethroughTextType", "charlie"),
        text(20, 25, "UnderlineTextType", "delta"),
        text(6, 11, "HighlightTextType", "bravo"),
    ], false).unwrap();
    let ids: Vec<String> = ed.pdf_comments().unwrap().iter().map(|c| c.id.clone()).collect();
    let imported = ed.parts().clone();
    ed.apply("Apply Comment", &[command(json!({"op": "acceptPdfComment", "id": ids[0]}))]).unwrap();
    // "charlie" and the space after it go (a space stood on both sides), one U+FEFF stays
    assert!(ed.part("Stories/Story_s.xml").unwrap().contains("Alpha bravo \u{feff}delta echo.") || ed.part("Stories/Story_s.xml").unwrap().contains("Alpha bravo &#xFEFF;delta echo."), "{}", ed.part("Stories/Story_s.xml").unwrap());
    let comments = ed.pdf_comments().unwrap();
    assert_eq!((comments[0].status.as_str(), comments[0].orphan, comments[0].applied), ("ResolvedStatus", true, Some(true)));
    // the underline of "delta" followed its text (20..25 → 13..18); the highlight before it stayed
    let d = |i: usize| { let d = comments[i].destination.as_ref().unwrap(); (d.start, d.end) };
    assert_eq!((d(1), d(2)), ((13, 18), (6, 11)));
    assert!(!comments[1].orphan && !comments[2].orphan);
    // a second accept, a highlight and an orphan are refused
    assert!(ed.apply("x", &[command(json!({"op": "acceptPdfComment", "id": ids[0]}))]).is_err());
    assert_eq!(ed.apply("x", &[command(json!({"op": "acceptPdfComment", "id": ids[2]}))]).unwrap_err().code, "unsupported");
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &imported);
}
