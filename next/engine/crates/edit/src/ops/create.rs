//! New page items (oracle basic-create / basic-create-defaults, create-*).
//! Shapes use the document's graphic style plus PageItemDefault overrides. Placeholder
//! frames use [None], text frames use the text style. New items stack atop their layer.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

mod defaults;
mod path;
mod placement;
mod text_preferences;
pub(super) use path::shape_path;
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
enum Frame {
    Page,
    View,
    Drag,
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
    let mut placement = placement::resolve(
        item_transform(&ix, &t, pi),
        item_transform(&ix, &t, spread),
        ix.attr(&t, pi, "GeometricBounds").unwrap_or("0 0 0 0"),
        kind,
        rect,
        frame,
        custom.is_some(),
    )?;
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
    let (style, mut a) = defaults::item_attributes(
        kind,
        &defaults,
        &sx,
        &styles,
        part.starts_with("MasterSpreads/"),
        &placement,
    )?;
    let id = w.new_id();
    a.insert("Self".into(), id.clone());
    a.insert("ItemLayer".into(), layer.into());
    a.insert("AppliedObjectStyle".into(), style.into());
    a.insert("ItemTransform".into(), placement.matrix.text());
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
            placement.page_rect[2] - placement.page_rect[0],
            false,
            false,
        )?;
        placement.local_rect[2] = placement.local_rect[0] + width;
        extra.push_str(&frame_prefs);
    }
    defaults::append_options(&mut extra, kind, &px, &prefs)?;
    let geometry = path::geometry(&placement, kind, sides, star, custom)?;
    let tag = kind.tag();
    let xml = format!(
        "<{tag}{}><Properties><PathGeometry><GeometryPathType PathOpen=\"{}\"><PathPointArray>{geometry}</PathPointArray></GeometryPathType></PathGeometry></Properties>{extra}</{tag}>",
        attrs_text(&a),
        kind == ItemKind::Line
    );
    let at = insertion_point(&ix, &t, spread, &dx, &dm, layer);
    w.splice(&part, vec![(at..at, format!("\n\t\t{xml}"))])?;
    w.created.push(id);
    Ok(())
}

/// The new item goes above the last item on its layer, ahead of higher layers.
fn insertion_point(ix: &Index, t: &str, spread: usize, dx: &Index, dm: &str, layer: &str) -> usize {
    let siblings: Vec<_> = ix
        .children(spread)
        .iter()
        .copied()
        .filter(|&i| is_item(ix, t, i))
        .collect();
    let layers: Vec<_> = (0..dx.nodes.len())
        .filter(|&i| dx.local(dm, i) == "Layer")
        .filter_map(|i| dx.attr(dm, i, "Self"))
        .collect();
    let rank = |l: Option<&str>| layers.iter().position(|s| Some(*s) == l).unwrap_or(0);
    siblings
        .iter()
        .rev()
        .find(|&&i| ix.attr(t, i, "ItemLayer") == Some(layer))
        .map(|&i| ix.nodes[i].end)
        .or_else(|| {
            siblings
                .iter()
                .find(|&&i| rank(ix.attr(t, i, "ItemLayer")) > rank(Some(layer)))
                .map(|&i| element_span(ix, t, i).start)
        })
        .unwrap_or(ix.nodes[spread].close_start)
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
