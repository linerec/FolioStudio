//! Equations: Insert MathML, change one, delete one (rules/mathml.ko.md, "Folio's own layout").
//!
//! An equation is a rectangle with a `MathObject` child, the way InDesign stores one: the MathML
//! source, the font size and colour, and the drawing (`MathmlSvgData`, outlines in an SVG).
//! `folio-math` lays the source out and draws it; the rectangle is the drawing's size, so the
//! equation shows on screen and in PDF without a font, InDesign opens the IDML with the drawing
//! it expects, and importing the IDML again gives the same equation back (editable).
use super::*;
use folio_math::{Equation, Options};

fn laid_out(mathml: &str, size: f64, color: &[f64; 3]) -> Result<(String, Equation), EditError> {
    // InDesign reads the size and colour of an equation from `mathsize` / `mathcolor` on `<math>`
    let source = folio_math::styled_source(mathml, size, *color).map_err(|e| EditError::new("invalid", e.to_string()))?;
    let eq = folio_math::layout(&source, &Options { size, display: false, color: *color }).map_err(|e| EditError::new("invalid", e.to_string()))?;
    if !(eq.width > 0.0 && eq.height > 0.0 && eq.width.is_finite() && eq.height.is_finite()) {
        return Err(EditError::new("invalid", "the equation has no size"));
    }
    Ok((source, eq))
}

fn check_color(color: &[f64; 3]) -> Result<(), EditError> {
    if color.iter().all(|v| v.is_finite() && (0.0..=255.0).contains(v)) {
        Ok(())
    } else {
        Err(EditError::new("invalid", "the colour is three numbers from 0 to 255"))
    }
}

fn rgb_text(color: &[f64; 3]) -> String {
    format!("{} {} {}", color[0], color[1], color[2])
}

/// The size to write: `10` and not `10.0`.
fn number(v: f64) -> String {
    format!("{v}")
}

/// `insertMathml`: a new equation with its top-left corner at `at` (page coordinates, points).
pub(crate) fn insert(w: &mut Work<'_>, page: &str, mathml: &str, at: &[f64; 2], size: &f64, color: &[f64; 3]) -> Result<(), EditError> {
    check_color(color)?;
    if !at.iter().all(|v| v.is_finite()) {
        return Err(EditError::new("invalid", "the position is two numbers"));
    }
    let (source, eq) = laid_out(mathml, *size, color)?;
    let rect = [at[0], at[1], at[0] + eq.width, at[1] + eq.height];
    create::run(w, page, create::ItemKind::Rectangle, rect, None, 6, None)?;
    let id = w.created.last().cloned().ok_or_else(|| EditError::new("xml", "the equation's frame was not made"))?;
    // like the frame InDesign makes for an equation: no fill, no stroke
    set_attrs(
        w,
        &id,
        &[("FillColor", "Swatch/None".into()), ("StrokeColor", "Swatch/None".into()), ("StrokeWeight", "0".into())],
    )?;
    let object = w.new_id();
    let xml = format!(
        "\t<MathObject Self=\"{object}\" AppliedMathMLFontSize=\"{}\" AppliedMathMLRgbColor=\"{}\" TintValue=\"100\" IsMathMLObject=\"true\" MathMLDescription=\"{}\" MathmlSvgData=\"{}\">\n\t\t\t\t<Properties>\n\t\t\t\t\t<AppliedMathMLSwatch type=\"enumeration\">Nothing</AppliedMathMLSwatch>\n\t\t\t\t</Properties>\n\t\t\t</MathObject>\n\t\t",
        number(*size),
        rgb_text(color),
        escape(&source),
        escape(&eq.svg_data()),
    );
    let (part, ix, i) = w.find(&id)?;
    let end = ix.nodes[i].close_start;
    w.splice(&part, vec![(end..end, xml)])?;
    Ok(())
}

/// `setMathml`: the equation in frame `id` is laid out again from new source, size and colour; the
/// frame becomes the new drawing's size and keeps its top-left corner.
pub(crate) fn set(w: &mut Work<'_>, id: &str, mathml: &str, size: &f64, color: &[f64; 3]) -> Result<(), EditError> {
    check_color(color)?;
    let (source, eq) = laid_out(mathml, *size, color)?;
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let object = ix
        .children(i)
        .iter()
        .copied()
        .find(|&c| ix.local(&t, c) == "MathObject")
        .ok_or_else(|| EditError::new("invalid", format!("{id} is not an equation")))?;
    let edits = vec![
        attr_edit(&ix, &t, object, "AppliedMathMLFontSize", &number(*size)),
        attr_edit(&ix, &t, object, "AppliedMathMLRgbColor", &rgb_text(color)),
        attr_edit(&ix, &t, object, "IsMathMLObject", "true"),
        attr_edit(&ix, &t, object, "MathMLDescription", &source),
        attr_edit(&ix, &t, object, "MathmlSvgData", &eq.svg_data()),
    ];
    w.splice(&part, edits)?;
    resize(w, id, Target::Size((eq.width, eq.height), Anchor::TopLeft, false))
}

/// `deleteMathml`: removes the equation's frame; a frame that holds no equation is refused.
pub(crate) fn remove(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if !ix.children(i).iter().any(|&c| ix.local(&t, c) == "MathObject") {
        return Err(EditError::new("invalid", format!("{id} is not an equation")));
    }
    delete(w, id)
}
