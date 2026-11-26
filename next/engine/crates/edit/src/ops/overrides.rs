//! Parent items on document pages: Override (Ctrl+Shift+click on a parent item, Pages ▸ Override All Parent Page Items),
//! Remove All Local Overrides, Detach All Objects from Parent, Allow Parent Item Overrides. Measured in InDesign 2025
//! (next/oracle/indesign/probe-master-override.jsx; oracle pages-override):
//! - the page's `OverrideList` is pairs "parent item, page copy"; once the copy is deleted or detached the pair reads
//!   "parent item, n" and the parent item stays off the page (a second override of it is refused);
//! - an override is the parent item's XML with new ids (text and stories copied) and without `AllowOverrides`; its
//!   `ItemTransform` carries it from the parent page onto this one; it is the first of the spread's items (behind the
//!   page's own), a new one in front of the older ones;
//! - removing the override takes copy and pair away, the parent item shows again; Detach changes nothing but the pair;
//! - a parent item with `AllowOverrides="false"` is refused ("This parent page item does not allow overrides").
//!   Not measured: the DOM overrides only the first rectangle of a parent page and puts a left-hand parent's item one page
//!   too far left, so the oracle covers rectangles of right-hand pages; other kinds and left-hand pages follow the same
//!   rules (the copy keeps the parent item's place on the page). `OverriddenPageItemProps` (what a copy changed, as
//!   InDesign's property numbers) is left empty. Override All puts the copies in the parent's stacking order.
use super::*;

/// "parent item, copy" pairs of an `OverrideList`.
fn pairs(list: &str) -> Vec<(String, String)> {
    let v: Vec<&str> = list.split_ascii_whitespace().collect();
    v.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_owned(), c[1].to_owned())).collect()
}

fn list_text(p: &[(String, String)]) -> String {
    p.iter().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect::<Vec<_>>().join(" ")
}

fn rect_of(ix: &Index, t: &str, page: usize) -> Option<[f64; 4]> {
    let b: Vec<f64> = ix.attr(t, page, "GeometricBounds")?.split_ascii_whitespace().filter_map(|v| v.parse().ok()).collect();
    let [top, left, bottom, right] = <[f64; 4]>::try_from(b).ok()?;
    let m = item_transform(ix, t, page);
    let c = [m.apply((left, top)), m.apply((right, top)), m.apply((left, bottom)), m.apply((right, bottom))];
    Some([
        c.iter().map(|p| p.0).fold(f64::MAX, f64::min),
        c.iter().map(|p| p.1).fold(f64::MAX, f64::min),
        c.iter().map(|p| p.0).fold(f64::MIN, f64::max),
        c.iter().map(|p| p.1).fold(f64::MIN, f64::max),
    ])
}

/// A page of the document: its part, text, index, node and spread node.
struct PageAt {
    part: String,
    text: String,
    ix: Index,
    node: usize,
    spread: usize,
}

fn page_at(w: &mut Work<'_>, page: &str) -> Result<PageAt, EditError> {
    let (part, ix, node) = w.find(page)?;
    let text = w.text(&part)?.to_owned();
    if ix.local(&text, node) != "Page" || !part.starts_with("Spreads/") {
        return Err(EditError::new("invalid", format!("{page} is not a page of the document")));
    }
    let spread = ix.ancestors(node).find(|&a| ix.local(&text, a) == "Spread").ok_or_else(|| EditError::new("xml", "a page outside a spread"))?;
    Ok(PageAt { part, text, ix, node, spread })
}

impl PageAt {
    fn pairs(&self) -> Vec<(String, String)> {
        pairs(self.ix.attr(&self.text, self.node, "OverrideList").unwrap_or_default())
    }

    /// The edit that writes `OverrideList`.
    fn write(&self, list: &[(String, String)]) -> (std::ops::Range<usize>, String) {
        attr_edit(&self.ix, &self.text, self.node, "OverrideList", &list_text(list))
    }
}

/// The parent items an override can copy onto this page, and how the parent's coordinates map onto the page's.
struct Parent {
    /// Parent spread coordinates → this spread's.
    to_doc: Mat,
    /// The page in this spread's coordinates.
    area: [f64; 4],
    part: String,
    text: String,
    ix: Index,
    /// The parent spread's own items, back to front.
    items: Vec<usize>,
}

