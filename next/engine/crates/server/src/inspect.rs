//! Read-only inspection for the editor's canvas and panels.
//!
//! `{"cmd":"documentInfo"}`: units, ruler origin, facing pages, layers, swatches, masters,
//! pages and styles. `{"cmd":"pageItems","page":N}`: one document page's size, margins,
//! columns, guides, items and master items with their geometry in page coordinates (points
//! from the page's top-left, y down: the convention of preflight `bounds` and render
//! regions).
//!
//! Draw order follows `folio_render::render_page`: the applied master's items, then the
//! spread's, each list stably sorted by layer; a group's members and a frame's pasted items
//! the same way inside their parent. `z` numbers that order over the whole spread: the
//! master items of each of the spread's pages (in page order), then the spread's items,
//! parents before their members.
//!
//! Nothing here changes the document or lays text out again; facts every request needs are
//! gathered once per document ([`Index`]). Replies are written as JSON text straight from
//! typed records (a page of detailed vector art lists thousands of items).

use folio_color::{BlendSpace, Colors};
use folio_model::layout::DocumentLayout;
use folio_model::{Affine, ColorModel, ColorSpace, Document, Graphic, ItemKind, LineCap, LineJoin, Page, PageItem, Paint, Point, Rect, Spread, StrokeAlignment, SubPath};
use serde::Serialize;
use crate::wire;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Facts shared by every request of one document.
pub struct Index {
    /// The `documentInfo` reply body, as JSON text.
    info: String,
    /// Stories with text that did not fit their frames.
    overset: HashSet<String>,
    /// Stories without a character (Object ▸ Content ▸ Graphic / Unassigned of their frame).
    empty: HashSet<String>,
    /// Vertical stories (`Story::vertical`, Type ▸ Type Orientation).
    vertical: HashSet<String>,
    /// Page index → (spread index, position in the spread): the document's spreads, then the master spreads
    /// (`MASTER_PAGE_BASE` and up) counted on from them.
    pages: HashMap<usize, (usize, usize)>,
    layers: HashMap<String, LayerFacts>,
    facing: bool,
    anchors: std::collections::BTreeMap<String, Vec<PageItem>>,
    anchored_ids: HashSet<String>,
}

#[derive(Clone, Copy)]
struct LayerFacts {
    order: usize,
    visible: bool,
    locked: bool,
    lock_guides: bool,
}

/// The reply header of an inspect command as JSON object text, without `id`, `ok` and
/// `bodyBytes`; `ms` is the time taken, writing the text included.
pub fn command(req: &Value, doc: &Document, layout: &DocumentLayout, colors: &Colors, index: &std::sync::OnceLock<Index>) -> Result<String, String> {
    let started = Instant::now();
    let index = index.get_or_init(|| Index::new(doc, layout, colors));
    let mut reply = match req["cmd"].as_str().unwrap_or("") {
        "documentInfo" => format!(r#"{{"type":"documentInfo","info":{}"#, index.info),
        "pageItems" => {
            let page = wire::required_index(req, "page", "pageItems needs a page number")?;
            let items = page_items(doc, colors, index, page)?;
            let mut text = br#"{"type":"pageItems","items":"#.to_vec();
            serde_json::to_writer(&mut text, &items).map_err(|e| e.to_string())?;
            String::from_utf8(text).map_err(|e| e.to_string())?
        }
        other => return Err(format!("unknown cmd {other}")),
    };
    reply.push_str(&format!(r#","ms":{}}}"#, round(started.elapsed().as_secs_f64() * 1000.0, 3)));
    Ok(reply)
}

impl Index {
    pub fn new(doc: &Document, layout: &DocumentLayout, colors: &Colors) -> Index {
        let facing = if doc.print.facing_pages_stored { doc.print.facing_pages } else { doc.spreads.iter().any(|s| s.pages.len() >= 2) };
        let mut pages = HashMap::new();
        // the document's pages first: a master page whose number is taken (only in a document built by hand) stays unlisted
        for (si, s) in doc.spreads.iter().chain(&doc.master_spreads).enumerate() {
            for (pi, p) in s.pages.iter().enumerate() {
                pages.entry(p.index).or_insert((si, pi));
            }
        }
        let layers = doc
            .layers
            .iter()
            .map(|l| (l.id.clone(), LayerFacts { order: l.order, visible: l.visible, locked: l.locked, lock_guides: l.lock_guides }))
            .collect();
        let info = document_info(doc, colors, facing).to_string();
        let empty = doc.stories.iter().filter(|(_, s)| s.paragraphs.len() <= 1 && s.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty() && r.inline.is_empty()))).map(|(id, _)| id.clone()).collect();
        let anchors = folio_model::layout::anchored_items(doc, layout);
        let anchored_ids = anchors.values().flatten().map(|i| i.id.clone()).collect();
        let vertical = doc.stories.iter().filter(|(_, s)| s.vertical).map(|(id, _)| id.clone()).collect();
        Index { info, overset: layout.overset.iter().cloned().collect(), empty, vertical, pages, layers, facing, anchors, anchored_ids }
    }
}

// ---------------------------------------------------------------------------------------
// Numbers

fn round(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    // + 0.0 turns -0 into 0
    (v * k).round() / k + 0.0
}

fn r3(v: f64) -> f64 {
    round(v, 3)
}

fn rect4(r: &Rect) -> [f64; 4] {
    [r3(r.left), r3(r.top), r3(r.right), r3(r.bottom)]
}

fn affine6(t: &Affine) -> [f64; 6] {
    [round(t.a, 6), round(t.b, 6), round(t.c, 6), round(t.d, 6), r3(t.tx), r3(t.ty)]
}

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(&b)),
        (a, b) => a.or(b),
    }
}

// ---------------------------------------------------------------------------------------
// Geometry

/// A cubic segment through `t` (straight segments have their handles on their ends).
fn segment(t: &Affine, p: &folio_model::PathPoint, q: &folio_model::PathPoint) -> folio_model::geom::Seg {
    folio_model::geom::Seg { a: t.apply(p.anchor), b: t.apply(p.right), c: t.apply(q.left), d: t.apply(q.anchor) }
}

/// The subpath's segments through `t`, the closing one included for a closed subpath.
fn segments(sp: &SubPath, t: &Affine) -> impl Iterator<Item = folio_model::geom::Seg> {
    let n = sp.points.len();
    let count = if n < 2 { 0 } else if sp.open { n - 1 } else { n };
    (0..count).map(move |i| segment(t, &sp.points[i], &sp.points[(i + 1) % n]))
}

/// Parameters in (0, 1) where one coordinate of a cubic has an extreme.
fn extremes(p0: f64, p1: f64, p2: f64, p3: f64) -> impl Iterator<Item = f64> {
    // derivative / 3 = a t² + b t + c
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let mut out = [f64::NAN; 2];
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            out[0] = -c / b;
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let s = disc.sqrt();
            out = [(-b + s) / (2.0 * a), (-b - s) / (2.0 * a)];
        }
    }
    out.into_iter().filter(|t| *t > 0.0 && *t < 1.0)
}

fn include(r: &mut Option<Rect>, p: Point) {
    let pr = Rect { left: p.x, top: p.y, right: p.x, bottom: p.y };
    *r = Some(r.map_or(pr, |r| r.union(&pr)));
}

/// Tight bounds of paths through `t` (curves at their extremes, not their handles).
fn paths_bounds(paths: &[SubPath], t: &Affine) -> Option<Rect> {
    let mut anchors = None;
    for p in paths.iter().flat_map(|sp| &sp.points) {
        include(&mut anchors, t.apply(p.anchor));
    }
    let a = anchors?;
    // A curve stays inside the hull of its points: only a segment with a handle outside the
    // anchors' box can reach beyond it.
    let inside = |p: Point| p.x >= a.left && p.x <= a.right && p.y >= a.top && p.y <= a.bottom;
    let mut r = anchors;
    for sp in paths {
        for s in segments(sp, t) {
            if s.is_line() || (inside(s.b) && inside(s.c)) {
                continue;
            }
            for u in extremes(s.a.x, s.b.x, s.c.x, s.d.x).chain(extremes(s.a.y, s.b.y, s.c.y, s.d.y)) {
                include(&mut r, s.eval(u));
            }
        }
    }
    r
}

/// Geometric bounds of an item through `t` (its inner → target): its paths, or for a group
/// its members'. Strokes are not included.
fn geometry_bounds(item: &PageItem, t: &Affine) -> Option<Rect> {
    match &item.kind {
        ItemKind::Group { children } => children.iter().fold(None, |acc, c| union(acc, geometry_bounds(c, &t.then_inner(&c.transform)))),
        _ => paths_bounds(&item.paths, t),
    }
}

