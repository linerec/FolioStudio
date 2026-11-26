//! Draws each output sheet and collects the regions that require raster flattening.

use super::{OutPage, PageOut, draw_marks, master_sources};
use crate::content::{ResUse, Stream};
use crate::draw::{Drawer, Source};
use crate::registry::{FormDef, GsDef, Registry};
use crate::{ExportOptions, FlattenReport, flatten};
use folio_model::layout::DocumentLayout;
use folio_model::{Affine, Document, Rect};
use folio_text::FontStore;
use std::collections::{BTreeMap, HashMap};

pub(crate) type MarkGlyphs = (Vec<(u16, f32)>, Vec<(u16, f32)>);

pub(crate) struct PageDrawInput<'a, 'd> {
    pub(crate) doc: &'d Document,
    pub(crate) layout: &'d DocumentLayout,
    pub(crate) fonts: &'d FontStore,
    pub(crate) opts: &'a ExportOptions,
    pub(crate) pages: &'a [OutPage<'d>],
    pub(crate) flattening: bool,
    pub(crate) info_font: Option<u32>,
    pub(crate) info_glyphs: &'a [MarkGlyphs],
}

pub(crate) struct DrawnPages {
    pub(crate) outs: Vec<PageOut>,
    pub(crate) flat_jobs: Vec<flatten::FlatJob>,
    pub(crate) flat_report: FlattenReport,
    pub(crate) flat_kinds: BTreeMap<&'static str, usize>,
    pub(crate) flat_page_slot: HashMap<usize, usize>,
}

