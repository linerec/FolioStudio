//! Spreads, pages and page items (Spreads/*.xml, MasterSpreads/*.xml and anchored objects).
//!
//! Geometry stays as IDML stores it: an item's `transform` is its `ItemTransform` (inner →
//! parent), `paths` are `PathGeometry` points in inner coordinates. Fill, stroke, text frame
//! options, wrap and opacity come from the item, falling back to its object style chain.

mod appearance;
mod graphics;
mod spreads;
mod text_frame;
mod transparency;

pub use graphics::load_previews;
pub use spreads::{place_guides, ui_color};

use crate::color::Colors;
use crate::props::{is_none_ref, nums};
use crate::strokes::StrokeStyles;
use crate::styles::{Cat, ObjectProps, StyleSheet};
use crate::warn::Warnings;
use crate::xml::Element;
use folio_model::{Affine, ItemKind, PageItem, PathPoint, Point, Rect, SubPath};
use std::collections::HashMap;

/// Deepest group / paste-into nesting followed.
const MAX_ITEM_DEPTH: usize = 64;

/// Page item elements (anything else inside a spread or frame is a setting).
const ITEM_ELEMENTS: &[&str] = &[
    "Rectangle",
    "Oval",
    "Polygon",
    "GraphicLine",
    "TextFrame",
    "EndnoteTextFrame",
    "Group",
    "EPSText",
    "Button",
    "MultiStateObject",
    "CheckBox",
    "ComboBox",
    "ListBox",
    "RadioButton",
    "TextBox",
    "SignatureField",
    "HtmlItem",
    "Movie",
    "Sound",
];

/// Placed content of a graphic frame.
const GRAPHIC_ELEMENTS: &[&str] = &["Image", "EPS", "PDF", "ImportedPage", "WMF", "PICT", "SVG"];

/// Settings elements read through properties or knowingly ignored.
const SETTINGS_ELEMENTS: &[&str] = &[
    "Properties",
    "TextWrapPreference",
    "ObjectExportOption",
    "InCopyExportOption",
    "FrameFittingOption",
    "TextFramePreference",
    "TransparencySetting",
    "StrokeTransparencySetting",
    "FillTransparencySetting",
    "ContentTransparencySetting",
    "AnchoredObjectSetting",
    "BaselineFrameGridOption",
    "TextFrameFootnoteOptionsObject",
    "StoryPreference",
    "Label",
    "ContourOption",
    "MetadataPacketPreference",
    "ColumnRule",
    "AutoSizingOption",
    "MathObject",
    // content links of the Content Placer (kept as written, read by folio-edit `contentlink`)
    "LinkedPageItemOption",
    "Link",
];

pub fn is_item(name: &str) -> bool {
    ITEM_ELEMENTS.contains(&name)
}

/// Graphic id → absolute preview file (from the INDD reader's result.json).
pub type Previews = HashMap<String, String>;

pub struct ItemReader<'a, 's> {
    pub text_path_frames: &'s crate::textpath::FrameIds,
    pub sheet: &'s StyleSheet<'a>,
    pub colors: &'s Colors,
    pub previews: &'s Previews,
    /// The document's own stroke styles (Graphic.xml).
    pub strokes: &'s StrokeStyles,
    /// `Link` Self → where its file is on this machine (`.folio` references).
    pub links: &'s HashMap<String, crate::LinkOverride>,
    /// Graphic Self → its extracted embedded file (`.folio` assets).
    pub embedded: &'s HashMap<String, std::path::PathBuf>,
    /// The document's global light: angle and altitude, degrees (`TransparencyPreference`).
    pub global_light: (f64, f64),
}

pub fn affine(v: Option<&str>, w: &mut Warnings) -> Affine {
    let Some(s) = v else { return Affine::IDENTITY };
    match nums(s) {
        Some(n) if n.len() == 6 => Affine::new(n[0], n[1], n[2], n[3], n[4], n[5]),
        _ => {
            w.add(format!("unreadable transform {s:?} (identity used)"));
            Affine::IDENTITY
        }
    }
}

/// `"x y"` without allocating.
pub(crate) fn point(s: Option<&str>) -> Option<Point> {
    let mut it = s?.split_ascii_whitespace();
    let x = crate::props::num(it.next()?)?;
    let y = crate::props::num(it.next()?)?;
    it.next().is_none().then_some(Point::new(x, y))
}

fn bounds4(s: Option<&str>) -> Option<Rect> {
    let n = nums(s?)?;
    (n.len() == 4).then(|| Rect::from_idml_bounds([n[0], n[1], n[2], n[3]]))
}

