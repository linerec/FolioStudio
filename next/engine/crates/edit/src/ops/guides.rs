//! Ruler guides (next/oracle/scenarios/basic-guides.json). A guide is a child of a Page: a page
//! guide has FitToPage="true" and PageIndex -1 on a page left of the spine, 1 on the right; a
//! spread guide (Ctrl-drag) sits in the spread's first page with FitToPage="false" and
//! PageIndex="0"; a vertical spread guide sits in the page it stands over (smartguides-c3: real
//! drags on a document spread and on a parent, and the DOM; tests/smartguides_c3.rs).
//! Location is measured from the ruler origin, the inverse of the reader's
//! `place_guides`. New guides are Cyan, on the given layer or the active one.
use super::*;

/// A position on a page as the UI gives it: `at` points from the page's top (horizontal guide)
/// or left (vertical guide) edge.
pub(super) struct At<'a> {
    pub page: &'a str,
    pub horizontal: bool,
    pub at: f64,
}

struct Spread {
    part: String,
    t: String,
    ix: Index,
    node: usize,
    /// Each page's node and top-left corner in spread coordinates.
    pages: Vec<(usize, (f64, f64))>,
}

fn spread_of(w: &mut Work<'_>, page: &str) -> Result<(Spread, usize), EditError> {
    let (part, ix, i) = w.find(page)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "Page" {
        return Err(EditError::new("invalid", format!("{page} is not a page")));
    }
    let node = ix.ancestors(i).find(|&a| matches!(ix.local(&t, a), "Spread" | "MasterSpread") && ix.attr(&t, a, "Self").is_some())
        .ok_or_else(|| EditError::new("xml", "page outside a spread"))?;
    let mut pages = Vec::new();
    for &c in ix.children(node).iter().filter(|&&c| ix.local(&t, c) == "Page") {
        pages.push((c, corner(&ix, &t, c)?));
    }
    Ok((Spread { part, t, ix, node, pages }, i))
}

/// The top-left corner of a page's transformed bounds.
fn corner(ix: &Index, t: &str, page: usize) -> Result<(f64, f64), EditError> {
    let b: Vec<f64> = ix.attr(t, page, "GeometricBounds").unwrap_or_default().split_whitespace().filter_map(|v| v.parse().ok()).collect();
    let [top, left, bottom, right] = b[..] else { return Err(EditError::new("xml", "invalid page bounds")) };
    let m = item_transform(ix, t, page);
    let pts = [(left, top), (right, top), (left, bottom), (right, bottom)].map(|p| m.apply(p));
    Ok(pts.iter().fold((f64::INFINITY, f64::INFINITY), |a, p| (a.0.min(p.0), a.1.min(p.1))))
}

fn ruler_origin(w: &Work<'_>) -> Result<String, EditError> {
    let t = w.text("Resources/Preferences.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    Ok((0..ix.nodes.len()).find(|&i| ix.local(t, i) == "ViewPreference")
        .and_then(|i| ix.attr(t, i, "RulerOrigin")).filter(|v| !v.is_empty()).unwrap_or("SpreadOrigin").to_owned())
}

impl Spread {
    fn corner_of(&self, page: usize) -> (f64, f64) {
        self.pages.iter().find(|p| p.0 == page).map_or((0.0, 0.0), |p| p.1)
    }

    /// The point a guide's Location is measured from; `own` is its page (None: a spread guide).
    fn origin(&self, ruler: &str, own: Option<usize>, horizontal: bool) -> (f64, f64) {
        let spread = self.pages.iter().fold((f64::INFINITY, f64::INFINITY), |a, p| (a.0.min(p.1.0), a.1.min(p.1.1)));
        match ruler {
            "PageOrigin" => own.map_or(spread, |p| self.corner_of(p)),
            "SpineOrigin" if !horizontal && self.pages.len() == 2 => (self.pages[0].1.0.max(self.pages[1].1.0), spread.1),
            _ => spread,
        }
    }

