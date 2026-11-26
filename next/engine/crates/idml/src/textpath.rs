//! Reads open text paths without changing the parent path or source frame links.

use crate::{warn::Warnings, xml::Element};
use folio_model::{FirstBaseline, SubPath, TextFrame, textpath::{HorizontalTextPath, PathAlignment}};
use std::collections::{BTreeMap, HashMap};

/// TextPath child ids resolve to the page items that own their layout frames.
#[derive(Default)]
pub(crate) struct FrameIds(HashMap<String, String>);

impl FrameIds {
    /// The map of the parts' `TextPath` children and, by part path, the pairs each part adds to it (child id, owning item id).
    pub fn tracked<'a, 'b>(
        roots: impl IntoIterator<Item = (&'b str, &'b Element<'a>)>,
    ) -> (Self, BTreeMap<String, Vec<(String, String)>>)
    where
        'a: 'b,
    {
        let by_part: BTreeMap<_, _> = roots
            .into_iter()
            .map(|(part, root)| (part.to_owned(), Self::entries(root)))
            .collect();
        (Self::from_entries(by_part.values().flatten()), by_part)
    }

    /// The `TextPath` children below `root` and the items that own them.
    pub fn entries(root: &Element<'_>) -> Vec<(String, String)> {
        fn collect(item: &Element<'_>, entries: &mut Vec<(String, String)>) {
            let owner = if matches!(item.name, "GraphicLine" | "Oval" | "Rectangle" | "Polygon") {
                link_id(item.attr("Self"))
            } else {
                None
            };
            for child in item.elements() {
                if child.name == "TextPath" {
                    if let (Some(owner), Some(child)) = (owner, link_id(child.attr("Self"))) {
                        entries.push((child.to_owned(), owner.to_owned()));
                    }
                }
                collect(child, entries);
            }
        }

        let mut entries = Vec::new();
        collect(root, &mut entries);
        entries
    }

    /// The map of the pairs the parts hold.
    pub fn from_entries<'e>(entries: impl IntoIterator<Item = &'e (String, String)>) -> Self {
        Self(entries.into_iter().cloned().collect())
    }

    pub fn resolve(&self, id: Option<&str>) -> Option<String> {
        let id = link_id(id)?;
        Some(self.0.get(id).map_or(id, String::as_str).to_owned())
    }
}

fn link_id(id: Option<&str>) -> Option<&str> {
    id.filter(|id| !id.is_empty() && *id != "n")
}

/// Editing remains limited to unthreaded paths within the readable subset.
pub fn supported(source: &str) -> Option<HorizontalTextPath> {
    let item = crate::xml::parse(source).ok()?;
    let mut warnings = Warnings::default();
    let paths = crate::items::paths(&item, false, &mut warnings);
    read(&item, &paths, &mut warnings)
        .filter(|path| path.frame.previous.is_none() && path.frame.next.is_none())
}

pub(crate) fn read(item: &Element<'_>, paths: &[SubPath], warnings: &mut Warnings) -> Option<HorizontalTextPath> {
    let mut children = item.elements().filter(|e| e.name == "TextPath");
    let text = children.next()?;
    let parsed = (children.next().is_none() && matches!(item.name, "GraphicLine" | "Oval" | "Rectangle" | "Polygon"))
        .then(|| horizontal(item, text, paths)).flatten();
    if parsed.is_none() {
        warnings.add("TextPath outside the supported open baseline subset; original IDML preserved");
    }
    parsed
}

/// The offset path of the item's stroke that Top and Bottom path alignment walk (the stroke is read from the item's own attributes; what an
/// object style gives is not known here, so a missing weight counts as InDesign's default 1 pt).
fn aligned_offset(item: &Element<'_>, measure: &folio_model::textpath::PathMeasure, alignment: PathAlignment, flipped: bool) -> Option<folio_model::textpath::Offset> {
    use folio_model::{LineJoin, textpath::OffsetJoin};
    let weight = item.attr("StrokeWeight").and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
    let height = alignment.height(if flipped { -weight } else { weight });
    if height == 0.0 { return None; }
    let join = match item.attr("EndJoin") { Some("RoundEndJoin") => LineJoin::Round, Some("BevelEndJoin") => LineJoin::Bevel, _ => LineJoin::Miter };
    let limit = item.attr("MiterLimit").and_then(|v| v.parse::<f64>().ok()).filter(|m| *m >= 1.0).unwrap_or(4.0);
    measure.offset_with(height, OffsetJoin::of_stroke(join, limit))
}

