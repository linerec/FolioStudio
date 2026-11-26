//! Print Booklet: which pages go on which printer sheet, and where. The document's pages are never reordered: `plan` and `layout`
//! only say where each page goes, and `ExportOptions::booklet` (set from `layout`) makes `export` draw every page of a sheet
//! into its place, clipped to the page (the sheet composer, in `lib.rs`).
//!
//! Every rule below was measured in InDesign 2025 (`artifacts/printing-20261023/measurement/printing-booklet*.json`,
//! `booklet-raster-measurements.json`; summary in `next/oracle/rules/printing.ko.md`); each is covered by `tests/printing_booklet.rs`.
//! What is a decision (not measured) is marked "decision".
use folio_model::Document;
use folio_model::printing::PrintSelection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BookletType {
    /// 2-up saddle stitch (InDesign default).
    Saddle,
    /// 2-up perfect bound, folded in signatures.
    PerfectBound,
    Consecutive2,
    Consecutive3,
    Consecutive4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Binding {
    /// What the document says: right to left binding (`PrintSetup::right_to_left`) folds the booklet the other way (InDesign has no
    /// booklet option for it: the document's own page binding decides, measured by switching the binding of a document).
    Document,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct BookletOptions {
    pub booklet_type: BookletType,
    pub binding: Binding,
    /// 4, 8, 12, 16 or 32 pages (perfect bound only).
    pub signature_size: usize,
    /// Space between the pages of a sheet, points. Not for saddle stitch (measured: InDesign refuses the setting there and prints none).
    pub gap: f64,
    /// Bleed between pages, points: perfect bound only (measured: refused for saddle stitch and the consecutive types). The page is not
    /// clipped at its inner edge but this far beyond it, into the space between the pages.
    pub bleed_between_pages: f64,
    /// Saddle stitch: the outermost sheet moves its pages this far apart, the innermost not at all (measured: +12 moved the
    /// outer sheet's left page by -6 and right page by +6, the inner sheet by 0).
    pub creep: f64,
    /// Margins top, bottom, left, right (points); used when `auto_margins` is off.
    pub margins: [f64; 4],
    /// Automatically adjust margins: InDesign's measured default of 36 pt each side in a file output.
    pub auto_margins: bool,
    /// Print blank printer spreads: sheets are rounded up to an even count (two faces of a sheet of paper).
    pub print_blank_printer_spreads: bool,
}

impl Default for BookletOptions {
    fn default() -> Self {
        Self {
            booklet_type: BookletType::Saddle,
            binding: Binding::Document,
            signature_size: 4,
            gap: 0.0,
            bleed_between_pages: 0.0,
            creep: 0.0,
            margins: [36.0; 4],
            auto_margins: true,
            print_blank_printer_spreads: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookletCell {
    /// Index into the page list the plan was made for; `None` is an empty place.
    pub page: Option<usize>,
    /// Top left of the page on the sheet, points from the sheet's top left (creep included).
    pub origin: [f64; 2],
    /// Top left of the place the page is clipped to, creep not included (measured: with creep the page moves inside its place and
    /// what leaves the place is cut off: a page moved 6 pt to the left shows nothing of its left 6 pt).
    pub slot: [f64; 2],
    /// How far beyond the page the clip goes on its left and right edge (Bleed Between Pages).
    pub bleed: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookletSheet {
    pub cells: Vec<BookletCell>,
    pub size: [f64; 2],
}

const SIGNATURES: [usize; 5] = [4, 8, 12, 16, 32];

/// Pages of one folded group laid out like a saddle-stitched booklet, in sheet order: the outer sheet's front (last, first),
/// its back (second, last but one), then inwards. `first..first + len`, `len` rounded up to a multiple of 4 with empty places at the end.
fn folded(first: usize, len: usize, count: usize) -> Vec<[Option<usize>; 2]> {
    let m = len.div_ceil(4) * 4;
    let page = |k: usize| (k < len).then_some(first + k).filter(|p| *p < count);
    let mut out = Vec::new();
    for k in 0..m / 4 {
        out.push([page(m - 1 - 2 * k), page(2 * k)]);
        out.push([page(2 * k + 1), page(m - 2 - 2 * k)]);
    }
    out
}

/// The sheets for `count` pages of `page_size` (width, height; one size for all pages: mixed sizes are not measured).
pub fn plan(count: usize, page_size: [f64; 2], o: &BookletOptions) -> Result<Vec<BookletSheet>, String> {
    if count == 0 {
        return Err("no pages selected for the booklet".into());
    }
    if o.booklet_type == BookletType::PerfectBound && !SIGNATURES.contains(&o.signature_size) {
        return Err("signature size must be 4, 8, 12, 16 or 32".into());
    }
    let (columns, rows): (usize, Vec<Vec<Option<usize>>>) = match o.booklet_type {
        BookletType::Saddle => (2, folded(0, count, count).into_iter().map(|c| c.to_vec()).collect()),
        BookletType::PerfectBound => {
            // decision: every signature is folded like a booklet of its own; the last, shorter one rounds up to a multiple of 4
            let mut rows = Vec::new();
            for first in (0..count).step_by(o.signature_size) {
                rows.extend(folded(first, o.signature_size.min(count - first), count).into_iter().map(|c| c.to_vec()));
            }
            (2, rows)
        }
        BookletType::Consecutive2 | BookletType::Consecutive3 | BookletType::Consecutive4 => {
            let n = match o.booklet_type {
                BookletType::Consecutive2 => 2,
                BookletType::Consecutive3 => 3,
                _ => 4,
            };
            (n, (0..count).step_by(n).map(|first| (first..first + n).map(|p| (p < count).then_some(p)).collect()).collect())
        }
    };
    let mut rows = rows;
    if o.binding == Binding::Right {
        // measured (right to left document, 8 pages): saddle (1, 8), (7, 2); perfect bound (1, 4), (3, 2); 2-up consecutive (2, 1), (4, 3);
        // 3-up consecutive (3, 2, 1), (6, 5, 4): every sheet reads right to left
        for row in &mut rows {
            row.reverse();
        }
    }
    if o.print_blank_printer_spreads {
        // measured: 8 pages three-up print 4 sheets, the last one empty; sheets of paper have two faces
        if rows.len() % 2 == 1 {
            rows.push(vec![None; columns]);
        }
    } else {
        rows.retain(|r| r.iter().any(Option::is_some));
    }
    let [top, bottom, left, right] = if o.auto_margins { [36.0; 4] } else { o.margins };
    let [pw, ph] = page_size;
    let gap = if o.booklet_type == BookletType::Saddle { 0.0 } else { o.gap };
    let bleed = if o.booklet_type == BookletType::PerfectBound { o.bleed_between_pages } else { 0.0 };
    let size = [left + right + columns as f64 * pw + (columns - 1) as f64 * gap, top + bottom + ph];
    let faces = rows.len();
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(face, row)| {
            // saddle stitch: the two faces of a sheet of paper creep together, the outermost sheet most
            let sheets = faces / 2;
            let shift = if o.booklet_type == BookletType::Saddle && sheets > 1 {
                o.creep * (sheets - 1 - face / 2) as f64 / (sheets - 1) as f64 / 2.0
            } else {
                0.0
            };
            BookletSheet {
                size,
                cells: row
                    .into_iter()
                    .enumerate()
                    .map(|(c, page)| {
                        let creep = if c == 0 { -shift } else { shift };
                        let x = left + c as f64 * (pw + gap);
                        // between pages: the inner edges of the pages of a sheet
                        let inner = [if c > 0 { bleed } else { 0.0 }, if c + 1 < columns { bleed } else { 0.0 }];
                        BookletCell { page, origin: [x + creep, top], slot: [x, top], bleed: inner }
                    })
                    .collect(),
            }
        })
        .collect())
}

/// A document page placed on a sheet: the document page index and the top left of the page on the sheet (points from the sheet's top left).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub page: usize,
    pub origin: [f64; 2],
    /// The place the page is clipped to (`BookletCell::slot`).
    pub slot: [f64; 2],
    /// `BookletCell::bleed`.
    pub bleed: [f64; 2],
}

/// A sheet of the booklet in terms of the document's pages (the input of `ExportOptions::booklet`).
#[derive(Clone, Debug, PartialEq)]
pub struct LaidSheet {
    pub size: [f64; 2],
    /// One entry per place of the sheet, left to right; `None` is an empty place.
    pub cells: Vec<Option<Placed>>,
}

/// The sheets for the pages `selection` names (print order), the pages being placed by `plan`. Pages of different sizes are refused
/// (not measured). The document is not changed.
pub fn layout(doc: &Document, selection: &PrintSelection, o: &BookletOptions) -> Result<Vec<LaidSheet>, String> {
    let wanted = super::selected_indices(doc, selection)?;
    let pages: Vec<_> = wanted.iter().filter_map(|i| doc.spreads.iter().flat_map(|sp| &sp.pages).find(|p| p.index == *i)).collect();
    let first = pages.first().ok_or("no pages selected for the booklet")?;
    let size = [first.bounds.width(), first.bounds.height()];
    if pages.iter().any(|p| (p.bounds.width() - size[0]).abs() > 0.01 || (p.bounds.height() - size[1]).abs() > 0.01) {
        return Err("booklets of pages of different sizes have not been verified".into());
    }
    let binding = match o.binding {
        Binding::Document if doc.print.right_to_left => Binding::Right,
        Binding::Document => Binding::Left,
        explicit => explicit,
    };
    Ok(plan(pages.len(), size, &BookletOptions { binding, ..o.clone() })?
        .into_iter()
        .map(|sheet| LaidSheet {
            size: sheet.size,
            cells: sheet.cells.into_iter().map(|c| c.page.map(|p| Placed { page: pages[p].index, origin: c.origin, slot: c.slot, bleed: c.bleed })).collect(),
        })
        .collect())
}