    /// `at` on `page` as the Location of a guide owned by `own`.
    fn location(&self, ruler: &str, page: usize, own: Option<usize>, horizontal: bool, at: f64) -> f64 {
        let (c, o) = (self.corner_of(page), self.origin(ruler, own, horizontal));
        if horizontal { c.1 + at - o.1 } else { c.0 + at - o.0 }
    }

    /// The page a new spread guide is written into. InDesign 2025 (smartguides-c3, rules/smartguides-c3.ko.md): a horizontal one goes into the
    /// spread's first page wherever it was dropped; a vertical one into the page it stands over (Ctrl-drag over the right page, a double click on
    /// the ruler, `spread.guides.add`), on a document spread and on a parent alike. A vertical one beside the pages is kept in the first page
    /// (InDesign holds it on the spread and leaves it out of the IDML it exports; it reads ours). With the page-origin ruler InDesign measures
    /// the Location of such a guide from the page it is written into; the reader's `place_guides` measures every spread guide from the spread,
    /// so there the first page stays the host (the same place either way).
    fn spread_guide_host(&self, ruler: &str, page: usize, horizontal: bool, at: f64) -> usize {
        let first = self.pages[0].0;
        if horizontal || ruler == "PageOrigin" {
            return first;
        }
        let x = self.corner_of(page).0 + at;
        let over = |p: &(usize, (f64, f64))| x >= p.1.0 && x <= p.1.0 + page_size(&self.ix, &self.t, p.0).0;
        // the page the position was given on when it is over that one (a spine belongs to it), else the page under it
        match self.pages.iter().find(|p| p.0 == page).filter(|p| over(p)).or_else(|| self.pages.iter().find(|p| over(p))) {
            Some(p) => p.0,
            None => first,
        }
    }

    /// The page left (-1) or right (1) of the spine, as PageIndex records it. In a parent (master spread) InDesign
    /// numbers the pages instead: 1 for the first page of the spread, 2 for the second (parent-guides, measured).
    fn side(&self, page: usize) -> i32 {
        let k = self.pages.iter().position(|p| p.0 == page).unwrap_or(0);
        if self.ix.local(&self.t, self.node) == "MasterSpread" {
            return k as i32 + 1;
        }
        let binding = self.ix.attr(&self.t, self.node, "BindingLocation").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        if k < binding { -1 } else { 1 }
    }
}

/// A page's width and height (its GeometricBounds).
fn page_size(ix: &Index, t: &str, page: usize) -> (f64, f64) {
    let b: Vec<f64> = ix.attr(t, page, "GeometricBounds").unwrap_or_default().split_whitespace().filter_map(|v| v.parse().ok()).collect();
    match b[..] {
        [top, left, bottom, right] => (right - left, bottom - top),
        _ => (0.0, 0.0),
    }
}

/// A MarginPreference value of a page.
fn margin(ix: &Index, t: &str, page: usize, key: &str) -> Option<f64> {
    let m = *ix.named(t, page, "MarginPreference").first()?;
    ix.attr(t, m, key).and_then(|v| v.parse().ok())
}

/// The edges of `count` bands between `a` and `b` with `gutter` between them, as Create Guides
/// places them: both ends, and both sides of every gutter (one line when the gutter is 0).
fn bands(a: f64, b: f64, count: u32, gutter: f64) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let n = count as f64;
    let size = (b - a - gutter * (n - 1.0)) / n;
    let mut out = vec![a];
    for k in 1..count {
        let edge = a + k as f64 * size + (k - 1) as f64 * gutter;
        out.push(edge);
        if gutter > 0.0 {
            out.push(edge + gutter);
        }
    }
    out.push(b);
    out
}

