//! The text of a scaled text frame takes the scale too (oracle: tfscale-c1; InDesign 2025, measured by transform() and the
//! plural specifier on text frames, artifacts/tfscale-c1-20261023/STATUS.ko.md).
//!
//! Only the text the frame shows is scaled: its characters take the character rules, and every paragraph that has a character in the
//! frame takes the paragraph rules once per frame (a paragraph that runs through two scaled frames is scaled twice; text that is
//! overset or in another frame of the thread stays). Values the style gives are written as overrides, the way InDesign does; a value
//! that comes out as the style's own is not an override.
//!
//! By the scale (sx across, sy down):
//! - `sy`: point size, leading, baseline shift, underline and strike-through offset and weight, kenten and ruby size, space before and
//!   after, the offset and weight of the rules above and below, the top and bottom offsets of paragraph shading and border, and the
//!   minimum space before and after of a spanning paragraph;
//! - `sx`: the indents (first line, left, right, last line, the rules' left and right), the hyphenation zone, the split column gutters,
//!   the left and right offsets of paragraph shading and border, and the tab stops' positions;
//! - `sx / sy`: the horizontal scale (and kenten and ruby horizontal scale);
//! - `sqrt(sx · sy)`: the stroke weight, the miter limit, the paragraph border line weights and the corner radii of border and shading.
//!
//! Everything else (tracking, kerning, word and letter spacing, rule and kenten offsets) stays.
use super::*;
use crate::geom;

