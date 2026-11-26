//! Where every page item is: full transform to spread coordinates, bounds, the page it is
//! on, and whether it prints (visible, printable layer, not nonprinting, not hidden by an
//! ancestor). Items anchored in text are placed through the layout's anchor points.

use folio_model::layout::DocumentLayout;
use folio_model::{Affine, Document, ItemKind, Layer, PageItem, Rect, Spread, StrokeAlignment, paths_bounds};
use std::collections::HashMap;

/// A page of a spread in spread coordinates.
#[derive(Clone, Debug)]
pub struct PageBox {
    /// `Page::index` (document pages) or the position in the master spread.
    pub index: usize,
    pub name: String,
    pub rect: Rect,
    /// Edges that touch another page of the same spread (no bleed there): left, right.
    pub spine_left: bool,
    pub spine_right: bool,
    /// Page is left of another page in its spread (left-hand page of facing pages).
    pub left_hand: bool,
}

#[derive(Clone, Debug)]
pub struct Placed<'d> {
    pub item: &'d PageItem,
    /// Index into `Doc::spreads` (document spreads first, then masters).
    pub spread: usize,
    /// Inner → spread coordinates.
    pub to_spread: Affine,
    /// Path bounds in spread coordinates (`None` for items without paths, e.g. groups).
    pub bounds: Option<Rect>,
    /// Bounds including the visible stroke.
    pub visual: Option<Rect>,
    /// Page the item is on (most overlap), `None` on the pasteboard.
    pub page: Option<usize>,
    /// Draw order within the spread (parents before children).
    pub z: usize,
    pub parent: Option<usize>,
    /// Effective visibility (item, ancestors, layer).
    pub visible: bool,
    /// Item or an ancestor is `Nonprinting`.
    pub nonprinting: bool,
    /// Layer prints (visible and printable).
    pub layer_prints: bool,
    pub layer: Option<&'d Layer>,
    /// Pasted inside another frame (clipped by it).
    pub clipped: bool,
    /// Anchored in text.
    pub anchored: bool,
}

impl Placed<'_> {
    /// Ends up in print output.
    pub fn prints(&self) -> bool {
        self.visible && !self.nonprinting && self.layer_prints
    }
}

/// A spread with its pages (document spreads, then masters).
pub struct SpreadInfo<'d> {
    pub spread: &'d Spread,
    pub master: bool,
    pub pages: Vec<PageBox>,
}

pub struct Doc<'d> {
    pub spreads: Vec<SpreadInfo<'d>>,
    pub items: Vec<Placed<'d>>,
    /// Item id → index in `items`.
    pub by_id: HashMap<&'d str, usize>,
    /// Document page index → (spread index, position in `SpreadInfo::pages`).
    pub page_at: HashMap<usize, (usize, usize)>,
}

/// Scale of `t` along x and y (lengths of the transformed unit vectors).
pub fn scales(t: &Affine) -> (f64, f64) {
    ((t.a * t.a + t.b * t.b).sqrt(), (t.c * t.c + t.d * t.d).sqrt())
}

fn area(r: &Rect) -> f64 {
    r.width().max(0.0) * r.height().max(0.0)
}

pub fn intersection(a: &Rect, b: &Rect) -> Option<Rect> {
    let r = Rect { left: a.left.max(b.left), top: a.top.max(b.top), right: a.right.min(b.right), bottom: a.bottom.min(b.bottom) };
    (r.right > r.left && r.bottom > r.top).then_some(r)
}

/// Visible stroke outside the path: half the weight (centre), all (outside), none (inside),
/// scaled by the item's transform.
fn stroke_extent(item: &PageItem, t: &Affine) -> f64 {
    if item.stroke.paint == folio_model::Paint::None || item.stroke.weight <= 0.0 {
        return 0.0;
    }
    let (sx, sy) = scales(t);
    let w = item.stroke.weight * (sx * sy).sqrt();
    match item.stroke.alignment {
        StrokeAlignment::Center => w / 2.0,
        StrokeAlignment::Outside => w,
        StrokeAlignment::Inside => 0.0,
    }
}

fn pages_of(s: &Spread, master: bool) -> Vec<PageBox> {
    let rects: Vec<Rect> = s.pages.iter().map(|p| p.transform.apply_rect(&p.bounds)).collect();
    let touches = |i: usize, x: f64| {
        rects.iter().enumerate().any(|(j, r)| {
            j != i && ((r.left - x).abs() < 0.5 || (r.right - x).abs() < 0.5) && r.top < rects[i].bottom && rects[i].top < r.bottom
        })
    };
    s.pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let r = rects[i];
            let spine_left = touches(i, r.left);
            let spine_right = touches(i, r.right);
            PageBox {
                index: if master { i } else { p.index },
                name: p.name.clone(),
                rect: r,
                spine_left,
                spine_right,
                left_hand: spine_right && !spine_left,
            }
        })
        .collect()
}

