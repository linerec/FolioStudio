//! IDML TOC definitions. Editing retains the source XML for fields outside this model.
use crate::xml::{self, Element};
use folio_model::toc::{TocEntry, TocStyle};

fn value(e: &Element<'_>, key: &str) -> String {
    e.attr(key).map(str::to_owned).or_else(|| e.child("Properties")?.child(key).map(Element::text)).unwrap_or_default()
}
fn flag(e: &Element<'_>, key: &str) -> bool { e.attr(key) == Some("true") }

pub fn styles(source: &str) -> Result<Vec<TocStyle>, String> {
    let root = xml::parse(source).map_err(|e| e.0)?;
    Ok(from_element(&root))
}

pub(crate) fn from_element(root: &Element<'_>) -> Vec<TocStyle> {
    let mut elements = Vec::new();
    if root.name == "TOCStyle" { elements.push(root); }
    root.descendants_named("TOCStyle", &mut elements);
    elements.into_iter().map(|e| TocStyle {
        id: value(e, "Self"), name: value(e, "Name"), title: value(e, "Title"),
        title_style: value(e, "TitleStyle"), include_hidden: flag(e, "IncludeHidden"),
        include_book_documents: flag(e, "IncludeBookDocuments"), create_bookmarks: flag(e, "CreateBookmarks"),
        make_anchor: flag(e, "MakeAnchor"), run_in: flag(e, "RunIn"),
        remove_forced_line_break: flag(e, "RemoveForcedLineBreak"), numbered_paragraphs: value(e, "NumberedParagraphs"),
        entries: e.children_named("TOCStyleEntry").map(|c| TocEntry {
            name: value(c, "Name"), format_style: value(c, "FormatStyle"),
            level: c.attr("Level").and_then(|s| s.parse().ok()).unwrap_or(1),
            page_number_position: value(c, "PageNumberPosition"), page_number_style: value(c, "PageNumberStyle"),
            separator: value(c, "Separator"), separator_style: value(c, "SeparatorStyle"), sort_alphabet: flag(c, "SortAlphabet"),
        }).collect(),
    }).collect()
}