/// Subpaths with more anchors than this are outlined by their anchors alone.
const FLATTEN_MAX_ANCHORS: usize = 16;
/// Most points listed for one subpath; detailed art keeps evenly spaced anchors (enough to
/// hit-test it; the bounds stay exact).
const OUTLINE_MAX_POINTS: usize = 64;

type Outline = Vec<Vec<[f64; 2]>>;

/// Most path points listed for an item: more is detailed art, which is not edited point by point.
const POINTS_MAX: usize = 300;

#[derive(Serialize)]
struct PathInfo {
    open: bool,
    /// `[anchor x, y, left x, y, right x, y]`.
    points: Vec<[f64; 6]>,
}

/// The outline through `t`: each subpath's anchors, the curved segments of a simple subpath
/// flattened into a few points, at most [`OUTLINE_MAX_POINTS`] per subpath; `None` for a
/// plain rectangle (its bounds say it all).
fn outline(paths: &[SubPath], t: &Affine, bounds: Option<&Rect>) -> Option<Outline> {
    if paths.is_empty() {
        return None;
    }
    if let (Some(b), [sp]) = (bounds, paths)
        && !sp.open
        && sp.points.len() == 4
        && sp.points.iter().all(|p| p.left == p.anchor && p.right == p.anchor)
    {
        let on_corner = |p: Point| {
            let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
            (near(p.x, b.left) || near(p.x, b.right)) && (near(p.y, b.top) || near(p.y, b.bottom))
        };
        if sp.points.iter().all(|p| on_corner(t.apply(p.anchor))) {
            return None;
        }
    }
    let mut out = Vec::with_capacity(paths.len());
    for sp in paths {
        let Some(first) = sp.points.first() else { continue };
        let n = sp.points.len();
        // detailed art: its anchors, evenly spaced ones when there are many
        if n > FLATTEN_MAX_ANCHORS {
            let step = n.div_ceil(OUTLINE_MAX_POINTS).max(1);
            let kept = sp.points.iter().enumerate().filter(|(i, _)| i % step == 0 || (sp.open && *i == n - 1));
            out.push(kept.map(|(_, p)| t.apply(p.anchor)).map(|p| [r3(p.x), r3(p.y)]).collect());
            continue;
        }
        // simple shapes (ovals, rounded corners): their curves flattened
        let mut pts = vec![t.apply(first.anchor)];
        for (i, s) in segments(sp, t).enumerate() {
            if !s.is_line() {
                let hull = (s.b.x - s.a.x).hypot(s.b.y - s.a.y)
                    + (s.c.x - s.b.x).hypot(s.c.y - s.b.y)
                    + (s.d.x - s.c.x).hypot(s.d.y - s.c.y);
                let steps = (hull / 6.0).ceil().clamp(2.0, 8.0) as usize;
                for k in 1..steps {
                    pts.push(s.eval(k as f64 / steps as f64));
                }
            }
            // the closing segment ends where the subpath started
            if i + 1 < n {
                pts.push(s.d);
            }
        }
        let step = pts.len().div_ceil(OUTLINE_MAX_POINTS).max(1);
        let last = pts.len() - 1;
        let kept = pts.iter().enumerate().filter(|(i, _)| i % step == 0 || (sp.open && *i == last));
        out.push(kept.map(|(_, p)| [r3(p.x), r3(p.y)]).collect());
    }
    Some(out)
}

/// Pages of a spread in spread coordinates.
fn page_rects(spread: &Spread) -> Vec<Rect> {
    spread.pages.iter().map(|p| p.transform.apply_rect(&p.bounds)).collect()
}

/// The page that holds point `c`: the first whose bounds contain it, else the nearest.
fn page_for(rects: &[Rect], c: Point) -> usize {
    if let Some(i) = rects.iter().position(|r| c.x >= r.left && c.x <= r.right && c.y >= r.top && c.y <= r.bottom) {
        return i;
    }
    let dist = |r: &Rect| {
        let dx = (r.left - c.x).max(c.x - r.right).max(0.0);
        let dy = (r.top - c.y).max(c.y - r.bottom).max(0.0);
        dx * dx + dy * dy
    };
    (0..rects.len()).min_by(|&a, &b| dist(&rects[a]).total_cmp(&dist(&rects[b]))).unwrap_or(0)
}

/// Left or right of the spine (x = 0 in spread coordinates), as `folio_render` decides it;
/// "single" without facing pages.
fn side(facing: bool, spread: &Spread, page: &Page) -> &'static str {
    if !facing {
        return "single";
    }
    let r = page.transform.apply_rect(&page.bounds);
    let right = if spread.pages.len() == 1 { r.left >= -1.0 } else { (r.left + r.right) / 2.0 > 0.0 };
    if right { "right" } else { "left" }
}

// ---------------------------------------------------------------------------------------
// documentInfo

fn model_name(m: ColorModel) -> &'static str {
    match m {
        ColorModel::Process => "process",
        ColorModel::Spot => "spot",
        ColorModel::Registration => "registration",
        ColorModel::None => "none",
        ColorModel::Paper => "paper",
        ColorModel::Mixed => "mixedInk",
    }
}

fn space_name(s: ColorSpace) -> &'static str {
    match s {
        ColorSpace::Cmyk => "CMYK",
        ColorSpace::Rgb => "RGB",
        ColorSpace::Lab => "LAB",
        ColorSpace::Gray => "Gray",
    }
}

/// The colour halfway along a gradient's ramp.
fn gradient_middle(doc: &Document, colors: &Colors, g: &folio_model::Gradient) -> [u8; 3] {
    let mut stops: Vec<&folio_model::GradientStop> = g.stops.iter().collect();
    stops.sort_by(|a, b| a.location.total_cmp(&b.location));
    let at = stops.iter().position(|s| s.location >= 50.0);
    let pair = match at {
        Some(0) => stops.first().map(|s| (*s, *s)),
        Some(i) => Some((stops[i - 1], stops[i])),
        None => stops.last().map(|s| (*s, *s)),
    };
    pair.and_then(|(a, b)| {
        let (sa, sb) = (doc.swatches.get(&a.color)?, doc.swatches.get(&b.color)?);
        let span = b.location - a.location;
        let t = if span > 1e-9 { (50.0 - a.location) / span } else { 0.0 };
        colors.swatch_mix(sa, sb, folio_color::GradientCurve::new(b.midpoint).sample(t), BlendSpace::Rgb)
    })
    .or_else(|| colors.gradient_flat(g, &doc.swatches, BlendSpace::Rgb))
    .unwrap_or([128, 128, 128])
}

fn swatch_json(doc: &Document, colors: &Colors, id: &str) -> Option<Value> {
    if let Some(s) = doc.swatches.get(id) {
        let none = s.model == ColorModel::None;
        return Some(json!({
            "id": s.id, "name": s.name, "model": if s.base.is_some() { "tint" } else { model_name(s.model) },
            "space": if none { "" } else { space_name(s.space) },
            "values": s.values, "tint": s.tint, "base": s.base,
            "rgb": colors.swatch_rgb(s, 100.0, BlendSpace::Rgb).unwrap_or([255, 255, 255]),
        }));
    }
    let g = doc.gradients.get(id)?;
    let kind = match g.kind {
        folio_model::GradientKind::Linear => "Linear",
        folio_model::GradientKind::Radial => "Radial",
    };
    Some(json!({"id": g.id, "name": g.name, "model": "gradient", "space": kind, "values": [], "tint": null, "base": null,
                "rgb": gradient_middle(doc, colors, g), "stops": g.stops}))
}

/// A paint of the document's own (the default fill and stroke of new objects, `PageItemDefault`) as an item's fill is described: kind, swatch name,
/// tint, screen colour, and for an unnamed colour its space and values.
pub(crate) fn paint_value(doc: &Document, colors: &Colors, index: &Index, id: &str, tint: f64) -> Value {
    let paint = if id.is_empty() || id == "Swatch/None" {
        Paint::None
    } else if doc.gradients.contains_key(id) {
        Paint::Gradient { id: id.to_owned(), fallback_rgb: [128, 128, 128] }
    } else {
        Paint::Swatch { id: id.to_owned(), tint }
    };
    let ctx = Items { doc, colors, index, hidden: HashSet::new(), master: false, rgb: Default::default() };
    let mut v = serde_json::to_value(ctx.paint(&paint)).unwrap_or(Value::Null);
    v["id"] = json!(id);
    v
}