/// The `GeometryPathType` subpaths of a `PathGeometry` (or `ClippingPathGeometry`) element.
fn geometry_paths(geom: &Element<'_>, w: &mut Warnings) -> Vec<SubPath> {
    let mut out = Vec::new();
    for gp in geom.children_named("GeometryPathType") {
        let arr = gp.child("PathPointArray");
        let mut sp = SubPath { points: Vec::with_capacity(arr.map_or(0, |a| a.children.len())), open: gp.attr("PathOpen") == Some("true") };
        if let Some(arr) = arr {
            for pt in arr.children_named("PathPointType") {
                let a = pt.attr("Anchor");
                let Some(anchor) = point(a) else {
                    w.add("path point without a readable Anchor skipped");
                    continue;
                };
                let handle = |k: &str| match pt.attr(k) {
                    h if h == a => anchor,
                    h => point(h).unwrap_or(anchor),
                };
                sp.points.push(PathPoint { anchor, left: handle("LeftDirection"), right: handle("RightDirection") });
            }
        }
        out.push(sp);
    }
    out
}

/// `PathGeometry` subpaths, or a rectangle/ellipse from `GeometricBounds` (older IDML).
pub(crate) fn paths(e: &Element<'_>, oval: bool, w: &mut Warnings) -> Vec<SubPath> {
    let mut out = Vec::new();
    if let Some(geom) = e.child("Properties").and_then(|p| p.child("PathGeometry")) {
        for gp in geom.children_named("GeometryPathType") {
            let arr = gp.child("PathPointArray");
            let mut sp = SubPath {
                points: Vec::with_capacity(arr.map_or(0, |a| a.children.len())),
                open: gp.attr("PathOpen") == Some("true"),
            };
            if let Some(arr) = arr {
                for pt in arr.children_named("PathPointType") {
                    let a = pt.attr("Anchor");
                    let Some(anchor) = point(a) else {
                        w.add("path point without a readable Anchor skipped");
                        continue;
                    };
                    // Straight segments repeat the anchor text in their handles.
                    let handle = |k: &str| match pt.attr(k) {
                        h if h == a => anchor,
                        h => point(h).unwrap_or(anchor),
                    };
                    sp.points.push(PathPoint { anchor, left: handle("LeftDirection"), right: handle("RightDirection") });
                }
            }
            out.push(sp);
        }
        return out;
    }
    if let Some(r) = bounds4(e.attr("GeometricBounds")) {
        let p = |x: f64, y: f64| Point::new(x, y);
        if oval {
            // Four-arc ellipse; κ = 0.5523 places the handles.
            let (cx, cy) = ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);
            let (rx, ry) = (r.width() / 2.0 * 0.552_284_75, r.height() / 2.0 * 0.552_284_75);
            let pts = [
                (p(cx, r.top), p(cx + rx, r.top), p(cx - rx, r.top)),
                (p(r.left, cy), p(r.left, cy - ry), p(r.left, cy + ry)),
                (p(cx, r.bottom), p(cx - rx, r.bottom), p(cx + rx, r.bottom)),
                (p(r.right, cy), p(r.right, cy + ry), p(r.right, cy - ry)),
            ];
            out.push(SubPath {
                points: pts.iter().map(|&(anchor, left, right)| PathPoint { anchor, left, right }).collect(),
                open: false,
            });
        } else {
            let corners = [p(r.left, r.top), p(r.left, r.bottom), p(r.right, r.bottom), p(r.right, r.top)];
            out.push(SubPath {
                points: corners.iter().map(|&a| PathPoint { anchor: a, left: a, right: a }).collect(),
                open: e.name == "GraphicLine",
            });
        }
    }
    out
}

/// `LocalDisplaySetting` (Object ▸ Display Performance); a value this version does not know reads as the view's.
fn local_display(e: &Element<'_>) -> folio_model::LocalDisplay {
    e.attr("LocalDisplaySetting").and_then(folio_model::LocalDisplay::from_idml).unwrap_or_default()
}