pub(crate) fn draw_pages<'d>(reg: &mut Registry<'d>, input: PageDrawInput<'_, 'd>) -> DrawnPages {
    let PageDrawInput { doc, layout, fonts, opts, pages, flattening, info_font, info_glyphs } = input;
    // Flattening: the regions to rasterise and what the drawer found.
    let tile = flatten::tile_pt(opts.flatten.smooth_ppi);
    let mut flat_jobs: Vec<flatten::FlatJob> = Vec::new();
    let mut flat_report = FlattenReport { line_art_ppi: opts.flatten.line_art_ppi, smooth_ppi: opts.flatten.smooth_ppi, ..Default::default() };
    let mut flat_kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut flat_page_slot: HashMap<usize, usize> = HashMap::new();

    let mut outs: Vec<PageOut> = Vec::with_capacity(pages.len());
    for (pi, op) in pages.iter().enumerate() {
        let [ml, mt, mr, mb] = op.margin;
        let (w, h) = (op.trim.width() + ml + mr, op.trim.height() + mt + mb);
        let trim = [ml, mb, ml + op.trim.width(), mb + op.trim.height()];
        let [bl, bt, br, bb] = op.bleed;
        let bleed = [trim[0] - bl, trim[1] - bb, trim[2] + br, trim[3] + bt];
        let mut s = Stream::new(Some(Rect { left: 0.0, top: 0.0, right: w, bottom: h }));
        // spread (y down) → PDF user space (y up), trim at its place in the media box
        let base = Affine::new(1.0, 0.0, 0.0, -1.0, ml - op.trim.left, mb + op.trim.bottom);
        let flat = if op.cells.is_empty() && opts.booklet.is_some() {
            None // an empty face of a booklet
        } else if !op.cells.is_empty() {
            // Print Booklet: every page at its place on the sheet, clipped to the page (measured: nothing of a page shows outside it)
            for cell in &op.cells {
                let page_rect = cell.page.transform.apply_rect(&cell.page.bounds);
                let (x0, y1) = (cell.origin[0], h - cell.origin[1]);
                let (sx, sy) = (cell.slot[0], h - cell.slot[1]);
                // the page where it is, cut at the place it was meant for (measured with creep: a page moved 6 pt to the left shows
                // nothing of its left 6 pt, one moved 6 pt to the right nothing of its right 6 pt)
                let place = Rect {
                    left: x0.max(sx) - cell.bleed[0],
                    top: (y1 - page_rect.height()).max(sy - page_rect.height()),
                    right: (x0 + page_rect.width()).min(sx + page_rect.width()) + cell.bleed[1],
                    bottom: y1.min(sy),
                };
                s.push();
                let outer = s.set_cull(Some(place));
                s.rect(&place);
                s.clip(false);
                s.concat(&Affine::new(1.0, 0.0, 0.0, -1.0, x0 - page_rect.left, y1 + page_rect.top));
                let mut sources: Vec<Source> = Vec::new();
                for pg in &cell.spread.pages {
                    sources.extend(master_sources(doc, cell.spread, pg));
                }
                sources.push(Source { items: &cell.spread.items, transform: Affine::IDENTITY, master_page: None, hidden: Vec::new() });
                let mut d = Drawer::new(doc, layout, fonts, reg);
                d.page = cell.page.index;
                d.draw_sources(&mut s, &sources);
                s.set_cull(outer);
                s.pop();
            }
            None
        } else {
            s.push();
            if opts.marks.any() {
                // marks print past the bleed: the page's content stops at the bleed box (measured in InDesign's print: an object of the
                // neighbouring page does not show in the margin where the marks are)
                s.rect(&Rect { left: bleed[0], top: bleed[1], right: bleed[2], bottom: bleed[3] });
                s.clip(false);
            }
            s.concat(&base);
            let mut sources: Vec<Source> = Vec::new();
            for pg in &op.spread.pages {
                // facing pages' masters too: they show in the bleed at the spine
                sources.extend(master_sources(doc, op.spread, pg));
            }
            sources.push(Source { items: &op.spread.items, transform: Affine::IDENTITY, master_page: None, hidden: Vec::new() });
            let flat = {
                let mut d = Drawer::new(doc, layout, fonts, reg);
                d.page = op.pages[0].index;
                if flattening && d.may_have_transparency(&sources) {
                    d.flat = Some(flatten::PageFlat::default());
                }
                d.draw_sources(&mut s, &sources);
                d.flat.take()
            };
            s.pop();
            flat
        };
        let out_partial = PageOut { content: Vec::new(), used: ResUse::default(), transparency: false, media: [w, h], trim, bleed };
        if opts.marks.any() {
            draw_marks(&mut s, reg, opts, &out_partial, info_font.map(|f| (f, info_glyphs[pi].0.as_slice(), info_glyphs[pi].1.as_slice())));
        }
        let (mut content, mut used, transparency) = s.finish();
        if let Some(flat) = flat.filter(|f| !f.sites.is_empty()) {
            // regions rasterised after their last transparent unit, clipped out before it
            let clusters = flatten::clusters(&flat, [w, h], tile);
            if !clusters.is_empty() {
                let gs_off = reg.gs(GsDef::Overprint { fill: false, stroke: false });
                let mut forms = Vec::new();
                for c in &clusters {
                    let job = flat_jobs.len();
                    flat_jobs.push(flatten::FlatJob { page: op.pages[0].index, to_page: base, cluster: c.clone(), out_page: pi, smooth_ppi: None });
                    let f = reg.add_form(FormDef::Flat(job));
                    used.forms.insert(f);
                    forms.push(f);
                }
                used.ext_gstates.insert(gs_off);
                let (c, clipped) = flatten::assemble(&content, &flat, &clusters, &forms, gs_off, [w, h], tile);
                content = c;
                flat_report.clipped_units += clipped;
                flat_report.regions += clusters.len();
                flat_report.pages += 1;
                if flat_report.page_list.len() < 200 {
                    flat_report.page_list.push(op.pages[0].index);
                }
                flat_page_slot.insert(pi, flat_report.per_page.len());
                flat_report.per_page.push((op.pages[0].index, flat.sites.len(), 0));
            }
            flat_report.objects += flat.sites.len();
            for (k, n) in &flat.kinds {
                *flat_kinds.entry(k).or_default() += n;
            }
        }
        outs.push(PageOut { content, used, transparency, ..out_partial });
        reg.counts.pages += 1;
    }

    DrawnPages { outs, flat_jobs, flat_report, flat_kinds, flat_page_slot }
}
