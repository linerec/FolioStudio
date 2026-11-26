//! Plans output sheets and draws printer's marks.

pub(super) mod content;

use crate::content::{ResUse, Stream};
use crate::draw::Source;
use crate::registry::Registry;
use crate::{ExportOptions, printing};
use folio_model::{Document, Page, Rect, Spread};
use folio_text::FontStore;

/// One output page: the pages it shows and its boxes.
pub(super) struct OutPage<'d> {
    pub(super) spread: &'d Spread,
    pub(super) pages: Vec<&'d Page>,
    /// Trim in spread coordinates.
    pub(super) trim: Rect,
    /// Bleed beyond the trim: left, top, right, bottom (points).
    pub(super) bleed: [f64; 4],
    /// Media margin beyond the trim, same order.
    pub(super) margin: [f64; 4],
    pub(super) name: String,
    /// Print Booklet: the pages placed on this sheet (empty for an ordinary page or spread).
    pub(super) cells: Vec<BookletCell<'d>>,
}

/// A page of the document drawn at `origin` (its top left, points from the sheet's top left) of a booklet sheet.
pub(super) struct BookletCell<'d> {
    pub(super) spread: &'d Spread,
    pub(super) page: &'d Page,
    pub(super) origin: [f64; 2],
    /// Where the page is clipped (`origin` without creep).
    pub(super) slot: [f64; 2],
    /// How far beyond the page the clip goes at its left and right edge (Bleed Between Pages).
    pub(super) bleed: [f64; 2],
}

pub(super) struct PageOut {
    pub(super) content: Vec<u8>,
    pub(super) used: ResUse,
    pub(super) transparency: bool,
    pub(super) media: [f64; 2],
    pub(super) trim: [f64; 4],
    pub(super) bleed: [f64; 4],
}

/// Crop marks: length beyond the offset; bleed marks: gap outside the bleed box and length.
const CROP_LENGTH: f64 = 15.0;
const BLEED_GAP: f64 = 3.0;
const BLEED_LENGTH: f64 = 18.0;
/// Half the length of a registration target's cross arms.
const REGISTRATION_ARM: f64 = 6.0;
/// The page information line's strip below the bleed.
const PAGE_INFO_STRIP: f64 = 30.0;

/// A page is right-hand when it is the last of its spread or lies right of the spine.
fn is_right_hand(spread: &Spread, page: &Page) -> bool {
    let r = page.transform.apply_rect(&page.bounds);
    if spread.pages.len() == 1 {
        return r.left >= -1.0;
    }
    (r.left + r.right) / 2.0 > 0.0
}

/// Print Booklet: one output page per sheet; the sheet is the whole page (no margin, bleed or marks beyond it: the margins are the sheet's own).
fn booklet_plan<'d>(doc: &'d Document, sheets: &[printing::booklet::LaidSheet]) -> Vec<OutPage<'d>> {
    let find = |index: usize| doc.spreads.iter().find_map(|spread| spread.pages.iter().find(|p| p.index == index).map(|page| (spread, page)));
    sheets
        .iter()
        .enumerate()
        .filter_map(|(n, sheet)| {
            let cells: Vec<BookletCell<'d>> = sheet
                .cells
                .iter()
                .flatten()
                .filter_map(|c| find(c.page).map(|(spread, page)| BookletCell { spread, page, origin: c.origin, slot: c.slot, bleed: c.bleed }))
                .collect();
            // a sheet without any page still is a page of the output (an empty face); it needs some spread to stand on
            let spread = cells.first().map(|c| c.spread).or_else(|| doc.spreads.first())?;
            let name = format!("Sheet {}", n + 1);
            let trim = Rect { left: 0.0, top: 0.0, right: sheet.size[0], bottom: sheet.size[1] };
            Some(OutPage { spread, pages: cells.iter().map(|c| c.page).collect(), trim, bleed: [0.0; 4], margin: [0.0; 4], name, cells })
        })
        .collect()
}