impl<'d> Doc<'d> {
    pub fn new(doc: &'d Document, layout: &DocumentLayout) -> Doc<'d> {
        let layers: HashMap<&str, &Layer> = doc.layers.iter().map(|l| (l.id.as_str(), l)).collect();
        let mut spreads = Vec::new();
        for s in &doc.spreads {
            spreads.push(SpreadInfo { spread: s, master: false, pages: pages_of(s, false) });
        }
        for s in &doc.master_spreads {
            spreads.push(SpreadInfo { spread: s, master: true, pages: pages_of(s, true) });
        }
        let mut d = Doc { spreads, items: Vec::new(), by_id: HashMap::new(), page_at: HashMap::new() };
        for (si, info) in d.spreads.iter().enumerate() {
            if !info.master {
                for (pi, p) in info.pages.iter().enumerate() {
                    d.page_at.insert(p.index, (si, pi));
                }
            }
        }
        let mut items = Vec::new();
        for si in 0..d.spreads.len() {
            let spread = d.spreads[si].spread;
            for it in &spread.items {
                walk(&mut items, &layers, it, si, Affine::IDENTITY, None, true, false, false, false);
            }
        }
        // Anchored objects: placed at their anchor in the frame that shows them.
        let inline: HashMap<&str, &PageItem> = doc
            .stories
            .values()
            .flat_map(|s| s.all_runs())
            .flat_map(|r| &r.inline)
            .map(|i| (i.id.as_str(), i))
            .collect();
        if !inline.is_empty() {
            let frames: HashMap<&str, usize> = items.iter().enumerate().map(|(i, p): (usize, &Placed<'d>)| (p.item.id.as_str(), i)).collect();
            for (fid, ft) in &layout.frames {
                let Some(&fi) = frames.get(fid.as_str()) else { continue };
                let (si, ft_t, vis, np, lp) = {
                    let f = &items[fi];
                    (f.spread, f.to_spread, f.visible, f.nonprinting, f.layer_prints)
                };
                for line in &ft.lines {
                    for o in &line.objects {
                        if let Some(it) = inline.get(o.item.as_str()) {
                            let t = ft_t.then_inner(&Affine::translate(o.x, o.y));
                            let start = items.len();
                            walk(&mut items, &layers, it, si, t, Some(fi), vis, np, false, true);
                            for p in &mut items[start..] {
                                p.layer_prints &= lp;
                            }
                        }
                    }
                }
            }
        }
        // Pages.
        for (z, p) in items.iter_mut().enumerate() {
            p.z = z;
            let pages = &d.spreads[p.spread].pages;
            // Lines and points have no area: give them a hair of width to find their page.
            let r = p.visual.or(p.bounds).map(|r| Rect {
                left: r.left.min(r.right - 0.01),
                top: r.top.min(r.bottom - 0.01),
                right: r.right.max(r.left + 0.01),
                bottom: r.bottom.max(r.top + 0.01),
            });
            // Most overlap; on a tie (spread-wide items) the first page, as InDesign does.
            p.page = r.and_then(|r| {
                let mut best: Option<(f64, usize)> = None;
                for pg in pages {
                    if let Some(x) = intersection(&r, &pg.rect) {
                        let a = area(&x);
                        if best.is_none_or(|b| a > b.0 + 1e-6) {
                            best = Some((a, pg.index));
                        }
                    }
                }
                best.map(|b| b.1)
            });
        }
        // Groups take the page of their first child on a page.
        for i in (0..items.len()).rev() {
            if items[i].page.is_none() && matches!(items[i].item.kind, ItemKind::Group { .. }) {
                let child = items.iter().skip(i + 1).find(|c| c.parent == Some(i) && c.page.is_some()).and_then(|c| c.page);
                items[i].page = child;
            }
        }
        d.by_id = items.iter().enumerate().map(|(i, p)| (p.item.id.as_str(), i)).collect();
        d.items = items;
        d
    }

    pub fn master(&self, p: &Placed<'_>) -> bool {
        self.spreads[p.spread].master
    }

    /// Master spread name for items on masters.
    pub fn master_name(&self, p: &Placed<'_>) -> Option<&str> {
        let s = &self.spreads[p.spread];
        s.master.then(|| s.spread.name.as_deref().unwrap_or(s.spread.id.as_str()))
    }

    /// Page box of a document page.
    pub fn page_box(&self, page: usize) -> Option<&PageBox> {
        let (si, pi) = self.page_at.get(&page)?;
        self.spreads.get(*si)?.pages.get(*pi)
    }

    /// The document page an item is on (`None` for masters and the pasteboard).
    pub fn doc_page(&self, p: &Placed<'_>) -> Option<usize> {
        if self.master(p) { None } else { p.page }
    }
}

// Recursion carries the parent's placement and visibility state without changing it.
#[allow(clippy::too_many_arguments)]
fn walk<'d>(
    out: &mut Vec<Placed<'d>>,
    layers: &HashMap<&str, &'d Layer>,
    item: &'d PageItem,
    spread: usize,
    parent_t: Affine,
    parent: Option<usize>,
    parent_visible: bool,
    parent_nonprinting: bool,
    clipped: bool,
    anchored: bool,
) {
    let t = parent_t.then_inner(&item.transform);
    let bounds = paths_bounds(&item.paths).map(|r| t.apply_rect(&r));
    let ext = stroke_extent(item, &t);
    let visual = bounds.map(|b| Rect { left: b.left - ext, top: b.top - ext, right: b.right + ext, bottom: b.bottom + ext });
    let layer = item.layer.as_deref().and_then(|l| layers.get(l).copied());
    // Children inherit their parent's layer when they have none of their own.
    let parent_layer = parent.and_then(|p| out[p].layer);
    let layer = layer.or(parent_layer);
    let layer_prints = layer.is_none_or(|l| l.visible && l.printable);
    let visible = parent_visible && item.visible && layer.is_none_or(|l| l.visible);
    let nonprinting = parent_nonprinting || item.print.nonprinting;
    let idx = out.len();
    out.push(Placed {
        item,
        spread,
        to_spread: t,
        bounds,
        visual,
        page: None,
        z: 0,
        parent,
        visible,
        nonprinting,
        layer_prints,
        layer,
        clipped,
        anchored,
    });
    if let ItemKind::Group { children } = &item.kind {
        for c in children {
            walk(out, layers, c, spread, t, Some(idx), visible, nonprinting, clipped, anchored);
        }
    }
    for c in &item.clipped {
        walk(out, layers, c, spread, t, Some(idx), visible, nonprinting, true, anchored);
    }
}
