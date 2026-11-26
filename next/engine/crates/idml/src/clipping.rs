use crate::{xml::Element, props::Props};
use folio_model::clipping::Options;

pub(crate) fn read(g: &Element<'_>) -> Option<Options> {
    let c = g.child("ClippingPathSettings")?;
    Some(Options {
        kind: c.attr("ClippingType").unwrap_or("None").into(),
        threshold: c.f64("Threshold").unwrap_or(25.0),
        tolerance: c.f64("Tolerance").unwrap_or(2.0),
        inset: c.f64("InsetFrame").unwrap_or(0.0),
        invert: c.bool("InvertPath").unwrap_or(false),
        inside_edges: c.bool("IncludeInsideEdges").unwrap_or(false),
        restrict_to_frame: c.bool("RestrictToFrame").unwrap_or(false),
        high_resolution: c.bool("UseHighResolutionImage").unwrap_or(true),
        path_name: c.attr("AppliedPathName").unwrap_or("$ID/").into(),
        index: c.attr("Index").and_then(|s| s.parse().ok()).unwrap_or(-1),
    })
}
