//! New page items (oracle basic-create / basic-create-defaults, create-*).
//! Shapes use the document's graphic style plus PageItemDefault overrides. Placeholder
//! frames use [None], text frames use the text style. New items stack atop their layer.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

mod text_preferences;
use text_preferences::text_frame_preferences;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Rectangle,
    Ellipse,
    Polygon,
    Line,
    RectangleFrame,
    EllipseFrame,
    PolygonFrame,
    TextFrame,
}
pub(super) fn six() -> u32 {
    6
}
pub(crate) fn polygon(w: &mut Work<'_>, page: &str, kind: &str, rect: &[f64; 4], layer: &str, sides: &u32, inset: &f64) -> Result<(), EditError> {
    let kind = match kind {
        "polygon" => ItemKind::Polygon,
        "polygonFrame" => ItemKind::PolygonFrame,
        _ => return Err(EditError::new("invalid", "expected polygon or polygonFrame")),
    };
    run_in(w, page, kind, *rect, (!layer.is_empty()).then_some(layer), *sides, Some(*inset), Frame::Page, None)
}
/// `createInView`: any shape, its rect read as the page is seen in the view. A spread turned by View ▸ Rotate Spread shows its
/// pages turned: the rect is measured from the top-left corner of the page as shown (a drag on the screen, a script's geometric
/// bounds), and the shape is stored in the spread's own space (rules/drawtools.toml, drawtools.turned.*). On an unturned page it
/// is `createItem` / `createPolygon`.
pub(crate) fn in_view(w: &mut Work<'_>, page: &str, kind: &str, rect: &[f64; 4], layer: &str, sides: &u32, inset: &f64) -> Result<(), EditError> {
    let kind: ItemKind = serde_json::from_value(serde_json::Value::String(kind.into()))
        .map_err(|_| EditError::new("invalid", format!("{kind} is not a shape to draw")))?;
    let star = matches!(kind, ItemKind::Polygon | ItemKind::PolygonFrame).then_some(*inset);
    run_in(w, page, kind, *rect, (!layer.is_empty()).then_some(layer), *sides, star, Frame::View, None)
}
/// `createByDrag`: a shape drawn with the mouse (or by typed dimensions) in the view. On an unturned page it is `createInView`; on a spread turned by
/// View ▸ Rotate Spread InDesign stores what the mouse draws upright in the view: the item carries the inverse of the spread's turn as its
/// ItemTransform (no translation) and the shape is made, in the standard path order and without mirroring, in the turned box; a placeholder frame keeps
/// the gradient handles at zero (liveinput-c2: 71 real drags, next/oracle/measured/turned-draw-2025.json). A text frame and a line are stored as
/// `createInView` stores them (the mouse was not measured for them).
pub(crate) fn by_drag(w: &mut Work<'_>, page: &str, kind: &str, rect: &[f64; 4], layer: &str, sides: &u32, inset: &f64) -> Result<(), EditError> {
    let kind: ItemKind = serde_json::from_value(serde_json::Value::String(kind.into()))
        .map_err(|_| EditError::new("invalid", format!("{kind} is not a shape to draw")))?;
    let star = matches!(kind, ItemKind::Polygon | ItemKind::PolygonFrame).then_some(*inset);
    let frame = if matches!(kind, ItemKind::Line | ItemKind::TextFrame) { Frame::View } else { Frame::Drag };
    run_in(w, page, kind, *rect, (!layer.is_empty()).then_some(layer), *sides, star, frame, None)
}
/// How a new item's rect is read: in the spread's own space (what an unturned page shows), or as the view shows the page (`Drag`: and the item is
/// stored as the mouse makes it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Frame {
    Page,
    View,
    Drag,
}
/// The scale of a transform whose linear part is a turn by some angle (with a uniform scale of 1), if it is nothing else.
fn turn_by_any_angle(m: &Mat) -> Option<f64> {
    let [a, b, c, d, _, _] = m.0;
    let s = a.hypot(b);
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * s.max(1.);
    (s > 1e-9 && near(a, d) && near(b, -c) && (s - 1.).abs() <= 1e-9).then_some(s)
}
/// A page's transform whose linear part is a turn by some angle and nothing else, its numbers as an IDML written elsewhere rounds them (six
/// decimals: `0.866025 0.5 -0.5 0.866025` for 30 degrees).
fn turned_page(m: &Mat) -> bool {
    let [a, b, c, d, _, _] = m.0;
    let near = |x: f64, y: f64| (x - y).abs() <= 2e-6;
    near(a, d) && near(b, -c) && near(a.hypot(b), 1.)
}
/// Quarter turn (0, 90, 180, 270) and scale of a transform's linear part, if it is nothing else.
fn quarter(m: &Mat) -> Option<(u32, f64)> {
    let [a, b, c, d, _, _] = m.0;
    let s = a.hypot(b);
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * s.max(1.);
    if !(s > 1e-9 && near(a, d) && near(b, -c)) {
        return None;
    }
    let turn = if near(b, 0.) { if a > 0. { 0 } else { 180 } } else if near(a, 0.) { if b > 0. { 90 } else { 270 } } else { return None };
    Some((turn, s))
}
impl ItemKind {
    fn tag(self) -> &'static str {
        match self {
            Self::Rectangle | Self::RectangleFrame => "Rectangle",
            Self::Ellipse | Self::EllipseFrame => "Oval",
            Self::Polygon | Self::PolygonFrame => "Polygon",
            Self::Line => "GraphicLine",
            Self::TextFrame => "TextFrame",
        }
    }
    fn placeholder(self) -> bool {
        matches!(
            self,
            Self::RectangleFrame | Self::EllipseFrame | Self::PolygonFrame
        )
    }
}
type Values = BTreeMap<String, String>;
fn attributes(ix: &Index, t: &str, i: usize) -> Values {
    ix.attrs(t, i)
        .iter()
        .map(|a| {
            (
                t[a.name.0..a.name.1].into(),
                crate::xml::unescape(&t[a.value.0..a.value.1]),
            )
        })
        .collect()
}
fn properties(ix: &Index, t: &str, i: usize) -> Values {
    let mut out = Values::new();
    for p in ix.named(t, i, "Properties") {
        for &k in ix.children(p) {
            let n = &ix.nodes[k];
            out.insert(ix.local(t, k).into(), t[n.start..n.end].into());
        }
    }
    out
}
fn find_tag(ix: &Index, t: &str, tag: &str) -> Result<usize, EditError> {
    (0..ix.nodes.len())
        .find(|&i| ix.local(t, i) == tag)
        .ok_or_else(|| EditError::new("xml", format!("missing {tag}")))
}
fn resolved(ix: &Index, t: &str, id: &str) -> Result<(Values, Values), EditError> {
    resolved_child(ix, t, id, None)
}
fn resolved_child(
    ix: &Index,
    t: &str,
    id: &str,
    child: Option<&str>,
) -> Result<(Values, Values), EditError> {
    let mut chain = Vec::new();
    let mut name = id.to_owned();
    let mut seen = BTreeSet::new();
    while name != "n" && !name.is_empty() {
        if !seen.insert(name.clone()) {
            return Err(EditError::new("xml", "cyclic style inheritance"));
        }
        let i = (0..ix.nodes.len())
            .find(|&i| ix.attr(t, i, "Self") == Some(&name))
            .ok_or_else(|| EditError::new("unsupported", format!("missing style {name}")))?;
        chain.push(i);
        name = ix
            .named(t, i, "Properties")
            .iter()
            .flat_map(|&p| ix.named(t, p, "BasedOn"))
            .next()
            .map(|k| crate::xml::unescape(t[ix.nodes[k].open_end..ix.nodes[k].close_start].trim()))
            .unwrap_or_default();
        // IDML BasedOn omits the style-family prefix (basic-create, create-rectangle).
        if !name.is_empty() && name != "n" && !name.starts_with(&format!("{}/", ix.local(t, i))) {
            name = format!("{}/{name}", ix.local(t, i));
        }
    }
    let (mut attrs, mut props) = (Values::new(), Values::new());
    for i in chain.into_iter().rev() {
        let i = match child {
            Some(tag) => match ix.named(t, i, tag).first().copied() {
                Some(child) => child,
                None => continue,
            },
            None => i,
        };
        attrs.extend(attributes(ix, t, i));
        props.extend(properties(ix, t, i));
    }
    Ok((attrs, props))
}
fn attrs_text(values: &Values) -> String {
    values
        .iter()
        .map(|(k, v)| format!(" {k}=\"{}\"", escape(v)))
        .collect()
}
fn raw(ix: &Index, t: &str, i: usize) -> String {
    t[ix.nodes[i].start..ix.nodes[i].end].to_owned()
}
fn normalized(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn run(
    w: &mut Work<'_>,
    page: &str,
    kind: ItemKind,
    rect: [f64; 4],
    layer: Option<&str>,
    sides: u32,
    custom: Option<&[PathPoint]>,
) -> Result<(), EditError> {
    run_in(w, page, kind, rect, layer, sides, None, Frame::Page, custom)
}

/// `star`: the inset of a polygon or star (the points are made here, from the box the shape ends up in).
// Shape creation takes the existing independent geometry, style and placement inputs.
#[allow(clippy::too_many_arguments)]
fn run_in(
    w: &mut Work<'_>,
    page: &str,
    kind: ItemKind,
    rect: [f64; 4],
    layer: Option<&str>,
    sides: u32,
    star: Option<f64>,
    frame: Frame,
    custom: Option<&[PathPoint]>,
) -> Result<(), EditError> {
    if rect.iter().any(|v| !v.is_finite())
        || (kind != ItemKind::Line && (rect[2] <= rect[0] || rect[3] <= rect[1]))
        || (kind == ItemKind::Line && rect[0] == rect[2] && rect[1] == rect[3])
        || !(3..=100).contains(&sides)
    {
        return Err(EditError::new("invalid", "invalid new item geometry"));
    }
    let (part, ix, pi) = w.find(page)?;
    let t = w.text(&part)?.to_owned();
    // a page of the document or of a master spread (drawing in a master opened in the editor)
    if ix.local(&t, pi) != "Page" || !(part.starts_with("Spreads/") || part.starts_with("MasterSpreads/")) {
        return Err(EditError::new(
            "invalid",
            "destination is not a document page",
        ));
    }
    let spread = ix.nodes[pi]
        .parent
        .ok_or_else(|| EditError::new("xml", "page without spread"))?;
    let pm = item_transform(&ix, &t, pi);
    // The transforms drawing understands: a page that is not turned or scaled, and (when the rect is read in the view) a spread
    // with a quarter turn and a scale (View ▸ Rotate Spread; a scale only comes with an IDML written elsewhere). A page turned by its own
    // transform (an IDML written elsewhere) is drawn on from its bounding-box corner (below); InDesign prints such a page turned, Folio's PDF
    // keeps its pages upright (rules/drawtools.toml, drawtools.turned.other). Read in the spread's own space, the spread's transform does not matter.
    let sm = item_transform(&ix, &t, spread);
    let unsupported = || EditError::new("unsupported", "drawing on a spread turned other than by quarter turns, or on a turned or scaled page, has not been measured");
    // A path given by its points (the Pen, the Pencil: `custom`) on a page turned or scaled by its own transform is made too: the points are
    // page points as Folio shows the page (its bounding box on the spread, the objects as they lie there: server pageItems), so they go to the
    // spread from that box's top-left corner (`origin` below) whatever the page's transform (pen-c2; rules/limits.toml draw-tools-rotated).
    // A shape (the drawing tools, typed dimensions) on a page turned by any angle on an unturned spread is made the same way (pen-c3): measured from
    // the page's bounding-box corner and upright on the spread, as InDesign's own script bounds make it on such a page (drawtools.turned.other)
    // and as the mouse draws there (the view is not turned, only the page).
    let unturned_spread = matches!(quarter(&sm), Some((0, ss)) if (ss - 1.).abs() <= 1e-9);
    if !matches!(quarter(&pm), Some((0, ps)) if (ps - 1.).abs() <= 1e-9)
        && !(frame == Frame::Page && custom.is_some())
        && !(turned_page(&pm) && unturned_spread)
    {
        return Err(unsupported());
    }
    // a shape the mouse draws also stands upright on a spread turned by any angle (an IDML written elsewhere: measured at 30 degrees); `1` stands for such a turn
    let (spread_turn, scale) = match frame {
        Frame::Page => (0, 1.),
        Frame::View => quarter(&sm).ok_or_else(unsupported)?,
        Frame::Drag => quarter(&sm).or_else(|| turn_by_any_angle(&sm).map(|s| (1, s))).ok_or_else(unsupported)?,
    };
    let page_bounds = ix
        .attr(&t, pi, "GeometricBounds")
        .unwrap_or("0 0 0 0")
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| EditError::new("xml", "invalid page bounds"))?;
    if page_bounds.len() != 4 {
        return Err(EditError::new("xml", "invalid page bounds"));
    }
    // A placeholder frame on a turned or scaled spread gets gradient handles InDesign works out in the view (angles of -90, -180,
    // -270 and lengths that are not the frame's; drawtools.turned.frames): not reproduced.
    if kind.placeholder() && (spread_turn, scale) != (0, 1.) && frame != Frame::Drag {
        return Err(EditError::new("unsupported", "a placeholder frame on a turned or scaled page or spread has not been measured"));
    }
    // the page's top-left corner in the spread's own space: of the page as it lies there (a turned page shows its bounding box)
    let corners = [(page_bounds[1], page_bounds[0]), (page_bounds[3], page_bounds[0]), (page_bounds[3], page_bounds[2]), (page_bounds[1], page_bounds[2])]
        .map(|c| pm.apply(c));
    let origin = corners.iter().fold((f64::INFINITY, f64::INFINITY), |o, c| (o.0.min(c.0), o.1.min(c.1)));
    // The view: the spread's linear part shows its own space turned and scaled. A rect read in the view is measured from the
    // top-left of the page's bounding box there and comes back through the inverse into the spread's own space (the sides swap
    // for a quarter turn). `home` is where InDesign puts the item's origin: the middle of its default 10 point box at that corner.
    let mut rect = rect;
    let mut home = (origin.0 + 5., origin.1 + 5.);
    let mut mirror = (false, false);
    // a shape the mouse drew on a turned spread: the item stands upright in the view (ItemTransform: the spread's linear part inverted, no translation)
    let mut upright: Option<(Mat, [f64; 4])> = None;
    if frame != Frame::Page && (spread_turn, scale) != (0, 1.) {
        let lin = Mat([sm.0[0], sm.0[1], sm.0[2], sm.0[3], 0., 0.]);
        let inv = lin.invert().ok_or_else(|| EditError::new("xml", "singular spread transform"))?;
        let shown = corners.map(|c| lin.apply(c));
        let corner = shown.iter().fold((f64::INFINITY, f64::INFINITY), |o, c| (o.0.min(c.0), o.1.min(c.1)));
        let back = |p: (f64, f64)| inv.apply((corner.0 + p.0, corner.1 + p.1));
        let view_box = [corner.0 + rect[0], corner.1 + rect[1], corner.0 + rect[2], corner.1 + rect[3]];
        let (a, b) = (back((rect[0], rect[1])), back((rect[2], rect[3])));
        rect = if kind == ItemKind::Line {
            [a.0 - origin.0, a.1 - origin.1, b.0 - origin.0, b.1 - origin.1]
        } else {
            [a.0.min(b.0) - origin.0, a.1.min(b.1) - origin.1, a.0.max(b.0) - origin.0, a.1.max(b.1) - origin.1]
        };
        home = back((5., 5.));
        // InDesign stores a shape drawn on a quarter-turned spread mirrored in the spread's own space: a turn of 90 flips it top to
        // bottom, 270 left to right, 180 both (drawtools.turned.mirror). A line keeps the two points as given.
        mirror = match spread_turn { 90 => (false, true), 180 => (true, true), 270 => (true, false), _ => (false, false) };
        if matches!(kind, ItemKind::Line | ItemKind::TextFrame) { mirror = (false, false); }
        if frame == Frame::Drag { mirror = (false, false); upright = Some((inv, view_box)); }
    }
    let dm = w.text("designmap.xml")?.to_owned();
    let dx = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let di = find_tag(&dx, &dm, "Document")?;
    let layer = layer
        .or_else(|| dx.attr(&dm, di, "ActiveLayer"))
        .ok_or_else(|| EditError::new("xml", "no active layer"))?;
    let li = (0..dx.nodes.len())
        .find(|&i| dx.local(&dm, i) == "Layer" && dx.attr(&dm, i, "Self") == Some(layer))
        .ok_or_else(|| EditError::new("notFound", "layer not found"))?;
    if dx.attr(&dm, li, "Locked") == Some("true") || dx.attr(&dm, li, "Visible") == Some("false") {
        return Err(EditError::new(
            "locked",
            "cannot draw on a locked or hidden layer",
        ));
    }
    let prefs = w.text("Resources/Preferences.xml")?.to_owned();
    let px = Index::scan(&prefs).map_err(|e| EditError::new("xml", e))?;
    let defaults = attributes(&px, &prefs, find_tag(&px, &prefs, "PageItemDefault")?);
    let styles = w.text("Resources/Styles.xml")?.to_owned();
    let sx = Index::scan(&styles).map_err(|e| EditError::new("xml", e))?;
    let style = if kind.placeholder() {
        "ObjectStyle/$ID/[None]"
    } else {
        defaults
            .get(if kind == ItemKind::TextFrame {
                "AppliedTextObjectStyle"
            } else {
                "AppliedGraphicObjectStyle"
            })
            .map(String::as_str)
            .ok_or_else(|| EditError::new("xml", "missing default object style"))?
    };
    let (style_attrs, _) = resolved(&sx, &styles, style)?;
    let mut a = Values::new();
    for (k, v) in [
        (
            "ContentType",
            if kind.placeholder() {
                "GraphicType"
            } else if kind == ItemKind::TextFrame {
                "TextType"
            } else {
                "Unassigned"
            },
        ),
        ("OverriddenPageItemProps", ""),
        (
            "HorizontalLayoutConstraints",
            "FlexibleDimension FixedDimension FlexibleDimension",
        ),
        (
            "VerticalLayoutConstraints",
            "FlexibleDimension FixedDimension FlexibleDimension",
        ),
        ("Locked", "false"),
        ("LocalDisplaySetting", "Default"),
        ("Visible", "true"),
        ("Name", "$ID/"),
        ("GradientFillStart", "0 0"),
        ("GradientFillLength", "0"),
        ("GradientFillAngle", "0"),
        ("GradientStrokeStart", "0 0"),
        ("GradientStrokeLength", "0"),
        ("GradientStrokeAngle", "0"),
        ("GradientFillHiliteLength", "0"),
        ("GradientFillHiliteAngle", "0"),
        ("GradientStrokeHiliteLength", "0"),
        ("GradientStrokeHiliteAngle", "0"),
    ] {
        a.insert(k.into(), v.into());
    }
    // InDesign writes AllowOverrides on every item drawn in a parent (a page item of a document does not carry it)
    if part.starts_with("MasterSpreads/") {
        a.insert("AllowOverrides".into(), "true".into());
    }
    if kind == ItemKind::Line {
        a.insert("LockState".into(), "None".into());
    } else if kind != ItemKind::TextFrame {
        a.insert("StoryTitle".into(), "$ID/".into());
    }
    if !kind.placeholder() && kind != ItemKind::TextFrame {
        for (k, v) in &defaults {
            if k.starts_with("Applied") || k == "Self" {
                continue;
            }
            if k.starts_with("Overprint") && v == "false" && !style_attrs.contains_key(k) {
                continue;
            }
            if style_attrs.get(k) != Some(v) || a.contains_key(k) {
                a.insert(k.clone(), v.clone());
            }
        }
    }
    let id = w.new_id();
    a.insert("Self".into(), id.clone());
    a.insert("ItemLayer".into(), layer.into());
    a.insert("AppliedObjectStyle".into(), style.into());
    // add(geometricBounds) starts from InDesign's default 10pt box: its 5pt centre
    // remains in the transform and the requested dimensions change the path (create-*).
    let matrix = if let Some((inverse, _)) = upright {
        inverse
    } else if kind == ItemKind::Line {
        Mat::ID
    } else {
        Mat([1., 0., 0., 1., home.0, home.1])
    };
    a.insert("ItemTransform".into(), matrix.text());
    let mut local = if let Some((_, view_box)) = upright {
        // the box the mouse drew, in the view: the item's own (upright) box, the item carrying the turn's inverse
        view_box
    } else if kind == ItemKind::Line {
        [
            rect[0] + origin.0,
            rect[1] + origin.1,
            rect[2] + origin.0,
            rect[3] + origin.1,
        ]
    } else {
        [rect[0] + origin.0 - home.0, rect[1] + origin.1 - home.1, rect[2] + origin.0 - home.0, rect[3] + origin.1 - home.1]
    };
    // Explicit [None] resets the gradient handles too (create-*Frame); a frame the mouse drew keeps them at zero (liveinput-c2).
    if kind.placeholder() && upright.is_none() {
        for paint in ["Fill", "Stroke"] {
            a.insert(
                format!("Gradient{paint}Start"),
                geom::pair((local[0], local[3])),
            );
            a.insert(
                format!("Gradient{paint}Length"),
                geom::num(local[2] - local[0]),
            );
        }
    }
    let mut extra = String::new();
    if kind == ItemKind::TextFrame {
        let story = w.new_id();
        a.insert("ParentStory".into(), story.clone());
        a.insert("PreviousTextFrame".into(), "n".into());
        a.insert("NextTextFrame".into(), "n".into());
        let text = new_story(
            &px,
            &prefs,
            &sx,
            &styles,
            &story,
            dx.attr(&dm, di, "DOMVersion").unwrap_or("20.4"),
        )?;
        w.add_part(format!("Stories/Story_{story}.xml"), text)?;
        story_copy::register(w, &story)?;
        w.recompose(&story);
        let (base, _) = resolved_child(&sx, &styles, style, Some("TextFramePreference"))?;
        let (frame_prefs, width) = text_frame_preferences(
            &px,
            &prefs,
            &base,
            rect[2] - rect[0],
            false,
            false,
        )?;
        local[2] = local[0] + width;
        extra.push_str(&frame_prefs);
    }
    let wi = find_tag(&px, &prefs, "TextWrapPreference")?;
    let mut wrap = raw(&px, &prefs, wi);
    if let Some(&ci) = px.named(&prefs, wi, "ContourOption").first() {
        let n = &px.nodes[ci];
        wrap.replace_range(n.start - px.nodes[wi].start..n.end - px.nodes[wi].start, "");
    }
    extra.push_str(&wrap);
    if kind != ItemKind::TextFrame && kind != ItemKind::Line {
        extra.push_str(INCOPY);
        if !kind.placeholder() {
            extra.push_str(&raw(
                &px,
                &prefs,
                find_tag(&px, &prefs, "FrameFittingOption")?,
            ));
        }
    }
    extra.push_str(EXPORT);
    // a path of the Pen tool: the points as given on the page, in the item's own space like the box's (create-*)
    let star_points: Option<Vec<PathPoint>> = match star {
        Some(inset) => Some(
            folio_model::polygon::points(if upright.is_some() { local } else { rect }, sides, inset)
                .map_err(|e| EditError::new("invalid", e))?
                .into_iter()
                .map(|p| PathPoint { anchor: p, left: p, right: p })
                .collect(),
        ),
        None => None,
    };
    let mut points: Vec<PathPoint> = match custom.or(star_points.as_deref()) {
        Some(c) => {
            let shift = if kind == ItemKind::Line { origin } else if upright.is_some() && custom.is_none() { (0., 0.) } else { (origin.0 - home.0, origin.1 - home.1) };
            let at = |p: (f64, f64)| (p.0 + shift.0, p.1 + shift.1);
            c.iter().map(|p| PathPoint { anchor: at(p.anchor), left: at(p.left), right: at(p.right) }).collect()
        }
        None => path(kind, local, sides),
    };
    if mirror != (false, false) {
        // about the middle of the box the shape was made in
        let (cx, cy) = (local[0] + local[2], local[1] + local[3]);
        let flip = |p: (f64, f64)| (if mirror.0 { cx - p.0 } else { p.0 }, if mirror.1 { cy - p.1 } else { p.1 });
        for p in &mut points {
            *p = PathPoint { anchor: flip(p.anchor), left: flip(p.left), right: flip(p.right) };
        }
    }
    let geometry = points
        .iter()
        .map(|p| {
            format!(
                "<PathPointType Anchor=\"{}\" LeftDirection=\"{}\" RightDirection=\"{}\"/>",
                geom::pair(p.anchor),
                geom::pair(p.left),
                geom::pair(p.right)
            )
        })
        .collect::<String>();
    let tag = kind.tag();
    let xml = format!(
        "<{tag}{}><Properties><PathGeometry><GeometryPathType PathOpen=\"{}\"><PathPointArray>{geometry}</PathPointArray></GeometryPathType></PathGeometry></Properties>{extra}</{tag}>",
        attrs_text(&a),
        kind == ItemKind::Line
    );
    let siblings: Vec<_> = ix
        .children(spread)
        .iter()
        .copied()
        .filter(|&i| is_item(&ix, &t, i))
        .collect();
    let layers: Vec<_> = (0..dx.nodes.len())
        .filter(|&i| dx.local(&dm, i) == "Layer")
        .filter_map(|i| dx.attr(&dm, i, "Self"))
        .collect();
    let rank = |l: Option<&str>| layers.iter().position(|s| Some(*s) == l).unwrap_or(0);
    let at = siblings
        .iter()
        .rev()
        .find(|&&i| ix.attr(&t, i, "ItemLayer") == Some(layer))
        .map(|&i| ix.nodes[i].end)
        .or_else(|| {
            siblings
                .iter()
                .find(|&&i| rank(ix.attr(&t, i, "ItemLayer")) > rank(Some(layer)))
                .map(|&i| element_span(&ix, &t, i).start)
        })
        .unwrap_or(ix.nodes[spread].close_start);
    w.splice(&part, vec![(at..at, format!("\n\t\t{xml}"))])?;
    w.created.push(id);
    Ok(())
}