fn parent_of(w: &mut Work<'_>, p: &PageAt) -> Result<Parent, EditError> {
    let master = p.ix.attr(&p.text, p.node, "AppliedMaster").filter(|m| *m != "n").ok_or_else(|| EditError::new("invalid", "the page has no parent page"))?.to_owned();
    let (part, ix, mi) = w.find(&master)?;
    let text = w.text(&part)?.to_owned();
    let area = rect_of(&p.ix, &p.text, p.node).ok_or_else(|| EditError::new("xml", "a page without bounds"))?;
    // the parent's page this page takes (the same choice as the renderer's): the right-hand one of two for a right-hand page
    let count = p.ix.named(&p.text, p.spread, "Page").len();
    let right = if count == 1 { area[0] >= -1.0 } else { (area[0] + area[2]) / 2.0 > 0.0 };
    let pages = ix.named(&text, mi, "Page");
    let mp = if pages.len() >= 2 && right { pages[pages.len() - 1] } else { *pages.first().ok_or_else(|| EditError::new("xml", "a parent without pages"))? };
    let inverse = item_transform(&ix, &text, mp).invert().ok_or_else(|| EditError::new("unsupported", "the parent page cannot be inverted"))?;
    let on_master = p.ix.attr(&p.text, p.node, "MasterPageTransform").and_then(Mat::parse).unwrap_or(Mat::ID);
    let to_doc = item_transform(&p.ix, &p.text, p.node).then_after(&on_master).then_after(&inverse);
    let items = ix.children(mi).iter().copied().filter(|&c| is_item(&ix, &text, c)).collect();
    Ok(Parent { to_doc, area, part, text, ix, items })
}

impl Parent {
    /// Whether the item shows on the page: it meets it (a line or a point gets a hair of width).
    fn shows(&self, i: usize) -> bool {
        let Some(b) = spread_bounds(&self.ix, &self.text, i) else { return false };
        let c = [self.to_doc.apply((b[0], b[1])), self.to_doc.apply((b[2], b[1])), self.to_doc.apply((b[0], b[3])), self.to_doc.apply((b[2], b[3]))];
        let span = |k: usize| {
            let lo = c.iter().map(|p| [p.0, p.1][k]).fold(f64::MAX, f64::min);
            let hi = c.iter().map(|p| [p.0, p.1][k]).fold(f64::MIN, f64::max);
            if hi - lo < 0.02 { ((lo + hi) / 2.0 - 0.01, (lo + hi) / 2.0 + 0.01) } else { (lo, hi) }
        };
        let ((l, r), (tp, bt)) = (span(0), span(1));
        l < self.area[2] && self.area[0] < r && tp < self.area[3] && self.area[1] < bt
    }

    fn allows(&self, i: usize) -> bool {
        self.ix.attr(&self.text, i, "AllowOverrides") != Some("false")
    }
}

/// Copies of parent items on `page`: `ids` (the parent items' ids; none: every one that shows on the page, allows it and is not
/// overridden yet or taken off the page).
pub(super) fn override_items(w: &mut Work<'_>, page: &str, ids: &[String]) -> Result<(), EditError> {
    let p = page_at(w, page)?;
    if p.ix.attr(&p.text, p.spread, "ShowMasterItems") == Some("false") {
        return Err(EditError::new("invalid", "the page shows no parent items (Show Parent Items is off)"));
    }
    let parent = parent_of(w, &p)?;
    let mut list = p.pairs();
    let chosen: Vec<usize> = if ids.is_empty() {
        // every item in the parent's stacking order: each copy goes to the back, so the last one made is the back-most
        parent.items.iter().rev().copied().filter(|&i| parent.allows(i) && parent.shows(i) && !list.iter().any(|(m, _)| Some(m.as_str()) == parent.ix.attr(&parent.text, i, "Self"))).collect()
    } else {
        let mut out = Vec::new();
        let mut seen: Vec<&String> = Vec::new();
        for id in ids {
            // a parent item named twice is overridden once
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let i = parent.ix.by_self(&parent.text, id).ok_or_else(|| EditError::new("invalid", format!("{id} is not an item of the page's parent")))?;
            if !parent.items.contains(&i) {
                return Err(EditError::new("invalid", format!("{id} is not a parent item of the page")));
            }
            if !parent.allows(i) {
                return Err(EditError::new("invalid", format!("{id}: this parent page item does not allow overrides")));
            }
            if list.iter().any(|(m, _)| m == id) {
                return Err(EditError::new("invalid", format!("{id}: this parent page item is already overridden on the page")));
            }
            out.push(i);
        }
        // each copy goes to the back: the front-most parent item first, so the copies keep the parent's stacking order
        out.sort_by_key(|i| std::cmp::Reverse(parent.items.iter().position(|x| x == i)));
        out
    };
    if chosen.is_empty() {
        return Ok(());
    }
    // what the copies need: the parent's part and the stories of its text frames
    let mut keep = std::collections::BTreeSet::from([parent.part.clone()]);
    for story in (0..parent.ix.nodes.len()).filter_map(|k| parent.ix.attr(&parent.text, k, "ParentStory")) {
        keep.insert(format!("Stories/Story_{story}.xml"));
    }
    let src = clipboard::Source::of_parts(w.ed, &keep);
    let indent = indent_before(&p.text, p.ix.nodes[p.spread].start).to_owned() + "\t";
    let mut xml = String::new();
    for i in chosen {
        let master_id = parent.ix.attr(&parent.text, i, "Self").unwrap_or_default().to_owned();
        let c = clipboard::copy_element(w, &src, &master_id)?;
        // onto this page: through the parent page's place on it
        let full = parent.to_doc.then_after(&c.parent).then_after(&c.transform);
        let mut copy = clipboard::with_attr(&c.xml, "ItemTransform", Some(&full.text()))?;
        copy = clipboard::with_attr(&copy, "AllowOverrides", None)?;
        // each one in front of the ones made before it: the back of the spread
        xml = format!("\n{indent}{copy}{xml}");
        list.push((master_id, c.id.clone()));
        w.created.push(c.id);
    }
    // after the spread's last page
    let last = *p.ix.named(&p.text, p.spread, "Page").last().ok_or_else(|| EditError::new("xml", "a spread without pages"))?;
    let at = p.ix.nodes[last].end;
    w.splice(&p.part, vec![p.write(&list), (at..at, xml)])
}

