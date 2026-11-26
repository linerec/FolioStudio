use super::*;

fn clear(ix: &Index, t: &str, graphic: usize, settings: usize) -> Vec<(std::ops::Range<usize>, String)> {
    let mut edits = vec![attr_edit(ix,t,settings,"ClippingType","None"), attr_edit(ix,t,settings,"AppliedPathName","$ID/"), attr_edit(ix,t,settings,"InvertPath","false")];
    if ix.attr(t, settings, "ClippingType") == Some("AlphaChannel") {
        edits.push(attr_edit(ix,t,settings,"Index","-1"));
    }
    if let Some(source) = ix.path(t,graphic,&["Properties","ClippingPathGeometry"]) {
        edits.push((ix.nodes[source].start..ix.nodes[source].end,String::new()));
    }
    if let Some(&io) = ix.named(t,graphic,"ImageIOPreference").first() {
        edits.push(attr_edit(ix,t,io,"ApplyPhotoshopClippingPath","false"));
    }
    edits
}

/// The numbers of Object ▸ Clipping Path ▸ Options (`ClippingPathSettings`); a field left out keeps its value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipSettings {
    pub threshold: Option<f64>,
    pub tolerance: Option<f64>,
    /// Inset Frame in points (negative grows).
    pub inset: Option<f64>,
    pub invert: Option<bool>,
    pub inside_edges: Option<bool>,
    pub restrict_to_frame: Option<bool>,
    pub high_resolution: Option<bool>,
    /// The alpha channel or Photoshop path by name.
    pub path_name: Option<String>,
    /// The alpha channel's position in the image.
    pub index: Option<i32>,
}

/// One subpath of a made clipping path in the graphic's coordinates: (left direction, anchor, right direction) per point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipPath {
    pub points: Vec<[[f64; 2]; 3]>,
    #[serde(default)]
    pub open: bool,
}

const MADE: &[&str] = &["DetectEdges", "AlphaChannel", "PhotoshopPath"];

