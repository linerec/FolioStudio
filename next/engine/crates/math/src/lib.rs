//! MathML presentation markup laid out by TeX's rules (TeXbook appendix G) with the OpenType MATH
//! constants of an installed font, and drawn as glyph outlines in an SVG that needs no font.
//!
//! InDesign stores an equation as its MathML source plus the SVG it drew from it (`viewBox` in
//! thousandths of an em, y turned, every shape filled with `currentColor`). `layout` produces the
//! same kind of drawing, so the document shows the equation on screen and in PDF without any font,
//! and InDesign opens it. The shapes differ from InDesign's (it draws with MathJax's TeX font and
//! its own rules); the dimensions are close and recorded as approximate (rules/mathml.ko.md).
//!
//! No font is bundled: the MATH table and the outlines come from a font installed on the
//! computer (`font.rs`). Without one a text font stands in and the equation is marked approximate.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

mod font;
mod layout;
mod svg;
mod xml;

use std::fmt;

/// Points to an em per point of the equation's font size. InDesign draws a 10 pt equation with an
/// em of 16.19 pt (measured on the stored drawings: 7.155 pt per `ex`, `ex` = 0.442 em), so the
/// same font size gives the same size here.
pub const EM_PER_SIZE: f64 = 1.6188;

#[derive(Clone, Debug, PartialEq)]
pub struct MathError(pub String);

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for MathError {}

#[derive(Clone, Debug)]
pub struct Options {
    /// The equation's font size in points (InDesign's `AppliedMathMLFontSize`).
    pub size: f64,
    /// Display style (`<math display="block">` also asks for it); inline text style otherwise.
    pub display: bool,
    /// The equation's colour, RGB 0..255. A colour other than black is written on the stored drawing
    /// (InDesign shows the drawing as stored: it does not apply the object's colour to it).
    pub color: [f64; 3],
}

impl Default for Options {
    fn default() -> Self {
        Options { size: 10.0, display: false, color: [0.0; 3] }
    }
}

#[derive(Clone, Debug)]
pub struct Equation {
    /// Size of the drawing in points: the frame that holds it.
    pub width: f64,
    pub height: f64,
    /// How far the drawing reaches below the baseline.
    pub depth: f64,
    /// The drawing as InDesign stores it (`svg_str` of `MathmlSvgData`).
    pub svg: String,
    /// The same drawing sized in points, for showing at once.
    pub preview_svg: String,
    /// Name of the font the outlines came from.
    pub font: String,
    /// The font has no MATH table: a text font was used with TeX-like constants.
    pub approximate: bool,
    /// What could not be drawn exactly (an element not supported, a missing character).
    pub notes: Vec<String>,
}

impl Equation {
    /// `MathmlSvgData`: the JSON InDesign keeps next to the source.
    pub fn svg_data(&self) -> String {
        serde_json::json!({ "height": self.height, "svg_str": self.svg, "width": self.width }).to_string()
    }
}

/// Source with the MathML namespace on the root, which InDesign's own parser needs.
pub fn normalize_source(mathml: &str) -> Result<String, MathError> {
    let root = xml::parse(mathml)?;
    if root.name != "math" {
        return Err(MathError(format!("MathML: the root element is <{}>, expected <math>", root.name)));
    }
    let trimmed = mathml.trim();
    let start = trimmed.find("<math").or_else(|| trimmed.find(":math")).ok_or_else(|| MathError("MathML: no <math> element".to_owned()))?;
    let tag_end = trimmed[start..].find('>').map_or(trimmed.len(), |e| start + e);
    if trimmed[start..tag_end].contains("xmlns") {
        return Ok(trimmed[start..].to_owned());
    }
    // `<math` is followed by a space, `>` or `/`: add the namespace right after the name
    let name_end = start + "<math".len();
    Ok(format!("{}<math xmlns=\"http://www.w3.org/1998/Math/MathML\"{}", &trimmed[..start], &trimmed[name_end..]))
}

