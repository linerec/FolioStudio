use crate::EditError;
use folio_model::{
    Document,
    toc::{TocEntry, TocSource, TocStyle},
};

pub(super) const BASIC_PARAGRAPH: &str = "ParagraphStyle/$ID/[No paragraph style]";
pub(super) const BASIC_CHARACTER: &str = "CharacterStyle/$ID/[None]";
const LAST_TOC_LEVEL: usize = 10;

pub(super) struct Entry<'a> {
    pub options: &'a TocEntry,
    pub format: &'a str,
    pub title: String,
    pub label: &'a str,
    pub page: Option<&'a str>,
}

pub(super) fn paragraph_style<'a>(doc: &'a Document, name: &str) -> Option<&'a str> {
    doc.styles
        .paragraph
        .iter()
        .find(|s| s.id == name || s.name == name)
        .map(|s| s.id.as_str())
}

pub(super) fn character_style<'a>(doc: &'a Document, name: &str) -> &'a str {
    doc.styles
        .character
        .iter()
        .find(|s| s.id == name || s.name == name)
        .map_or(BASIC_CHARACTER, |s| &s.id)
}

pub(super) fn stale_layout() -> EditError {
    EditError::new("staleLayout", "목차 원본의 조판 정보가 오래되었습니다")
}

// R3, R4, R6.
pub(super) fn entries<'a>(
    doc: &'a Document,
    style: &'a TocStyle,
    target: &str,
    sources: &'a [TocSource],
) -> Result<Vec<Entry<'a>>, EditError> {
    let pages: std::collections::BTreeMap<_, _> = doc
        .spreads
        .iter()
        .flat_map(|s| &s.pages)
        .map(|page| (page.index, page.id.as_str()))
        .collect();
    let mut entries = Vec::new();
    for source in sources {
        if source.hidden && !style.include_hidden {
            continue;
        }
        let (applied, heading) = if let Some(book) = &source.book {
            if !style.include_book_documents {
                continue;
            }
            (book.style.as_str(), book.text.clone())
        } else {
            if source.story == target || doc.toc_stories.contains_key(&source.story) {
                continue;
            }
            let paragraph = doc
                .stories
                .get(&source.story)
                .and_then(|story| story.paragraphs.get(source.paragraph))
                .ok_or_else(stale_layout)?;
            (
                paragraph.style.as_str(),
                paragraph.runs.iter().map(|r| r.text.as_str()).collect(),
            )
        };
        let source_format = paragraph_style(doc, applied);
        let Some(options) = style.entries.iter().find(|e| {
            e.name == applied
                || source_format.is_some_and(|id| paragraph_style(doc, &e.name) == Some(id))
                || applied.strip_prefix("ParagraphStyle/") == Some(e.name.as_str())
        }) else {
            continue;
        };
        let heading = if style.remove_forced_line_break {
            heading.replace(['\n', '\u{2028}'], " ")
        } else {
            heading
        };
        let title = format!("{}{heading}", source.number);
        let page = if source.book.is_none() {
            Some(*pages.get(&source.page_index).ok_or_else(stale_layout)?)
        } else {
            None
        };
        let format = paragraph_style(doc, &options.format_style)
            .or(source_format)
            .unwrap_or(BASIC_PARAGRAPH);
        entries.push(Entry {
            options,
            format,
            title,
            label: &source.page,
            page,
        });
    }
    Ok(sort_branches(&mut entries.into_iter().peekable(), None))
}

fn sort_branches<'a>(
    entries: &mut std::iter::Peekable<std::vec::IntoIter<Entry<'a>>>,
    parent_level: Option<usize>,
) -> Vec<Entry<'a>> {
    let mut branches = Vec::new();
    while let Some(entry) =
        entries.next_if(|entry| parent_level.is_none_or(|level| branch_level(entry) > level))
    {
        let children = sort_branches(entries, Some(branch_level(&entry)));
        branches.push((entry, children));
    }
    let sort_level = |entry: &Entry<'_>| {
        (entry.options.sort_alphabet && (1..=LAST_TOC_LEVEL).contains(&entry.options.level))
            .then_some(entry.options.level)
    };
    for siblings in branches.chunk_by_mut(|a, b| sort_level(&a.0) == sort_level(&b.0)) {
        if sort_level(&siblings[0].0).is_some() {
            siblings.sort_by_cached_key(|(entry, _)| entry.title.to_lowercase());
        }
    }
    branches
        .into_iter()
        .flat_map(|(entry, children)| std::iter::once(entry).chain(children))
        .collect()
}

fn branch_level(entry: &Entry<'_>) -> usize {
    entry.options.level.min(LAST_TOC_LEVEL + 1)
}
