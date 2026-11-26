use folio_model::{Document, Rect};

/// A page-local render region, bounded by the document's bleed and slug. An omitted
/// region still means the trim box. Shared with the ink renderer and its cache.
pub fn page_region(doc: &Document, index: usize, region: Option<Rect>) -> Option<Rect> {
    let (_, _, bbox) = crate::locate_page(doc, index)?;
    let trim = Rect { left: 0.0, top: 0.0, right: bbox.width(), bottom: bbox.height() };
    let Some(r) = region else { return Some(trim) };
    if ![r.left, r.top, r.right, r.bottom].iter().all(|v| v.is_finite()) { return None; }
    let margin = |i: usize| doc.print.bleed[i].max(doc.print.slug[i]).max(0.0);
    let horizontal = margin(2).max(margin(3));
    let area = Rect {
        left: r.left.max(-horizontal), top: r.top.max(-margin(0)),
        right: r.right.min(trim.right + horizontal), bottom: r.bottom.min(trim.bottom + margin(1)),
    };
    (area.width() > 0.0 && area.height() > 0.0).then_some(area)
}
