use crate::LineCap;
use crate::geom::{CornerAdjust, StrokePattern};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub values: Vec<f64>,
    pub adjustment: String,
    pub cap: String,
}

impl Definition {
    /// What a stroke of this document style paints, for a consumer that has the document's
    /// definitions but not the IDML (text decorations: `UnderlineType="DashedStrokeStyle/House dash"`).
    /// The same reading as `folio_idml::strokes::StrokeStyles::pattern` gives page items: a dashed
    /// list repeats when odd, dotted `values` are the gaps between dot centres, striped `values`
    /// are `from to` percentages of the weight. `None` for a style without values.
    pub fn pattern(&self) -> Option<StrokePattern> {
        let adjust = match self.adjustment.as_str() {
            "None" => CornerAdjust::None,
            "Dashes" => CornerAdjust::Dashes,
            "Gaps" => CornerAdjust::Gaps,
            _ => CornerAdjust::DashesAndGaps,
        };
        match self.kind.as_str() {
            "DashedStrokeStyle" => {
                let mut p: Vec<f64> = self.values.iter().map(|v| v.max(0.0)).collect();
                if p.len() % 2 == 1 {
                    let again = p.clone();
                    p.extend(again);
                }
                let cap = match self.cap.as_str() {
                    "ButtEndCap" => Some(LineCap::Butt),
                    "RoundEndCap" => Some(LineCap::Round),
                    "ProjectingEndCap" => Some(LineCap::Projecting),
                    _ => None,
                };
                (!p.is_empty()).then_some(StrokePattern::Dashed { pattern: p, adjust, cap, builtin: false })
            }
            "DottedStrokeStyle" => (!self.values.is_empty()).then(|| StrokePattern::Dotted { gaps: self.values.clone(), adjust, builtin: false }),
            "StripedStrokeStyle" => Some(StrokePattern::Striped(
                self.values.chunks_exact(2).map(|p| ((p[0] / 100.0).clamp(0.0, 1.0), (p[1] / 100.0).clamp(0.0, 1.0))).collect(),
            )),
            _ => None,
        }
    }
}
