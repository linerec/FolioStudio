//! Metadata of a frame's imported MathObject. Source package parts remain authoritative.

use crate::{warn::Warnings, xml::Element};
use folio_model::mathml::MathObject;

pub(crate) fn read(frame: &Element<'_>, warnings: &mut Warnings) -> Option<MathObject> {
    let math = frame.child("MathObject")?;
    let number = |name: &str| math.attr(name).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
    let rgb = math.attr("AppliedMathMLRgbColor").and_then(|value| {
        let values: Vec<f64> = value.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
        values.iter().all(|v| v.is_finite()).then_some(())?;
        values.try_into().ok()
    });
    warnings.add("MathML object read; the drawing InDesign stored with the equation is shown as is. If the equation is changed in Folio it is laid out again with Folio's own MathML layout (installed math font, TeX spacing), which differs a little from InDesign's drawing");
    Some(MathObject {
        id: math.attr("Self").unwrap_or_default().to_owned(),
        source: math.attr("MathMLDescription").map(str::to_owned),
        font_size: number("AppliedMathMLFontSize"),
        rgb,
        tint: number("TintValue"),
        swatch: math.child("Properties").and_then(|p| p.child("AppliedMathMLSwatch")).map(Element::text),
        is_mathml: math.attr("IsMathMLObject").and_then(|v| match v { "true" => Some(true), "false" => Some(false), _ => None }),
        svg_data: math.attr("MathmlSvgData").map(str::to_owned),
    })
}