/// Layout ▸ Create Guides (InDesign 2025, oracle basic-layout guides-create-*): on every page of
/// the spread, its horizontal guides top to bottom, then its vertical ones left to right, in the
/// margins or on the page. A vertical guide goes in the first page element whose width holds
/// it (the right page's left edge in the left page) and keeps its own page's PageIndex. Each new
/// guide goes in front of the page element's guides, so the file lists them newest first.
/// `remove_existing` first removes every ruler guide on the spread's pages.
// Keep the independently supplied guide command fields in their existing order.
#[allow(clippy::too_many_arguments)]
pub(super) fn create_guides(w: &mut Work<'_>, page: &str, rows: u32, columns: u32, row_gutter: f64, column_gutter: f64, fit_margins: bool, remove_existing: bool) -> Result<(), EditError> {
    if !(row_gutter.is_finite() && column_gutter.is_finite()) || row_gutter < 0.0 || column_gutter < 0.0 {
        return Err(EditError::new("invalid", "gutters are numbers of 0 or more"));
    }
    if rows > 40 || columns > 40 {
        return Err(EditError::new("invalid", "at most 40 rows and 40 columns"));
    }
    if remove_existing {
        let (s, _) = spread_of(w, page)?;
        let edits: Vec<_> = s.pages.iter().flat_map(|&(p, _)| s.ix.children(p).iter().copied().filter(|&c| s.ix.local(&s.t, c) == "Guide").collect::<Vec<_>>())
            .map(|g| (element_span(&s.ix, &s.t, g), String::new())).collect();
        if !edits.is_empty() {
            w.splice(&s.part, edits)?;
        }
    }
    let layer = active_layer(w)?;
    let ruler = ruler_origin(w)?;
    let facing = {
        let t = w.text("Resources/Preferences.xml")?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "DocumentPreference").and_then(|i| ix.attr(t, i, "FacingPages")) == Some("true")
    };
    let (s, _) = spread_of(w, page)?;
    let sizes: Vec<(f64, f64)> = s.pages.iter().map(|&(p, _)| page_size(&s.ix, &s.t, p)).collect();
    // (host page node, guide XML) in the order the guides are made
    let mut made: Vec<(usize, String)> = Vec::new();
    for (k, &(p, corner)) in s.pages.iter().enumerate() {
        let (width, height) = sizes[k];
        let m = |key| margin(&s.ix, &s.t, p, key).unwrap_or(0.0);
        // Left/Right are inside/outside on facing pages: mirrored on a left-hand page
        let (left, right) = if facing && s.side(p) < 0 { (m("Right"), m("Left")) } else { (m("Left"), m("Right")) };
        let (x0, x1, y0, y1) = if fit_margins { (left, width - right, m("Top"), height - m("Bottom")) } else { (0.0, width, 0.0, height) };
        let lines = bands(y0, y1, rows, row_gutter).into_iter().map(|at| (true, at))
            .chain(bands(x0, x1, columns, column_gutter).into_iter().map(|at| (false, at)));
        for (horizontal, at) in lines {
            let location = s.location(&ruler, p, Some(p), horizontal, at);
            let host = if horizontal {
                p
            } else {
                let x = corner.0 + at;
                s.pages.iter().zip(&sizes).find(|((_, c), sz)| x >= c.0 - 1e-6 && x <= c.0 + sz.0 + 1e-6).map_or(p, |((q, _), _)| *q)
            };
            let id = w.new_id();
            made.push((host, format!(
                "<Guide Self=\"{id}\" OverriddenPageItemProps=\"\" Orientation=\"{}\" Location=\"{}\" FitToPage=\"true\" ViewThreshold=\"5\" Locked=\"false\" ItemLayer=\"{}\" PageIndex=\"{}\" GuideType=\"Ruler\" GuideZone=\"1\">",
                if horizontal { "Horizontal" } else { "Vertical" }, geom::num(location), escape(&layer), s.side(p))));
        }
    }
    let mut edits = Vec::new();
    for &(p, _) in &s.pages {
        let mine: Vec<&String> = made.iter().filter(|(h, _)| *h == p).map(|(_, x)| x).rev().collect();
        if mine.is_empty() {
            continue;
        }
        let n = &s.ix.nodes[p];
        let page_ind = indent_before(&s.t, n.start).to_owned();
        let ind = format!("{page_ind}\t");
        let text: String = mine.iter().map(|open| format!(
            "{ind}{open}\n{ind}\t<Properties>\n{ind}\t\t<GuideColor type=\"enumeration\">Cyan</GuideColor>\n{ind}\t</Properties>\n{ind}</Guide>\n")).collect();
        let first_guide = s.ix.children(p).iter().copied().find(|&c| s.ix.local(&s.t, c) == "Guide");
        edits.push(match first_guide {
            Some(g) => {
                let at = s.ix.nodes[g].start;
                let line = s.t[..at].rfind('\n').map_or(at, |k| k + 1);
                (line..line, text)
            }
            None if n.empty => {
                let open = s.t[n.start..n.end].trim_end_matches("/>").trim_end();
                (n.start..n.end, format!("{open}>\n{text}{page_ind}</Page>"))
            }
            None => {
                let line = s.t[..n.close_start].rfind('\n').map_or(n.close_start, |k| k + 1);
                (line..line, text)
            }
        });
    }
    w.splice(&s.part, edits)
}