pub(super) fn plan<'d>(doc: &'d Document, opts: &ExportOptions) -> Vec<OutPage<'d>> {
    if let Some(sheets) = &opts.booklet {
        return booklet_plan(doc, sheets);
    }
    let p = &doc.print;
    let bleed_values = opts.custom_bleed.unwrap_or(if opts.bleed { p.bleed } else { [0.0; 4] });
    let wanted = |pg: &Page| opts.pages.as_ref().is_none_or(|v| v.contains(&pg.index));
    let side_bleed = |spread: &Spread, page: &Page| -> [f64; 4] {
        let [top, bottom, inside, outside] = bleed_values;
        let (left, right) = if p.facing_pages && is_right_hand(spread, page) {
            (inside, outside)
        } else if p.facing_pages {
            (outside, inside)
        } else {
            (inside, outside)
        };
        [left, top, right, bottom]
    };
    let mut out = Vec::new();
    for spread in &doc.spreads {
        // a hidden spread is never output (InDesign leaves it out even of an explicit page range), unless printing
        if spread.hidden && !opts.include_hidden_spreads {
            continue;
        }
        let pages: Vec<&Page> = spread.pages.iter().filter(|pg| wanted(pg)).collect();
        if pages.is_empty() {
            continue;
        }
        let make = |pages: Vec<&'d Page>, bleed: [f64; 4]| {
            let trim = pages.iter().map(|pg| pg.transform.apply_rect(&pg.bounds)).reduce(|a, b| a.union(&b)).unwrap_or_default();
            let mut margin = bleed;
            if opts.marks.any() {
                // What each mark set needs beside the page. Measured in InDesign's print to a PostScript file (printing-marks-*, printing-bleed-*):
                // crop marks reach offset + 15 pt from the page, bleed marks 3 + 18 pt past the bleed, registration targets offset + 7.5 + 6 pt (so
                // registration marks alone make the sheet narrower than crop marks do), and nothing else (page information alone) widens the sides.
                for (value, bleed) in margin.iter_mut().zip(bleed) {
                    let mut need = 0.0_f64;
                    if opts.marks.crop { need = need.max(opts.mark_offset + CROP_LENGTH); }
                    if opts.marks.registration { need = need.max(opts.mark_offset + CROP_LENGTH / 2.0 + REGISTRATION_ARM); }
                    if opts.marks.bleed { need = need.max(bleed + BLEED_GAP + BLEED_LENGTH); }
                    *value = bleed.max(need);
                }
                // the page information line below the page: a strip 30 pt under the bleed, whatever the offset is
                if opts.marks.page_info { margin[3] = margin[3].max(bleed[3] + PAGE_INFO_STRIP); }
            }
            let name = pages.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("-");
            OutPage { spread, pages, trim, bleed, margin, name, cells: Vec::new() }
        };
        if opts.spreads {
            let first = pages.first().copied();
            let last = pages.last().copied();
            let (lb, rb) = (first.map(|f| side_bleed(spread, f)).unwrap_or_default(), last.map(|l| side_bleed(spread, l)).unwrap_or_default());
            let bleed = [lb[0], lb[1].max(rb[1]), rb[2], lb[3].max(rb[3])];
            out.push(make(pages, bleed));
        } else {
            for pg in pages {
                let b = side_bleed(spread, pg);
                out.push(make(vec![pg], b));
            }
        }
    }
    if opts.ordered_pages
        && let Some(order) = &opts.pages {
        out.sort_by_key(|sheet| sheet.pages.iter().filter_map(|page| order.iter().position(|i| *i == page.index)).min().unwrap_or(usize::MAX));
    }
    out
}

pub(super) fn master_sources<'d>(doc: &'d Document, spread: &'d Spread, page: &'d Page) -> Vec<Source<'d>> {
    folio_model::parents::sources(doc, spread, page).into_iter().map(|s| Source {
        items: &s.spread.items, transform: s.transform, master_page: Some((s.range, page.index)), hidden: s.hidden,
    }).collect()
}

/// Text of the page information mark: file and page at the left, date at the right.
pub(super) fn page_info_text(opts: &ExportOptions, page: &OutPage<'_>, date: &str) -> (String, String) {
    let name = if opts.document_name.is_empty() { "document" } else { &opts.document_name };
    (format!("{name}  {}", page.name), date.to_owned())
}