fn horizontal(item: &Element<'_>, text: &Element<'_>, paths: &[SubPath]) -> Option<HorizontalTextPath> {
    let [geometry] = paths else { return None };
    let measure = folio_model::textpath::PathMeasure::new(geometry)?;
    let flipped = match text.attr("FlipPathEffect")? { "NotFlipped" => false, "Flipped" => true, _ => return None };
    use folio_model::textpath::PathEffect;
    let effect = match text.attr("PathEffect")? {
        "RainbowPathEffect" => PathEffect::Rainbow,
        "SkewPathEffect" => PathEffect::Skew,
        "RibbonPathEffect" => PathEffect::Ribbon,
        "StairStepPathEffect" => PathEffect::Stair,
        "GravityPathEffect" => PathEffect::Gravity,
        _ => return None,
    };
    use folio_model::textpath::PathTextAlignment;
    let text_alignment = match text.attr("TextAlignment")? {
        "BaselineTextAlignment" => PathTextAlignment::Baseline,
        "AscenderTextAlignment" => PathTextAlignment::Ascender,
        "DescenderTextAlignment" => PathTextAlignment::Descender,
        "CenterTextAlignment" => PathTextAlignment::Center,
        _ => return None,
    };
    let horizontal = horizontal_segment(paths).filter(|_| !flipped && effect == PathEffect::Rainbow && text_alignment == PathTextAlignment::Baseline);
    let path_alignment = match text.attr("PathAlignment")? {
        "TopPathAlignment" => PathAlignment::Top,
        "CenterPathAlignment" => PathAlignment::Center,
        "BottomPathAlignment" => PathAlignment::Bottom,
        _ => return None,
    };
    let number = |name| text.attr(name)?.parse::<f64>().ok().filter(|v| v.is_finite());
    let spacing = number("PathSpacing")?;
    let (start, end) = (number("StartBracket")?, number("EndBracket")?);
    // R6
    let aligned = aligned_offset(item, &measure, path_alignment, flipped);
    let length = aligned.as_ref().map_or_else(|| measure.length(), |o| o.path.length());
    if start < 0.0 || start > length+0.01 || end < 0.0 || end > length+0.01 || end == start { return None; }
    let descends = end < start;
    // A forward range may run past the last anchor. Backwards ranges keep
    // their descending endpoints so composition can walk back from the start.
    let wraps = !geometry.open && !measure.text_runs_backwards(flipped) && descends;
    // An open path takes end < start only when it is flipped: InDesign's flipped text starts at the start bracket and runs back toward the path's start,
    // as long as the two brackets are apart (typepath-c14, rules/typepath-c14.ko.md section 6; what the Selection tool's centre drag across the path writes).
    if descends && geometry.open && !flipped { return None; }
    let story = text.attr("ParentStory").filter(|s| !s.is_empty() && *s != "n")?.to_owned();
    Some(HorizontalTextPath {
        frame: TextFrame {
            story, columns: 1, first_baseline: FirstBaseline::Fixed,
            previous: FrameIds::default().resolve(text.attr("PreviousTextFrame")),
            next: FrameIds::default().resolve(text.attr("NextTextFrame")),
            ..TextFrame::default()
        },
        start: horizontal.map_or(0.0, |(a,_)| a.x) + start,
        end: horizontal.map_or(0.0, |(a,_)| a.x) + if wraps { end + length } else { end.min(length) },
        baseline: horizontal.map_or(0.0, |(a,_)| a.y),
        curve: horizontal.is_none().then(|| geometry.clone()),
        flipped,
        effect,
        text_alignment,
        path_alignment,
        spacing,
    })
}

fn horizontal_segment(paths: &[SubPath]) -> Option<(folio_model::Point, folio_model::Point)> {
    let [path] = paths else { return None };
    let [a, b] = path.points.as_slice() else { return None };
    if !path.open || a.anchor != a.left || a.anchor != a.right || b.anchor != b.left || b.anchor != b.right
        || a.anchor.y != b.anchor.y || b.anchor.x <= a.anchor.x
    {
        return None;
    }
    Some((a.anchor, b.anchor))
}