/// The style's name as the panels show it: InDesign's own styles by their interface names,
/// others without `$ID/` and without the group path InDesign writes before them.
fn style_name(raw: &str, group: &[String]) -> String {
    let builtin = match raw {
        "$ID/[No paragraph style]" => Some("[No Paragraph Style]"),
        "$ID/NormalParagraphStyle" => Some("[Basic Paragraph]"),
        "$ID/[No character style]" | "$ID/[None]" => Some("[None]"),
        "$ID/[Normal Graphics Frame]" => Some("[Basic Graphics Frame]"),
        "$ID/[Normal Text Frame]" => Some("[Basic Text Frame]"),
        "$ID/[Normal Grid]" => Some("[Basic Grid]"),
        _ => None,
    };
    if let Some(b) = builtin {
        return b.to_owned();
    }
    let name = raw.strip_prefix("$ID/").unwrap_or(raw);
    if !group.is_empty() {
        let prefix = group.iter().map(|g| g.strip_prefix("$ID/").unwrap_or(g)).collect::<Vec<_>>().join(":") + ":";
        if let Some(rest) = name.strip_prefix(&prefix)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    name.to_owned()
}

/// Styles with InDesign's own root styles first (in the panels' order), the rest in
/// document order.
fn styles_json(entries: &[folio_model::StyleEntry], first: &[&str]) -> Value {
    let mut list: Vec<&folio_model::StyleEntry> = entries.iter().collect();
    list.sort_by_key(|e| if e.group.is_empty() { first.iter().position(|f| *f == e.name).unwrap_or(usize::MAX) } else { usize::MAX });
    Value::Array(
        list.into_iter()
            .map(|e| {
                let group: Vec<&str> = e.group.iter().map(|g| g.strip_prefix("$ID/").unwrap_or(g)).collect();
                json!({"id": e.id, "name": style_name(&e.name, &e.group), "group": group})
            })
            .collect(),
    )
}

/// Cell and table styles: with the style each is based on and its settings (the style dialogs edit them).
fn table_styles_json(entries: &[folio_model::StyleEntry], first: &[&str]) -> Value {
    let mut list: Vec<&folio_model::StyleEntry> = entries.iter().collect();
    list.sort_by_key(|e| if e.group.is_empty() { first.iter().position(|f| *f == e.name).unwrap_or(usize::MAX) } else { usize::MAX });
    Value::Array(list.into_iter().map(|e| {
        let group: Vec<&str> = e.group.iter().map(|g| g.strip_prefix("$ID/").unwrap_or(g)).collect();
        json!({"id": e.id, "name": style_name(&e.name, &e.group), "group": group, "basedOn": e.based_on, "settings": e.settings})
    }).collect())
}

/// The cell or table style groups, each with its `Self` and the group names from the top (`CellStyleGroup/$ID/Grp%3aSub`: Grp, Sub).
fn style_groups_json(ids: &[String]) -> Value {
    Value::Array(ids.iter().map(|id| {
        let rest = id.split_once('/').map_or(id.as_str(), |(_, r)| r);
        let path: Vec<&str> = rest.strip_prefix("$ID/").unwrap_or(rest).split("%3a").collect();
        json!({"id": id, "path": path})
    }).collect())
}

/// The object styles as the panel lists them, with `used`: applied to an item of a page or of a parent page (Select All Unused).
fn object_styles_json(doc: &Document, entries: &[folio_model::StyleEntry]) -> Value {
    fn collect<'a>(items: &'a [folio_model::PageItem], used: &mut std::collections::HashSet<&'a str>) {
        for item in items {
            if let Some(s) = item.object_style.as_deref() {
                used.insert(s);
            }
            if let folio_model::ItemKind::Group { children } = &item.kind {
                collect(children, used);
            }
        }
    }
    let mut used = std::collections::HashSet::new();
    for spread in doc.spreads.iter().chain(&doc.master_spreads) {
        collect(&spread.items, &mut used);
    }
    let Value::Array(mut list) = styles_json(entries, &["$ID/[None]", "$ID/[Normal Graphics Frame]", "$ID/[Normal Text Frame]", "$ID/[Normal Grid]"]) else { return Value::Null };
    for row in &mut list {
        let is_used = row["id"].as_str().is_some_and(|id| used.contains(id));
        row["used"] = json!(is_used);
    }
    Value::Array(list)
}

