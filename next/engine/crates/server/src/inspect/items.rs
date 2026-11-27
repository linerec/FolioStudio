//! Describe one page item, its paints and text options, and its descendants in draw order.

use super::geometry::{Outline, affine6, geometry_bounds, outline, paths_bounds, rect4, union};
use super::swatches::gradient_middle;
use super::{Index, r3, round};
use folio_color::{BlendSpace, Colors};
use folio_model::{Affine, Document, Graphic, ItemKind, LineCap, LineJoin, PageItem, Paint, Point, Rect, StrokeAlignment};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Most path points listed for an item: more is detailed art, which is not edited point by point.
const POINTS_MAX: usize = 300;

#[derive(Serialize)]
struct PathInfo {
    open: bool,
    /// `[anchor x, y, left x, y, right x, y]`.
    points: Vec<[f64; 6]>,
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
    let ctx = Items::new(doc, colors, index, HashSet::new(), false);
    let mut v = serde_json::to_value(ctx.paint(&paint)).unwrap_or(Value::Null);
    v["id"] = json!(id);
    v
}

#[derive(Serialize)]
pub(super) struct ItemInfo<'a> {
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

pub(super) struct Items<'a> {
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
    pub(super) fn new(doc: &'a Document, colors: &'a Colors, index: &'a Index, hidden: HashSet<&'a str>, master: bool) -> Self {
        Self { doc, colors, index, hidden, master, rgb: Default::default() }
    }

    pub(super) fn root_item(&self, item: &'a PageItem, to_page: &Affine, z: &mut usize) -> ItemInfo<'a> {
        let inherit = Inherit { layer: None, visible: true, locked: false };
        self.item(item, to_page, inherit, z).0
    }

    /// Layer rank, as the renderer sorts.
    fn rank(&self, item: &PageItem) -> usize {
        item.layer.as_deref().and_then(|l| self.index.layers.get(l)).map_or(0, |l| l.order)
    }

    /// Members in draw order (stably sorted by layer), without the hidden ones and the
    /// vector art of a placed graphic (shown as the frame's `content`).
    pub(super) fn ordered(&self, items: &'a [PageItem]) -> Vec<&'a PageItem> {
        let mut v: Vec<&PageItem> = items.iter().filter(|i| !self.hidden.contains(i.id.as_str()) && !i.print.placed_art).collect();
        if v.iter().any(|i| i.layer.is_some()) {
            v.sort_by_cached_key(|i| self.rank(i));
        }
        v
    }

    /// Pasted items in layer order, then anchored items in layout order.
    fn attachments(&self, item: &'a PageItem) -> impl Iterator<Item = &'a PageItem> {
        let anchors = self.index.anchors.get(&item.id).into_iter().flatten();
        self.ordered(&item.clipped).into_iter().chain(anchors)
    }

    /// Items listed for this item: itself and its members.
    pub(super) fn count(&self, item: &'a PageItem) -> usize {
        let members = match &item.kind {
            ItemKind::Group { children } => self.ordered(children),
            _ => Vec::new(),
        };
        1 + members.into_iter().chain(self.attachments(item)).map(|c| self.count(c)).sum::<usize>()
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
        for c in self.attachments(item) {
            children.push(self.item(c, &t, mine, z).0);
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
