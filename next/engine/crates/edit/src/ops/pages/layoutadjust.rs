//! File ▸ Adjust Layout (`doc.adjustLayout` with a new page width and height and new margins; oracle: layoutadj-c2 to c4, measured with
//! InDesign 2025, rules in `next/oracle/rules/layoutadj-c2.ko.md` to `-c4.ko.md`).
//!
//! Every page keeps (or gets the new) margins; what is inside them is stretched. Along each axis a page falls into zones: the margins
//! and the gutters between columns move rigidly, the columns (the one text area of a page without columns, vertically always) scale.
//! A coordinate x on the page goes through that piecewise linear map (a margin that changes scales its own zone). An object is
//! adjusted by its *visible* bounds (the stroke included).
//! - A rectangle, text frame or polygon whose path is an axis aligned box (also a rotated one that comes out as such) is stretched:
//!   the outer edge of each side goes through the map and the stroke comes back, so an edge on a margin stays on the margin. A framed
//!   object never becomes narrower than twice the reach of its stroke (a hairline: 1 pt); one without a stroke may.
//! - A straight line (two points) is stretched by its visible bounds as well, but the stroke is worked out again for the new angle,
//!   and a horizontal or vertical line keeps its zero thickness.
//! - Everything else (ovals, other polygons, curves, open paths of more points, rotated objects, groups) keeps its proportions: the
//!   top left of its visible bounds goes through the map and the visible bounds are scaled by the smaller of the two scales the map
//!   gives on the object's own extent. A group scales all members by that, their strokes keep their reach (layoutadj-c4).
//!
//! Guides, also the spread's, go through the same map (the page they are over; beyond the pages the nearest one). Parent pages are
//! adjusted like document pages.
//! Margins as arguments change the parent pages and the pages that follow them (a page follows when its margins and columns equal its
//! parent's: what InDesign makes of a document read from IDML, measured by layoutadj-c4).
//! On a spread of one page (no facing pages) an object that lies wholly off the page, and a locked object when `allow_locked` is off,
//! is not adjusted: it moves rigidly (layoutadj-c3).
//! On a spread of facing pages (one page or two) an object across both pages goes through one map of the two pages side by side (the
//! page maps joined at the spine, the end beyond the spread moving with the whole growth), and an object wholly off the pages moves
//! rigidly: beyond the left or right side it keeps its place from that side (the far side moves with the growth of the spread) and moves
//! half the growth in height; above or below the pages it keeps its place to the spine and moves 0 or the whole height (layoutadj-c5).
//! An object over both ends of a spread (boxes only) grows by the growth of one page and stays at the left end (moves along with it
//! when bound right to left).
//! The spreads lie 36 pt plus the room asked below one and above the next apart; an object wholly above or below the pages asks for
//! max(pasteboard room, its nearest visible edge + 2 pt), so after the objects moved the spreads are laid out again (layoutadj-c6).
//! Not built (refused with a list): the other spreads of several pages, a locked object on them, font size and leading adjustment.
use super::fontfit;
use super::*;
use crate::ops::{alignment, resize_group};
use std::collections::BTreeMap;

/// One axis of one page: the old and new extent and margins, the columns that scale (or, for a spread, the chain of its pages).
#[derive(Clone)]
struct Axis {
    size: f64,
    lead: f64,
    trail: f64,
    new_size: f64,
    new_lead: f64,
    new_trail: f64,
    /// Old columns (`[from, to]` along the axis) and the new ones; one entry for a page without columns (and for the vertical axis).
    old: Vec<[f64; 2]>,
    new: Vec<[f64; 2]>,
    /// The pages of a spread side by side along this axis: the old and new offset of each from the first one, and its own axis.
    /// Empty for one page. Then `size` and `new_size` are the extent of all of them.
    chain: Vec<(f64, f64, Axis)>,
}

impl Axis {
    // An axis compares old and new dimensions, margins and columns without changing their evaluation order.
    #[allow(clippy::too_many_arguments)]
    fn new(size: f64, new_size: f64, lead: f64, trail: f64, new_lead: f64, new_trail: f64, count: usize, gutter: f64) -> Result<Self, EditError> {
        let count = count.max(1);
        let split = |size: f64, lead: f64, trail: f64| -> Result<Vec<[f64; 2]>, EditError> {
            let inner = size - lead - trail - gutter * (count as f64 - 1.0);
            #[allow(clippy::neg_cmp_op_on_partial_ord, reason = "reject NaN widths along with nonpositive widths")]
            if !(inner > 0.0) {
                return Err(EditError::new("invalid", "the margins leave no room on the page"));
            }
            let each = inner / count as f64;
            Ok((0..count).map(|k| { let a = lead + k as f64 * (each + gutter); [a, a + each] }).collect())
        };
        if (lead != new_lead && lead <= 0.0) || (trail != new_trail && trail <= 0.0) {
            return Err(EditError::new("unsupported", "changing a margin of 0 has not been measured"));
        }
        Ok(Self { size, lead, trail, new_size, new_lead, new_trail, old: split(size, lead, trail)?, new: split(new_size, new_lead, new_trail)?, chain: Vec::new() })
    }
    /// The pages of a spread laid side by side (the first one at 0): a point goes through the map of the page it is over, beyond the
    /// spread the first page's start stays and the end moves with the whole growth (layoutadj-c5).
    fn spread(pages: &[Axis]) -> Self {
        let (mut old_at, mut new_at, mut chain) = (0.0, 0.0, Vec::new());
        for a in pages {
            chain.push((old_at, new_at, a.clone()));
            old_at += a.size;
            new_at += a.new_size;
        }
        Self { size: old_at, lead: 0.0, trail: 0.0, new_size: new_at, new_lead: 0.0, new_trail: 0.0, old: Vec::new(), new: Vec::new(), chain }
    }
    /// A point of the old page on the new page.
    fn map(&self, x: f64) -> f64 {
        if !self.chain.is_empty() {
            if x <= 0.0 { return x; }
            if x >= self.size { return x + (self.new_size - self.size); }
            return self.chain.iter().find(|(at, _, a)| x <= at + a.size).map_or(x, |(at, to, a)| to + a.map(x - at));
        }
        if x <= 0.0 { return x; }
        if x <= self.lead { return x * self.new_lead / self.lead; }
        if x >= self.size { return x + (self.new_size - self.size); }
        if x >= self.size - self.trail { return self.new_size - self.new_trail + (x - (self.size - self.trail)) * self.new_trail / self.trail; }
        for k in 0..self.old.len() {
            let (o, n) = (self.old[k], self.new[k]);
            if x <= o[1] { return n[0] + (x - o[0]) * (n[1] - n[0]) / (o[1] - o[0]); }
            if let Some(next) = self.old.get(k + 1)
                && x < next[0] { return x - o[1] + n[1]; }
        }
        x
    }
    /// The edges `[a, b]` of an object whose stroke reaches `o` outside them.
    fn edges(&self, a: f64, b: f64, o: f64) -> [f64; 2] {
        let l = self.map(a - o) + o;
        let r = self.map(b + o) - o;
        [l, if r - l < 2.0 * o { l + 2.0 * o } else { r }]
    }
    /// How much the map scales the extent `[a, b]` (infinite for an extent of no size).
    fn scale_of(&self, a: f64, b: f64) -> f64 {
        if b > a { (self.map(b) - self.map(a)) / (b - a) } else { f64::INFINITY }
    }
}

