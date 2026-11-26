//! Table of contents definitions and composed source paragraph locations.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TocStyle {
    pub id: String,
    pub name: String,
    pub title: String,
    pub title_style: String,
    pub entries: Vec<TocEntry>,
    pub include_hidden: bool,
    pub include_book_documents: bool,
    pub create_bookmarks: bool,
    pub make_anchor: bool,
    pub run_in: bool,
    pub remove_forced_line_break: bool,
    pub numbered_paragraphs: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TocEntry {
    pub name: String,
    pub format_style: String,
    pub level: usize,
    pub page_number_position: String,
    pub page_number_style: String,
    pub separator: String,
    pub separator_style: String,
    pub sort_alphabet: bool,
}
impl Default for TocEntry {
    fn default() -> Self {
        Self { name: String::new(), format_style: String::new(), level: 1,
            page_number_position: "AfterEntry".into(), page_number_style: String::new(),
            separator: "^t".into(), separator_style: String::new(), sort_alphabet: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TocSource {
    pub story: String,
    pub paragraph: usize,
    pub page: String,
    pub page_index: usize,
    pub hidden: bool,
    pub number: String,
    /// A paragraph of another document of the book (a TOC that includes the book's documents): read by the server from
    /// that document's composed pages; `story` is empty and `page` is that document's page name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<BookParagraph>,
}

/// The paragraph of another book document a book-wide TOC lists: the document (its project path), the paragraph style
/// (the id in that document) and the text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookParagraph {
    pub document: String,
    pub style: String,
    pub text: String,
}

/// A composed line's character interval, sharing the TOC's page and paragraph location.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLocation {
    pub source: TocSource,
    pub start: usize,
    pub end: usize,
    pub paragraph_start: bool,
}
