//! How a book numbers its documents (InDesign 2025, measured with `book.repaginate` in
//! `next/oracle/indesign/bookx-c1-repaginate-probe.jsx` and `-repaginate2-probe.jsx`; numbers in
//! `artifacts/bookx-c1-20261023/measurement/`):
//! - The first document starts at its own number when its first section has one, else at 1.
//! - A document after it starts at the last page's number plus one (the number style, section prefix and a restarted section
//!   inside the document do not matter: the last page's *number* counts).
//! - A document whose first section has a number of its own keeps it, whatever the options say (cases e1, e3).
//! - "Next odd" / "Next even page": a start of the wrong parity moves on by one page, leaving that number out; with
//!   "insert blank page" the document before gets one blank page at its end instead, so no number is left out (cases d2-d5).
//!   The first document is never moved (case e2).
//!   The last document gets its blank page too when a document after it would start on the wrong side (cases d3, d5).
//! - Blank pages stay when the option is switched off again (case d6).
//!
//! Only the order of the decisions lives here. What a document's pages are called after it is given its start is the editor's
//! (`folio_edit::Editor::numbering`), reached through the `probe` closure, so this module is exact without opening a document.
use folio_model::book::Options;
use serde::Serialize;

/// What one document looks like once it has been given a start and the blank pages the book asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    pub first_name: String,
    pub last_name: String,
    pub first_number: i64,
    pub last_number: i64,
    pub pages: usize,
    /// The first section has a number of its own: the book leaves it.
    pub own_start: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DocPlan {
    /// The number the book asked the first page to take (what `probe` was given; the first page's own number when `own_start`).
    pub start: i64,
    pub end: i64,
    /// `3-5`, `i-iii`, `3`: the first and last page names as the document shows them.
    pub range: String,
    pub pages: usize,
    /// Blank pages the book appends at the end of this document.
    pub blanks: u32,
    pub own_start: bool,
}

fn plan(f: &Facts, blanks: u32) -> DocPlan {
    let range = if f.pages <= 1 { f.first_name.clone() } else { format!("{}-{}", f.first_name, f.last_name) };
    DocPlan { start: f.first_number, end: f.last_number, range, pages: f.pages, blanks, own_start: f.own_start }
}

fn wrong_parity(kind: &str, start: i64) -> bool {
    match kind {
        "nextOdd" => start % 2 == 0,
        "nextEven" => start % 2 != 0,
        _ => false,
    }
}

/// Numbers `count` documents in order. `probe(index, start, blanks)` gives document `index` the start and `blanks` blank pages at
/// its end (starting from the document as it is on disk, so calling it again is safe) and says what came out.
pub fn chain<E>(options: &Options, count: usize, mut probe: impl FnMut(usize, u32, u32) -> Result<Facts, E>) -> Result<Vec<DocPlan>, E> {
    let mut plans: Vec<DocPlan> = Vec::with_capacity(count);
    let mut facts: Vec<Facts> = Vec::with_capacity(count);
    let mut given: Vec<(u32, u32)> = Vec::with_capacity(count);
    for i in 0..count {
        let raw = if i == 0 { 1 } else { (facts[i - 1].last_number + 1).clamp(1, 999_999) as u32 };
        let mut start = raw;
        let mut f = probe(i, start, 0)?;
        if i > 0 && !f.own_start && wrong_parity(&options.repagination, f.first_number) {
            if options.insert_blank_page {
                let (before_start, before_blanks) = given[i - 1];
                let again = probe(i - 1, before_start, before_blanks + 1)?;
                start = (again.last_number + 1).clamp(1, 999_999) as u32;
                plans[i - 1] = plan(&again, before_blanks + 1);
                facts[i - 1] = again;
                given[i - 1].1 = before_blanks + 1;
            } else {
                start = raw + 1;
            }
            f = probe(i, start, 0)?;
        }
        plans.push(plan(&f, 0));
        facts.push(f);
        given.push((start, 0));
    }
    // with blank pages the last document is padded too, so that a document added after it would start right (cases d3, d5)
    if options.insert_blank_page && count > 0 && wrong_parity(&options.repagination, facts[count - 1].last_number + 1) {
        let (start, blanks) = given[count - 1];
        let again = probe(count - 1, start, blanks + 1)?;
        plans[count - 1] = plan(&again, blanks + 1);
    }
    Ok(plans)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document of `pages` pages that may have a number of its own.
    fn run(options: &Options, docs: &[(usize, Option<u32>)]) -> Vec<String> {
        let plans = chain::<()>(options, docs.len(), |i, start, blanks| {
            let (pages, own) = docs[i];
            let first = own.unwrap_or(start) as i64;
            let pages = pages + blanks as usize;
            Ok(Facts { first_name: first.to_string(), last_name: (first + pages as i64 - 1).to_string(), first_number: first, last_number: first + pages as i64 - 1, pages, own_start: own.is_some() })
        })
        .unwrap();
        plans.into_iter().map(|p| p.range).collect()
    }
    fn opts(kind: &str, blank: bool) -> Options {
        Options { repagination: kind.into(), insert_blank_page: blank, automatic: true }
    }
    const FOUR: [(usize, Option<u32>); 4] = [(2, None), (1, None), (2, None), (3, None)];

    // The expectations below are what InDesign 2025 produced (probe cases d1-d5 / e1-e3 / c6).
    #[test]
    fn next_page_continues() {
        assert_eq!(run(&opts("nextPage", false), &FOUR), ["1-2", "3", "4-5", "6-8"]);
    }
    #[test]
    fn next_odd_skips_a_number_without_a_blank_page() {
        assert_eq!(run(&opts("nextOdd", false), &FOUR), ["1-2", "3", "5-6", "7-9"]);
    }
    #[test]
    fn next_odd_with_blank_pages_appends_them_to_the_document_before() {
        assert_eq!(run(&opts("nextOdd", true), &FOUR), ["1-2", "3-4", "5-6", "7-10"]);
    }
    #[test]
    fn next_even_skips_a_number_without_a_blank_page() {
        assert_eq!(run(&opts("nextEven", false), &FOUR), ["1-2", "4", "6-7", "8-10"]);
    }
    #[test]
    fn next_even_with_blank_pages() {
        assert_eq!(run(&opts("nextEven", true), &FOUR), ["1-3", "4-5", "6-7", "8-11"]);
    }
    #[test]
    fn a_document_with_its_own_start_keeps_it_and_the_next_follows() {
        assert_eq!(run(&opts("nextPage", false), &[(3, None), (2, Some(10)), (2, None)]), ["1-3", "10-11", "12-13"]);
        assert_eq!(run(&opts("nextEven", false), &[(2, Some(5)), (2, None)]), ["5-6", "8-9"]);
        assert_eq!(run(&opts("nextOdd", false), &[(3, None), (2, Some(4)), (1, None)]), ["1-3", "4-5", "7"]);
    }
    #[test]
    fn two_documents_and_the_empty_book() {
        assert_eq!(run(&opts("nextOdd", false), &[(3, None), (2, None)]), ["1-3", "5-6"]);
        assert!(run(&opts("nextPage", false), &[]).is_empty());
    }
}