/// The source as InDesign keeps it for an equation of this size and colour: the namespace on the root
/// and `mathsize="12pt"` / `mathcolor="#115395"` on `<math>` (InDesign reads the size and colour from
/// these, not from `AppliedMathMLFontSize`/`AppliedMathMLRgbColor`, when it opens the file). The
/// defaults, 10 pt and black, are not written, as InDesign does not write them.
pub fn styled_source(mathml: &str, size: f64, rgb: [f64; 3]) -> Result<String, MathError> {
    let source = normalize_source(mathml)?;
    let start = source.find("<math").ok_or_else(|| MathError("MathML: no <math> element".to_owned()))?;
    // the end of the root's start tag, skipping `>` inside quotes
    let mut quote = None;
    let mut end = None;
    for (i, c) in source[start..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => {
                end = Some(start + i);
                break;
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| MathError("MathML: the <math> tag is not closed".to_owned()))?;
    let empty = source[..end].ends_with('/');
    let tag_end = if empty { end - 1 } else { end };
    // keep the other attributes, drop the two we own
    let inner = &source[start + "<math".len()..tag_end];
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut rest = inner.trim_start();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| MathError("MathML: attribute without a value".to_owned()))?;
        let key = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let q = after.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or_else(|| MathError("MathML: attribute value is not quoted".to_owned()))?;
        let vend = after[1..].find(q).ok_or_else(|| MathError("MathML: unterminated attribute value".to_owned()))?;
        attrs.push((key, after[1..1 + vend].to_owned()));
        rest = after[vend + 2..].trim_start();
    }
    attrs.retain(|(k, _)| k != "mathsize" && k != "mathcolor");
    if (size - 10.0).abs() > 1e-9 {
        attrs.push(("mathsize".to_owned(), format!("{size}pt")));
    }
    let hex = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    if rgb.iter().any(|v| v.round() != 0.0) {
        attrs.push(("mathcolor".to_owned(), format!("#{:02x}{:02x}{:02x}", hex(rgb[0]), hex(rgb[1]), hex(rgb[2]))));
    }
    let tag: String = attrs.iter().map(|(k, v)| format!(" {k}=\"{v}\"")).collect();
    Ok(format!("{}<math{tag}{}{}", &source[..start], if empty { "/" } else { "" }, &source[end..]))
}

/// Lays out `mathml` and draws it.
pub fn layout(mathml: &str, opts: &Options) -> Result<Equation, MathError> {
    if mathml.len() > 200_000 {
        return Err(MathError("MathML: the source is too long".to_owned()));
    }
    if !(opts.size.is_finite() && opts.size > 0.0 && opts.size <= 1000.0) {
        return Err(MathError("MathML: the font size must be between 0 and 1000 pt".to_owned()));
    }
    let root = xml::parse(mathml)?;
    if root.name != "math" {
        return Err(MathError(format!("MathML: the root element is <{}>, expected <math>", root.name)));
    }
    let fonts = font::fonts().map_err(MathError)?;
    let mut lay = layout::Layout::new(fonts);
    let laid = lay.math(&root, opts.display);
    let em_pt = opts.size * EM_PER_SIZE;
    let ink = opts.color.map(|v| v.round().clamp(0.0, 255.0) as u8);
    let (svg, [x0, y0, x1, y1]) = svg::render(&laid, fonts, svg::Dim::Ex, (ink != [0, 0, 0]).then_some(ink));
    let (preview_svg, _) = svg::render(&laid, fonts, svg::Dim::Pt(em_pt), None);
    Ok(Equation {
        width: (x1 - x0) * em_pt,
        height: (y1 - y0) * em_pt,
        depth: -y0 * em_pt,
        svg,
        preview_svg,
        font: fonts.primary().name.clone(),
        approximate: !fonts.math,
        notes: lay.notes,
    })
}

/// The name of the font equations are set in, and whether it has a MATH table (cheap after the first call).
pub fn font_info() -> Result<(String, bool), MathError> {
    let fonts = font::fonts().map_err(MathError)?;
    Ok((fonts.primary().name.clone(), fonts.math))
}