/// What happens to one object.
enum Op {
    /// A box shaped rectangle, text frame or polygon: the outer edge of every side goes through the map (visible bounds, stroke outset `o`).
    Stretch { page: String, o: f64, local: [f64; 4] },
    /// An unrotated oval: its visible bounds go to `dest` (page units), its stroke keeps its reach `o`.
    Uniform { page: String, o: f64, dest: [f64; 4] },
    /// Left unadjusted (locked with the option off, or off the page): it moves rigidly, `dx`, `dy` in page units.
    Shift { page: String, local: [f64; 4], dx: f64, dy: f64 },
    /// Anything else: its visible bounds go to `dest` (page units); members keep their stroke reach.
    Visible { page: String, dest: [f64; 4] },
    /// A straight line along an axis: its path box `local` and visible bounds `vis` (page units) go through the map; the thickness stays 0.
    AxisLine { page: String, local: [f64; 4], vis: [f64; 4] },
}

/// How an object of a kind is adjusted.
#[derive(PartialEq)]
enum Class { Stretch, Line, Uniform }

struct Plan {
    /// per page id: the horizontal and vertical map and the old origin of the page in its spread
    pages: BTreeMap<String, (Axis, Axis, [f64; 2])>,
    /// per item: the part it is in, its id and what happens to it
    items: Vec<(String, String, Op)>,
    /// per page id: the new margins `[top, bottom, left, right]` (as IDML writes them, left and right being inside and outside)
    margins: BTreeMap<String, [f64; 4]>,
    /// The document's new default margins `[top, bottom, left, right]` when the arguments change them.
    doc_margins: Option<[f64; 4]>,
}

type PageMargins = (f64, f64, f64, f64, usize, f64);

/// Ends the key in the plan of the maps of both pages of a spread of facing pages together.
const SPREAD_KEY: &str = "+spread";

fn margins(page: &str) -> Result<PageMargins, EditError> {
    let ix = scan(page)?;
    let &m = ix.named(page, 0, "MarginPreference").first().ok_or_else(|| EditError::new("xml", "page without margins"))?;
    let g = |k: &str, d: f64| ix.attr(page, m, k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    Ok((g("Top", 0.0), g("Bottom", 0.0), g("Left", 0.0), g("Right", 0.0), g("ColumnCount", 1.0) as usize, g("ColumnGutter", 12.0)))
}

fn page_rect(p: &Page) -> [f64; 4] {
    let (b, m) = (bounds(&p.xml), mat(&p.xml));
    let (x, y) = m.apply((b[0], b[1]));
    [x, y, x + b[2] - b[0], y + b[3] - b[1]]
}

/// What InDesign counts as the reach of a stroke: a hairline (weight 0) still reaches half a point.
fn outset(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> f64 {
    let weight: f64 = stroke_attr(w, ix, t, i, "StrokeWeight").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let applied = stroke_attr(w, ix, t, i, "StrokeColor").is_some_and(|c| c != "Swatch/None");
    if applied && weight <= 0.0 { 0.5 } else { stroke_outset(w, ix, t, i) }
}

/// A page follows its parent when its margins and columns are the ones of the parent page it is based on (measured: after IDML, a page
/// whose margins were set to the parent's own value follows it like a page never touched; one with other values does not).
///
/// What a page takes from the margin arguments also depends on its parent page: a parent page whose margins are not the document's old default
/// ones (`doc_old`) keeps its own, and then the pages that follow it do too (layoutadj-c7: a page and a parent both on 30 while the document's
/// defaults were 12, 18, 24, 36 stay on 30).
fn follows_parent(book: &Book, p: &Page, mine: &PageMargins, doc_old: Option<[f64; 4]>) -> bool {
    let master = get(&p.xml, "AppliedMaster");
    let Some(ms) = book.masters.iter().find(|s| s.id() == master) else { return false };
    let r = page_rect(p);
    // a document without facing pages uses the first page of a parent only (the second one keeps its margins: layoutadj-c4)
    let left = book.pref("FacingPages") == "true" && (r[0] + r[2]) / 2.0 < 0.0;
    let right_hand = book.pref("FacingPages") == "true" && !left;
    let Some(mp) = ms.pages.get(if right_hand { ms.pages.len().saturating_sub(1) } else { 0 }) else { return false };
    let Ok(theirs) = margins(&mp.xml) else { return false };
    let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
    if doc_old.is_some_and(|d| !(same(theirs.0, d[0]) && same(theirs.1, d[1]) && same(theirs.2, d[2]) && same(theirs.3, d[3]))) { return false; }
    same(mine.0, theirs.0) && same(mine.1, theirs.1) && same(mine.2, theirs.2) && same(mine.3, theirs.3) && mine.4 == theirs.4 && same(mine.5, theirs.5)
}

/// Whether the path of an item is an axis aligned box on the spread: one closed straight path of four points that all lie on the box and
/// are its four corners (a fifth point along a side makes it another shape: layoutadj-c4).
fn is_box(path: &OwnPath, m: &Mat) -> bool {
    let [(_, pts, closed)] = path.subpaths.as_slice() else { return false };
    if !*closed || pts.len() != 4 || pts.iter().any(|p| p.anchor != p.left || p.anchor != p.right) { return false; }
    let q: Vec<(f64, f64)> = pts.iter().map(|p| m.apply(p.anchor)).collect();
    let (x0, x1) = (q.iter().map(|p| p.0).fold(f64::INFINITY, f64::min), q.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max));
    let (y0, y1) = (q.iter().map(|p| p.1).fold(f64::INFINITY, f64::min), q.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max));
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let on_box = q.iter().all(|p| near(p.0, x0) || near(p.0, x1) || near(p.1, y0) || near(p.1, y1));
    let corner = |x: f64, y: f64| q.iter().any(|p| near(p.0, x) && near(p.1, y));
    on_box && corner(x0, y0) && corner(x1, y0) && corner(x0, y1) && corner(x1, y1)
}

