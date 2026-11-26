//! What a book needs to know about one document to number the next one (oracle probes `bookx-c1-repaginate`, `-repaginate2`):
//! the name and the number of every page in document order, and whether the first section has a number of its own.
//! InDesign takes the next document's first number from the last page's number plus one, whatever the number style or prefix
//! of the sections is (cases c9, c11, e4, e5), so the numbers come from the page descriptors, not from the names.
//!
//! A page with no descriptor (a document Folio made itself with File > New writes none; InDesign writes one again when it
//! saves) gets its number from the section settings and the page order, the rule `Book::number` of the page editor applies
//! when it renames the pages. A descriptor that is there always wins: the number is InDesign's own.
use crate::Editor;
use crate::xml::Index;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageNumber {
    /// The name the page shows (`iii`, `A-1`, `3`).
    pub name: String,
    /// The number the name stands for.
    pub number: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Numbering {
    pub pages: Vec<PageNumber>,
    /// The first section continues the numbering (no number of its own).
    pub first_continues: bool,
    /// `PageNumberStart` of the first section, when it has one.
    pub first_start: Option<u32>,
}

/// The settings of one `Section` of the designmap that decide the numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionRun {
    /// `Self` of the section's first page.
    pub page_start: String,
    /// `ContinueNumbering` (true when the attribute is missing).
    pub continues: bool,
    /// `PageNumberStart`.
    pub start: Option<u32>,
}

/// The number of every page (in document order) from the sections alone, as the page descriptor's fifth item spells it. The
/// first section starts on the first page; a section on a page the document does not have is ignored; a section that does
/// not continue numbering starts at its `PageNumberStart` (1 when it has none); the first one, while it continues, takes
/// its `PageNumberStart` as the number of the first page (that is how a book gives a document its first number).
pub fn numbers_from_sections(order: &[String], sections: &[SectionRun]) -> Vec<i64> {
    let mut sections = sections.to_vec();
    if let (Some(first), Some(page)) = (sections.first_mut(), order.first()) {
        first.page_start = page.clone();
    }
    let at = |id: &str| order.iter().position(|p| p == id);
    sections.retain(|s| at(&s.page_start).is_some());
    sections.sort_by_key(|s| at(&s.page_start));
    let mut number = sections.first().filter(|s| s.continues).and_then(|s| s.start).filter(|&n| n > 0).map_or(1, i64::from);
    let mut numbers = vec![0; order.len()];
    for (k, s) in sections.iter().enumerate() {
        let start = at(&s.page_start).unwrap_or(0);
        let end = sections.get(k + 1).and_then(|n| at(&n.page_start)).unwrap_or(order.len());
        if !s.continues {
            number = s.start.map_or(1, i64::from);
        }
        for slot in &mut numbers[start..end] {
            *slot = number;
            number += 1;
        }
    }
    numbers
}

impl Editor {
    /// The numbering of the document as it stands (pages of hidden spreads included, as InDesign's book counts them).
    pub fn numbering(&self) -> Result<Numbering, String> {
        let design = self.part("designmap.xml").ok_or("designmap.xml missing")?;
        let dx = Index::scan(design).map_err(|e| e.to_string())?;
        // (id, name, the number its descriptor gives when it has one: Ok(None) = no descriptor at all)
        let mut found: Vec<(String, String, Result<Option<i64>, ()>)> = Vec::new();
        for i in 0..dx.nodes.len() {
            if dx.local(design, i) != "Spread" {
                continue;
            }
            let Some(src) = dx.attr(design, i, "src") else { continue };
            let Some(text) = self.part(src) else { continue };
            let ix = Index::scan(text).map_err(|e| e.to_string())?;
            for p in 0..ix.nodes.len() {
                if ix.local(text, p) != "Page" {
                    continue;
                }
                let id = ix.value(text, p, "Self").unwrap_or_default();
                let name = ix.value(text, p, "Name").unwrap_or_default();
                let descriptor = ix.path(text, p, &["Properties", "Descriptor"]);
                let described = match descriptor {
                    None => Ok(None),
                    Some(d) => {
                        let items: Vec<usize> = ix.children(d).iter().copied().filter(|&c| ix.local(text, c) == "ListItem").collect();
                        // the descriptor's fifth item is the page's number
                        items
                            .get(4)
                            .and_then(|&c| text[ix.nodes[c].open_end..ix.nodes[c].close_start].trim().parse::<i64>().ok())
                            .map(Some)
                            .ok_or(())
                    }
                };
                found.push((id, name, described));
            }
        }
        let sections: Vec<SectionRun> = (0..dx.nodes.len())
            .filter(|&i| dx.local(design, i) == "Section")
            .map(|i| SectionRun {
                page_start: dx.attr(design, i, "PageStart").unwrap_or_default().to_owned(),
                continues: dx.attr(design, i, "ContinueNumbering") != Some("false"),
                start: dx.attr(design, i, "PageNumberStart").and_then(|v| v.parse().ok()),
            })
            .collect();
        let computed = if found.iter().any(|f| matches!(f.2, Ok(None))) {
            let order: Vec<String> = found.iter().map(|f| f.0.clone()).collect();
            numbers_from_sections(&order, &sections)
        } else {
            Vec::new()
        };
        let mut pages = Vec::new();
        for (at, (_, name, described)) in found.into_iter().enumerate() {
            let number = match described {
                Ok(Some(n)) => n,
                Ok(None) => computed[at],
                Err(()) => return Err(format!("page {name}: no number")),
            };
            pages.push(PageNumber { name, number });
        }
        let first = (0..dx.nodes.len()).find(|&i| dx.local(design, i) == "Section");
        let first_continues = first.is_none_or(|i| dx.attr(design, i, "ContinueNumbering") != Some("false"));
        let first_start = first.and_then(|i| dx.attr(design, i, "PageNumberStart")).and_then(|v| v.parse().ok());
        Ok(Numbering { pages, first_continues, first_start })
    }
}