/// The settings written where the frame's graphic keeps them, and the made path as its `ClippingPathGeometry`
/// (before `GraphicBounds`, where InDesign writes it).
fn make(ix: &Index, t: &str, graphic: usize, node: usize, kind: &str, s: &ClipSettings, paths: &[ClipPath]) -> Result<Vec<(std::ops::Range<usize>, String)>, EditError> {
    let num = |v: f64| { let r = (v * 1e6).round() / 1e6; if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") } };
    let mut edits = vec![attr_edit(ix, t, node, "ClippingType", kind)];
    for (name, value) in [("Threshold", s.threshold.map(num)), ("Tolerance", s.tolerance.map(num)), ("InsetFrame", s.inset.map(num)),
        ("InvertPath", s.invert.map(|b| b.to_string())), ("IncludeInsideEdges", s.inside_edges.map(|b| b.to_string())),
        ("RestrictToFrame", s.restrict_to_frame.map(|b| b.to_string())), ("UseHighResolutionImage", s.high_resolution.map(|b| b.to_string()))] {
        if let Some(v) = value { edits.push(attr_edit(ix, t, node, name, &v)); }
    }
    let name = match kind {
        "DetectEdges" => "$ID/".to_owned(),
        _ => s.path_name.clone().or_else(|| ix.attr(t, node, "AppliedPathName").map(str::to_owned)).unwrap_or_default(),
    };
    edits.push(attr_edit(ix, t, node, "AppliedPathName", &name));
    edits.push(attr_edit(ix, t, node, "Index", &match kind { "DetectEdges" => -1, _ => s.index.unwrap_or(0) }.to_string()));
    let geometry: String = paths.iter().map(|p| {
        let pts: Vec<PathPoint> = p.points.iter().map(|[l, a, r]| PathPoint { anchor: (a[0], a[1]), left: (l[0], l[1]), right: (r[0], r[1]) }).collect();
        compound::path_xml(&pts, !p.open, "\t\t\t\t")
    }).collect();
    if let Some(g) = ix.path(t, graphic, &["Properties", "ClippingPathGeometry"]) {
        edits.push((ix.nodes[g].open_end..ix.nodes[g].close_start, format!("{geometry}\n\t\t\t\t")));
    } else {
        let props = ix.named(t, graphic, "Properties").first().copied().ok_or_else(|| EditError::new("xml", "the picture has no properties"))?;
        let at = ix.named(t, props, "GraphicBounds").first().map(|&b| ix.nodes[b].start).unwrap_or(ix.nodes[props].open_end);
        edits.push((at..at, format!("<ClippingPathGeometry>{geometry}\n\t\t\t\t</ClippingPathGeometry>")));
    }
    Ok(edits)
}

/// Object ▸ Clipping Path ▸ Options: None takes the path away; Detect Edges, Alpha Channel and Photoshop Path
/// write the settings and the path made from the linked image (`paths`, supplied by the host from the file:
/// `folio_render::clipextract`). One frame at a time when a path is made.
pub(crate) fn set(w: &mut Work<'_>, ids: &[String], kind: &str, settings: &Option<ClipSettings>, paths: &Option<Vec<ClipPath>>) -> Result<(), EditError> {
    if kind != "None" && !MADE.contains(&kind) {
        return Err(EditError::new("invalid", format!("unknown clipping kind {kind}")));
    }
    if paths.is_some() && ids.len() > 1 {
        return Err(EditError::new("invalid", "a made clipping path belongs to one frame"));
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let content = content_of(&ix, &t, i).ok_or_else(|| EditError::new("invalid", "select a picture frame"))?;
        let node = ix.named(&t, content.graphic, "ClippingPathSettings").first().copied()
            .ok_or_else(|| EditError::new("unsupported", "the picture has no clipping settings"))?;
        let current = ix.attr(&t, node, "ClippingType").unwrap_or("None");
        if kind == current && (kind == "None" || (settings.is_none() && paths.is_none())) { continue; }
        if kind != "None" {
            let made = paths.as_deref().filter(|p| !p.is_empty())
                .ok_or_else(|| EditError::new("unsupported", "the clipping path is made from the linked image, which could not be read"))?;
            let edits = make(&ix, &t, content.graphic, node, kind, &settings.clone().unwrap_or_default(), made)?;
            w.splice(&part, edits)?;
            continue;
        }
        let mut edits = clear(&ix, &t, content.graphic, node);
        if let Some(bounds) = own_bounds(&ix, &t, i, &Mat::ID) {
            edits.extend(fitting_crops(&ix, &t, i, bounds));
        }
        w.splice(&part,edits)?;
    }
    Ok(())
}

pub(crate) fn to_frame(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let content = content_of(&ix, &t, i).ok_or_else(|| EditError::new("invalid", "select a picture frame"))?;
        let settings = ix.named(&t, content.graphic, "ClippingPathSettings").first().copied()
            .filter(|n| ix.attr(&t, *n, "ClippingType").is_some_and(|k| k != "None"))
            .ok_or_else(|| EditError::new("invalid", "the picture has no clipping path"))?;
        let source = ix.path(&t, content.graphic, &["Properties", "ClippingPathGeometry"])
            .ok_or_else(|| EditError::new("unsupported", "the saved clipping geometry is missing"))?;
        let dest = ix.path(&t, i, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "frame geometry missing"))?;
        let mut shapes = Vec::new();
        for g in ix.named(&t, source, "GeometryPathType") {
            let array = ix.named(&t, g, "PathPointArray").first().copied().ok_or_else(|| EditError::new("xml", "clipping path points missing"))?;
            let mut points = Vec::new();
            for p in ix.named(&t, array, "PathPointType") {
                let pair = |key| ix.attr(&t, p, key).and_then(geom::parse_pair).map(|p| content.m.apply(p))
                    .ok_or_else(|| EditError::new("xml", "invalid clipping path point"));
                points.push(PathPoint { anchor: pair("Anchor")?, left: pair("LeftDirection")?, right: pair("RightDirection")? });
            }
            shapes.push((points, ix.attr(&t, g, "PathOpen") != Some("true")));
        }
        let bounds = geom::path_bounds(&shapes).ok_or_else(|| EditError::new("invalid", "empty clipping path"))?;
        let geometry = shapes.iter().map(|(points,closed)| compound::path_xml(points,*closed,"\t\t\t")).collect::<String>();
        let kind = if shapes.len() == 1 && shapes[0].1 && pathedit::is_rectangle(&shapes[0].0) { "Rectangle" } else { "Polygon" };
        let node = &ix.nodes[i];
        let mut edits = vec![
            (ix.nodes[dest].open_end..ix.nodes[dest].close_start, geometry),
            (node.name.clone(),kind.into()),
            (node.close_start+2..node.close_start+2+ix.local(&t,i).len(),kind.into()),
        ];
        edits.extend(clear(&ix, &t, content.graphic, settings));
        edits.extend(fitting_crops(&ix,&t,i,bounds));
        w.splice(&part,edits)?;
    }
    Ok(())
}