fn classify(ix: &Index, t: &str, kind: &str, m: &Mat) -> Class {
    if kind == "Group" { return Class::Uniform; }
    let Some(path) = own_path(ix, t, 0) else { return Class::Uniform };
    if kind == "GraphicLine" {
        return match path.subpaths.as_slice() {
            [(_, pts, false)] if pts.len() == 2 && pts.iter().all(|p| p.anchor == p.left && p.anchor == p.right) => Class::Line,
            _ => Class::Uniform,
        };
    }
    if matches!(kind, "Rectangle" | "TextFrame" | "Polygon") && is_box(&path, m) { Class::Stretch } else { Class::Uniform }
}

/// New margins `[top, bottom, left, right]` for a page that has the old ones `old`.
fn with_args(old: &PageMargins, args: &[Option<f64>; 4]) -> [f64; 4] {
    [args[0].unwrap_or(old.0), args[1].unwrap_or(old.1), args[2].unwrap_or(old.2), args[3].unwrap_or(old.3)]
}

fn plan(w: &Work<'_>, book: &Book, width: f64, height: f64, allow_locked: bool, args: &[Option<f64>; 4]) -> Result<Plan, EditError> {
    let facing = book.pref("FacingPages") == "true";
    let doc_old = doc_margins(&book.prefs);
    let (old_w, old_h): (f64, f64) = (book.pref("PageWidth").parse().unwrap_or(0.0), book.pref("PageHeight").parse().unwrap_or(0.0));
    let mut out = Plan { pages: BTreeMap::new(), items: Vec::new(), margins: BTreeMap::new(), doc_margins: doc_old.map(|d| with_args(&(d[0], d[1], d[2], d[3], 1, 12.0), args)).filter(|n| Some(*n) != doc_old) };
    let mut refused = Vec::new();
    for s in book.spreads.iter().chain(book.masters.iter()) {
        let master = s.part.starts_with("MasterSpreads/");
        for (n, p) in s.pages.iter().enumerate() {
            let r = page_rect(p);
            let (pw, ph) = (r[2] - r[0], r[3] - r[1]);
            if (pw - old_w).abs() > 1e-6 || (ph - old_h).abs() > 1e-6 {
                refused.push(format!("{}: page size differs from the document's", p.id));
                continue;
            }
            let mine = margins(&p.xml)?;
            let (top, bottom, left, right, count, gutter) = mine;
            // a parent page takes the new margins when its own are the document's default ones, a page when it follows its parent
            // (layoutadj-c7; a document that does not list its default margins counts every page of a parent of facing pages and the first
            // of any other: layoutadj-c4)
            let takes = if master {
                doc_old.map_or(facing || n == 0, |d| { let same = |a: f64, b: f64| (a - b).abs() < 1e-9; same(top, d[0]) && same(bottom, d[1]) && same(left, d[2]) && same(right, d[3]) })
            } else {
                follows_parent(book, p, &mine, doc_old)
            };
            let new = if takes { with_args(&mine, args) } else { [top, bottom, left, right] };
            if new != [top, bottom, left, right] { out.margins.insert(p.id.clone(), new); }
            let left_hand = facing && (r[0] + r[2]) / 2.0 < 0.0;
            let (lead, trail, new_lead, new_trail) = if left_hand { (right, left, new[3], new[2]) } else { (left, right, new[2], new[3]) };
            out.pages.insert(p.id.clone(), (Axis::new(pw, width, lead, trail, new_lead, new_trail, count, gutter)?, Axis::new(ph, height, top, bottom, new[0], new[1], 1, 0.0)?, [r[0], r[1]]));
        }
        // a spread of one page of a document without facing pages: the page grows around the centre of the spread
        let alone = !facing && s.pages.len() == 1;
        let (dw, dh) = (width - old_w, height - old_h);
        // the pages of a spread of facing pages side by side, left to right; objects across them go through one map (layoutadj-c5)
        let mut sorted: Vec<&Page> = s.pages.iter().collect();
        sorted.sort_by(|a, b| page_rect(a)[0].total_cmp(&page_rect(b)[0]));
        let spread_pages = facing && (1..=2).contains(&sorted.len());
        let chain_key = sorted.first().map(|p| format!("{}{SPREAD_KEY}", p.id)).unwrap_or_default();
        if spread_pages && sorted.len() == 2 {
            let (a, b) = (page_rect(sorted[0]), page_rect(sorted[1]));
            if let (Some((h0, v0, o0)), Some((h1, _, _))) = (out.pages.get(&sorted[0].id), out.pages.get(&sorted[1].id))
                && (b[0] - a[2]).abs() < 1e-6 && (b[1] - a[1]).abs() < 1e-6 {
                let both = (Axis::spread(&[h0.clone(), h1.clone()]), v0.clone(), *o0);
                out.pages.insert(chain_key.clone(), both);
            }
        }
        let left_hand = |p: &Page| { let r = page_rect(p); (r[0] + r[2]) / 2.0 < 0.0 };
        for it in &s.items {
            let ix = scan(&it.xml)?;
            let kind = ix.local(&it.xml, 0).to_owned();
            let id = ix.attr(&it.xml, 0, "Self").unwrap_or_default().to_owned();
            if !matches!(kind.as_str(), "Rectangle" | "TextFrame" | "Oval" | "Polygon" | "GraphicLine" | "Group") {
                refused.push(format!("{id}: {kind}"));
                continue;
            }
            let m = item_transform(&ix, &it.xml, 0);
            let plain = m.0[..4] == [1.0, 0.0, 0.0, 1.0];
            let Some(b) = spread_bounds(&ix, &it.xml, 0) else { refused.push(format!("{id}: no bounds")); continue };
            let Some(vis) = alignment::visible_hair(w, &ix, &it.xml, 0, m, true) else { refused.push(format!("{id}: no bounds")); continue };
            let o = outset(w, &ix, &it.xml, 0);
            let locked = ix.attr(&it.xml, 0, "Locked") == Some("true");
            let any_locked = (0..ix.nodes.len()).any(|k| ix.attr(&it.xml, k, "Locked") == Some("true"));
            // the page the object belongs to: the only page of its spread, else the one it overlaps
            let on: Vec<&Page> = s.pages.iter().filter(|p| {
                let r = page_rect(p);
                vis[2].min(r[2]) - vis[0].max(r[0]) > 0.0 && vis[3].min(r[3]) - vis[1].max(r[1]) > 0.0
            }).collect();
            let single = if alone { Some(&s.pages[0]) } else if on.len() == 1 { Some(on[0]) } else { None };
            // the page the local coordinates count from, the key of its maps and the width of what the object may lie over; a facing
            // spread with two pages gets one chain of both for an object across them (layoutadj-c5)
            let (page, key, tw) = match single {
                Some(p) => (p, p.id.clone(), old_w),
                None if !spread_pages => {
                    refused.push(format!("{id}: {}", if on.is_empty() { "off the pages" } else { "across pages" }));
                    continue;
                }
                None if on.is_empty() => (sorted[0], sorted[0].id.clone(), old_w * sorted.len() as f64),
                None if out.pages.contains_key(&chain_key) => (sorted[0], chain_key.clone(), old_w * 2.0),
                None => {
                    refused.push(format!("{id}: across pages that do not meet"));
                    continue;
                }
            };
            let Some((hx, vy, _)) = out.pages.get(&key) else { continue };
            let r = page_rect(page);
            let local = [b[0] - r[0], b[1] - r[1], b[2] - r[0], b[3] - r[1]];
            let lv = [vis[0] - r[0], vis[1] - r[1], vis[2] - r[0], vis[3] - r[1]];
            let (_pw, ph) = (r[2] - r[0], r[3] - r[1]);
            let part = s.part.clone();
            let box_kind = matches!(kind.as_str(), "Rectangle" | "Oval" | "TextFrame");
            let rigid = |dx: f64, dy: f64| -> Op {
                if plain && box_kind { Op::Shift { page: key.clone(), local, dx, dy } }
                else { Op::Visible { page: key.clone(), dest: [lv[0] + dx, lv[1] + dy, lv[2] + dx, lv[3] + dy] } }
            };
            if (locked || any_locked) && !allow_locked {
                // measured (layoutadj-c3, 40 sizes): the object keeps its size and moves by the whole growth across and half of it down
                if !alone { refused.push(format!("{id}: locked, on a spread of several pages")); continue; }
                if kind == "Group" || !box_kind || !plain { refused.push(format!("{id}: locked {kind}")); continue; }
                out.items.push((part, id, rigid(dw, dh / 2.0)));
                continue;
            }
            // off the page on the side of an axis (the visible bounds): across the page that wins, else down the page
            let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
            if near(lv[2], 0.0) || near(lv[0], tw) || near(lv[3], 0.0) || near(lv[1], ph) {
                refused.push(format!("{id}: touches the edge of the page"));
                continue;
            }
            let off = |lo: f64, hi: f64, size: f64| if hi < 0.0 { Some(0) } else if lo > size { Some(1) } else { None };
            let (ox, oy) = (off(lv[0], lv[2], tw), off(lv[1], lv[3], ph));
            if ox.is_some() || oy.is_some() {
                if !alone && !spread_pages { refused.push(format!("{id}: off the page of a spread with several pages")); continue; }
                // beyond the side of the pages it keeps its place from that side, the growth of the whole spread moving the far side;
                // above or below them it keeps its place to the spine (the centre of a page without facing pages), measured in layoutadj-c5
                let pages_n = if alone { 1.0 } else { sorted.len() as f64 };
                let left_of_spine = if alone { 0.5 } else { sorted.iter().filter(|p| left_hand(p)).count() as f64 };
                let (dx, dy) = match (ox, oy) {
                    (Some(side), _) => (if side == 0 { 0.0 } else { pages_n * dw }, dh / 2.0),
                    (None, Some(side)) => (left_of_spine * dw, if side == 0 { 0.0 } else { dh }),
                    (None, None) => unreachable!(),
                };
                out.items.push((part, id, rigid(dx, dy)));
                continue;
            }
            let class = classify(&ix, &it.xml, &kind, &m);
            if key == chain_key && lv[0] < 0.0 && lv[2] > tw {
                // an object over both ends of a facing spread is not mapped by the pages: its width grows by one page's growth, the
                // left side stays (left to right binding) or goes along by that growth (right to left), whatever its extent, and across
                // it follows the page map (layoutadj-c5, 9 extents, 3 sizes; only boxes were measured)
                if class != Class::Stretch || !plain { refused.push(format!("{id}: over both ends of a spread, not a box")); continue; }
                let left = if book.pref("PageBinding") == "RightToLeft" { dw } else { 0.0 };
                let v = vy.edges(lv[1] + o, lv[3] - o, o);
                out.items.push((part, id, Op::Visible { page: key.clone(), dest: [lv[0] + left, v[0] - o, lv[2] + left + dw, v[1] + o] }));
                continue;
            }
            let uniform = || {
                let s = hx.scale_of(lv[0], lv[2]).min(vy.scale_of(lv[1], lv[3]));
                let (x, y) = (hx.map(lv[0]), vy.map(lv[1]));
                [x, y, x + (lv[2] - lv[0]) * s, y + (lv[3] - lv[1]) * s]
            };
            let sides = || [hx.map(lv[0]), vy.map(lv[1]), hx.map(lv[2]), vy.map(lv[3])];
            let op = match class {
                Class::Stretch if plain => Op::Stretch { page: key.clone(), o, local },
                Class::Stretch => {
                    let (hx_e, vy_e) = (hx.edges(lv[0] + o, lv[2] - o, o), vy.edges(lv[1] + o, lv[3] - o, o));
                    Op::Visible { page: key.clone(), dest: [hx_e[0] - o, vy_e[0] - o, hx_e[1] + o, vy_e[1] + o] }
                }
                Class::Line if plain && ((b[2] - b[0]).abs() < 1e-9 || (b[3] - b[1]).abs() < 1e-9) => Op::AxisLine { page: key.clone(), local, vis: lv },
                Class::Line => Op::Visible { page: key.clone(), dest: sides() },
                Class::Uniform if plain && kind == "Oval" => Op::Uniform { page: key.clone(), o, dest: uniform() },
                Class::Uniform => Op::Visible { page: key.clone(), dest: uniform() },
            };
            out.items.push((part, id, op));
        }
    }
    if !refused.is_empty() {
        refused.truncate(12);
        return Err(EditError::new("unsupported", format!("layout adjustment of these has not been measured: {}", refused.join("; "))));
    }
    Ok(out)
}