/// The `StartBracket` and `EndBracket` InDesign writes when the path alignment of the text path of `source` becomes `new`: each bracket
/// moves to the point of the new offset path straight across from where it was (typepath-c10, DOM measurements on rectangles; curves are
/// not matched yet). A range that covers a whole closed path becomes the default one (0.1 .. length - 0.1), a bracket past the path end
/// on a closed path goes round to 0.
pub fn brackets_for_alignment(source: &str, new: PathAlignment) -> Option<(f64, f64)> {
    let item = crate::xml::parse(source).ok()?;
    let paths = crate::items::paths(&item, false, &mut Warnings::default());
    let [geometry] = paths.as_slice() else { return None };
    let measure = folio_model::textpath::PathMeasure::new(geometry)?;
    let text = item.elements().find(|e| e.name == "TextPath")?;
    let old = match text.attr("PathAlignment")? {
        "TopPathAlignment" => PathAlignment::Top, "CenterPathAlignment" => PathAlignment::Center, "BottomPathAlignment" => PathAlignment::Bottom, _ => return None,
    };
    let flipped = text.attr("FlipPathEffect")? == "Flipped";
    let number = |name| text.attr(name)?.parse::<f64>().ok().filter(|v| v.is_finite());
    let (start, end) = (number("StartBracket")?, number("EndBracket")?);
    if old == new { return Some((start, end)); }
    let (from, to) = (aligned_offset(&item, &measure, old, flipped), aligned_offset(&item, &measure, new, flipped));
    let closed = !geometry.open;
    let old_length = from.as_ref().map_or_else(|| measure.length(), |o| o.path.length());
    let new_length = to.as_ref().map_or_else(|| measure.length(), |o| o.path.length());
    let across = |value: f64| {
        let source = from.as_ref().map_or(value, |o| o.unmap(value));
        to.as_ref().map_or(source, |o| o.map(source))
    };
    let end = if closed && end < start { end + old_length } else { end };
    let (mut start, mut end) = (across(start), across(end));
    if closed {
        start = start.max(0.0);
        if start <= 1e-9 && end >= new_length - 1e-9 { return Some((0.1, new_length - 0.1)); }
        if end >= new_length - 1e-9 { end = (end - new_length).max(0.0); }
    } else {
        start = start.clamp(0.0, new_length);
        end = end.clamp(0.0, new_length);
    }
    let tidy = |v: f64| (v*1e6).round()/1e6;
    Some((tidy(start), tidy(end)))
}

/// Default attributes for a new text path within the supported geometry subset: an open path of one subpath
/// (a straight line or any number of Bezier segments), or a closed oval, rectangle or polygon. InDesign ends the new text
/// at the end of an open path; on a closed shape it leaves 0.1 pt at both ends of the outline (StartBracket 0.1,
/// EndBracket = length - 0.1, measured on a rectangle, a pentagon and an oval in InDesign 2025).
pub fn creation_attributes(source: &str) -> Option<std::collections::BTreeMap<&'static str, String>> {
    let item = crate::xml::parse(source).ok()?;
    if item.elements().any(|e| e.name == "TextPath") { return None; }
    let paths = crate::items::paths(&item, false, &mut Warnings::default());
    let [path] = paths.as_slice() else { return None };
    let (start, end) = match (item.name, path.open) {
        ("GraphicLine", true) => {
            let length = match horizontal_segment(&paths) {
                Some((a, b)) if [a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()) => b.x - a.x,
                Some(_) => return None,
                None => folio_model::textpath::PathMeasure::new(path)?.length(),
            };
            (0.0, length)
        }
        ("Oval" | "Rectangle" | "Polygon", false) => {
            let length = folio_model::textpath::PathMeasure::new(path)?.length();
            if length <= 0.2 { return None; }
            (0.1, length - 0.1)
        }
        _ => return None,
    };
    Some([
        ("PathAlignment", "CenterPathAlignment".into()),
        ("TextAlignment", "BaselineTextAlignment".into()),
        ("PathEffect", "RainbowPathEffect".into()),
        ("FlipPathEffect", "NotFlipped".into()),
        ("PathSpacing", "0".into()), ("StartBracket", start.to_string()),
        ("EndBracket", end.to_string()),
        ("PreviousTextFrame", "n".into()), ("NextTextFrame", "n".into()),
    ].into())
}