/// A new shape's path in the box (also Convert Shape's).
pub(super) fn shape_path(kind: ItemKind, b: [f64; 4], sides: u32) -> Vec<PathPoint> {
    path(kind, b, sides)
}

fn path(kind: ItemKind, [x0, y0, x1, y1]: [f64; 4], sides: u32) -> Vec<PathPoint> {
    let point = |p| PathPoint {
        anchor: p,
        left: p,
        right: p,
    };
    let (cx, cy, rx, ry) = (
        (x0 + x1) / 2.,
        (y0 + y1) / 2.,
        (x1 - x0) / 2.,
        (y1 - y0) / 2.,
    );
    match kind {
        ItemKind::Line => vec![point((x0, y0)), point((x1, y1))],
        ItemKind::Ellipse | ItemKind::EllipseFrame => {
            // InDesign's exported cubic ellipse (create-ellipse).
            let k = 0.5522847466666667;
            vec![
                PathPoint {
                    anchor: (cx, y1),
                    left: (cx - rx * k, y1),
                    right: (cx + rx * k, y1),
                },
                PathPoint {
                    anchor: (x1, cy),
                    left: (x1, cy + ry * k),
                    right: (x1, cy - ry * k),
                },
                PathPoint {
                    anchor: (cx, y0),
                    left: (cx + rx * k, y0),
                    right: (cx - rx * k, y0),
                },
                PathPoint {
                    anchor: (x0, cy),
                    left: (x0, cy - ry * k),
                    right: (x0, cy + ry * k),
                },
            ]
        }
        ItemKind::Polygon | ItemKind::PolygonFrame => {
            folio_model::polygon::points([x0, y0, x1, y1], sides, 0.)
                .expect("validated polygon geometry").into_iter().map(point).collect()
        }
        _ => vec![
            point((x0, y0)),
            point((x0, y1)),
            point((x1, y1)),
            point((x1, y0)),
        ],
    }
}
pub(crate) fn paragraph_attr(k: &str) -> bool {
    k.starts_with("Paragraph")
        || k.starts_with("Rule")
        || k.starts_with("Keep")
        || k.starts_with("Hyphen")
        || k.starts_with("DropCap")
        || k.starts_with("Bullets")
        || k.starts_with("Numbering")
        || matches!(
            k,
            "FirstLineIndent"
                | "LeftIndent"
                | "RightIndent"
                | "LastLineIndent"
                | "SpaceBefore"
                | "SpaceAfter"
                | "Justification"
                | "SingleWordJustification"
                | "AutoLeading"
                | "StartParagraph"
                | "Composer"
                | "MinimumWordSpacing"
                | "MaximumWordSpacing"
                | "DesiredWordSpacing"
                | "MinimumLetterSpacing"
                | "MaximumLetterSpacing"
                | "DesiredLetterSpacing"
                | "MinimumGlyphScaling"
                | "MaximumGlyphScaling"
                | "DesiredGlyphScaling"
                | "SpanColumnType"
                | "SpanSplitColumnCount"
                | "SplitColumnInsideGutter"
                | "SplitColumnOutsideGutter"
                | "SpanColumnMinSpaceBefore"
                | "SpanColumnMinSpaceAfter"
        )
}
/// Detached text chains use the same document insertion defaults (thread-unlink-defaults).
pub(super) fn empty_story(w: &Work<'_>, id: &str) -> Result<String, EditError> {
    let prefs=w.text("Resources/Preferences.xml")?;
    let styles=w.text("Resources/Styles.xml")?;
    let px=Index::scan(prefs).map_err(|e|EditError::new("xml",e))?;
    let sx=Index::scan(styles).map_err(|e|EditError::new("xml",e))?;
    let dm=w.text("designmap.xml")?;
    let dx=Index::scan(dm).map_err(|e|EditError::new("xml",e))?;
    let di=find_tag(&dx,dm,"Document")?;
    new_story(&px,prefs,&sx,styles,id,dx.attr(dm,di,"DOMVersion").unwrap_or("20.4"))
}