/// Layout ▸ Margins and Columns on pages (oracle basic-layout margins-*): the given
/// MarginPreference values (IDML Left and Right are inside and outside on facing pages), and
/// ColumnsPositions recomputed for the live area; the page stops following its parent's grid
/// (UseMasterGrid false). Objects stay (Layout Adjustment off,
/// InDesign's default).
pub(super) fn set_margins(w: &mut Work<'_>, pages: &[String], values: [Option<f64>; 4], columns: Option<u32>, gutter: Option<f64>) -> Result<(), EditError> {
    if values.iter().flatten().chain(gutter.iter()).any(|v| !v.is_finite() || *v < 0.0) {
        return Err(EditError::new("invalid", "margins and gutter are numbers of 0 or more"));
    }
    if columns.is_some_and(|c| c == 0 || c > 216) {
        return Err(EditError::new("invalid", "1 to 216 columns"));
    }
    for page in pages {
        let (part, ix, i) = w.find(page)?;
        let t = w.text(&part)?.to_owned();
        if ix.local(&t, i) != "Page" {
            return Err(EditError::new("invalid", format!("{page} is not a page")));
        }
        let m = *ix.named(&t, i, "MarginPreference").first().ok_or_else(|| EditError::new("unsupported", format!("{page} has no MarginPreference")))?;
        let get = |k: &str| ix.attr(&t, m, k).and_then(|v| v.parse::<f64>().ok());
        let mut edits = Vec::new();
        for (key, v) in ["Top", "Bottom", "Left", "Right"].into_iter().zip(values) {
            if let Some(v) = v {
                edits.push(attr_edit(&ix, &t, m, key, &geom::num(v)));
            }
        }
        if let Some(c) = columns {
            edits.push(attr_edit(&ix, &t, m, "ColumnCount", &c.to_string()));
        }
        if let Some(g) = gutter {
            edits.push(attr_edit(&ix, &t, m, "ColumnGutter", &geom::num(g)));
        }
        let (width, _) = page_size(&ix, &t, i);
        let left = values[2].or_else(|| get("Left")).unwrap_or(0.0);
        let right = values[3].or_else(|| get("Right")).unwrap_or(0.0);
        let count = columns.or_else(|| get("ColumnCount").map(|c| c as u32)).unwrap_or(1).max(1);
        let gap = gutter.or_else(|| get("ColumnGutter")).unwrap_or(12.0);
        edits.push(attr_edit(&ix, &t, m, "ColumnsPositions", &column_positions(width, left, right, count, gap)));
        // the page no longer follows its parent's margins and columns (margins-page)
        edits.push(attr_edit(&ix, &t, i, "UseMasterGrid", "false"));
        w.splice(&part, edits)?;
    }
    Ok(())
}

pub(super) fn column_positions(width: f64, left: f64, right: f64, count: u32, gap: f64) -> String {
    let count = count.max(1);
    let size = (width - left - right - gap * (count as f64 - 1.0)) / count as f64;
    (0..count).flat_map(|k| { let x = k as f64 * (size + gap); [x, x + size] }).map(geom::num).collect::<Vec<_>>().join(" ")
}

fn active_layer(w: &Work<'_>) -> Result<String, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Document").and_then(|i| ix.attr(t, i, "ActiveLayer"))
        .map(str::to_owned).ok_or_else(|| EditError::new("invalid", "no active layer"))
}