/// A guide of a spread (not of a page): where it lies on the spread, after the pages changed. IDML counts its `Location` from the
/// left edge (down from the top edge) of the pages of the spread (layoutadj-c4). A vertical guide goes through the map of the page
/// it is over (beyond the pages: of the nearest one), a horizontal one through the map of the first page.
fn spread_guide(plan: &Plan, origin: &BTreeMap<String, [f64; 2]>, pages: &[Page], vertical: bool, at: f64) -> Option<f64> {
    let old = |p: &Page| plan.pages.get(&p.id).map(|(h, v, o)| (h, v, *o));
    let axis = usize::from(!vertical);
    let first_old = pages.iter().filter_map(|p| old(p).map(|(_, _, o)| o[axis])).fold(f64::INFINITY, f64::min);
    let first_new = pages.iter().filter_map(|p| origin.get(&p.id).map(|o| o[axis])).fold(f64::INFINITY, f64::min);
    if !first_old.is_finite() || !first_new.is_finite() { return None; }
    let at = at + first_old;
    if !vertical {
        let (_, vy, o) = old(pages.first()?)?;
        return Some(origin[&pages[0].id][1] + vy.map(at - o[1]) - first_new);
    }
    let mut best: Option<(&Page, f64)> = None;
    for p in pages {
        let Some((hx, _, o)) = old(p) else { continue };
        let gap = if at < o[0] { o[0] - at } else if at > o[0] + hx.size { at - o[0] - hx.size } else { 0.0 };
        if best.is_none_or(|(_, g)| gap < g) { best = Some((p, gap)); }
    }
    let (p, _) = best?;
    let (hx, _, o) = old(p)?;
    Some(origin[&p.id][0] + hx.map(at - o[0]) - first_new)
}

