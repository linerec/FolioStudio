//! Where a page reference of each kind ends (index-c3). Measured in InDesign 2025 with
//! `index.generate()`; the page number text is the first page, an en dash and the last page.
//!
//! - `ToEndOfStory`: the last page the story reaches (index-c2).
//! - `ToEndOfDocument`: the last page of the document, also past the story's own last page.
//! - `ToEndOfSection`: the last page of the section the marker is in.
//! - `ForNextNPages` n: the page n-1 after the marker's page, never past the document's last page
//!   (not past a section or the story).
//! - `ForNextNParagraphs` n: the page the n-th paragraph (the marker's own is the first) starts on.
//!   Not that paragraph's last line: a paragraph spanning pages 3 and 4 ends the range on 3. Never past the
//!   story's last paragraph. Not clamped to the marker's page: a marker on page 4 in a paragraph that starts
//!   on page 3 gives "4–3" (InDesign writes it so).
//! - `ToNextStyleChange`: up to the paragraph before the next paragraph with another paragraph style
//!   (character styles do not count), at that paragraph's last character; to the story's end when none.
//! - `ToNextUseOfStyle` (style in the reference's limit): the same, up to the first paragraph from the
//!   marker's own that uses the style; when the marker's paragraph is it, the range is the marker's page alone.
use super::invalid;
use crate::EditError;
use folio_model::{indexes::Reference, toc::TextLocation, Document};

/// Pages in document order: index, name and the section they are in (the index of its first page).
pub(super) struct Pages(Vec<(usize, String, usize)>);

impl Pages {
    pub fn new(doc: &Document) -> Pages {
        let mut v: Vec<_> = doc.spreads.iter().flat_map(|s| &s.pages).map(|p| (p.index, p.name.clone(), p.section_start)).collect();
        v.sort_by_key(|p| p.0);
        Pages(v)
    }
    pub fn name(&self, index: usize) -> Option<&str> { self.0.iter().find(|p| p.0 == index).map(|p| p.1.as_str()) }
    fn last(&self) -> Option<usize> { self.0.last().map(|p| p.0) }
    fn section_last(&self, index: usize) -> Option<usize> {
        let section = self.0.iter().find(|p| p.0 == index)?.2;
        self.0.iter().filter(|p| p.2 == section).map(|p| p.0).max()
    }
}

fn stale() -> EditError { EditError::new("staleLayout", "색인 참조가 조판된 쪽에 없습니다") }

fn same_style(a: &str, b: &str) -> bool {
    let key = |s: &str| s.trim_start_matches("ParagraphStyle/").trim_start_matches("$ID/").to_owned();
    key(a) == key(b)
}

/// The page index the range of `r` ends on. `lines` are the story's composed lines, `first` the line the marker is on.
pub(super) fn end_page(doc: &Document, pages: &Pages, lines: &[&TextLocation], r: &Reference, first: &TextLocation) -> Result<usize, EditError> {
    let start = first.source.page_index;
    let story_end = || lines.iter().max_by_key(|l| l.end).map(|l| l.source.page_index).ok_or_else(stale);
    let count = || r.limit.trim().parse::<usize>().ok().filter(|&n| n >= 1).ok_or_else(|| invalid("색인 참조의 개수가 없습니다"));
    let paragraphs = &doc.stories.get(&r.story).ok_or_else(|| invalid("색인 참조의 이야기를 찾을 수 없습니다"))?.paragraphs;
    let paragraph = first.source.paragraph;
    // the page of the character just before paragraph `q` (the last character of the paragraph before it)
    let page_before = |q: usize| -> Result<usize, EditError> {
        let at = lines.iter().filter(|l| l.paragraph_start && l.source.paragraph == q).map(|l| l.start).min().ok_or_else(stale)?;
        lines.iter().filter(|l| l.start < at).max_by_key(|l| l.start).map(|l| l.source.page_index).ok_or_else(stale)
    };
    let page = match r.kind.as_str() {
        "ToEndOfStory" => story_end()?,
        "ToEndOfDocument" => pages.last().ok_or_else(stale)?,
        "ToEndOfSection" => pages.section_last(start).ok_or_else(stale)?,
        "ForNextNPages" => (start + count()? - 1).min(pages.last().ok_or_else(stale)?),
        "ForNextNParagraphs" => {
            let q = (paragraph + count()? - 1).min(paragraphs.len().saturating_sub(1));
            return lines.iter().filter(|l| l.paragraph_start && l.source.paragraph == q).map(|l| l.source.page_index).min().ok_or_else(stale);
        }
        "ToNextStyleChange" | "ToNextUseOfStyle" => {
            let own = paragraphs.get(paragraph).ok_or_else(stale)?.style.as_str();
            let found = if r.kind == "ToNextStyleChange" {
                (paragraph + 1..paragraphs.len()).find(|&q| !same_style(&paragraphs[q].style, own))
            } else {
                if r.limit.is_empty() { return Err(invalid("스타일이 없는 '다음 스타일 사용까지' 참조입니다")); }
                (paragraph..paragraphs.len()).find(|&q| same_style(&paragraphs[q].style, &r.limit))
            };
            match found {
                None => story_end()?,
                Some(q) if q <= paragraph => start,
                Some(q) => page_before(q)?,
            }
        }
        _ => return Err(EditError::new("unsupported", "이 색인 참조 범위의 생성은 아직 측정하지 않았습니다")),
    };
    Ok(page.max(start))
}
