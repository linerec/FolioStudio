//! Reads text frame preferences and resolves their story links.

use super::ItemReader;
use crate::styles::{Cat, ObjectProps};
use crate::warn::Warnings;
use folio_model::{FirstBaseline, TextFrame, VerticalJustification};

impl<'a> ItemReader<'a, '_> {
    pub(super) fn text_frame(&self, o: &ObjectProps<'a, '_>, w: &mut Warnings) -> TextFrame {
        let p = ["TextFramePreference"];
        let g = Cat::FrameGeneral;
        let link = |k: &str| self.text_path_frames.resolve(o.item.attr(k));
        let insets = match o.get(&p, "InsetSpacing", g).and_then(|v| v.f64s()) {
            Some(v) if v.len() == 4 => [v[0], v[1], v[2], v[3]],
            Some(v) if v.len() == 1 => [v[0]; 4],
            None => [0.0; 4],
            Some(v) => {
                w.add(format!("InsetSpacing with {} values ignored", v.len()));
                [0.0; 4]
            }
        };
        let first_baseline = match o.str(&p, "FirstBaselineOffset", Cat::FrameBaseline) {
            Some("AscentOffset") | None => FirstBaseline::Ascent,
            Some("CapHeight") => FirstBaseline::CapHeight,
            Some("XHeight") => FirstBaseline::XHeight,
            Some("LeadingOffset") => FirstBaseline::Leading,
            Some("EmboxHeight") => FirstBaseline::EmBoxHeight,
            Some("FixedHeight") => FirstBaseline::Fixed,
            Some(other) => {
                w.add(format!("first baseline offset {other} read as Ascent"));
                FirstBaseline::Ascent
            }
        };
        let vertical_justification = match o.str(&p, "VerticalJustification", g) {
            Some("CenterAlign") => VerticalJustification::Center,
            Some("BottomAlign") => VerticalJustification::Bottom,
            Some("JustifyAlign") => VerticalJustification::Justify,
            _ => VerticalJustification::Top,
        };
        if o.bool(&p, "VerticalBalanceColumns", g) == Some(true) {
            w.add("balanced columns not modelled");
        }
        if o.bool(&p, "UseFlexibleColumnWidth", g) == Some(true) {
            w.add("flexible column width not modelled");
        }
        let fixed = o.bool(&p, "UseFixedColumnWidth", g) == Some(true);
        let story = o.item.attr("ParentStory").unwrap_or_default().to_owned();
        if story.is_empty() {
            w.add("text frame without ParentStory");
        }
        TextFrame {
            story,
            previous: link("PreviousTextFrame"),
            next: link("NextTextFrame"),
            columns: o.f64(&p, "TextColumnCount", g).map(|v| v.round().clamp(1.0, 1000.0) as u32).unwrap_or(1),
            gutter: o.f64(&p, "TextColumnGutter", g).unwrap_or(12.0),
            column_width: if fixed { o.f64(&p, "TextColumnFixedWidth", g) } else { None },
            insets,
            first_baseline,
            min_first_baseline_offset: o.f64(&p, "MinimumFirstBaselineOffset", Cat::FrameBaseline).unwrap_or(0.0),
            vertical_justification,
            ignore_wrap: o.bool(&p, "IgnoreWrap", g).unwrap_or(false),
            auto_size: auto_size(o),
        }
    }
}

/// A text frame's Auto-Size settings (off: `None`).
fn auto_size(o: &ObjectProps<'_, '_>) -> Option<folio_model::AutoSize> {
    let p = ["TextFramePreference"];
    let c = Cat::FrameAutoSize;
    let kind = match o.str(&p, "AutoSizingType", c)? {
        "HeightOnly" => folio_model::AutoSizeKind::HeightOnly,
        "WidthOnly" => folio_model::AutoSizeKind::WidthOnly,
        "HeightAndWidth" => folio_model::AutoSizeKind::HeightAndWidth,
        "HeightAndWidthProportionally" => folio_model::AutoSizeKind::HeightAndWidthProportionally,
        _ => return None,
    };
    let point = match o.str(&p, "AutoSizingReferencePoint", c).unwrap_or("CenterPoint") {
        "TopLeftPoint" => (0.0, 0.0),
        "TopCenterPoint" => (0.5, 0.0),
        "TopRightPoint" => (1.0, 0.0),
        "LeftCenterPoint" => (0.0, 0.5),
        "RightCenterPoint" => (1.0, 0.5),
        "BottomLeftPoint" => (0.0, 1.0),
        "BottomCenterPoint" => (0.5, 1.0),
        "BottomRightPoint" => (1.0, 1.0),
        _ => (0.5, 0.5),
    };
    let min = |on: &str, v: &str| (o.bool(&p, on, c) == Some(true)).then(|| o.f64(&p, v, c).unwrap_or(0.0));
    Some(folio_model::AutoSize {
        kind,
        point,
        min_height: min("UseMinimumHeightForAutoSizing", "MinimumHeightForAutoSizing"),
        min_width: min("UseMinimumWidthForAutoSizing", "MinimumWidthForAutoSizing"),
        no_line_breaks: o.bool(&p, "UseNoLineBreaksForAutoSizing", c) == Some(true),
    })
}
