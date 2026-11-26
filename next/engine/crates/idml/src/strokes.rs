//! Stroke styles: the document's own (Resources/Graphic.xml `DashedStrokeStyle`,
//! `DottedStrokeStyle`, `StripedStrokeStyle`) and InDesign's built-in ones, resolved from an
//! item's `StrokeType` to the pattern it paints (`folio_model::geom::StrokePattern`).

use crate::props::{clean_name, nums};
use crate::xml::Element;
use folio_model::LineCap;
use folio_model::geom::{CornerAdjust, StrokePattern};
use std::collections::HashMap;

#[derive(Clone, Debug)]
enum Custom {
    Dashed { dashes: Vec<f64>, adjust: CornerAdjust, cap: Option<LineCap> },
    Dotted { gaps: Vec<f64>, adjust: CornerAdjust },
    Striped { stripes: Vec<(f64, f64)> },
}

/// The document's stroke styles by `Self` id (e.g. `DashedStrokeStyle/House dash`).
#[derive(Default)]
pub struct StrokeStyles {
    custom: HashMap<String, Custom>,
    pub definitions: Vec<folio_model::stroke_styles::Definition>,
}

pub fn corner_adjust(v: Option<&str>) -> CornerAdjust {
    match v {
        Some("None") => CornerAdjust::None,
        Some("Dashes") => CornerAdjust::Dashes,
        Some("Gaps") => CornerAdjust::Gaps,
        _ => CornerAdjust::DashesAndGaps,
    }
}

pub fn line_cap(v: Option<&str>) -> Option<LineCap> {
    match v? {
        "ButtEndCap" => Some(LineCap::Butt),
        "RoundEndCap" => Some(LineCap::Round),
        "ProjectingEndCap" => Some(LineCap::Projecting),
        _ => None,
    }
}