fn document_info(doc: &Document, colors: &Colors, facing: bool) -> Value {
    let mut layer_items: HashMap<&str, usize> = HashMap::new();
    for item in doc.spreads.iter().flat_map(|s| &s.items) {
        if let Some(l) = &item.layer {
            *layer_items.entry(l.as_str()).or_default() += 1;
        }
    }
    let mut layers: Vec<&folio_model::Layer> = doc.layers.iter().collect();
    layers.sort_by_key(|l| std::cmp::Reverse(l.order));
    let layers: Vec<Value> = layers
        .into_iter()
        .map(|l| {
            json!({"id": l.id, "name": l.name, "visible": l.visible, "printable": l.printable, "locked": l.locked,
                   "color": l.color.rgb(), "colorName": match &l.color { folio_model::UiColor::Named(n) => Some(n.clone()), _ => None },
                   "showGuides": l.show_guides, "lockGuides": l.lock_guides,
                   "items": layer_items.get(l.id.as_str()).copied().unwrap_or(0)})
        })
        .collect();

    let order: Vec<&str> = if doc.swatch_order.is_empty() {
        // a document built without the reader's order: the specials first, then by id
        const FIRST: [&str; 4] = ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black"];
        let rest = doc.swatches.keys().chain(doc.gradients.keys()).map(String::as_str).filter(|id| !FIRST.contains(id));
        FIRST.iter().copied().filter(|f| doc.swatches.contains_key(*f)).chain(rest).collect()
    } else {
        doc.swatch_order.iter().map(String::as_str).collect()
    };
    let swatches: Vec<Value> = order.into_iter().filter_map(|id| swatch_json(doc, colors, id)).collect();

    let masters: Vec<Value> = doc
        .master_spreads
        .iter()
        .map(|m| {
            let name = m.name.clone().unwrap_or_else(|| m.id.clone());
            let split = name.split_once('-');
            let prefix = m.name_prefix.clone().or_else(|| split.map(|(p, _)| p.to_owned())).unwrap_or_default();
            let base = m.base_name.clone().or_else(|| split.map(|(_, b)| b.to_owned())).unwrap_or_else(|| name.clone());
            // Based On: the parent its pages take
            let based_on = m.pages.first().and_then(|p| p.master.clone());
            json!({"id": m.id, "name": name, "prefix": prefix, "baseName": base, "pages": m.pages.len(), "basedOn": based_on})
        })
        .collect();

    let mut pages = Vec::new();
    for (si, s) in doc.spreads.iter().enumerate() {
        let rects = page_rects(s);
        let ox = rects.iter().map(|r| r.left).fold(f64::INFINITY, f64::min);
        let oy = rects.iter().map(|r| r.top).fold(f64::INFINITY, f64::min);
        for (p, r) in s.pages.iter().zip(&rects) {
            pages.push(json!({"id": p.id, "index": p.index, "master": p.master, "spreadIndex": si,
                              "spreadOffset": [r3(r.left - ox), r3(r.top - oy)],
                              "spreadOrigin": [r.left, r.top], "side": side(facing, s, p)}));
        }
    }
    pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(0));
    // the pages of the master spreads (opened like a document in the editor: Pages panel, double click a master)
    let mut master_pages = Vec::new();
    for (mi, m) in doc.master_spreads.iter().enumerate() {
        let rects = page_rects(m);
        let ox = rects.iter().map(|r| r.left).fold(f64::INFINITY, f64::min);
        let oy = rects.iter().map(|r| r.top).fold(f64::INFINITY, f64::min);
        for (p, r) in m.pages.iter().zip(&rects) {
            master_pages.push(json!({"id": p.id, "index": p.index, "name": p.name, "master": p.master, "masterSpread": m.id, "spreadIndex": doc.spreads.len() + mi,
                                     "size": [r3(r.width()), r3(r.height())], "spreadOffset": [r3(r.left - ox), r3(r.top - oy)],
                                     "spreadOrigin": [r.left, r.top], "side": side(facing, m, p)}));
        }
    }

    let st = &doc.styles;
    json!({
        "units": {"horizontal": doc.view.horizontal_units, "vertical": doc.view.vertical_units},
        "rulerOrigin": doc.view.ruler_origin,
        "zeroPoint": doc.view.zero_point,
        // View ▸ Grids & Guides: the toggles and what the grids look like
        "view": {
            "strokeUnits": doc.view.stroke_units, "cursorKeyIncrement": doc.view.cursor_key_increment, "leadingKeyIncrement": doc.view.leading_key_increment,
            "baselineShiftKeyIncrement": doc.view.baseline_shift_key_increment, "kerningKeyIncrement": doc.view.kerning_key_increment, "snapZone": doc.view.snap_zone,
            "rulerGuides": {"threshold": doc.view.ruler_guides_threshold, "color": doc.view.ruler_guides_color.rgb(), "colorName": doc.view.ruler_guides_color.name()},
            "pasteboard": {"horizontal": doc.view.pasteboard_margins[0], "vertical": doc.view.pasteboard_margins[1], "matchPreviewBackground": doc.view.match_preview_background,
                           "previewColor": doc.view.preview_background_color.rgb(), "previewColorName": doc.view.preview_background_color.name(),
                           "bleedColor": doc.view.bleed_guide_color.rgb(), "bleedColorName": doc.view.bleed_guide_color.name(),
                           "slugColor": doc.view.slug_guide_color.rgb(), "slugColorName": doc.view.slug_guide_color.name()},
            "guidesLocked": doc.view.guides_locked, "guidesSnap": doc.view.guides_snap, "guidesInBack": doc.view.guides_in_back,
            "columnGuidesLocked": doc.view.column_guides_locked, "gridsInBack": doc.view.grid.grids_in_back,
            "baselineGridShown": doc.view.grid.baseline_shown, "documentGridShown": doc.view.grid.document_shown, "documentGridSnap": doc.view.grid.document_snap,
            "baseline": {"start": doc.view.grid.baseline_start, "division": doc.view.grid.baseline_division, "threshold": doc.view.grid.baseline_threshold,
                         "relativeToMargin": doc.view.grid.baseline_relative_to.starts_with("TopOfMargin"), "color": doc.view.grid.baseline_color.rgb(), "colorName": doc.view.grid.baseline_color.name()},
            "grid": {"horizontal": doc.view.grid.horizontal_division, "vertical": doc.view.grid.vertical_division,
                     "horizontalSub": doc.view.grid.horizontal_subdivision, "verticalSub": doc.view.grid.vertical_subdivision, "color": doc.view.grid.grid_color.rgb(), "colorName": doc.view.grid.grid_color.name()},
        },
        "editorial": doc.stories.iter().map(|(id,s)| json!({"id":id,"notes":s.notes,"changes":s.changes,"trackChanges":s.track_changes})).collect::<Vec<_>>(),
        "facingPages": facing,
        "masterPages": master_pages,
        "layers": layers,
        "swatches": swatches,
        "masters": masters,
        "pages": pages,
        "paragraphStyles": styles_json(&st.paragraph, &["$ID/[No paragraph style]", "$ID/NormalParagraphStyle"]),
        "characterStyles": styles_json(&st.character, &["$ID/[No character style]"]),
        "objectStyles": object_styles_json(doc, &st.object),
        "cellStyles": table_styles_json(&st.cell, &["$ID/[None]"]),
        "tableStyles": table_styles_json(&st.table, &["$ID/[No table style]", "$ID/[Basic Table]"]),
        "paragraphStyleGroups": style_groups_json(&st.paragraph_groups),
        "characterStyleGroups": style_groups_json(&st.character_groups),
        "cellStyleGroups": style_groups_json(&st.cell_groups),
        "objectStyleGroups": style_groups_json(&st.object_groups),
        "tableStyleGroups": style_groups_json(&st.table_groups),
        // the text variables Type > Text Variables > Insert Variable offers (the built-in cross-reference ones are not listed)
        // the Ink Manager's list: the four process inks, then the spot inks (name without `$ID/`; alias = the ink it prints on)
        "inks": doc.print.inks.iter().map(|k| json!({"name": k.name, "process": k.process, "convertToProcess": k.convert_to_process, "alias": k.alias, "inkType": k.ink_type, "print": k.print})).collect::<Vec<_>>(),
        "textVariables": doc.text_variables.values().filter(|v| !v.name.starts_with("<?AID")).map(|v| json!({"id": v.id, "name": v.name, "kind": v.kind, "settings": v.settings})).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------------------
// pageItems

/// `pageItems` reply body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageItems<'a> {
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

#[derive(Serialize)]
struct ItemInfo<'a> {
    id: &'a str,
    anchored: bool,
    kind: &'static str,
    name: Option<&'a str>,
    layer: Option<&'a str>,
    locked: bool,
    visible: bool,
    master: bool,
    /// An item of a parent page that pages may override (Allow Parent Item Overrides).
    allow_overrides: bool,
    /// Object ▸ Display Performance: `LocalDisplaySetting` (`Default`, `HighQuality`, `Typical`, `Optimized`).
    local_display: &'static str,
    fitting: &'a folio_model::fitting::FrameFitting,
    /// Attributes panel: `Nonprinting`, `OverprintFill`, `OverprintStroke`, `OverprintGap`.
    print: PrintFlags,
    z: usize,
    transform: [f64; 6],
    inner: [f64; 4],
    bounds: [f64; 4],
    outline: Option<Outline>,
    /// The path points in the item's own space (anchor, left and right direction), for the Pen and Direct Selection tools:
    /// shapes with a modest number of points only.
    paths: Option<Vec<PathInfo>>,
    fill: PaintInfo<'a>,
    stroke: StrokeInfo<'a>,
    fill_gradient: Option<folio_model::GradientGeometry>,
    stroke_gradient: Option<folio_model::GradientGeometry>,
    opacity: f64,
    /// Corner options: top left, top right, bottom left, bottom right (shape as IDML names it).
    corners: [CornerInfo; 4],
    /// Opacity (percent) and blend mode (IDML name) of the object, stroke, fill and graphic.
    blending: Blending,
    /// Group transparency: Isolate Blending, Knockout Group.
    isolate: bool,
    knockout: bool,
    /// The applied object style (`Self`).
    style: Option<&'a str>,
    content: Option<ContentInfo<'a>>,
    text: Option<TextInfo<'a>>,
    text_wrap: Option<&'a folio_model::TextWrap>,
    children: Vec<ItemInfo<'a>>,
}

#[derive(Serialize)]
struct Blending {
    object: Blend,
    stroke: Blend,
    fill: Blend,
    /// A text frame's text (`ContentTransparencySetting`).
    text: Blend,
    graphic: Blend,
}

#[derive(Serialize)]
struct Blend {
    opacity: f64,
    mode: String,
}

fn blend((opacity, mode): (f64, folio_model::BlendMode)) -> Blend {
    Blend { opacity: r3(opacity * 100.0), mode: format!("{mode:?}") }
}

#[derive(Serialize)]
struct CornerInfo {
    shape: &'static str,
    radius: f64,
}

fn corner_info(c: &folio_model::geom::CornerOption) -> CornerInfo {
    use folio_model::geom::CornerKind;
    let shape = match c.kind {
        CornerKind::None => "None",
        CornerKind::Rounded => "RoundedCorner",
        CornerKind::InverseRounded => "InverseRoundedCorner",
        CornerKind::Inset => "InsetCorner",
        CornerKind::Bevel => "BevelCorner",
        CornerKind::Fancy => "FancyCorner",
    };
    CornerInfo { shape, radius: r3(c.radius) }
}

#[derive(Serialize)]
struct PaintInfo<'a> {
    kind: &'static str,
    swatch: Option<&'a str>,
    tint: Option<f64>,
    rgb: Option<[u8; 3]>,
    /// An unnamed colour (the Color panel's sliders make these): its own space, values and `Self`.
    #[serde(skip_serializing_if = "Option::is_none")]
    space: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<&'a [f64]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StrokeInfo<'a> {
    kind: &'static str,
    swatch: Option<&'a str>,
    tint: Option<f64>,
    rgb: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    space: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<&'a [f64]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'a str>,
    weight: f64,
    alignment: &'static str,
    cap: &'static str,
    join: &'static str,
    miter_limit: f64,
    /// The stroke style's `Self` (`StrokeStyle/$ID/Solid` when solid).
    #[serde(rename = "type")]
    stroke_type: &'a str,
    start: &'static str,
    end: &'static str,
    start_scale: f64,
    end_scale: f64,
    gap: PaintInfo<'a>,
}

/// The Attributes panel's boxes (IDML `Nonprinting`, `OverprintFill`, `OverprintStroke`, `OverprintGap`).
#[derive(Serialize)]
struct PrintFlags {
    nonprinting: bool,
    overprint_fill: bool,
    overprint_stroke: bool,
    overprint_gap: bool,
}

#[derive(Serialize)]
struct ContentInfo<'a> {
    kind: &'a str,
    clipping: bool,
    clipping_options: Option<&'a folio_model::clipping::Options>,
    layer_options: Option<&'a folio_model::graphic_layers::Options>,
    image_color: Option<&'a folio_model::image_color::ImageColor>,
    image_space: Option<&'a str>,
    transform: [f64; 6],
    inner: [f64; 4],
    bounds: [f64; 4],
    link: Option<String>,
    ppi: Option<[f64; 2]>,
    /// The graphic's own `LocalDisplaySetting`.
    local_display: &'static str,
}