fn new_story(
    ix: &Index,
    t: &str,
    sx: &Index,
    styles: &str,
    id: &str,
    version: &str,
) -> Result<String, EditError> {
    let i = find_tag(ix, t, "TextDefault")?;
    let defaults = attributes(ix, t, i);
    let ps = defaults
        .get("AppliedParagraphStyle")
        .ok_or_else(|| EditError::new("xml", "missing default paragraph style"))?;
    let cs = defaults
        .get("AppliedCharacterStyle")
        .ok_or_else(|| EditError::new("xml", "missing default character style"))?;
    let (mut base, mut base_props) = resolved(sx, styles, ps)?;
    let (ca, cp) = resolved(sx, styles, cs)?;
    base.extend(ca);
    base_props.extend(cp);
    let (mut pa, mut chara) = (Values::new(), Values::new());
    pa.insert("AppliedParagraphStyle".into(), ps.clone());
    chara.insert("AppliedCharacterStyle".into(), cs.clone());
    for (k, v) in defaults {
        if matches!(
            k.as_str(),
            "Self" | "AppliedParagraphStyle" | "AppliedCharacterStyle" | "AppliedNamedGrid"
        ) || base.get(&k) == Some(&v)
        {
            continue;
        }
        if paragraph_attr(&k) {
            pa.insert(k, v);
        } else {
            chara.insert(k, v);
        }
    }
    let mut pprops = String::new();
    let mut cprops = String::new();
    // Story runs put Leading before AppliedFont; TextDefault uses the opposite order
    // (create-type-defaults). Keep the remaining source order instead of sorting names.
    let mut children: Vec<_> = ix
        .named(t, i, "Properties")
        .iter()
        .flat_map(|&p| ix.children(p))
        .copied()
        .collect();
    children.sort_by_key(|&child| ix.local(t, child) != "Leading");
    for child in children {
        let k = ix.local(t, child);
        let v = raw(ix, t, child);
        if base_props
            .get(k)
            .is_some_and(|b| normalized(b) == normalized(&v))
        {
            continue;
        }
        if paragraph_attr(k) {
            pprops.push_str(&v);
        } else {
            cprops.push_str(&v);
        }
    }
    let properties = |s: String| {
        if s.is_empty() {
            s
        } else {
            format!("<Properties>{s}</Properties>")
        }
    };
    let pref = raw(ix, t, find_tag(ix, t, "StoryPreference")?);
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><idPkg:Story xmlns:idPkg=\"http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging\" DOMVersion=\"{}\"><Story Self=\"{id}\" AppliedTOCStyle=\"n\" UserText=\"true\" IsEndnoteStory=\"false\" TrackChanges=\"false\" StoryTitle=\"$ID/\" AppliedNamedGrid=\"n\">{pref}{INCOPY}<ParagraphStyleRange{}>{}<CharacterStyleRange{}>{}</CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>",
        escape(version),
        attrs_text(&pa),
        properties(pprops),
        attrs_text(&chara),
        properties(cprops)
    ))
}
const INCOPY: &str =
    r#"<InCopyExportOption IncludeGraphicProxies="true" IncludeAllResources="false"/>"#;