/// Glyphs of `text` in `font` (cmap only) with advances (1/1000 em).
pub(super) fn simple_shape(fonts: &FontStore, font: u32, text: &str) -> Vec<(u16, f32)> {
    fonts
        .with_face(font, |f| {
            let upem = f32::from(f.units_per_em().max(1));
            text.chars()
                .map(|c| {
                    let g = f.glyph_index(c).map(|g| g.0).unwrap_or(0);
                    (g, f32::from(f.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0)) * 1000.0 / upem)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) type MarkInfo<'a> = (u32, &'a [(u16, f32)], &'a [(u16, f32)]);

/// Printer's marks as InDesign draws them (measured on its PDF/X-4 export): crop marks 15 pt
/// long from the offset, bleed marks 18 pt long 3 pt outside the bleed box, registration
/// targets centred on the crop mark span, each line over a 1.25 pt knockout halo; all in
/// the Registration colour (Separation /All).
pub(super) fn draw_marks(s: &mut Stream, reg: &mut Registry<'_>, opts: &ExportOptions, out: &PageOut, info: Option<MarkInfo<'_>>) {
    let ink = reg.registration(1.0);
    let halo = reg.registration(0.0);
    let [tx0, ty0, tx1, ty1] = out.trim;
    let [bx0, by0, bx1, by1] = out.bleed;
    let w = opts.mark_weight;
    let o = opts.mark_offset;
    s.push();
    s.set_overprint(reg, Some(false), Some(false));
    let line = |s: &mut Stream, x0: f64, y0: f64, x1: f64, y1: f64| {
        s.c.move_to(x0 as f32, y0 as f32);
        s.c.line_to(x1 as f32, y1 as f32);
    };
    // Each mark set is stroked twice: a white halo, then the mark.
    type MarkPath = Box<dyn Fn(&mut Stream)>;
    let mut paths: Vec<MarkPath> = Vec::new();
    if opts.marks.crop {
        paths.push(Box::new(move |s: &mut Stream| {
            for &(x, y, dx, dy) in &[(tx0, ty0, -1.0, -1.0), (tx1, ty0, 1.0, -1.0), (tx0, ty1, -1.0, 1.0), (tx1, ty1, 1.0, 1.0)] {
                line(s, x + dx * o, y, x + dx * (o + CROP_LENGTH), y);
                line(s, x, y + dy * o, x, y + dy * (o + CROP_LENGTH));
            }
        }));
    }
    if opts.marks.bleed && (bx0 < tx0 || by0 < ty0 || bx1 > tx1 || by1 > ty1) {
        paths.push(Box::new(move |s: &mut Stream| {
            for &(x, y, dx, dy) in &[(bx0, by0, -1.0, -1.0), (bx1, by0, 1.0, -1.0), (bx0, by1, -1.0, 1.0), (bx1, by1, 1.0, 1.0)] {
                line(s, x + dx * BLEED_GAP, y, x + dx * (BLEED_GAP + BLEED_LENGTH), y);
                line(s, x, y + dy * BLEED_GAP, x, y + dy * (BLEED_GAP + BLEED_LENGTH));
            }
        }));
    }
    let targets: Vec<(f64, f64)> = if opts.marks.registration {
        let d = o + CROP_LENGTH / 2.0;
        let (cx, cy) = ((tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0);
        vec![(cx, ty1 + d), (cx, ty0 - d), (tx0 - d, cy), (tx1 + d, cy)]
    } else {
        Vec::new()
    };
    if !targets.is_empty() {
        let t = targets.clone();
        paths.push(Box::new(move |s: &mut Stream| {
            for &(x, y) in &t {
                circle(s, x, y, 5.0);
                line(s, x - 6.0, y, x + 6.0, y);
                line(s, x, y - 6.0, x, y + 6.0);
                circle(s, x, y, 3.0);
            }
        }));
    }
    for (col, width) in [(&halo, 1.25), (&ink, w)] {
        s.set_stroke(col);
        s.line_style(width, folio_model::LineJoin::Miter, folio_model::LineCap::Butt, None);
        for p in &paths {
            p(s);
        }
        if !paths.is_empty() {
            s.stroke();
        }
    }
    // the targets' inner discs are solid
    if !targets.is_empty() {
        s.set_fill(&ink);
        for &(x, y) in &targets {
            circle(s, x, y, 3.0);
        }
        s.fill(false);
        s.set_stroke(&halo);
        s.line_style(w, folio_model::LineJoin::Miter, folio_model::LineCap::Butt, None);
        for &(x, y) in &targets {
            line(s, x - 3.0, y, x + 3.0, y);
            line(s, x, y - 3.0, x, y + 3.0);
        }
        s.stroke();
    }
    if opts.marks.page_info
        && let Some((font, left, right)) = info
    {
        // 6 pt, baseline 3 pt below the bleed box (InDesign: y 18 for a 21 pt bleed box)
        let size = 6.0;
        let y = (by0.min(ty0 - o) - BLEED_GAP).max(2.0);
        s.set_fill(&ink);
        s.begin_text();
        s.font(font, size);
        for (glyphs, align_right) in [(left, false), (right, true)] {
            let width: f64 = glyphs.iter().map(|(_, w)| *w as f64 / 1000.0 * size).sum();
            let x = if align_right { tx1 - width } else { tx0 };
            s.text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
            let mut bytes = Vec::new();
            for (g, _) in glyphs {
                if let Some((code, _)) = reg.fonts.glyph(font, *g) {
                    bytes.extend_from_slice(&code.to_be_bytes());
                }
            }
            s.show_positioned(&[(bytes, 0.0)]);
        }
        s.end_text();
        reg.fonts.mark_drawn(font, 0);
    }
    s.pop();
}

fn circle(s: &mut Stream, x: f64, y: f64, r: f64) {
    let k = 0.552_284_75 * r;
    let p = |a: f64, b: f64| (a as f32, b as f32);
    let (x0, y0) = p(x + r, y);
    s.c.move_to(x0, y0);
    let segs = [
        (x + r, y + k, x + k, y + r, x, y + r),
        (x - k, y + r, x - r, y + k, x - r, y),
        (x - r, y - k, x - k, y - r, x, y - r),
        (x + k, y - r, x + r, y - k, x + r, y),
    ];
    for (a, b, c, d, e, f) in segs {
        s.c.cubic_to(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32);
    }
    s.c.close_path();
}
