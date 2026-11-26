//! Cell Options ▸ Diagonal Lines (oracle `tables-cells`, measured with the DOM's cell properties `topLeftDiagonalLine`,
//! `topRightDiagonalLine`, `diagonalLineInFront` and `diagonalLineStroke…`).
//! - A diagonal is on when the cell has `TopLeftDiagonalLine="true"` (top left to bottom right) or `TopRightDiagonalLine="true"`;
//!   off is the default and writes nothing (cell-diag-default: turning one on writes only that attribute).
//! - The line's settings are written only when set: `DiagonalLineInFront`, `DiagonalLineStrokeWeight`, `…Type` (`StrokeStyle/$ID/Name`),
//!   `…Color` (`Color/…`), `…Tint`. The attributes sit together, in that order, just before `ClipContentToCell`
//!   (cell-diag-one, cell-diag-both).
use super::*;

/// Diagonal Lines of a cell; absent fields stay.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagonalLines {
    /// Top left to bottom right / top right to bottom left.
    #[serde(default)] pub top_left: Option<bool>,
    #[serde(default)] pub top_right: Option<bool>,
    /// The lines are drawn over the cell's text.
    #[serde(default)] pub in_front: Option<bool>,
    /// Weight (points), stroke style (`StrokeStyle/$ID/Dashed`), colour swatch (`Color/…`) and tint (percent).
    #[serde(default)] pub weight: Option<f64>,
    #[serde(default)] pub stroke_type: Option<String>,
    #[serde(default)] pub color: Option<String>,
    #[serde(default)] pub tint: Option<f64>,
}

/// The attributes in the order InDesign writes them.
const ORDER: [&str; 7] = ["TopLeftDiagonalLine", "TopRightDiagonalLine", "DiagonalLineInFront", "DiagonalLineStrokeWeight",
    "DiagonalLineStrokeType", "DiagonalLineStrokeColor", "DiagonalLineStrokeTint"];

impl DiagonalLines {
    pub(super) fn check(&self) -> Result<(), EditError> {
        if self.weight.is_some_and(|v| !(v.is_finite() && (0.0..=1000.0).contains(&v))) || self.tint.is_some_and(|v| !(0.0..=100.0).contains(&v)) {
            return Err(EditError::new("invalid", "diagonal line: weight 0 to 1000 pt, tint 0 to 100 %"));
        }
        if self.stroke_type.as_deref().is_some_and(|s| !s.starts_with("StrokeStyle/")) || self.color.as_deref().is_some_and(|s| s.is_empty()) {
            return Err(EditError::new("invalid", "diagonal line: stroke style as StrokeStyle/…, colour as a swatch"));
        }
        Ok(())
    }

    /// Puts the settings on a cell's attributes: what the cell has with the changes, written again together
    /// before ClipContentToCell. A line turned off removes its attribute (the default).
    pub(super) fn apply(&self, attrs: &mut Vec<(String, String)>) {
        let mut now: Vec<Option<String>> = ORDER.iter().map(|k| get(attrs, k).map(str::to_owned)).collect();
        let flag = |on: bool| on.then(|| "true".to_owned());
        if let Some(v) = self.top_left { now[0] = flag(v); }
        if let Some(v) = self.top_right { now[1] = flag(v); }
        if let Some(v) = self.in_front { now[2] = flag(v); }
        if let Some(v) = self.weight { now[3] = Some(geom::num(v)); }
        if let Some(v) = &self.stroke_type { now[4] = Some(v.clone()); }
        if let Some(v) = &self.color { now[5] = Some(v.clone()); }
        if let Some(v) = self.tint { now[6] = Some(geom::num(v)); }
        attrs.retain(|(k, _)| !ORDER.contains(&k.as_str()));
        let at = attrs.iter().position(|(k, _)| k == "ClipContentToCell").unwrap_or(attrs.len());
        let on: Vec<(String, String)> = ORDER.iter().zip(now).filter_map(|(k, v)| v.map(|v| (k.to_string(), v))).collect();
        attrs.splice(at..at, on);
    }

    /// A cell's diagonal lines as the IDML stores them (the Cell Options dialog's start values).
    pub(super) fn info(attrs: &[(String, String)]) -> serde_json::Value {
        let flag = |k: &str| get(attrs, k) == Some("true");
        let num = |k: &str| get(attrs, k).and_then(|v| v.parse::<f64>().ok());
        serde_json::json!({
            "topLeft": flag("TopLeftDiagonalLine"), "topRight": flag("TopRightDiagonalLine"), "inFront": flag("DiagonalLineInFront"),
            "weight": num("DiagonalLineStrokeWeight"), "strokeType": get(attrs, "DiagonalLineStrokeType"),
            "color": get(attrs, "DiagonalLineStrokeColor"), "tint": num("DiagonalLineStrokeTint"),
        })
    }
}
