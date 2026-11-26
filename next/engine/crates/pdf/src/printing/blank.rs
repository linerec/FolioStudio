//! Which pages Print leaves out when "Print Blank Pages" is off.
//!
//! Measured in InDesign 2025 (`printx-c1-20261023/measurement/printing-blank.json`, rules in
//! `next/oracle/rules/printing.ko.md`): a page is blank when nothing that can print touches it.
//! Not blank: a page with a filled or stroked object, a placed picture, a text frame holding any
//! character (even one space), or a parent-page object. Blank: no objects, an object with neither
//! fill nor stroke, an empty text frame, an object on a hidden layer, a Nonprinting object, an
//! object lying only on the pasteboard, a ruler guide.
use folio_model::{Affine, Document, ItemKind, Page, PageItem, Paint, Rect, Spread};
use std::collections::HashSet;

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

fn layer_prints(doc: &Document, item: &PageItem) -> bool {
    match item.layer.as_deref().and_then(|id| doc.layers.iter().find(|l| l.id == id)) {
        Some(layer) => layer.visible && layer.printable,
        None => true,
    }
}

/// The item's own bounds in the coordinates `to` leads to (`to` maps the item's parent space).
fn bounds(item: &PageItem, to: &Affine) -> Option<Rect> {
    let inner = folio_model::paths_bounds(&item.paths)?;
    Some(to.then_inner(&item.transform).apply_rect(&inner))
}

fn paints(item: &PageItem) -> bool {
    item.fill != Paint::None || (item.stroke.paint != Paint::None && item.stroke.weight > 0.0)
}

/// Whether the item (or what it holds) puts ink on the page rectangle `page`.
fn inks(doc: &Document, item: &PageItem, to: &Affine, page: &Rect, skip: &HashSet<&str>, top: bool) -> bool {
    if !item.visible || item.print.nonprinting || skip.contains(item.id.as_str()) {
        return false;
    }
    // a group's members carry their own layers in IDML; only a top-level item is held back by its layer here too
    if top && !layer_prints(doc, item) {
        return false;
    }
    if let ItemKind::Group { children } = &item.kind {
        let inner = to.then_inner(&item.transform);
        return children.iter().any(|c| inks(doc, c, &inner, page, skip, false));
    }
    let Some(b) = bounds(item, to) else { return false };
    if !overlaps(&b, page) {
        return false;
    }
    match &item.kind {
        ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => content.is_some() || paints(item),
        ItemKind::GraphicLine => paints(item),
        ItemKind::TextFrame(frame) => {
            paints(item) || doc.stories.get(&frame.story).is_some_and(|s| s.all_runs().iter().any(|r| !r.text.is_empty()))
        }
        ItemKind::Other { .. } => true,
        ItemKind::Group { .. } => unreachable!(),
    }
}

/// True when printing `page` of `spread` without "Print Blank Pages" leaves it out.
pub fn is_blank(doc: &Document, spread: &Spread, page: &Page) -> bool {
    let rect = page.transform.apply_rect(&page.bounds);
    let none = HashSet::new();
    if spread.items.iter().any(|i| inks(doc, i, &Affine::IDENTITY, &rect, &none, true)) {
        return false;
    }
    !folio_model::parents::sources(doc, spread, page).iter().any(|source| {
        let overridden = source.hidden.iter().copied().collect();
        source.spread.items.iter().any(|i| inks(doc, i, &source.transform, &rect, &overridden, true))
    })
}