/// The page's live pairs (a copy that still exists) a command names; `ids` none: all of them.
fn live_copies(p: &PageAt, ids: &[String]) -> Result<Vec<(String, String)>, EditError> {
    let list = p.pairs();
    for id in ids {
        if !list.iter().any(|(_, c)| c == id) {
            return Err(EditError::new("invalid", format!("{id} is not a copy of a parent item on this page")));
        }
    }
    Ok(list.into_iter().filter(|(_, c)| c != "n" && (ids.is_empty() || ids.contains(c))).collect())
}

/// Remove All Local Overrides: the page's copies go and the parent items show again. Taken-off parent items (a deleted or
/// detached copy leaves "parent item, n") stay off.
pub(super) fn remove_overrides(w: &mut Work<'_>, page: &str, ids: &[String]) -> Result<(), EditError> {
    let p = page_at(w, page)?;
    let gone = live_copies(&p, ids)?;
    if gone.is_empty() {
        return Ok(());
    }
    let rest: Vec<(String, String)> = p.pairs().into_iter().filter(|x| !gone.contains(x)).collect();
    w.splice(&p.part, vec![p.write(&rest)])?;
    for (_, copy) in gone {
        delete(w, &copy)?;
    }
    Ok(())
}

/// Detach All Objects from Parent: the copies stay, now standing alone; the parent items stay off the page.
pub(super) fn detach(w: &mut Work<'_>, page: &str, ids: &[String]) -> Result<(), EditError> {
    let p = page_at(w, page)?;
    let free = live_copies(&p, ids)?;
    if free.is_empty() {
        return Ok(());
    }
    let list: Vec<(String, String)> = p.pairs().into_iter().map(|x| if free.contains(&x) { (x.0, "n".into()) } else { x }).collect();
    w.splice(&p.part, vec![p.write(&list)])
}

/// Copies were deleted (an element and what is inside it): their pairs read "parent item, n" and the parent items stay off the
/// page (oracle: delete of a copy).
pub(super) fn forget_copy(w: &mut Work<'_>, part: &str, ids: &[String]) -> Result<(), EditError> {
    if !part.starts_with("Spreads/") {
        return Ok(());
    }
    let t = w.text(part)?;
    // nothing to do unless one of them is still named somewhere (a pair)
    if !ids.iter().any(|id| t.contains(id.as_str())) {
        return Ok(());
    }
    let t = t.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let mut edits = Vec::new();
    for i in (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "Page") {
        let list = pairs(ix.attr(&t, i, "OverrideList").unwrap_or_default());
        if list.iter().any(|(_, c)| ids.contains(c)) {
            let kept: Vec<(String, String)> = list.into_iter().map(|(m, c)| if ids.contains(&c) { (m, "n".into()) } else { (m, c) }).collect();
            edits.push(attr_edit(&ix, &t, i, "OverrideList", &list_text(&kept)));
        }
    }
    if edits.is_empty() { Ok(()) } else { w.splice(part, edits) }
}

/// Allow Parent Item Overrides on Selection: `AllowOverrides` of parent items (written as InDesign does, always).
pub(super) fn set_allow(w: &mut Work<'_>, ids: &[String], allow: bool) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let top = ix.nodes[i].parent.is_some_and(|p| ix.local(&t, p) == "MasterSpread");
        if !part.starts_with("MasterSpreads/") || !top {
            return Err(EditError::new("invalid", format!("{id} is not an item of a parent page")));
        }
        let now = ix.attr(&t, i, "AllowOverrides");
        if now == Some(bool_text(allow).as_str()) || (allow && now.is_none()) {
            continue;
        }
        w.splice(&part, vec![attr_edit(&ix, &t, i, "AllowOverrides", &bool_text(allow))])?;
    }
    Ok(())
}
