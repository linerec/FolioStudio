use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub id: String,
    pub topics: Vec<Topic>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    pub id: String,
    pub name: String,
    pub sort_order: String,
    pub topics: Vec<Topic>,
    pub references: Vec<Reference>,
    /// "See" and "See also" lines that point to another topic (index-c6).
    pub cross_references: Vec<CrossReference>,
}

/// `CrossReference` inside a `Topic`: a line such as "See also Beta" after the topic's page numbers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrossReference {
    pub id: String,
    /// IDML `CrossReferenceType`: See, SeeAlso, SeeHerein, SeeAlsoHerein, SeeOrAlsoBracket, CustomCrossReference, ...Before, ...After.
    pub kind: String,
    /// `CustomTypeString`: the words of a custom kind.
    pub custom: String,
    /// `ReferencedTopic`: the topic it points to.
    pub referenced: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub id: String,
    pub story: String,
    pub kind: String,
    pub limit: String,
    /// Character position of the marker in its story (the marker is one U+FEFF), counted as InDesign counts
    /// insertion points. A marker inside a footnote or table cell counts in that text's own space (`nested`).
    pub at: usize,
    pub nested: bool,
    /// For a marker in a footnote or table (`nested`): the position, in the story, of the footnote or table it is in (the page it is on is the
    /// page of that place). Zero for a marker in the story itself.
    #[serde(default)]
    pub anchor: usize,
}

/// One document of a book-wide index (Generate Index ▸ Include Book Documents), in the book's order. The server reads the
/// other documents' printed entries from their composed pages (each with its own page names, as Book ▸ Repaginate numbers
/// them); the document the index is made in is `own` and its entries are made by the generation itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BookIndexDocument {
    pub document: String,
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub entries: Vec<BookIndexEntry>,
}

/// A printed index entry of a book document: its topic path (level 1 first), sort key, printed name, pages and cross references.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BookIndexEntry {
    pub path: Vec<String>,
    pub key: String,
    pub printed: String,
    #[serde(default)]
    pub pages: Vec<BookIndexPage>,
    #[serde(default)]
    pub cross: Vec<BookIndexCross>,
}

/// A page reference of a book document: the first and last page names and indexes (in that document).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BookIndexPage {
    pub first: String,
    pub last: String,
    pub index: usize,
    pub end: usize,
}

/// A printed cross reference ("See also Beta"): `words` has the character style `Index Cross-reference`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BookIndexCross {
    pub lead: String,
    pub words: String,
    pub tail: String,
}