/// The measure of the one path of a page item's source (a single open or closed path), for tools that work from a pointer.
pub fn measure_of(source: &str) -> Option<folio_model::textpath::PathMeasure> {
    let item = crate::xml::parse(source).ok()?;
    let paths = crate::items::paths(&item, false, &mut Warnings::default());
    let [path] = paths.as_slice() else { return None };
    folio_model::textpath::PathMeasure::new(path)
}

/// What the Type on a Path tool makes when pressed on a closed shape at `point` (the item's own coordinates): the text starts where the
/// press lands on the outline (StartBracket = its distance along the path) and the end bracket sits one point behind it, so the text
/// goes all the way round: -1 on a shape that runs clockwise on screen, +1 on a counter-clockwise one (InDesign 2025, rectangles and
/// polygons both ways, typepath-c8 rules/typepath-c8.ko.md section 2). `reach` is how far from the outline a press still counts.
pub fn creation_attributes_at(source: &str, point: (f64, f64), reach: f64) -> Result<std::collections::BTreeMap<&'static str, String>, (&'static str, String)> {
    let mut attributes = creation_attributes(source).ok_or(("unsupported", "select a closed oval, rectangle or polygon without text".to_owned()))?;
    let item = crate::xml::parse(source).map_err(|e| ("xml", e.0))?;
    let paths = crate::items::paths(&item, false, &mut Warnings::default());
    let [path] = paths.as_slice() else { return Err(("unsupported", "select a shape of one path".to_owned())) };
    if path.open { return Err(("unsupported", "the press is for a closed shape".to_owned())); }
    let measure = folio_model::textpath::PathMeasure::new(path).ok_or(("unsupported", "the shape has no outline".to_owned()))?;
    let near = measure.nearest(folio_model::Point::new(point.0, point.1));
    if !near.distance.is_finite() || near.distance > reach { return Err(("notOnPath", "the press is not on the outline".to_owned())); }
    let length = measure.length();
    let start = near.at.min(length);
    let end = if measure.is_clockwise() {
        if start >= 1.0 { start - 1.0 } else { start - 1.0 + length }
    } else {
        (start + 1.0).min(length)
    };
    attributes.insert("StartBracket", start.to_string());
    attributes.insert("EndBracket", end.to_string());
    Ok(attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::{PathPoint, Point};

    #[test]
    fn unsupported_geometry_and_options_remain_unmodelled() {
        let source = r#"<GraphicLine><TextPath ParentStory="story" PathAlignment="CenterPathAlignment" TextAlignment="BaselineTextAlignment" PathEffect="RainbowPathEffect" FlipPathEffect="NotFlipped" PreviousTextFrame="n" NextTextFrame="n" PathSpacing="0" StartBracket="20" EndBracket="200"/></GraphicLine>"#;
        let point = |x| { let anchor = Point::new(x, 100.0); PathPoint { anchor, left: anchor, right: anchor } };
        let paths = vec![SubPath { points: vec![point(80.0), point(320.0)], open: true }];
        let element = crate::xml::parse(source).unwrap();
        let result = read(&element, &paths, &mut Warnings::default()).unwrap();
        assert_eq!((result.start, result.end, result.baseline), (100.0, 280.0, 100.0));
        let spaced = source.replace("PathSpacing=\"0\"", "PathSpacing=\"12\"");
        let spaced = read(&crate::xml::parse(&spaced).unwrap(), &paths, &mut Warnings::default()).unwrap();
        assert_eq!(spaced.spacing, 12.0);
        for (from, to) in [("RainbowPathEffect", "UnknownPathEffect"),
            ("PathSpacing=\"0\"", "PathSpacing=\"nope\""),
            ("EndBracket=\"200\"", "EndBracket=\"300\"")]
        {
            let changed = source.replace(from, to);
            let element = crate::xml::parse(&changed).unwrap();
            assert!(read(&element, &paths, &mut Warnings::default()).is_none(), "{to}");
        }
        let mut curved = paths.clone();
        curved[0].points[0].right.y += 1.0;
        assert!(read(&element, &curved, &mut Warnings::default()).is_some());
        let mut reversed = paths.clone();
        reversed[0].points.reverse();
        assert!(read(&element, &reversed, &mut Warnings::default()).is_some());
    }
}