#[derive(Serialize)]
struct TextInfo<'a> {
    story: &'a str,
    previous: Option<&'a str>,
    next: Option<&'a str>,
    columns: u32,
    overset: bool,
    /// The story has no character.
    empty: bool,
    /// Type ▸ Type Orientation of the story: `Horizontal` or `Vertical` (StoryPreference/@StoryOrientation).
    orientation: &'static str,
    /// Text Frame Options as the reader has them (IDML names for the choices).
    options: FrameOptionsInfo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameOptionsInfo {
    gutter: f64,
    /// The fixed column width, when the columns have one.
    fixed_width: Option<f64>,
    /// Top, left, bottom, right.
    insets: [f64; 4],
    vertical: &'static str,
    ignore_wrap: bool,
    first_baseline: &'static str,
    min_first_baseline: f64,
    /// Auto-Size: `Off` or the IDML type, the reference point, the minimums (when used) and
    /// No Line Breaks.
    auto_size: &'static str,
    auto_size_point: &'static str,
    min_height: Option<f64>,
    min_width: Option<f64>,
    no_line_breaks: bool,
}

/// What Text Frame Options starts from for a frame: its options as `pageItems` gives them (`text.options`), the column count and the
/// frame's inner width (both as the reader rounds them), and whether a corner is shaped (then the frame has one inset, as in InDesign).
pub(crate) fn text_frame_dialog_facts(item: &PageItem, tf: &folio_model::TextFrame) -> Value {
    let inner = paths_bounds(&item.paths, &Affine::IDENTITY).map(|r| rect4(&r)).unwrap_or_default();
    let shaped = item.stroke.corners.iter().map(corner_info).any(|c| c.shape != "None" && c.radius > 0.0);
    json!({"options": frame_options(tf), "columns": tf.columns, "width": inner[2] - inner[0], "shaped": shaped})
}

fn frame_options(tf: &folio_model::TextFrame) -> FrameOptionsInfo {
    use folio_model::{FirstBaseline as B, VerticalJustification as V};
    FrameOptionsInfo {
        gutter: r3(tf.gutter),
        fixed_width: tf.column_width.map(r3),
        insets: tf.insets.map(r3),
        vertical: match tf.vertical_justification { V::Top => "TopAlign", V::Center => "CenterAlign", V::Bottom => "BottomAlign", V::Justify => "JustifyAlign" },
        ignore_wrap: tf.ignore_wrap,
        first_baseline: match tf.first_baseline { B::Ascent => "AscentOffset", B::CapHeight => "CapHeight", B::XHeight => "XHeight", B::Leading => "LeadingOffset", B::EmBoxHeight => "EmboxHeight", B::Fixed => "FixedHeight" },
        min_first_baseline: r3(tf.min_first_baseline_offset),
        auto_size: match tf.auto_size.as_ref().map(|a| a.kind) {
            None => "Off",
            Some(folio_model::AutoSizeKind::HeightOnly) => "HeightOnly",
            Some(folio_model::AutoSizeKind::WidthOnly) => "WidthOnly",
            Some(folio_model::AutoSizeKind::HeightAndWidth) => "HeightAndWidth",
            Some(folio_model::AutoSizeKind::HeightAndWidthProportionally) => "HeightAndWidthProportionally",
        },
        auto_size_point: {
            let p = tf.auto_size.as_ref().map_or((0.5, 0.5), |a| a.point);
            const NAMES: [[&str; 3]; 3] = [["TopLeftPoint", "TopCenterPoint", "TopRightPoint"], ["LeftCenterPoint", "CenterPoint", "RightCenterPoint"], ["BottomLeftPoint", "BottomCenterPoint", "BottomRightPoint"]];
            NAMES[(p.1 * 2.0).round().clamp(0.0, 2.0) as usize][(p.0 * 2.0).round().clamp(0.0, 2.0) as usize]
        },
        min_height: tf.auto_size.as_ref().and_then(|a| a.min_height).map(r3),
        min_width: tf.auto_size.as_ref().and_then(|a| a.min_width).map(r3),
        no_line_breaks: tf.auto_size.as_ref().is_some_and(|a| a.no_line_breaks),
    }
}

/// What an item takes from its parent.
#[derive(Clone, Copy)]
struct Inherit<'a> {
    layer: Option<&'a str>,
    visible: bool,
    locked: bool,
}

type ScreenColors<'a> = HashMap<(&'a str, u64), Option<[u8; 3]>>;

struct Items<'a> {
    doc: &'a Document,
    colors: &'a Colors,
    index: &'a Index,
    /// Master items the page does not show (overridden or removed), at any depth.
    hidden: HashSet<&'a str>,
    master: bool,
    /// Screen colours already converted in this reply, by swatch and tint.
    rgb: std::cell::RefCell<ScreenColors<'a>>,
}

impl<'a> Items<'a> {
    /// Layer rank, as the renderer sorts.
    fn rank(&self, item: &PageItem) -> usize {
        item.layer.as_deref().and_then(|l| self.index.layers.get(l)).map_or(0, |l| l.order)
    }