pub(super) fn new_guide(w: &mut Work<'_>, at: At<'_>, spread: bool, layer: Option<&str>) -> Result<(), EditError> {
    if !at.at.is_finite() {
        return Err(EditError::new("invalid", "guide position is not a number"));
    }
    let layer = match layer { Some(l) => l.to_owned(), None => active_layer(w)? };
    let ruler = ruler_origin(w)?;
    let (s, page) = spread_of(w, at.page)?;
    let (host, own, index) = if spread { (s.spread_guide_host(&ruler, page, at.horizontal, at.at), None, 0) } else { (page, Some(page), s.side(page)) };
    let location = s.location(&ruler, page, own, at.horizontal, at.at);
    let (threshold, color) = guide_options::defaults(w)?;
    let id = w.new_id();
    let (t, n) = (&s.t, &s.ix.nodes[host]);
    let page_ind = indent_before(t, n.start).to_owned();
    let ind = format!("{page_ind}\t");
    let el = format!(
        "{ind}<Guide Self=\"{id}\" OverriddenPageItemProps=\"\" Orientation=\"{}\" Location=\"{}\" FitToPage=\"{}\" ViewThreshold=\"{threshold}\" Locked=\"false\" ItemLayer=\"{}\" PageIndex=\"{index}\" GuideType=\"Ruler\" GuideZone=\"1\">\n{ind}\t<Properties>\n{ind}\t\t{color}\n{ind}\t</Properties>\n{ind}</Guide>\n",
        if at.horizontal { "Horizontal" } else { "Vertical" }, geom::num(location), !spread, escape(&layer));
    let edit = if n.empty {
        let open = t[n.start..n.end].trim_end_matches("/>").trim_end();
        (n.start..n.end, format!("{open}>\n{el}{page_ind}</Page>"))
    } else if let Some(g) = s.ix.children(host).iter().copied().find(|&c| s.ix.local(t, c) == "Guide") {
        // the file lists a page's guides newest first (two guides on one page, measured on a page and on a parent): in front of the first
        let line = t[..s.ix.nodes[g].start].rfind('\n').map_or(s.ix.nodes[g].start, |k| k + 1);
        (line..line, el)
    } else {
        // the last child of the page, right before its closing tag's indentation
        let line = t[..n.close_start].rfind('\n').map_or(n.close_start, |k| k + 1);
        (line..line, el)
    };
    w.splice(&s.part, vec![edit])
}

/// The guide element, checked, and its spread.
pub(super) fn guide(w: &mut Work<'_>, id: &str) -> Result<(String, Index, usize, String), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "Guide" {
        return Err(EditError::new("invalid", format!("{id} is not a guide")));
    }
    Ok((part, ix, i, t))
}

pub(super) fn move_guide(w: &mut Work<'_>, id: &str, page: &str, at: f64) -> Result<(), EditError> {
    if !at.is_finite() {
        return Err(EditError::new("invalid", "guide position is not a number"));
    }
    let (_, ix, i, t) = guide(w, id)?;
    if ix.attr(&t, i, "Locked") == Some("true") {
        return Err(EditError::new("locked", "the guide is locked"));
    }
    let horizontal = ix.attr(&t, i, "Orientation") != Some("Vertical");
    let spread_guide = match ix.attr(&t, i, "PageIndex").and_then(|v| v.parse::<f64>().ok()) {
        Some(v) => v == 0.0,
        None => ix.attr(&t, i, "FitToPage") == Some("false"),
    };
    let ruler = ruler_origin(w)?;
    let (s, page) = spread_of(w, page)?;
    let parent = ix.nodes[i].parent;
    if !s.pages.iter().any(|p| Some(p.0) == parent) {
        return Err(EditError::new("invalid", "the guide is not on that spread"));
    }
    let own = if spread_guide { None } else { parent };
    let location = s.location(&ruler, page, own, horizontal, at);
    set_attrs(w, id, &[("Location", geom::num(location))])
}

pub(super) fn delete_guide(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i, t) = guide(w, id)?;
    w.splice(&part, vec![(element_span(&ix, &t, i), String::new())])
}