/// The document's default margins `[top, bottom, left, right]` (`MarginPreference` of `Resources/Preferences.xml`): what the Adjust Layout
/// dialog shows in its margin boxes. Not the margins of a parent page or a page (layoutadj-c7).
fn doc_margins(prefs: &str) -> Option<[f64; 4]> {
    let ix = scan(prefs).ok()?;
    let &m = ix.named(prefs, 0, "MarginPreference").first()?;
    let g = |k: &str| ix.attr(prefs, m, k).and_then(|v| v.parse::<f64>().ok());
    Some([g("Top")?, g("Bottom")?, g("Left")?, g("Right")?])
}

/// What "Auto-adjust margins to page size changes" puts in the margin boxes (layoutadj-c7, read off the dialog's boxes with real input):
/// every margin of the document's defaults grows like its page side, the top and bottom with the height and the left and right (inside
/// and outside) with the width. The product is not rounded (the boxes show three decimals; the margins that are set keep them all).
pub fn auto_margins(doc: [f64; 4], old: [f64; 2], new: [f64; 2]) -> [f64; 4] {
    let (kw, kh) = (new[0] / old[0], new[1] / old[1]);
    [doc[0] * kh, doc[1] * kh, doc[2] * kw, doc[3] * kw]
}

/// Whether the document remembers "Auto-adjust margins" as on (`AdjustLayoutPreference@EnableAutoAdjustMargins` of `designmap.xml`;
/// the dialog opens with the box ticked and the margin boxes grey, and OK leaves the setting on).
fn auto_pref(designmap: &str) -> Option<bool> {
    let ix = scan(designmap).ok()?;
    let &i = ix.named(designmap, 0, "AdjustLayoutPreference").first()?;
    Some(ix.attr(designmap, i, "EnableAutoAdjustMargins") == Some("true"))
}

/// What the dialog shows for a page size: the document's default margins, the margins auto-adjust would use for `width` x `height` (the
/// page size now when not given) and the remembered setting.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutAdjustSettings {
    /// `[top, bottom, left, right]` as stored (left and right are inside and outside on facing pages).
    pub doc_margins: [f64; 4],
    pub auto_margins: [f64; 4],
    pub auto_adjust_margins: bool,
}