    /// Members in draw order (stably sorted by layer), without the hidden ones and the
    /// vector art of a placed graphic (shown as the frame's `content`).
    fn ordered(&self, items: &'a [PageItem]) -> Vec<&'a PageItem> {
        let mut v: Vec<&PageItem> = items.iter().filter(|i| !self.hidden.contains(i.id.as_str()) && !i.print.placed_art).collect();
        if v.iter().any(|i| i.layer.is_some()) {
            v.sort_by_cached_key(|i| self.rank(i));
        }
        v
    }

    /// Items listed for this item: itself and its members.
    fn count(&self, item: &'a PageItem) -> usize {
        let members = match &item.kind {
            ItemKind::Group { children } => self.ordered(children),
            _ => Vec::new(),
        };
        1 + members.iter().chain(self.ordered(&item.clipped).iter()).map(|c| self.count(c)).sum::<usize>()
    }

    fn paint(&self, p: &'a Paint) -> PaintInfo<'a> {
        let none = PaintInfo { kind: "none", swatch: None, tint: None, rgb: None, space: None, values: None, color: None };
        match p {
            Paint::None => none,
            Paint::Swatch { id, tint } => match self.doc.swatches.get(id) {
                Some(s) => {
                    let rgb = *self.rgb.borrow_mut().entry((id.as_str(), tint.to_bits())).or_insert_with(|| self.colors.swatch_rgb(s, *tint, BlendSpace::Rgb));
                    // an unnamed tint (not in the Swatches panel) shows as its base colour
                    // at the combined tint, as InDesign's Control panel does
                    let base = s.base.as_ref().filter(|_| s.name.is_empty()).and_then(|b| self.doc.swatches.get(b));
                    let (name, tint) = match base {
                        Some(b) => (b.name.as_str(), s.tint.unwrap_or(100.0) * tint / 100.0),
                        None => (s.name.as_str(), *tint),
                    };
                    // an unnamed colour (pasted art, InDesign's `$ID/` colours) is no swatch
                    let kind = if name.is_empty() { "other" } else { "swatch" };
                    let own = kind == "other" && s.base.is_none();
                    let space = own.then_some(match s.space { folio_model::ColorSpace::Cmyk => "CMYK", folio_model::ColorSpace::Rgb => "RGB", folio_model::ColorSpace::Lab => "LAB", folio_model::ColorSpace::Gray => "GRAY" });
                    PaintInfo { kind, swatch: (!name.is_empty()).then_some(name), tint: Some(r3(tint)), rgb, space, values: own.then_some(s.values.as_slice()), color: own.then_some(id.as_str()) }
                }
                None => PaintInfo { kind: "other", tint: Some(r3(*tint)), ..none },
            },
            Paint::Gradient { id, fallback_rgb } => match self.doc.gradients.get(id) {
                Some(g) => PaintInfo { kind: "gradient", swatch: Some(g.name.as_str()), tint: None, rgb: Some(gradient_middle(self.doc, self.colors, g)), space: None, values: None, color: None },
                None => PaintInfo { kind: "gradient", rgb: Some(*fallback_rgb), ..none },
            },
        }
    }

    fn content(&self, g: &'a Graphic, t: &Affine) -> ContentInfo<'a> {
        let ct = t.then_inner(&g.transform);
        let uri = g.link.as_ref().map(|l| l.uri.as_str()).filter(|u| !u.is_empty()).or(g.link_uri.as_deref());
        let (sx, sy) = ((ct.a * ct.a + ct.b * ct.b).sqrt(), (ct.c * ct.c + ct.d * ct.d).sqrt());
        let ppi = g.actual_ppi.filter(|_| sx > 1e-9 && sy > 1e-9).map(|(x, y)| (x / sx, y / sy)).or(g.effective_ppi).map(|(x, y)| [round(x, 1), round(y, 1)]);
        ContentInfo { kind: &g.element, clipping: g.clip.as_ref().is_some_and(|c| !c.paths.is_empty()), clipping_options: g.clipping_options.as_ref(), layer_options: g.layer_options.as_ref(), image_color: g.image_color.as_ref(), image_space: g.image_space.as_deref(), transform: affine6(&ct), inner: rect4(&g.bounds), bounds: rect4(&ct.apply_rect(&g.bounds)), link: uri.and_then(link_name), ppi, local_display: g.local_display.idml() }
    }

    /// An item and its members, and its geometric bounds in page coordinates; `parent_t`
    /// maps the item's parent to page coordinates and `z` is the next draw-order number.
    fn item(&self, item: &'a PageItem, parent_t: &Affine, inherit: Inherit<'a>, z: &mut usize) -> (ItemInfo<'a>, Option<Rect>) {
        let t = parent_t.then_inner(&item.transform);
        let my_z = *z;
        *z += 1;
        let layer = item.layer.as_deref().or(inherit.layer);
        let layer_visible = layer.and_then(|l| self.index.layers.get(l)).is_none_or(|l| l.visible);
        let visible = inherit.visible && item.visible && layer_visible;
        let locked = inherit.locked || item.locked;
        let mine = Inherit { layer, visible, locked };

        let (kind, members, graphic, text) = match &item.kind {
            ItemKind::TextFrame(tf) => ("textFrame", None, None, Some(tf)),
            ItemKind::Rectangle { content } => ("rectangle", None, content.as_ref(), None),
            ItemKind::Oval { content } => ("oval", None, content.as_ref(), None),
            ItemKind::Polygon { content } => ("polygon", None, content.as_ref(), None),
            ItemKind::GraphicLine => ("line", None, None, item.text_frame()),
            ItemKind::Group { children } => ("group", Some(children), None, None),
            ItemKind::Other { .. } => ("other", None, None, None),
        };
        let mut children = Vec::new();
        let mut member_bounds = None;
        if let Some(m) = members {
            for c in self.ordered(m) {
                let (info, b) = self.item(c, &t, mine, z);
                member_bounds = union(member_bounds, b);
                children.push(info);
            }
        }
        for c in self.ordered(&item.clipped) {
            children.push(self.item(c, &t, mine, z).0);
        }
        if let Some(anchors) = self.index.anchors.get(&item.id) {
            for c in anchors { children.push(self.item(c, &t, mine, z).0); }
        }

        // Bounds from the members' (a group) or through an axis-aligned transform from the
        // inner bounds (exact either way); rotated and skewed items through their paths.
        let axis_aligned = t.b.abs() < 1e-12 && t.c.abs() < 1e-12;
        let (inner, page_bounds) = if members.is_some() {
            let inner = match (member_bounds, axis_aligned.then(|| t.invert()).flatten()) {
                (Some(b), Some(inv)) => Some(inv.apply_rect(&b)),
                _ => geometry_bounds(item, &Affine::IDENTITY),
            };
            (inner, member_bounds)
        } else {
            let inner = paths_bounds(&item.paths, &Affine::IDENTITY);
            (inner, if axis_aligned { inner.map(|r| t.apply_rect(&r)) } else { paths_bounds(&item.paths, &t) })
        };
        let inner = inner.unwrap_or_default();
        let bounds = page_bounds.unwrap_or_else(|| {
            let o = t.apply(Point::default());
            Rect { left: o.x, top: o.y, right: o.x, bottom: o.y }
        });
        let outline = if members.is_some() { None } else { outline(&item.paths, &t, Some(&bounds)) };
        let paths = (members.is_none() && !item.paths.is_empty() && item.paths.iter().map(|p| p.points.len()).sum::<usize>() <= POINTS_MAX)
            .then(|| item.paths.iter().map(|sp| PathInfo { open: sp.open, points: sp.points.iter().map(|p| [p.anchor.x, p.anchor.y, p.left.x, p.left.y, p.right.x, p.right.y]).collect() }).collect());
        // A QR code InDesign generated is drawn from its modules: its placed graphic.
        let content = match (graphic, item.clipped.iter().find(|c| c.print.placed_art)) {
            (Some(g), _) => Some(self.content(g, &t)),
            (None, Some(q)) => {
                let qt = t.then_inner(&q.transform);
                let inner = paths_bounds(&q.paths, &Affine::IDENTITY).unwrap_or_default();
                Some(ContentInfo { kind: "QRCode", clipping: false, clipping_options: None, layer_options: None, image_color: None, image_space: None, transform: affine6(&qt), inner: rect4(&inner), bounds: rect4(&qt.apply_rect(&inner)), link: None, ppi: None, local_display: "Default" })
            }
            (None, None) => None,
        };
        let text = text.map(|tf| TextInfo {
            story: &tf.story,
            previous: tf.previous.as_deref(),
            next: tf.next.as_deref(),
            columns: tf.columns,
            overset: tf.next.is_none() && self.index.overset.contains(&tf.story),
            empty: tf.previous.is_none() && tf.next.is_none() && self.index.empty.contains(&tf.story),
            orientation: if self.index.vertical.contains(&tf.story) { "Vertical" } else { "Horizontal" },
            options: frame_options(tf),
        });
        let sp = self.paint(&item.stroke.paint);
        let stroke = StrokeInfo {
            kind: sp.kind,
            swatch: sp.swatch,
            tint: sp.tint,
            rgb: sp.rgb,
            space: sp.space,
            values: sp.values,
            color: sp.color,
            weight: r3(item.stroke.weight),
            alignment: match item.stroke.alignment {
                StrokeAlignment::Center => "Center",
                StrokeAlignment::Inside => "Inside",
                StrokeAlignment::Outside => "Outside",
            },
            cap: match item.stroke.cap {
                LineCap::Butt => "ButtEndCap",
                LineCap::Round => "RoundEndCap",
                LineCap::Projecting => "ProjectingEndCap",
            },
            join: match item.stroke.join {
                LineJoin::Miter => "MiterEndJoin",
                LineJoin::Round => "RoundEndJoin",
                LineJoin::Bevel => "BevelEndJoin",
            },
            miter_limit: r3(item.stroke.miter_limit.unwrap_or(4.0)),
            stroke_type: item.stroke.style.as_deref().unwrap_or("StrokeStyle/$ID/Solid"),
            start: item.stroke.ends.start.idml(),
            end: item.stroke.ends.end.idml(),
            start_scale: r3(item.stroke.ends.start_scale),
            end_scale: r3(item.stroke.ends.end_scale),
            gap: self.paint(&item.stroke.gap),
        };
        let info = ItemInfo {
            id: &item.id,
            anchored: self.index.anchored_ids.contains(&item.id),
            kind,
            name: item.name.as_deref(),
            layer,
            locked,
            visible,
            master: self.master,
            allow_overrides: !item.overrides_off,
            local_display: item.local_display.idml(),
            fitting: &item.fitting,
            print: PrintFlags { nonprinting: item.print.nonprinting, overprint_fill: item.print.overprint_fill, overprint_stroke: item.print.overprint_stroke, overprint_gap: item.print.overprint_gap },
            z: my_z,
            transform: affine6(&t),
            inner: rect4(&inner),
            bounds: rect4(&bounds),
            outline,
            paths,
            fill: self.paint(&item.fill),
            fill_gradient: item.fill_gradient,
            stroke_gradient: item.stroke_gradient,
            stroke,
            text_wrap: item.text_wrap.as_ref(),
            opacity: r3(item.opacity * 100.0),
            corners: item.stroke.corners.each_ref().map(corner_info),
            style: item.object_style.as_deref(),
            blending: {
                let normal = (1.0, folio_model::BlendMode::Normal);
                Blending {
                    object: blend((item.opacity, item.blend_mode)),
                    stroke: blend(item.print.stroke_blend.unwrap_or(normal)),
                    fill: blend(item.print.fill_blend.unwrap_or(normal)),
                    text: blend(item.print.content_blend.unwrap_or(normal)),
                    graphic: blend(item.print.content_blend.unwrap_or(normal)),
                }
            },
            isolate: item.print.isolate_blending,
            knockout: item.print.knockout_group,
            content,
            text,
            children,
        };
        (info, page_bounds)
    }
}

/// The file name of a link (`file:` URI or path), without folders.
fn link_name(uri: &str) -> Option<String> {
    if let Some(name) = folio_links::parse_uri(uri).as_ref().and_then(|lp| lp.file_name().map(str::to_owned)) {
        return Some(name);
    }
    let last = uri.rsplit(['/', '\\', ':']).next().unwrap_or(uri);
    (!last.is_empty()).then(|| last.to_owned())
}

fn page_items<'a>(doc: &'a Document, colors: &'a Colors, index: &'a Index, page_index: usize) -> Result<PageItems<'a>, String> {
    let &(si, pi) = index.pages.get(&page_index).ok_or_else(|| format!("no page {page_index}"))?;
    let spread = doc.spreads.iter().chain(&doc.master_spreads).nth(si).ok_or_else(|| format!("no spread {si}"))?;
    let page = &spread.pages[pi];
    let rects = page_rects(spread);
    let pr = rects[pi];
    let to_page = Affine::translate(-pr.left, -pr.top);
    let top = Inherit { layer: None, visible: true, locked: false };

    // Master items of each page of the spread (they number first), this page's in full.
    let mut z = 0;
    let mut master_items = Vec::new();
    for (qi, q) in spread.pages.iter().enumerate() {
        for source in folio_model::parents::sources(doc, spread, q) {
            let (master, to_doc) = (source.spread, source.transform);
            let ctx = Items { doc, colors, index, hidden: source.hidden.iter().copied().collect(), master: true, rgb: Default::default() };
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
                    master_items.push(ctx.item(item, &t, top, &mut z).0);
                } else {
                    z += ctx.count(item);
                }
            }
        }
    }

    // The spread's items: each listed on the page that holds the centre of its bounds.
    let ctx = Items { doc, colors, index, hidden: HashSet::new(), master: false, rgb: Default::default() };
    let mut items = Vec::new();
    for item in ctx.ordered(&spread.items) {
        let on_page = rects.len() == 1 || {
            let c = geometry_bounds(item, &item.transform)
                .map(|b| Point::new((b.left + b.right) / 2.0, (b.top + b.bottom) / 2.0))
                .unwrap_or(Point::new(item.transform.tx, item.transform.ty));
            page_for(&rects, c) == pi
        };
        if on_page {
            items.push(ctx.item(item, &to_page, top, &mut z).0);
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

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::{Layer, PathPoint, SubPath, TextFrame};

    fn rect_path(l: f64, t: f64, r: f64, b: f64) -> Vec<SubPath> {
        let p = |x: f64, y: f64| {
            let a = Point::new(x, y);
            PathPoint { anchor: a, left: a, right: a }
        };
        vec![SubPath { points: vec![p(l, t), p(l, b), p(r, b), p(r, t)], open: false }]
    }

    fn rect_item(id: &str, l: f64, t: f64, r: f64, b: f64) -> PageItem {
        PageItem { id: id.into(), kind: ItemKind::Rectangle { content: None }, paths: rect_path(l, t, r, b), ..PageItem::default() }
    }

    fn page(id: &str, index: usize, tx: f64, master: Option<&str>) -> Page {
        Page {
            id: id.into(),
            name: format!("{}", index + 1),
            bounds: Rect { left: 0.0, top: 0.0, right: 600.0, bottom: 800.0 },
            transform: Affine::translate(tx, -400.0),
            master: master.map(str::to_owned),
            index,
            margins: [36.0, 48.0, 36.0, 24.0],
            ..Page::default()
        }
    }

    /// Two facing pages (x −600…0 and 0…600) on master "M" (two pages as well).
    fn doc() -> Document {
        let mut d = Document { layers: vec![
            Layer { id: "top".into(), name: "Top".into(), visible: true, printable: true, order: 1, ..Layer::default() },
            Layer { id: "bottom".into(), name: "Bottom".into(), visible: false, printable: true, order: 0, locked: true, ..Layer::default() },
        ], ..Document::default() };
        d.print.facing_pages = true;
        d.print.facing_pages_stored = true;
        let mut master = Spread { id: "M".into(), name: Some("A-Master".into()), ..Spread::default() };
        master.pages = vec![page("mp1", 0, -600.0, None), page("mp2", 1, 0.0, None)];
        master.items = vec![
            // left master page, overridden on document page 0
            rect_item("m-left", -580.0, -380.0, -500.0, -300.0),
            // right master page
            rect_item("m-right", 20.0, -380.0, 100.0, -300.0),
            // across the spine: shown on both pages
            rect_item("m-wide", -100.0, 0.0, 100.0, 50.0),
        ];
        d.master_spreads.push(master);
        let mut s = Spread { id: "S".into(), ..Spread::default() };
        let mut p0 = page("p0", 0, -600.0, Some("M"));
        p0.master_overrides = vec!["m-left".into()];
        s.pages = vec![p0, page("p1", 1, 0.0, Some("M"))];
        let mut wide = rect_item("wide", -400.0, -100.0, 300.0, 100.0);
        wide.layer = Some("top".into());
        // its centre (−50, 0) is left of the spine: listed on page 0 only
        let mut low = rect_item("low", 100.0, 100.0, 200.0, 200.0);
        low.layer = Some("bottom".into());
        low.name = Some("Low box".into());
        // rotated 90° about the spread origin, then moved right: a 100 × 20 bar standing up
        let mut rotated = rect_item("rot", 0.0, 0.0, 100.0, 20.0);
        rotated.transform = Affine::new(0.0, 1.0, -1.0, 0.0, 300.0, -200.0);
        rotated.locked = true;
        let mut frame = rect_item("tf", 50.0, -350.0, 250.0, -150.0);
        frame.kind = ItemKind::TextFrame(TextFrame { story: "st".into(), columns: 2, ..TextFrame::default() });
        s.items = vec![wide, low, rotated, frame];
        d.spreads.push(s);
        d
    }

    fn pages_of(d: &Document, page: usize) -> Value {
        let layout = DocumentLayout { overset: vec!["st".into()], ..DocumentLayout::default() };
        let colors = Colors::unmanaged();
        let index = Index::new(d, &layout, &colors);
        serde_json::to_value(page_items(d, &colors, &index, page).unwrap()).unwrap()
    }

    fn ids(v: &Value) -> Vec<&str> {
        v.as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect()
    }

    #[test]
    fn an_item_across_two_pages_is_listed_once_on_the_page_of_its_centre() {
        let d = doc();
        let p0 = pages_of(&d, 0);
        let p1 = pages_of(&d, 1);
        assert_eq!(ids(&p0["items"]), ["wide"]);
        // bottom layer first
        assert_eq!(ids(&p1["items"]), ["low", "rot", "tf"]);
        // page coordinates of page 0: its top-left is (−600, −400)
        assert_eq!(p0["items"][0]["bounds"], json!([200.0, 300.0, 900.0, 500.0]));
        assert_eq!(p1["items"][0]["bounds"], json!([100.0, 500.0, 200.0, 600.0]));
        // z is one order over the spread: masters of both pages (1 + 2), then the items
        assert_eq!(p0["items"][0]["z"], json!(6));
        assert_eq!(p1["items"][0]["z"], json!(3));
        assert_eq!(p1["items"][0]["visible"], json!(false));
        assert_eq!(p1["items"][0]["name"], json!("Low box"));
        assert_eq!(p1["items"][0]["layer"], json!("bottom"));
        assert_eq!(p1["side"], json!("right"));
        assert_eq!(p0["side"], json!("left"));
        let tf = &p1["items"][2];
        assert_eq!(tf["kind"], json!("textFrame"));
        assert_eq!(tf["text"]["overset"], json!(true));
        assert_eq!(tf["text"]["columns"], json!(2));
    }

    #[test]
    fn master_items_leave_out_overridden_ones_and_show_where_they_reach_the_page() {
        let d = doc();
        let p0 = pages_of(&d, 0);
        let p1 = pages_of(&d, 1);
        assert_eq!(ids(&p0["masterItems"]), ["m-wide"]);
        assert_eq!(ids(&p1["masterItems"]), ["m-right", "m-wide"]);
        assert_eq!(p0["masterItems"][0]["master"], json!(true));
        assert_eq!(p1["masterItems"][0]["bounds"], json!([20.0, 20.0, 100.0, 100.0]));
        assert_eq!(p1["masterItems"][1]["bounds"], json!([-100.0, 400.0, 100.0, 450.0]));
        assert_eq!(p0["masterItems"][0]["z"], json!(0));
        assert_eq!(p1["masterItems"][0]["z"], json!(1));
    }

    #[test]
    fn a_rotated_item_has_its_transform_and_bounds_in_page_coordinates() {
        let d = doc();
        let p1 = pages_of(&d, 1);
        let rot = &p1["items"][1];
        assert_eq!(rot["id"], json!("rot"));
        // inner → spread (0 1 −1 0 300 −200), then spread → page (0, +400)
        assert_eq!(rot["transform"], json!([0.0, 1.0, -1.0, 0.0, 300.0, 200.0]));
        assert_eq!(rot["inner"], json!([0.0, 0.0, 100.0, 20.0]));
        // (x, y) → (300 − y, 200 + x): x 280…300, y 200…300
        assert_eq!(rot["bounds"], json!([280.0, 200.0, 300.0, 300.0]));
        assert_eq!(rot["locked"], json!(true));
        // still an axis-aligned rectangle: no outline needed
        assert_eq!(rot["outline"], Value::Null);
        let mut d2 = d.clone();
        d2.spreads[0].items[2].transform = Affine::new(0.6, 0.8, -0.8, 0.6, 300.0, -200.0);
        let p = pages_of(&d2, 1);
        let rot = &p["items"][1];
        let o = rot["outline"][0].as_array().unwrap();
        assert_eq!(o.len(), 4);
        // (100, 20) → (300 + 60 − 16, −200 + 80 + 12) in the spread, y + 400 on the page
        assert_eq!(o[2], json!([344.0, 292.0]));
        assert_eq!(rot["bounds"], json!([284.0, 200.0, 360.0, 292.0]));
    }

    #[test]
    fn margins_columns_and_guides_are_in_page_coordinates() {
        let mut d = doc();
        d.spreads[0].pages[0].columns = folio_model::PageColumns { count: 2, gutter: 12.0, positions: Some(vec![0.0, 258.0, 270.0, 528.0]), direction: "Horizontal".into() };
        d.spreads[0].guides = vec![
            folio_model::Guide { id: "g1".into(), horizontal: false, position: -300.0, page: Some(0), ..Default::default() },
            folio_model::Guide { id: "g2".into(), horizontal: true, position: 0.0, page: None, fit_to_page: false, layer: Some("bottom".into()), ..Default::default() },
            folio_model::Guide { id: "g3".into(), horizontal: true, position: 10.0, page: Some(1), ..Default::default() },
        ];
        let p0 = pages_of(&d, 0);
        assert_eq!(p0["margins"], json!([36.0, 48.0, 36.0, 24.0]));
        // a left-hand page: the live area starts at the stored right (outside) margin
        assert_eq!(p0["columns"]["positions"], json!([24.0, 282.0, 294.0, 552.0]));
        let g = p0["guides"].as_array().unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g[0]["orientation"], json!("vertical"));
        assert_eq!(g[0]["location"], json!(300.0));
        assert_eq!(g[1]["location"], json!(400.0));
        assert_eq!(g[1]["fitToPage"], json!(false));
        // the guide's layer is locked
        assert_eq!(g[1]["locked"], json!(true));
        assert_eq!(g[0]["color"], json!([0, 210, 255]));
        let p1 = pages_of(&d, 1);
        assert_eq!(p1["guides"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn document_info_lists_layers_top_first_and_pages_with_their_side() {
        let d = doc();
        let info = document_info(&d, &Colors::unmanaged(), true);
        assert_eq!(ids(&info["layers"]), ["top", "bottom"]);
        assert_eq!(info["layers"][0]["items"], json!(1));
        assert_eq!(info["layers"][1]["locked"], json!(true));
        assert_eq!(info["layers"][0]["color"], json!([79, 153, 255]));
        assert_eq!(info["pages"][1], json!({"id": "p1", "index": 1, "master": "M", "spreadIndex": 0, "spreadOffset": [600.0, 0.0], "spreadOrigin":[0.0,-400.0], "side": "right"}));
        assert_eq!(info["masters"][0], json!({"id": "M", "name": "A-Master", "prefix": "A", "baseName": "Master", "pages": 2, "basedOn": null}));
        assert_eq!(info["facingPages"], json!(true));
    }

    #[test]
    fn style_names_as_the_panels_show_them() {
        assert_eq!(style_name("$ID/NormalParagraphStyle", &[]), "[Basic Paragraph]");
        assert_eq!(style_name("$ID/[No paragraph style]", &[]), "[No Paragraph Style]");
        assert_eq!(style_name("$ID/[Normal Text Frame]", &[]), "[Basic Text Frame]");
        assert_eq!(style_name("Heads:Sub:Title", &["$ID/Heads".into(), "Sub".into()]), "Title");
        assert_eq!(style_name("Body", &[]), "Body");
        let entries = vec![
            folio_model::StyleEntry { id: "ParagraphStyle/Body".into(), name: "Body".into(), group: vec![], ..Default::default() },
            folio_model::StyleEntry { id: "ParagraphStyle/$ID/NormalParagraphStyle".into(), name: "$ID/NormalParagraphStyle".into(), group: vec![], ..Default::default() },
            folio_model::StyleEntry { id: "ParagraphStyle/$ID/[No paragraph style]".into(), name: "$ID/[No paragraph style]".into(), group: vec![], ..Default::default() },
        ];
        let v = styles_json(&entries, &["$ID/[No paragraph style]", "$ID/NormalParagraphStyle"]);
        let names: Vec<&str> = v.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["[No Paragraph Style]", "[Basic Paragraph]", "Body"]);
    }

    #[test]
    fn replies_are_json_headers_and_detailed_outlines_are_capped() {
        let mut d = doc();
        // a 500-point zigzag: listed with at most 64 points
        let pts = (0..500).map(|i| {
            let a = Point::new(i as f64, if i % 2 == 0 { 0.0 } else { 5.0 });
            PathPoint { anchor: a, left: a, right: a }
        });
        let mut zig = rect_item("zig", 0.0, 0.0, 1.0, 1.0);
        zig.kind = ItemKind::Polygon { content: None };
        zig.paths = vec![SubPath { points: pts.collect(), open: true }];
        zig.transform = Affine::translate(10.0, 0.0);
        d.spreads[0].items.push(zig);
        let layout = DocumentLayout::default();
        let colors = Colors::unmanaged();
        let index = std::sync::OnceLock::new();
        let text = command(&json!({"cmd": "pageItems", "page": 1}), &d, &layout, &colors, &index).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["type"], json!("pageItems"));
        assert!(v["ms"].as_f64().is_some());
        let zig = v["items"]["items"].as_array().unwrap().iter().find(|i| i["id"] == "zig").unwrap();
        let o = zig["outline"][0].as_array().unwrap();
        assert!(o.len() <= 65, "{}", o.len());
        // the ends of an open path stay
        assert_eq!(o[0], json!([10.0, 400.0]));
        assert_eq!(o[o.len() - 1], json!([509.0, 405.0]));
        assert_eq!(zig["bounds"], json!([10.0, 400.0, 509.0, 405.0]));
        let info: Value = serde_json::from_str(&command(&json!({"cmd": "documentInfo"}), &d, &layout, &colors, &index).unwrap()).unwrap();
        assert_eq!(info["type"], json!("documentInfo"));
        assert_eq!(info["info"]["rulerOrigin"], json!("SpreadOrigin"));
        assert_eq!(info["info"]["zeroPoint"], json!([0.0, 0.0]));
        assert!(command(&json!({"cmd": "pageItems", "page": 9}), &d, &layout, &colors, &index).is_err());
    }

    #[test]
    fn curves_bound_at_their_extremes_not_their_handles() {
        // a circle of radius 10 around (0, 0) from four arcs
        let k = 10.0 * folio_model::geom::KAPPA;
        let pt = |x: f64, y: f64, lx: f64, ly: f64, rx: f64, ry: f64| PathPoint { anchor: Point::new(x, y), left: Point::new(lx, ly), right: Point::new(rx, ry) };
        let circle = vec![SubPath {
            points: vec![pt(0.0, -10.0, -k, -10.0, k, -10.0), pt(10.0, 0.0, 10.0, -k, 10.0, k), pt(0.0, 10.0, k, 10.0, -k, 10.0), pt(-10.0, 0.0, -10.0, k, -10.0, -k)],
            open: false,
        }];
        // rotated 45°: handles would reach 10·(1 + κ)/√2 ≈ 10.97; the circle stays at 10
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let b = paths_bounds(&circle, &Affine::new(c, c, -c, c, 0.0, 0.0)).unwrap();
        assert!((b.right - 10.0).abs() < 0.01 && (b.top + 10.0).abs() < 0.01, "{b:?}");
        // four anchors, two points inside each of the four arcs
        let o = outline(&circle, &Affine::IDENTITY, Some(&b)).unwrap();
        assert_eq!(o[0].len(), 12);
    }
}