/// One scaled value: its IDML name, whether a paragraph or a character owns it, the factor, the value when nothing says (the
/// InDesign defaults) and the type of a property element (`Leading`).
struct Rule {
    name: &'static str,
    paragraph: bool,
    factor: Factor,
    default: f64,
    element: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum Factor {
    Across,
    Down,
    Ratio,
    Mean,
}

impl Factor {
    fn of(self, sx: f64, sy: f64) -> f64 {
        match self {
            Factor::Across => sx,
            Factor::Down => sy,
            Factor::Ratio => sx / sy,
            Factor::Mean => (sx * sy).sqrt(),
        }
    }
}

const fn rule(name: &'static str, paragraph: bool, factor: Factor, default: f64) -> Rule {
    Rule { name, paragraph, factor, default, element: None }
}

const RULES: &[Rule] = &[
    rule("PointSize", false, Factor::Down, 12.0),
    Rule { name: "Leading", paragraph: false, factor: Factor::Down, default: f64::NAN, element: Some("unit") },
    rule("BaselineShift", false, Factor::Down, 0.0),
    rule("UnderlineOffset", false, Factor::Down, -9999.0),
    rule("UnderlineWeight", false, Factor::Down, -9999.0),
    rule("StrikeThroughOffset", false, Factor::Down, -9999.0),
    rule("StrikeThroughWeight", false, Factor::Down, -9999.0),
    rule("KentenFontSize", false, Factor::Down, -1.0),
    rule("RubyFontSize", false, Factor::Down, -1.0),
    rule("HorizontalScale", false, Factor::Ratio, 100.0),
    rule("KentenXScale", false, Factor::Ratio, 100.0),
    rule("RubyXScale", false, Factor::Ratio, 100.0),
    rule("StrokeWeight", false, Factor::Mean, 1.0),
    rule("MiterLimit", false, Factor::Mean, 4.0),
    Rule { name: "SameParaStyleSpacing", paragraph: true, factor: Factor::Down, default: -1.0, element: Some("unit") },
    rule("SpaceBefore", true, Factor::Down, 0.0),
    rule("SpaceAfter", true, Factor::Down, 0.0),
    rule("RuleAboveLineWeight", true, Factor::Down, 1.0),
    rule("RuleAboveOffset", true, Factor::Down, 0.0),
    rule("RuleBelowLineWeight", true, Factor::Down, 1.0),
    rule("RuleBelowOffset", true, Factor::Down, 0.0),
    rule("ParagraphShadingTopOffset", true, Factor::Down, 0.0),
    rule("ParagraphShadingBottomOffset", true, Factor::Down, 0.0),
    rule("ParagraphBorderTopOffset", true, Factor::Down, 0.0),
    rule("ParagraphBorderBottomOffset", true, Factor::Down, 0.0),
    rule("SpanColumnMinSpaceBefore", true, Factor::Down, 0.0),
    rule("SpanColumnMinSpaceAfter", true, Factor::Down, 0.0),
    rule("FirstLineIndent", true, Factor::Across, 0.0),
    rule("LeftIndent", true, Factor::Across, 0.0),
    rule("RightIndent", true, Factor::Across, 0.0),
    rule("LastLineIndent", true, Factor::Across, 0.0),
    rule("RuleAboveLeftIndent", true, Factor::Across, 0.0),
    rule("RuleAboveRightIndent", true, Factor::Across, 0.0),
    rule("RuleBelowLeftIndent", true, Factor::Across, 0.0),
    rule("RuleBelowRightIndent", true, Factor::Across, 0.0),
    rule("HyphenationZone", true, Factor::Across, 36.0),
    rule("SplitColumnInsideGutter", true, Factor::Across, 6.0),
    rule("SplitColumnOutsideGutter", true, Factor::Across, 0.0),
    rule("ParagraphShadingLeftOffset", true, Factor::Across, 0.0),
    rule("ParagraphShadingRightOffset", true, Factor::Across, 0.0),
    rule("ParagraphBorderLeftOffset", true, Factor::Across, 0.0),
    rule("ParagraphBorderRightOffset", true, Factor::Across, 0.0),
    rule("ParagraphBorderLeftLineWeight", true, Factor::Mean, 1.0),
    rule("ParagraphBorderTopLineWeight", true, Factor::Mean, 1.0),
    rule("ParagraphBorderRightLineWeight", true, Factor::Mean, 1.0),
    rule("ParagraphBorderBottomLineWeight", true, Factor::Mean, 1.0),
    rule("ParagraphShadingTopLeftCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphShadingTopRightCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphShadingBottomLeftCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphShadingBottomRightCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphBorderTopLeftCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphBorderTopRightCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphBorderBottomLeftCornerRadius", true, Factor::Mean, 12.0),
    rule("ParagraphBorderBottomRightCornerRadius", true, Factor::Mean, 12.0),
];

fn number(a: Option<&Property>) -> Option<f64> {
    a.and_then(|p| p.value.trim().parse::<f64>().ok())
}

/// A value of `a` (its own, else the style's), as a number.
fn effective(own: &Attrs, base: &Attrs, rule: &Rule) -> Option<f64> {
    match number(own.get(rule.name)).or_else(|| number(base.get(rule.name))) {
        Some(v) => Some(v),
        // a value that is not a number and has no default (Leading: Auto) is not scaled
        None if rule.default.is_nan() => None,
        None => Some(rule.default),
    }
}

fn scale_tabs(list: &str, sx: f64) -> String {
    let (open, close) = ("<Position type=\"unit\">", "</Position>");
    let mut out = String::new();
    let mut rest = list;
    while let Some(at) = rest.find(open) {
        let (head, tail) = rest.split_at(at + open.len());
        out.push_str(head);
        match tail.find(close) {
            Some(end) => {
                match tail[..end].trim().parse::<f64>() {
                    Ok(v) => out.push_str(&geom::num(v * sx)),
                    Err(_) => out.push_str(&tail[..end]),
                }
                rest = &tail[end..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

fn scale_paragraph(catalog: &Catalog, p: &Style, sx: f64, sy: f64) -> Result<Style, EditError> {
    let mut own = style_attrs(p);
    let base = catalog.para(&own)?;
    for rule in RULES.iter().filter(|r| r.paragraph) {
        let factor = rule.factor.of(sx, sy);
        if (factor - 1.0).abs() < 1e-12 {
            continue;
        }
        let Some(eff) = effective(&own, &base, rule) else { continue };
        // the spacing between paragraphs of one style has -1 (the style's own) for its least value
        let new = if rule.name == "SameParaStyleSpacing" { (eff * factor).max(-1.0) } else { eff * factor };
        if number(base.get(rule.name)).unwrap_or(rule.default) == new || same(new, number(base.get(rule.name)).unwrap_or(rule.default)) {
            own.remove(rule.name);
        } else if let Some(ty) = rule.element {
            own.insert(rule.name.into(), property(rule.name, ty, &geom::num(new)));
        } else {
            own.insert(rule.name.into(), attr(geom::num(new)));
        }
    }
    if (sx - 1.0).abs() > 1e-12 {
        let list = own.get("TabList").or_else(|| base.get("TabList")).cloned();
        // (an empty list is the style's default: nothing to scale)
        if let Some(Property { xml: Some(xml), .. }) = list.filter(|p| p.xml.as_deref().is_some_and(|x| x.contains("<Position"))) {
            let scaled = scale_tabs(&xml, sx);
            let (open_end, close_start) = (scaled.find('>').map_or(0, |i| i + 1), scaled.rfind("</").unwrap_or(scaled.len()));
            own.insert("TabList".into(), Property { value: scaled[open_end..close_start].trim().into(), xml: Some(scaled) });
        }
    }
    Ok(make_style(p, &own))
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn scale_character(catalog: &Catalog, p: &Style, c: &Style, sx: f64, sy: f64) -> Result<Style, EditError> {
    let pa = style_attrs(p);
    let mut own = style_attrs(c);
    let base = catalog.character(&pa, &own)?;
    for rule in RULES.iter().filter(|r| !r.paragraph) {
        let factor = rule.factor.of(sx, sy);
        if (factor - 1.0).abs() < 1e-12 {
            continue;
        }
        let Some(eff) = effective(&own, &base, rule) else { continue };
        // the "automatic" marker of underline and strike-through is not a length
        if rule.default < -9000.0 && eff < -9000.0 {
            continue;
        }
        let new = eff * factor;
        let style_value = number(base.get(rule.name)).unwrap_or(rule.default);
        if same(new, style_value) {
            own.remove(rule.name);
        } else if let Some(ty) = rule.element {
            own.insert(rule.name.into(), property(rule.name, ty, &geom::num(new)));
        } else {
            own.insert(rule.name.into(), attr(geom::num(new)));
        }
    }
    Ok(make_style(c, &own))
}

/// Scales the text of one scaled frame: the characters `[start, end)` of the story `id` (what the frame shows). Refuses a story with
/// tables, footnotes or anchored objects (their scaling was not measured).
pub(crate) fn scale_range(w: &mut Work<'_>, id: &str, start: usize, end: usize, sx: f64, sy: f64) -> Result<(), EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let (part, ix, i, id) = locate(w, id)?;
    {
        let t = w.text(&part)?;
        let story = &t[ix.nodes[i].start..ix.nodes[i].end];
        if ["<Table ", "<Footnote", "<AnchoredObjectSetting", "<Cell "].iter().any(|k| story.contains(k)) {
            return Err(unsupported("scaling a text frame whose text holds a table, a footnote or an anchored object (not measured)"));
        }
    }
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    let end = end.min(s.atoms.len());
    if start >= end {
        return Ok(());
    }
    let touched = paragraph_range(&s, start, end);
    let mut paragraphs: BTreeMap<usize, usize> = BTreeMap::new();
    let mut characters: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for at in touched {
        let (p, c) = (s.atoms[at].p, s.atoms[at].c);
        let np = match paragraphs.get(&p) {
            Some(&np) => np,
            None => {
                let style = scale_paragraph(&catalog, &s.paragraphs[p], sx, sy)?;
                s.paragraphs.push(style);
                paragraphs.insert(p, s.paragraphs.len() - 1);
                s.paragraphs.len() - 1
            }
        };
        s.atoms[at].p = np;
        if at >= start && at < end {
            let nc = match characters.get(&(p, c)) {
                Some(&nc) => nc,
                None => {
                    let style = scale_character(&catalog, &s.paragraphs[np], &s.characters[c], sx, sy)?;
                    s.characters.push(style);
                    characters.insert((p, c), s.characters.len() - 1);
                    s.characters.len() - 1
                }
            };
            s.atoms[at].c = nc;
        }
    }
    write(w, &part, &id, &s)
}
