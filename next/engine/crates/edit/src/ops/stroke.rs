//! The Stroke panel (next/oracle/scenarios/basic-stroke.json): cap, join, miter limit,
//! alignment, stroke type, arrowheads and their scale, gap colour and tint. Each is an attribute
//! of the object, written where it differs from what the object style gives (or InDesign's
//! default) and removed where it does not. Choosing "Dashed" also writes InDesign's first dash
//! (12 pt) and corner adjustment (dashes and gaps); another type takes them away.
use super::*;

/// A change from the Stroke panel; absent fields stay as they are. Values as IDML names them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeChange {
    #[serde(default)] pub cap: Option<String>,
    #[serde(default)] pub join: Option<String>,
    #[serde(default)] pub miter_limit: Option<f64>,
    #[serde(default)] pub alignment: Option<String>,
    /// A stroke style's `Self` ("StrokeStyle/$ID/Dashed").
    #[serde(default)] pub kind: Option<String>,
    #[serde(default)] pub start: Option<String>,
    #[serde(default)] pub end: Option<String>,
    #[serde(default)] pub start_scale: Option<f64>,
    #[serde(default)] pub end_scale: Option<f64>,
    /// A swatch's `Self` (`Swatch/None` for none).
    #[serde(default)] pub gap_color: Option<String>,
    #[serde(default)] pub gap_tint: Option<f64>,
}

const CAPS: [&str; 3] = ["ButtEndCap", "RoundEndCap", "ProjectingEndCap"];
const JOINS: [&str; 3] = ["MiterEndJoin", "RoundEndJoin", "BevelEndJoin"];
const ALIGNMENTS: [&str; 3] = ["CenterAlignment", "InsideAlignment", "OutsideAlignment"];
const ARROWHEADS: [&str; 12] = ["None", "SimpleArrowHead", "SimpleWideArrowHead", "TriangleArrowHead", "TriangleWideArrowHead", "BarbedArrowHead",
    "CurvedArrowHead", "CircleArrowHead", "CircleSolidArrowHead", "SquareArrowHead", "SquareSolidArrowHead", "BarArrowHead"];
const DASHED: &str = "StrokeStyle/$ID/Dashed";

/// (attribute, InDesign's default, the object style category it belongs to)
fn default_of(name: &str) -> (&'static str, &'static str) {
    match name {
        "EndCap" => ("ButtEndCap", "EnableStrokeAndCornerOptions"),
        "EndJoin" => ("MiterEndJoin", "EnableStrokeAndCornerOptions"),
        "MiterLimit" => ("4", "EnableStrokeAndCornerOptions"),
        "StrokeAlignment" => ("CenterAlignment", "EnableStrokeAndCornerOptions"),
        "LeftLineEnd" | "RightLineEnd" => ("None", "EnableStrokeAndCornerOptions"),
        "LeftArrowHeadScale" | "RightArrowHeadScale" => ("100", "EnableStrokeAndCornerOptions"),
        "StrokeType" => ("StrokeStyle/$ID/Solid", "EnableStroke"),
        "GapColor" => ("Swatch/None", "EnableStroke"),
        "GapTint" => ("-1", "EnableStroke"),
        _ => ("", "EnableStroke"),
    }
}

pub(super) fn set_stroke(w: &mut Work<'_>, ids: &[String], c: &StrokeChange) -> Result<(), EditError> {
    let bad = |m: &str| Err(EditError::new("invalid", m.to_owned()));
    let one_of = |v: &Option<String>, list: &[&str]| v.as_deref().is_some_and(|v| !list.contains(&v));
    if one_of(&c.cap, &CAPS) || one_of(&c.join, &JOINS) || one_of(&c.alignment, &ALIGNMENTS) || one_of(&c.start, &ARROWHEADS) || one_of(&c.end, &ARROWHEADS) {
        return bad("unknown stroke setting");
    }
    if c.miter_limit.is_some_and(|v| !(1.0..=500.0).contains(&v)) {
        return bad("miter limit: 1 to 500");
    }
    if [c.start_scale, c.end_scale].iter().flatten().any(|v| !(1.0..=1000.0).contains(v)) {
        return bad("arrowhead scale: 1 to 1000 %");
    }
    if c.gap_tint.is_some_and(|v| !(0.0..=100.0).contains(&v) && v != -1.0) {
        return bad("gap tint: 0 to 100 %");
    }
    if c.kind.as_deref().is_some_and(|k| !k.starts_with("StrokeStyle/") && !["DashedStrokeStyle/","DottedStrokeStyle/","StripedStrokeStyle/"].iter().any(|prefix|k.starts_with(prefix))) {
        return bad("a stroke type is a stroke style");
    }
    if let Some(kind)=&c.kind { let(t,ix)=swatches::scan(w,"Resources/Graphic.xml")?;if ix.by_self(&t,kind).is_none(){return bad("unknown stroke style");} }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let mut want: Vec<(&str, Option<String>)> = Vec::new();
        let text = |v: &Option<String>| v.clone();
        let num = |v: Option<f64>| v.map(geom::num);
        want.extend([("EndCap", text(&c.cap)), ("EndJoin", text(&c.join)), ("MiterLimit", num(c.miter_limit)), ("StrokeAlignment", text(&c.alignment)),
            ("StrokeType", text(&c.kind)), ("LeftLineEnd", text(&c.start)), ("RightLineEnd", text(&c.end)),
            ("LeftArrowHeadScale", num(c.start_scale)), ("RightArrowHeadScale", num(c.end_scale)), ("GapColor", text(&c.gap_color)), ("GapTint", num(c.gap_tint))]);
        let mut edits = Vec::new();
        for (name, value) in want {
            let Some(value) = value else { continue };
            let (default, enable) = default_of(name);
            let base = objectstyle::from_style(w, &ix, &t, i, &[], name, enable).unwrap_or_else(|| default.to_owned());
            if value == base || same_number(&value, &base) {
                edits.extend(attr_removal(&ix, &t, i, name));
            } else {
                edits.push(attr_edit(&ix, &t, i, name, &value));
            }
        }
        if let Some(kind) = &c.kind {
            if kind == DASHED {
                if ix.attr(&t, i, "StrokeDashAndGap").is_none() {
                    edits.push(attr_edit(&ix, &t, i, "StrokeDashAndGap", "12"));
                }
                if ix.attr(&t, i, "StrokeCornerAdjustment").is_none() {
                    edits.push(attr_edit(&ix, &t, i, "StrokeCornerAdjustment", "DashesAndGaps"));
                }
            } else {
                edits.extend(attr_removal(&ix, &t, i, "StrokeDashAndGap"));
                edits.extend(attr_removal(&ix, &t, i, "StrokeCornerAdjustment"));
            }
        }
        if !edits.is_empty() {
            w.splice(&part, edits)?;
        }
    }
    Ok(())
}