impl<'a> ItemReader<'a, '_> {
    /// Reads a page item element (see [`is_item`]).
    pub fn item(&self, e: &'a Element<'a>, w: &mut Warnings, depth: usize) -> PageItem {
        let o = ObjectProps {
            item: e,
            sheet: self.sheet,
            chain: self.sheet.object_chain(e.attr("AppliedObjectStyle"), w),
        };
        let mut item = PageItem {
            id: e.attr("Self").unwrap_or_default().to_owned(),
            transform: affine(e.attr("ItemTransform"), w),
            visible: e.attr("Visible") != Some("false"),
            layer: e.attr("ItemLayer").filter(|l| !is_none_ref(l)).map(str::to_owned),
            name: e.attr("Name").filter(|n| !n.is_empty() && !n.starts_with("$ID/")).map(str::to_owned),
            locked: e.attr("Locked") == Some("true"),
            overrides_off: e.attr("AllowOverrides") == Some("false"),
            local_display: local_display(e),
            fitting: crate::fitting::read(&o),
            object_style: e.attr("AppliedObjectStyle").filter(|s| !s.is_empty() && !is_none_ref(s)).map(str::to_owned),
            math: crate::mathml::read(e, w),
            ..PageItem::default()
        };
        if depth > MAX_ITEM_DEPTH {
            w.add(format!("page item nesting deeper than {MAX_ITEM_DEPTH}; {} kept as Other", e.name));
            item.kind = ItemKind::Other { element: e.name.to_owned() };
            return item;
        }
        let is_group = e.name == "Group";
        if !is_group {
            item.paths = paths(e, e.name == "Oval", w);
            self.paint(&o, &mut item, w);
            item.print.overprint_fill = o.bool(&[], "OverprintFill", Cat::Fill).unwrap_or(false);
            item.print.overprint_stroke = o.bool(&[], "OverprintStroke", Cat::Stroke).unwrap_or(false);
        }
        // Object styles carry Nonprinting in their "Text Wrap & Other" category.
        item.print.nonprinting = o.bool(&[], "Nonprinting", Cat::Wrap).unwrap_or(false);
        self.transparency(&o, &mut item, w);
        self.wrap(&o, &mut item);

        item.kind = match e.name {
            "Group" => {
                let children = e
                    .elements()
                    .filter(|c| is_item(c.name))
                    .map(|c| self.item(c, w, depth + 1))
                    .collect();
                self.unknown_children(e, w);
                ItemKind::Group { children }
            }
            "TextFrame" | "EndnoteTextFrame" => {
                // an EndnoteTextFrame is a text frame whose story is the endnote story; the element name stays in the saved XML
                self.unknown_children(e, w);
                ItemKind::TextFrame(self.text_frame(&o, w))
            }
            "Rectangle" | "Oval" | "Polygon" | "GraphicLine" => {
                let mut graphics = e.elements().filter(|c| GRAPHIC_ELEMENTS.contains(&c.name));
                let mut content = graphics.next().map(|g| (g, self.graphic(g, w)));
                let extra = graphics.count();
                if extra > 0 {
                    w.add_n("frame with more than one graphic: extra graphics dropped", extra);
                }
                // A QR code InDesign generated is drawn from its modules, not as a placed EPS.
                let qr = content.as_ref().and_then(|(g, graphic)| self.qr_code(g, graphic, w));
                if qr.is_some() {
                    content = None;
                }
                let content = content.map(|(_, graphic)| graphic);
                item.clipped = qr
                    .into_iter()
                    .chain(e.elements().filter(|c| is_item(c.name)).map(|c| self.item(c, w, depth + 1)))
                    .collect();
                self.unknown_children(e, w);
                match e.name {
                    "Rectangle" => ItemKind::Rectangle { content },
                    "Oval" => ItemKind::Oval { content },
                    "Polygon" => ItemKind::Polygon { content },
                    _ => {
                        if content.is_some() {
                            w.add("graphic inside a GraphicLine dropped");
                        }
                        ItemKind::GraphicLine
                    }
                }
            }
            other => {
                w.add(format!("page item {other} not modelled (kept as Other)"));
                // Movies and sounds keep their file link on the item itself.
                item.print.media_link = e.child("Link").map(|l| self.link(l));
                if matches!(other, "Movie" | "Sound") {
                    item.print.poster = e.child("Image").map(|g| Box::new(self.graphic(g, w)));
                }
                ItemKind::Other { element: other.to_owned() }
            }
        };
        item.text_path = crate::textpath::read(e, &item.paths, w);
        if let Some(path) = &mut item.text_path {
            path.frame.previous = self.text_path_frames.resolve(path.frame.previous.as_deref());
            path.frame.next = self.text_path_frames.resolve(path.frame.next.as_deref());
        }
        item
    }

    /// The anchored object settings of an item anchored in text (`anchor_settings`).
    pub fn anchor(&self, e: &'a Element<'a>, w: &mut Warnings) -> Option<folio_model::AnchorSettings> {
        let o = ObjectProps { item: e, sheet: self.sheet, chain: self.sheet.object_chain(e.attr("AppliedObjectStyle"), w) };
        crate::anchored::read(&o)
    }

    fn unknown_children(&self, e: &Element<'_>, w: &mut Warnings) {
        for c in e.elements() {
            let n = c.name;
            if n != "TextPath" && !is_item(n) && !GRAPHIC_ELEMENTS.contains(&n) && !SETTINGS_ELEMENTS.contains(&n) {
                w.add(format!("element {n} inside {} not read", e.name));
            }
        }
    }
}