impl crate::Editor {
    /// The state of File ▸ Adjust Layout (the same numbers the command `adjustLayout` uses with `auto_adjust_margins`).
    pub fn layout_adjust_settings(&self, width: Option<f64>, height: Option<f64>) -> Result<LayoutAdjustSettings, EditError> {
        let prefs = self.part("Resources/Preferences.xml").ok_or_else(|| EditError::new("notFound", "Resources/Preferences.xml"))?;
        let doc = doc_margins(prefs).ok_or_else(|| EditError::new("unsupported", "the document does not list its default margins"))?;
        let ix = scan(prefs)?;
        let &d = ix.named(prefs, 0, "DocumentPreference").first().ok_or_else(|| EditError::new("xml", "DocumentPreference"))?;
        let size = |k: &str| ix.attr(prefs, d, k).and_then(|v| v.parse::<f64>().ok()).ok_or_else(|| EditError::new("xml", k.to_owned()));
        let old = [size("PageWidth")?, size("PageHeight")?];
        let new = [width.unwrap_or(old[0]), height.unwrap_or(old[1])];
        if new.iter().any(|v| !v.is_finite() || !(1.0..=15552.0).contains(v)) {
            return Err(EditError::new("invalid", "page dimensions must be 1–15552 pt"));
        }
        Ok(LayoutAdjustSettings { doc_margins: doc, auto_margins: auto_margins(doc, old, new), auto_adjust_margins: self.part("designmap.xml").and_then(auto_pref).unwrap_or(false) })
    }
}

