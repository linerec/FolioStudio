//! Assemble the pageItems reply for a single page and its applied parent pages.

use super::geometry::{geometry_bounds, page_for, page_rects, side};
use super::items::{ItemInfo, Items};
use super::{Index, r3};
use folio_color::Colors;
use folio_model::{Affine, Document, Point, Rect};
use serde::Serialize;
use std::collections::HashSet;

/// `pageItems` reply body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PageItems<'a> {
    page: usize,
    size: [f64; 2],
    side: &'static str,
    /// As IDML stores them: top, left (inside on facing pages), bottom, right.
    margins: [f64; 4],
    columns: Columns<'a>,
    guides: Vec<GuideInfo<'a>>,
    items: Vec<ItemInfo<'a>>,
    master_items: Vec<ItemInfo<'a>>,
}

#[derive(Serialize)]
struct Columns<'a> {
    count: u32,
    gutter: f64,
    positions: Option<Vec<f64>>,
    direction: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuideInfo<'a> {
    id: &'a str,
    /// A spread guide (across the spread's pages), not one page's.
    spread: bool,
    master: bool,
    orientation: &'static str,
    location: f64,
    fit_to_page: bool,
    color: [u8; 3],
    layer: Option<&'a str>,
    locked: bool,
    own_locked: bool,
    view_threshold: f64,
    color_name: Option<&'a str>,
    guide_type: &'a str,
}

pub(super) fn page_items<'a>(doc: &'a Document, colors: &'a Colors, index: &'a Index, page_index: usize) -> Result<PageItems<'a>, String> {
    let &(si, pi) = index.pages.get(&page_index).ok_or_else(|| format!("no page {page_index}"))?;
    let spread = doc.spreads.iter().chain(&doc.master_spreads).nth(si).ok_or_else(|| format!("no spread {si}"))?;
    let page = &spread.pages[pi];
    let rects = page_rects(spread);
    let pr = rects[pi];
    let to_page = Affine::translate(-pr.left, -pr.top);

    // Master items of each page of the spread (they number first), this page's in full.
    let mut z = 0;
    let mut master_items = Vec::new();
    for (qi, q) in spread.pages.iter().enumerate() {
        for source in folio_model::parents::sources(doc, spread, q) {
            let (master, to_doc) = (source.spread, source.transform);
            let ctx = Items::new(doc, colors, index, source.hidden.iter().copied().collect(), true);
            let qr = rects[qi];
            let t = Affine::translate(-qr.left, -qr.top).then_inner(&to_doc);
            let area = Rect { left: 0.0, top: 0.0, right: qr.width(), bottom: qr.height() };
            for item in ctx.ordered(&master.items) {
                // shown where it overlaps the page (lines and points get a hair of width; an
                // item that only touches the page's edge is not on it)
                let hair = |lo: f64, hi: f64| if hi - lo < 0.02 { ((lo + hi) / 2.0 - 0.01, (lo + hi) / 2.0 + 0.01) } else { (lo, hi) };
                let shown = geometry_bounds(item, &t.then_inner(&item.transform)).is_some_and(|b| {
                    let ((l, r), (tp, bt)) = (hair(b.left, b.right), hair(b.top, b.bottom));
                    l < area.right && area.left < r && tp < area.bottom && area.top < bt
                });
                if !shown {
                    continue;
                }
                if qi == pi {
                    master_items.push(ctx.root_item(item, &t, &mut z));
                } else {
                    z += ctx.count(item);
                }
            }
        }
    }

    // The spread's items: each listed on the page that holds the centre of its bounds.
    let ctx = Items::new(doc, colors, index, HashSet::new(), false);
    let mut items = Vec::new();
    for item in ctx.ordered(&spread.items) {
        let on_page = rects.len() == 1 || {
            let c = geometry_bounds(item, &item.transform)
                .map(|b| Point::new((b.left + b.right) / 2.0, (b.top + b.bottom) / 2.0))
                .unwrap_or(Point::new(item.transform.tx, item.transform.ty));
            page_for(&rects, c) == pi
        };
        if on_page {
            items.push(ctx.root_item(item, &to_page, &mut z));
        } else {
            z += ctx.count(item);
        }
    }

    let side = side(index.facing, spread, page);
    let cols = &page.columns;
    // Column edges from the page's left live-area edge: stored `Left` is the inside margin
    // on facing pages, so on a left-hand page the left edge is the stored `Right`.
    let x0 = if side == "left" { page.margins[3] } else { page.margins[1] };
    let parents = folio_model::parents::sources(doc, spread, page);
    let guide_sources = parents.iter().map(|s| (s.spread, s.page, s.transform, true))
        .chain(std::iter::once((spread, page, Affine::IDENTITY, false)));
    let guides = guide_sources.flat_map(|(owner, owner_page, transform, master)| {
        let owner_pi = owner.pages.iter().position(|p| p.id == owner_page.id).unwrap_or(0);
        owner.guides.iter().filter(move |g| g.page.is_none_or(|p| p == owner_pi)).map(move |g| {
            let lf = g.layer.as_deref().and_then(|l| index.layers.get(l));
            let position = transform.apply(Point::new(if g.horizontal { 0.0 } else { g.position }, if g.horizontal { g.position } else { 0.0 }));
            GuideInfo {
                id: &g.id,
                spread: g.page.is_none(),
                master,
                orientation: if g.horizontal { "horizontal" } else { "vertical" },
                location: r3(if g.horizontal { position.y - pr.top } else { position.x - pr.left }),
                fit_to_page: g.fit_to_page,
                color: g.color.rgb(),
                layer: g.layer.as_deref(),
                locked: master || g.locked || doc.view.guides_locked || lf.is_some_and(|l| l.locked || l.lock_guides),
                own_locked: g.locked,
                view_threshold: g.view_threshold.unwrap_or(5.0),
                color_name: g.color.name(),
                guide_type: g.guide_type.as_deref().unwrap_or("Ruler"),
            }
        })
    }).collect();
    Ok(PageItems {
        page: page_index,
        size: [r3(pr.width()), r3(pr.height())],
        side,
        margins: page.margins.map(r3),
        columns: Columns {
            count: cols.count,
            gutter: r3(cols.gutter),
            positions: cols.positions.as_ref().map(|p| p.iter().map(|v| r3(x0 + v)).collect()),
            direction: &cols.direction,
        },
        guides,
        items,
        master_items,
    })
}