impl StrokeStyles {
    pub fn read(roots: &[&Element<'_>]) -> StrokeStyles {
        let mut custom = HashMap::new();
        let mut definitions=Vec::new();
        for r in roots {
            for e in r.elements() {
                let Some(id) = e.attr("Self") else { continue };
                if ["StrokeStyle","DashedStrokeStyle","DottedStrokeStyle","StripedStrokeStyle"].contains(&e.name) {
                    let key=match e.name {"DashedStrokeStyle"=>"DashArray","DottedStrokeStyle"=>"DotArray",_=>"StripeArray"};
                    definitions.push(folio_model::stroke_styles::Definition{id:id.into(),name:clean_name(e.attr("Name").unwrap_or(id)).into(),kind:e.name.into(),values:e.attr(key).and_then(nums).unwrap_or_default(),adjustment:e.attr("StrokeCornerAdjustment").unwrap_or("DashesAndGaps").into(),cap:e.attr("EndCap").unwrap_or("ButtEndCap").into()});
                }
                let list = |k: &str| e.attr(k).and_then(nums).unwrap_or_default();
                let adjust = corner_adjust(e.attr("StrokeCornerAdjustment"));
                let def = match e.name {
                    "DashedStrokeStyle" => Custom::Dashed { dashes: list("DashArray"), adjust, cap: line_cap(e.attr("EndCap")) },
                    "DottedStrokeStyle" => Custom::Dotted { gaps: list("DotArray"), adjust },
                    "StripedStrokeStyle" => Custom::Striped {
                        stripes: list("StripeArray").chunks_exact(2).map(|p| ((p[0] / 100.0).clamp(0.0, 1.0), (p[1] / 100.0).clamp(0.0, 1.0))).collect(),
                    },
                    _ => continue,
                };
                custom.insert(id.to_owned(), def);
            }
        }
        StrokeStyles { custom, definitions }
    }

    /// The name InDesign shows for a `StrokeType` value.
    pub fn name(stroke_type: &str) -> &str {
        let after = stroke_type.split_once('/').map_or(stroke_type, |(_, n)| n);
        clean_name(after)
    }

    /// What a stroke of `StrokeType` `t` and weight `w` paints; `None` for a style this
    /// document does not define (drawn solid by the caller).
    pub fn pattern(&self, t: &str, w: f64, dash_and_gap: Option<&[f64]>, adjust: CornerAdjust) -> Option<StrokePattern> {
        if let Some(c) = self.custom.get(t) {
            return match c {
                Custom::Dashed { dashes, adjust, cap } => {
                    let mut p: Vec<f64> = dashes.iter().map(|v| v.max(0.0)).collect();
                    if p.len() % 2 == 1 {
                        let again = p.clone();
                        p.extend(again);
                    }
                    (!p.is_empty()).then_some(StrokePattern::Dashed { pattern: p, adjust: *adjust, cap: *cap, builtin: false })
                }
                Custom::Dotted { gaps, adjust } => (!gaps.is_empty()).then(|| StrokePattern::Dotted { gaps: gaps.clone(), adjust: *adjust, builtin: false }),
                Custom::Striped { stripes } => Some(StrokePattern::Striped(stripes.clone())),
            };
        }
        if t.starts_with("StrokeStyle/") || !t.contains('/') {
            return StrokePattern::builtin(Self::name(t), w, dash_and_gap, adjust);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn reads_document_stroke_styles() {
        let root = parse(
            "<idPkg:Graphic><StrokeStyle Self=\"StrokeStyle/$ID/Dashed\" Name=\"$ID/Dashed\"/>\
             <DottedStrokeStyle Self=\"DottedStrokeStyle/Atlas dots\" DotArray=\"10 18\" StrokeCornerAdjustment=\"Gaps\" Name=\"Atlas dots\"/>\
             <StripedStrokeStyle Self=\"StripedStrokeStyle/T stripes\" StripeArray=\"0 20 50 100\" Name=\"T stripes\"/>\
             <DashedStrokeStyle Self=\"DashedStrokeStyle/T dashes\" DashArray=\"0 8 14 20\" StrokeCornerAdjustment=\"Dashes\" EndCap=\"ProjectingEndCap\" Name=\"T dashes\"/>\
             </idPkg:Graphic>",
        )
        .unwrap();
        let s = StrokeStyles::read(&[&root]);
        assert_eq!(
            s.pattern("DottedStrokeStyle/Atlas dots", 8.0, None, CornerAdjust::DashesAndGaps),
            Some(StrokePattern::Dotted { gaps: vec![10.0, 18.0], adjust: CornerAdjust::Gaps, builtin: false })
        );
        assert_eq!(s.pattern("StripedStrokeStyle/T stripes", 12.0, None, CornerAdjust::DashesAndGaps), Some(StrokePattern::Striped(vec![(0.0, 0.2), (0.5, 1.0)])));
        assert!(matches!(
            s.pattern("DashedStrokeStyle/T dashes", 4.0, None, CornerAdjust::DashesAndGaps),
            Some(StrokePattern::Dashed { adjust: CornerAdjust::Dashes, cap: Some(LineCap::Projecting), builtin: false, .. })
        ));
        assert_eq!(
            s.pattern("StrokeStyle/$ID/Dashed", 6.0, Some(&[12.0, 6.0]), CornerAdjust::Gaps),
            Some(StrokePattern::Dashed { pattern: vec![12.0, 6.0], adjust: CornerAdjust::Gaps, cap: None, builtin: true })
        );
        assert!(matches!(s.pattern("StrokeStyle/$ID/Right Slant Hash", 1.0, None, CornerAdjust::DashesAndGaps), Some(StrokePattern::Hash(_))));
        assert_eq!(s.pattern("DashedStrokeStyle/missing", 1.0, None, CornerAdjust::DashesAndGaps), None);
        assert_eq!(StrokeStyles::name("StrokeStyle/$ID/Japanese Dots"), "Japanese Dots");
        assert_eq!(StrokeStyles::name("DottedStrokeStyle/Atlas dots"), "Atlas dots");
    }
}