// Default export metadata emitted on new items (create-*); no external assets are embedded.
pub(super) const EXPORT: &str = r#"<ObjectExportOption EpubType="$ID/" SizeType="DefaultSize" CustomSize="$ID/" PreserveAppearanceFromLayout="PreserveAppearanceDefault" AltTextSourceType="SourceXMLStructure" ActualTextSourceType="SourceXMLStructure" CustomAltText="$ID/" CustomActualText="$ID/" ApplyTagType="TagFromStructure" ImageConversionType="JPEG" ImageExportResolution="Ppi300" GIFOptionsPalette="AdaptivePalette" GIFOptionsInterlaced="true" JPEGOptionsQuality="High" JPEGOptionsFormat="BaselineEncoding" ImageAlignment="AlignLeft" ImageSpaceBefore="0" ImageSpaceAfter="0" UseImagePageBreak="false" ImagePageBreak="PageBreakBefore" CustomImageAlignment="false" SpaceUnit="CssPixel" CustomLayout="false" CustomLayoutType="AlignmentAndSpacing"><Properties><AltMetadataProperty NamespacePrefix="$ID/" PropertyPath="$ID/"/><ActualMetadataProperty NamespacePrefix="$ID/" PropertyPath="$ID/"/></Properties></ObjectExportOption>"#;

