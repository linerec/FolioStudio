//! PDF comments kept in a document (IDML designmap `PDFComment` with its `Reply` children and the `PDFComment*Destination`
//! elements it points to). Measured with InDesign 2025: `next/oracle/rules/pdfcomments.ko.md`, scenarios `pdfcomments3`.
use serde::{Deserialize, Serialize};

/// One imported comment. A comment of a PDF's top level only; the answers are `replies`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PdfComment {
    /// IDML `Self`; empty in a comment that is still to be imported (the editor gives it one).
    pub id: String,
    pub reviewer: String,
    pub content: String,
    /// `CommentDate`: local time without zone, `YYYY-MM-DDTHH:MM:SS` (empty when the PDF had none).
    pub date: String,
    /// IDML `CommentType` (`StickyNoteType`, `HighlightTextType`, `StrikethroughTextType`, `TextBoxType` ...).
    pub kind: String,
    /// IDML `CommentFilePath` as stored: `$ID/` and a project reference (`folio:path-ref/<id>`), never a path of this computer
    /// inside a project (principle 1); a native path only where a document without a project was read.
    pub file: String,
    /// `OpenStatus` or `ResolvedStatus` (the only two states).
    pub status: String,
    /// `CommentIsOrphan`: the comment is not tied to the layout (unmapped, or its text was accepted away).
    pub orphan: bool,
    /// `CommentIsApplied`: only text-editing comments have it (accepting a strikethrough applies it).
    pub applied: Option<bool>,
    pub destination: Option<PdfDestination>,
    pub replies: Vec<PdfReply>,
}

/// An answer to a comment: no status, no answers of its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PdfReply {
    pub id: String,
    pub reviewer: String,
    pub content: String,
    pub date: String,
}

/// Where a comment points in the layout (`CommentDestination` names the element by its `Self`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PdfDestination {
    pub id: String,
    /// `page` (`PDFCommentPageDestination`), `pageItem` (`PDFCommentPageItemDestination`) or `text` (`PDFCommentTextDestination`).
    pub kind: String,
    /// The page, page item or story `Self`.
    pub target: String,
    /// `…TargetPoint` of a page or page item destination.
    pub point: [f64; 2],
    /// `…HighlightRect` (`Left Top Right Bottom`) of a page or page item destination.
    pub rect: [f64; 4],
    /// A text destination's characters `[start, end)` of the story.
    pub start: usize,
    pub end: usize,
}
