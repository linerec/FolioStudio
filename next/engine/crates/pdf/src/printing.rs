//! Printer selection and file-proof planning, sharing the PDF page boxes and drawing path.
use folio_model::{Document, Page};
use folio_model::printing::{PrintJob, PrintSelection};
use serde::Serialize;
use crate::ExportOptions;

mod blank;
pub mod booklet;
pub mod presets;
pub mod device;
pub mod settings;
pub use blank::is_blank;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintSheet {
    pub page_ids: Vec<String>,
    pub label: String,
    pub size: [f64; 2],
}

fn endpoint(pages: &[&Page], text: &str) -> Option<usize> {
    let text = text.trim();
    if let Some(number) = text.strip_prefix('+') {
        return number.parse::<usize>().ok()?.checked_sub(1).filter(|i| *i < pages.len());
    }
    let mut found = pages.iter().enumerate().filter(|(_, p)| p.name == text);
    let first = found.next()?.0;
    found.next().is_none().then_some(first)
}

fn range(pages: &[&Page], value: &str) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in value.split(',') {
        let positions = if let Some(i) = endpoint(pages, part) { vec![i] } else {
            let candidates: Vec<_> = part.match_indices('-').filter_map(|(at, _)| {
                Some((endpoint(pages, &part[..at])?, endpoint(pages, &part[at + 1..])?))
            }).collect();
            if candidates.len() != 1 { return Err(format!("invalid or ambiguous print range: {part}")); }
            let (start, end) = candidates[0];
            if start > end { return Err("descending print ranges are not supported".into()); }
            (start..=end).collect()
        };
        for position in positions {
            let index = pages[position].index;
            if out.contains(&index) { return Err("overlapping print ranges are not supported".into()); }
            out.push(index);
        }
    }
    Ok(out)
}

/// The ids of the document pages a range text names (`+2-+4,A-11`, the print range grammar), in the order written.
pub fn page_range(doc: &Document, value: &str) -> Result<Vec<String>, String> {
    let mut pages: Vec<_> = doc.spreads.iter().flat_map(|s| &s.pages).collect();
    pages.sort_by_key(|p| p.index);
    Ok(range(&pages, value)?.into_iter().filter_map(|i| pages.iter().find(|p| p.index == i).map(|p| p.id.clone())).collect())
}

/// Document page indices a print selection names, in print order.
pub fn selected_indices(doc: &Document, selection: &PrintSelection) -> Result<Vec<usize>, String> {
    let mut pages: Vec<_> = doc.spreads.iter().flat_map(|s| &s.pages).collect();
    pages.sort_by_key(|p| p.index);
    Ok(match selection {
        PrintSelection::All => pages.iter().map(|p| p.index).collect(),
        PrintSelection::Range { value } => range(&pages, value)?,
        PrintSelection::Pages { ids } => {
            let mut selected = Vec::new();
            for id in ids {
                let page = pages.iter().find(|p| &p.id == id).ok_or_else(|| format!("unknown print page: {id}"))?;
                if selected.contains(&page.index) { return Err("duplicate print page".into()); }
                selected.push(page.index);
            }
            selected
        }
    })
}

pub fn configure(doc: &Document, job: &PrintJob, opts: &mut ExportOptions) -> Result<(), String> {
    let selected = selected_indices(doc, &job.selection)?;
    let mut expanded = Vec::new();
    for index in selected {
        let spread = doc.spreads.iter().find(|s| s.pages.iter().any(|p| p.index == index)).ok_or("missing print spread")?;
        // Print outputs hidden spreads like any other (measured), unlike PDF export.
        let indices: Vec<usize> = if job.spreads { spread.pages.iter().map(|p| p.index).collect() } else { vec![index] };
        // Print Blank Pages off: a page nothing prints on is left out; a spread only when all its pages are blank.
        if !job.print_blank_pages && indices.iter().all(|i| spread.pages.iter().find(|p| p.index == *i).is_some_and(|p| blank::is_blank(doc, spread, p))) { continue; }
        for index in indices { if !expanded.contains(&index) { expanded.push(index); } }
    }
    if expanded.is_empty() { return Err("no pages selected for printing".into()); }
    opts.pages = Some(expanded);
    opts.spreads = job.spreads;
    opts.ordered_pages = true;
    opts.include_hidden_spreads = true;
    Ok(())
}

pub fn sheets(doc: &Document, opts: &ExportOptions) -> Vec<PrintSheet> {
    super::plan(doc, opts).into_iter().map(|sheet| PrintSheet {
        page_ids: sheet.pages.iter().map(|p| p.id.clone()).collect(),
        label: sheet.name,
        size: [sheet.trim.width() + sheet.margin[0] + sheet.margin[2], sheet.trim.height() + sheet.margin[1] + sheet.margin[3]],
    }).collect()
}