pub(super) const INCOPY_SPACED: &str = r#"<InCopyExportOption IncludeGraphicProxies="true" IncludeAllResources="false" />"#;
const FOOTNOTE: &str = r#"<TextFrameFootnoteOptionsObject EnableOverrides="false" SpanFootnotesAcross="false" MinimumSpacingOption="12" SpaceBetweenFootnotes="6" />"#;

/// The element `i` again with its tag, its leading attributes, and its children changed: the
/// children named in `drop` go, the elements `add` come (one per line, like the children) before
/// the first child named `before`, else after the last. Nothing else changes, so a shape made a
/// text frame and back is the text it was.
fn recast(ix: &Index, t: &str, i: usize, tag: &str, lead: &[(&str, &str)], skip: &[&str], drop: &[&str], before: &str, add: &[&str]) -> String {
    let mut open = format!("<{tag}");
    for (k, v) in lead {
        open.push_str(&format!(" {k}=\"{v}\""));
    }
    for a in ix.attrs(t, i) {
        let name = &t[a.name.0..a.name.1];
        if !skip.contains(&name) {
            open.push_str(&format!(" {name}=\"{}\"", &t[a.value.0..a.value.1]));
        }
    }
    let n = &ix.nodes[i];
    let children = ix.children(i).to_vec();
    // the children's indentation: the whitespace before the first one on its line
    let indent = children.first().map_or("", |&c| {
        let st = ix.nodes[c].start;
        let line = t[..st].rfind(char::from(10)).map_or(0, |k| k + 1);
        if t[line..st].trim().is_empty() { &t[line..st] } else { "" }
    });
    let lines: String = add.iter().map(|e| format!("{}{indent}{e}", char::from(10))).collect();
    // where the new elements go, and which children go
    let at = children.iter().find(|&&c| ix.local(t, c) == before).map(|&c| element_span(ix, t, c).start)
        .or_else(|| children.last().map(|&c| ix.nodes[c].end)).unwrap_or(n.close_start);
    let mut inner = String::new();
    let mut from = n.open_end;
    let mut inserted = false;
    for &c in &children {
        let span = element_span(ix, t, c);
        if !inserted && span.start >= at {
            inner.push_str(&t[from..at]);
            inner.push_str(&lines);
            from = at;
            inserted = true;
        }
        if drop.contains(&ix.local(t, c)) {
            inner.push_str(&t[from..span.start]);
            from = span.end;
        }
    }
    if !inserted {
        inner.push_str(&t[from..at]);
        inner.push_str(&lines);
        from = at;
    }
    inner.push_str(&t[from..n.close_start]);
    format!("{open}>{inner}</{tag}>")
}