// The registry passes the optional layout adjustment wire fields separately.
#[allow(clippy::too_many_arguments)]
pub(crate) fn adjust_layout(w: &mut Work<'_>, width: &Option<f64>, height: &Option<f64>, allow_locked: &Option<bool>, top_margin: &Option<f64>, bottom_margin: &Option<f64>, left_margin: &Option<f64>, right_margin: &Option<f64>, auto_adjust_margins: &Option<bool>, adjust_font_size: &Option<bool>, font_size_min: &Option<f64>, font_size_max: &Option<f64>, fit_metrics: &Option<String>) -> Result<(), EditError> {
    // "Adjust font size": the sizes of the text of the frames the adjustment stretches (fontfit.rs); the host hands over the composition at the old size
    let fit = if *adjust_font_size == Some(true) { Some(fontfit::Metrics::parse(fit_metrics)?) } else { None };
    let limits = (font_size_min.unwrap_or(6.0), font_size_max.unwrap_or(324.0));
    if limits.0 > limits.1 || limits.0 <= 0.0 || !limits.0.is_finite() || !limits.1.is_finite() {
        return Err(EditError::new("invalid", "the font size limits are 0 < minimum <= maximum"));
    }
    let book = Book::read(w)?;
    let old_w: f64 = book.pref("PageWidth").parse().map_err(|_| EditError::new("xml", "page width"))?;
    let old_h: f64 = book.pref("PageHeight").parse().map_err(|_| EditError::new("xml", "page height"))?;
    let (nw, nh) = (width.unwrap_or(old_w), height.unwrap_or(old_h));
    if [nw, nh].iter().any(|v| !v.is_finite() || !(1.0..=15552.0).contains(v)) {
        return Err(EditError::new("invalid", "page dimensions must be 1–15552 pt"));
    }
    let mut args = [*top_margin, *bottom_margin, *left_margin, *right_margin];
    if args.iter().flatten().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(EditError::new("invalid", "margins are numbers of 0 or more"));
    }
    if *auto_adjust_margins == Some(true) {
        // the dialog greys the margin boxes out and fills them in; they go to adjustLayout like margins typed in (layoutadj-c7)
        if args.iter().any(Option::is_some) {
            return Err(EditError::new("invalid", "the margins are worked out when auto_adjust_margins is on: give no margins"));
        }
        let doc = doc_margins(&book.prefs).ok_or_else(|| EditError::new("unsupported", "the document does not list its default margins"))?;
        let scaled = auto_margins(doc, [old_w, old_h], [nw, nh]);
        args = std::array::from_fn(|k| (scaled[k] != doc[k]).then_some(scaled[k]));
    }
    if nw == old_w && nh == old_h && args.iter().all(Option::is_none) {
        drop(book);
        if let Some(on) = auto_adjust_margins { set_auto_pref(w, *on)?; }
        return Ok(());
    }
    let plan = plan(w, &book, nw, nh, allow_locked.unwrap_or(true), &args)?;
    drop(book);
    // the pages first (every page and parent page to the new size, columns recomputed), then margins, objects and guides
    super::run_resizing(w, &Command::DocumentSetup { width: nw, height: nh })?;
    let mut book = Book::read(w)?;
    let mut origin: BTreeMap<String, [f64; 2]> = BTreeMap::new();
    for s in book.spreads.iter().chain(book.masters.iter()) {
        for p in &s.pages {
            let r = page_rect(p);
            origin.insert(p.id.clone(), [r[0], r[1]]);
        }
    }
    // a chain of the pages of a spread starts where its first page does
    for key in plan.pages.keys().filter(|k| k.ends_with(SPREAD_KEY)) {
        if let Some(&o) = origin.get(key.trim_end_matches(SPREAD_KEY)) { origin.insert(key.clone(), o); }
    }
    for s in book.spreads.iter_mut().chain(book.masters.iter_mut()) {
        // the guides and margins of every page (IDML keeps the guides of a spread inside its pages: those that do not fit to the
        // page lie on the spread, counted from its left edge)
        let mut all = Vec::new();
        for p in &s.pages {
            let mut edits = Vec::new();
            if let Some((hx, vy, _)) = plan.pages.get(&p.id) {
                let ix = scan(&p.xml)?;
                for g in ix.named(&p.xml, 0, "Guide") {
                    let Some(at) = ix.attr(&p.xml, g, "Location").and_then(|v| v.parse::<f64>().ok()) else { continue };
                    let vertical = ix.attr(&p.xml, g, "Orientation") == Some("Vertical");
                    let to = if ix.attr(&p.xml, g, "FitToPage") == Some("false") {
                        let Some(to) = spread_guide(&plan, &origin, &s.pages, vertical, at) else { continue };
                        to
                    } else {
                        (if vertical { hx } else { vy }).map(at)
                    };
                    edits.push(attr_edit(&ix, &p.xml, g, "Location", &geom::num(to)));
                }
                if let Some(&[top, bottom, left, right]) = plan.margins.get(&p.id) {
                    let &m = ix.named(&p.xml, 0, "MarginPreference").first().ok_or_else(|| EditError::new("xml", "page without margins"))?;
                    for (key, v) in ["Top", "Bottom", "Left", "Right"].into_iter().zip([top, bottom, left, right]) {
                        edits.push(attr_edit(&ix, &p.xml, m, key, &geom::num(v)));
                    }
                    let get = |k: &str, d: f64| ix.attr(&p.xml, m, k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
                    edits.push(attr_edit(&ix, &p.xml, m, "ColumnsPositions", &crate::ops::guides::column_positions(hx.new_size, left, right, get("ColumnCount", 1.0) as u32, get("ColumnGutter", 12.0))));
                }
            }
            all.push(edits);
        }
        for (p, mut edits) in s.pages.iter_mut().zip(all) {
            edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
            for (r, v) in edits { p.xml.replace_range(r, &v); }
        }
    }
    for s in &book.masters { rewrite(w, &s.part, s.output()?)?; }
    // the document's default margins take the new ones too (what the next auto-adjust starts from)
    if let Some(new) = plan.doc_margins {
        let ix = scan(&book.prefs)?;
        if let Some(&m) = ix.named(&book.prefs, 0, "MarginPreference").first() {
            let mut edits: Vec<_> = ["Top", "Bottom", "Left", "Right"].into_iter().zip(new).map(|(k, v)| attr_edit(&ix, &book.prefs, m, k, &geom::num(v))).collect();
            edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
            for (r, v) in edits { book.prefs.replace_range(r, &v); }
        }
    }
    book.write(w)?;
    if let Some(on) = auto_adjust_margins { set_auto_pref(w, *on)?; }
    for (_, id, op) in &plan.items {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        let m = item_transform(&ix, t, i).0;
        let text_frame = fit.is_some() && ix.local(t, i) == "TextFrame";
        // the box a text frame had and gets: the sizes of its text follow (a frame the adjustment does not stretch like a box is refused)
        let old_box = match op { Op::Stretch { local, .. } | Op::Shift { local, .. } => Some(*local), _ => None };
        if text_frame && old_box.is_none() {
            return Err(EditError::new("unsupported", format!("{id}: a rotated or non-box text frame is not supported when the font size is adjusted")));
        }
        let (page, [x0, y0, x1, y1]) = match op {
            Op::Stretch { page, o, local } => {
                let (hx, vy, _) = &plan.pages[page];
                let [x0, x1] = hx.edges(local[0], local[2], *o);
                let [y0, y1] = vy.edges(local[1], local[3], *o);
                (page, [x0, y0, x1, y1])
            }
            Op::Uniform { page, o, dest } => (page, [dest[0] + o, dest[1] + o, dest[2] - o, dest[3] - o]),
            Op::Shift { page, local, dx, dy } => (page, [local[0] + dx, local[1] + dy, local[2] + dx, local[3] + dy]),
            Op::AxisLine { page, local, vis } => {
                let (hx, vy, _) = &plan.pages[page];
                let along_x = (local[3] - local[1]).abs() < 1e-9;
                // the ends go through the map (a butt cap reaches nothing along the line); across, the visible edge does and the reach stays
                let (x0, x1) = if along_x { (hx.map(vis[0]) + local[0] - vis[0], hx.map(vis[2]) + local[2] - vis[2]) } else { let a = hx.map(vis[0]) + local[0] - vis[0]; (a, a) };
                let (y0, y1) = if along_x { let a = vy.map(vis[1]) + local[1] - vis[1]; (a, a) } else { (vy.map(vis[1]) + local[1] - vis[1], vy.map(vis[3]) + local[3] - vis[3]) };
                (page, [x0, y0, x1, y1])
            }
            Op::Visible { page, dest } => {
                let at = origin[page];
                resize_group::visible_to(w, id, [dest[0] + at[0], dest[1] + at[1], dest[2] + at[0], dest[3] + at[1]])?;
                continue;
            }
        };
        let at = origin[page];
        resize(w, id, Target::Box([x0 + at[0] - m[4], y0 + at[1] - m[5], x1 + at[0] - m[4], y1 + at[1] - m[5]]))?;
        if let (true, Some(metrics), Some(old)) = (text_frame, &fit, old_box) {
            fontfit::apply(w, id, old, [x0, y0, x1, y1], metrics, limits)?;
        }
    }
    lay_out_spreads(w)
}

/// The dialog leaves "Auto-adjust margins" as it was at OK (`designmap.xml`: `AdjustLayoutPreference@EnableAutoAdjustMargins`). A document
/// that has no such element (one made by Folio) keeps none: the setting is only what the dialog opens with.
fn set_auto_pref(w: &mut Work<'_>, on: bool) -> Result<(), EditError> {
    let t = w.text("designmap.xml")?.to_owned();
    let ix = scan(&t)?;
    let Some(&i) = ix.named(&t, 0, "AdjustLayoutPreference").first() else { return Ok(()) };
    let (r, v) = attr_edit(&ix, &t, i, "EnableAutoAdjustMargins", if on { "true" } else { "false" });
    let mut t = t;
    t.replace_range(r, &v);
    rewrite(w, "designmap.xml", t)
}