/// Object ▸ Content ▸ Text on an empty Rectangle, Oval or Polygon: the shape becomes a text
/// frame with a new empty story. What InDesign writes (obj-content-text*): the element is a
/// TextFrame with `ParentStory`, `PreviousTextFrame`, `NextTextFrame` after `Self` and
/// `ContentType="TextType"`, no `StoryTitle`; the text frame, baseline grid and footnote
/// options of the document's defaults (in full: the shape's object style has none) come before
/// the wrap options; `InCopyExportOption` and `FrameFittingOption` go. The column is the
/// frame's width less what its stroke takes inside (158 of 160 with a 2 pt stroke), and a shape
/// that is not a rectangle has one inset (`unit`), not four.
pub(super) fn to_text_frame(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let tag = ix.local(&t, i).to_owned();
    if !matches!(tag.as_str(), "Rectangle" | "Oval" | "Polygon") {
        return Err(EditError::new("unsupported", format!("{id}: not an empty shape")));
    }
    let b = own_bounds(&ix, &t, i, &Mat::ID).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let width = (b[2] - b[0]) - 2.0 * textframe::stroke_inset(w, &ix, &t, i);
    let prefs = w.text("Resources/Preferences.xml")?.to_owned();
    let px = Index::scan(&prefs).map_err(|e| EditError::new("xml", e))?;
    let styles = w.text("Resources/Styles.xml")?.to_owned();
    let sx = Index::scan(&styles).map_err(|e| EditError::new("xml", e))?;
    let style = ix.attr(&t, i, "AppliedObjectStyle").unwrap_or("ObjectStyle/$ID/[None]");
    let (base, style_properties) = resolved_child(&sx, &styles, style, Some("TextFramePreference"))?;
    let (frame_prefs, _) = text_preferences::convert(&px, &prefs, &base, style_properties, width, tag != "Rectangle")?;
    let grid = raw(&px, &prefs, find_tag(&px, &prefs, "BaselineFrameGridOption")?);
    let dm = w.text("designmap.xml")?.to_owned();
    let dx = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let di = find_tag(&dx, &dm, "Document")?;
    let story = w.new_id();
    let text = new_story(&px, &prefs, &sx, &styles, &story, dx.attr(&dm, di, "DOMVersion").unwrap_or("20.4"))?;
    w.add_part(format!("Stories/Story_{story}.xml"), text)?;
    story_copy::register(w, &story)?;
    w.recompose(&story);
    let self_id = ix.attr(&t, i, "Self").unwrap_or(id).to_owned();
    let element = recast(
        &ix,
        &t,
        i,
        "TextFrame",
        &[("Self", &self_id), ("ParentStory", &story), ("PreviousTextFrame", "n"), ("NextTextFrame", "n"), ("ContentType", "TextType")],
        &["Self", "ContentType", "StoryTitle"],
        &["InCopyExportOption", "FrameFittingOption"],
        "TextWrapPreference",
        &[frame_prefs.as_str(), grid.as_str(), FOOTNOTE],
    );
    w.splice(&part, vec![(ix.nodes[i].start..ix.nodes[i].end, element)])
}

/// Object ▸ Content ▸ Graphic / Unassigned on an empty text frame: the shape it was drawn as
/// (from its path: four curved points an Oval, four square corners a Rectangle, else a
/// Polygon) with its story gone (obj-content-text-back, -to-graphic, -oval-back, -poly-back:
/// the document is as before the frame became text).
pub(super) fn from_text_frame(w: &mut Work<'_>, id: &str, graphic: bool) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let no = |m: &str| Err(EditError::new("unsupported", format!("{id}: {m}")));
    if ix.local(&t, i) != "TextFrame" {
        return no("not a text frame");
    }
    if ix.attr(&t, i, "PreviousTextFrame").is_some_and(|v| v != "n") || ix.attr(&t, i, "NextTextFrame").is_some_and(|v| v != "n") {
        return no("a threaded text frame keeps its text");
    }
    let story = ix.attr(&t, i, "ParentStory").unwrap_or_default().to_owned();
    let empty = w.parts_named("Stories/").into_iter().any(|p| {
        w.text(&p).ok().is_some_and(|s| s.contains(&format!("Self=\"{story}\"")) && crate::text::contents(s, &story).is_ok_and(|c| c.is_empty()))
    });
    if !empty {
        return no("a text frame with text keeps it");
    }
    let path = own_path(&ix, &t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let points = path.subpaths.first().map(|s| s.1.clone()).unwrap_or_default();
    let curved = points.iter().any(|p| p.left != p.anchor || p.right != p.anchor);
    let square = points.len() == 4 && (0..4).all(|k| {
        let (a, b) = (points[k].anchor, points[(k + 1) % 4].anchor);
        a.0 == b.0 || a.1 == b.1
    });
    let tag = if points.len() == 4 && curved { "Oval" } else if square && !curved { "Rectangle" } else { "Polygon" };
    let prefs = w.text("Resources/Preferences.xml")?.to_owned();
    let px = Index::scan(&prefs).map_err(|e| EditError::new("xml", e))?;
    let fitting = raw(&px, &prefs, find_tag(&px, &prefs, "FrameFittingOption")?);
    let self_id = ix.attr(&t, i, "Self").unwrap_or(id).to_owned();
    let content = if graphic { "GraphicType" } else { "Unassigned" };
    let element = recast(
        &ix,
        &t,
        i,
        tag,
        &[("Self", &self_id), ("ContentType", content), ("StoryTitle", "$ID/")],
        &["Self", "ContentType", "ParentStory", "PreviousTextFrame", "NextTextFrame", "StoryTitle"],
        &["TextFramePreference", "BaselineFrameGridOption", "TextFrameFootnoteOptionsObject"],
        "ObjectExportOption",
        &[INCOPY_SPACED, fitting.as_str()],
    );
    w.splice(&part, vec![(ix.nodes[i].start..ix.nodes[i].end, element)])?;
    delete_story(w, &story)
}