/// The room a spread asks above and below its pages is the pasteboard's own (`MinimumSpaceAboveAndBelow`, 72 pt by default) or, when an
/// object lies wholly above or below the pages, the distance of its nearest visible edge plus 2 pt, whichever is larger (layoutadj-c6,
/// measured: the farthest object counts, not the sum; its size and sideways place do not matter, nor whether it is hidden or non
/// printing). The spreads lie 36 pt plus the room below one and above the next apart. InDesign does this whenever the objects of a
/// spread change, so after an adjustment the objects moved off the pages decide where the spreads lie: each spread moves down by the
/// extra room asked above it and below the ones before.
pub(super) fn lay_out_spreads(w: &mut Work<'_>) -> Result<(), EditError> {
    let mut book = Book::read(w)?;
    let space = book.pasteboard_space();
    let (mut down_by, mut below_before) = (0.0, space);
    let mut moved = false;
    for (n, s) in book.spreads.iter_mut().enumerate() {
        let top = s.pages.iter().map(|p| page_rect(p)[1]).fold(f64::INFINITY, f64::min);
        let bottom = s.pages.iter().map(|p| page_rect(p)[3]).fold(f64::NEG_INFINITY, f64::max);
        let (mut up, mut down) = (space, space);
        for it in &s.items {
            let ix = scan(&it.xml)?;
            let m = item_transform(&ix, &it.xml, 0);
            let Some(vis) = alignment::visible_hair(w, &ix, &it.xml, 0, m, true) else { continue };
            let (above, below) = (top - vis[3], vis[1] - bottom);
            if above.max(below) + 2.0 <= space { continue; }
            if m.0[..4] != [1.0, 0.0, 0.0, 1.0] {
                // measured once (a box turned 10 degrees: InDesign took an edge 0.96 pt farther than its outline's box)
                return Err(EditError::new("unsupported", "a turned object far above or below the pages decides where the spreads lie: not measured"));
            }
            up = up.max(above + 2.0);
            down = down.max(below + 2.0);
        }
        if n > 0 { down_by += (below_before - space) + (up - space); }
        below_before = down;
        if down_by != 0.0 {
            let mut m = mat(&s.xml);
            m.0[5] += down_by;
            set(&mut s.xml, "ItemTransform", &m.text());
            moved = true;
        }
    }
    if moved { book.write(w)?; }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: [f64; 2], b: [f64; 2]) { assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6, "{a:?} vs {b:?}"); }

    /// Measured with InDesign 2025 (`measurement/layoutadj-c2.json`, A-x): 300 wide with margins 30, to 600; hairline frames.
    #[test]
    fn one_column_page_matches_measurements() {
        let a = Axis::new(300.0, 600.0, 30.0, 30.0, 30.0, 30.0, 1, 12.0).unwrap();
        close(a.edges(50.0, 130.0, 0.5), [74.375, 255.625]);
        close(a.edges(30.0, 270.0, 0.0), [30.0, 570.0]);
        close(a.edges(0.0, 300.0, 0.0), [0.0, 600.0]);
        close(a.edges(31.0, 130.0, 0.5), [31.625, 255.625]);
        close(a.edges(0.0, 30.0, 0.5), [0.0, 30.625]);
        close(a.edges(380.0, 420.0, 0.5), [680.0, 720.0]);
        // a hairline frame keeps 1 pt, a frame without stroke may be narrower
        close(a.edges(0.0, 0.25, 0.5), [0.0, 1.0]);
        close(a.edges(31.0, 31.6, 0.0), [32.25, 33.6]);
        // guides go through the same map
        assert!((a.map(50.0) - 75.0).abs() < 1e-9);
    }

    /// D2: two columns of 110 with a gutter of 20 (page 300, margins 30) to 600: columns 260, gutter and margins rigid.
    #[test]
    fn columns_scale_and_gutters_move() {
        let a = Axis::new(300.0, 600.0, 30.0, 30.0, 30.0, 30.0, 2, 20.0).unwrap();
        assert!((a.map(140.5) - 290.5).abs() < 1e-9);
        assert!((a.map(150.0) - 300.0).abs() < 1e-9);
        assert!((a.map(160.5) - 311.181818181818).abs() < 1e-6);
        assert!((a.map(270.5) - 570.5).abs() < 1e-9);
    }

    /// The same size changes nothing.
    #[test]
    fn same_size_keeps_edges() {
        let a = Axis::new(300.0, 300.0, 36.0, 36.0, 36.0, 36.0, 1, 12.0).unwrap();
        close(a.edges(50.0, 100.0, 0.5), [50.0, 100.0]);
    }

    /// layoutadj-c4 (f-three-idml): margins 30 all round to top 50, bottom 40, left 60, right 20 on a 300 x 400 page; hairline frames.
    #[test]
    fn changed_margins_scale_their_zones() {
        let h = Axis::new(300.0, 300.0, 30.0, 30.0, 60.0, 20.0, 1, 12.0).unwrap();
        close(h.edges(35.5 + 0.5, 264.5 - 0.5, 0.5), [65.541666666667, 274.458333333333]);
        close(h.edges(0.0, 300.0, 0.0), [0.0, 300.0]);
        let v = Axis::new(400.0, 400.0, 30.0, 30.0, 50.0, 40.0, 1, 0.0).unwrap();
        close(v.edges(100.0, 200.0, 0.5), [113.867647058824, 204.955882352941]);
    }

    /// layoutadj-c4 (c1-600x500): a shape keeps its proportions by the smaller of the scales the map gives on its own extent.
    #[test]
    fn scale_of_an_extent_counts_the_margin_zone() {
        let h = Axis::new(300.0, 600.0, 30.0, 30.0, 30.0, 30.0, 1, 12.0).unwrap();
        let v = Axis::new(400.0, 500.0, 30.0, 30.0, 30.0, 30.0, 1, 0.0).unwrap();
        // an oval over the top margin and 10 pt of the area: 9.5..50.5 goes to 9.5..56.529 (ratio 1.1471)
        assert!((v.scale_of(9.5, 50.5) - 1.14707).abs() < 1e-4);
        // the whole page less 9.5 on each side: the vertical scale (481 / 381) is the smaller one
        assert!((v.scale_of(9.5, 390.5) - 481.0 / 381.0).abs() < 1e-9);
        assert!(h.scale_of(9.5, 290.5) > v.scale_of(9.5, 390.5));
    }
}
