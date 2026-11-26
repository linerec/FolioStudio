//! The drawing of an equation: the shapes of the SVG that InDesign stores with a MathML object
//! (`MathmlSvgData`), read as filled outlines in the frame's own coordinates. The screen and the
//! PDF writer share this.
//!
//! The SVG is document content (InDesign wrote the glyph outlines into the file), so drawing it
//! needs no font. The drawing is InDesign's (an imported equation) or Folio's own (`folio-math` lays
//! out and draws a new or changed equation in this same form). An equation without stored SVG, or
//! with SVG this reader does not know, draws nothing.
//!
//! Subset read (what InDesign's equation output uses): `svg` with `viewBox` (the outermost one),
//! `g` with `transform` (`translate`, `scale`, `matrix`) and `fill`, `path` (every command but
//! the elliptical arc), `rect`. Anything else makes the whole equation unsupported rather than
//! drawn wrongly.

use folio_model::mathml::MathObject;
use folio_model::{Affine, PathPoint, Point, Rect, SubPath};

/// One filled shape (a glyph outline, a fraction bar, a root's line).
#[derive(Clone, Debug, PartialEq)]
pub struct MathShape {
    pub paths: Vec<SubPath>,
    /// Colour the SVG gives this shape; `None` is `currentColor`, the equation's own colour.
    pub fill: Option<[u8; 3]>,
}

/// The shapes of `math`, with the SVG's view box laid onto `frame` (the frame's path bounds in
/// its own coordinates). `None` when the equation has no stored SVG or it is not readable.
pub fn shapes(math: &MathObject, frame: &Rect) -> Option<Vec<MathShape>> {
    let json: serde_json::Value = serde_json::from_str(math.svg_data.as_deref()?).ok()?;
    let svg = json.get("svg_str")?.as_str()?;
    parse(svg, frame)
}

/// The colour `currentColor` stands for: the object's RGB with its tint (a tint lightens towards
/// white). Black when the file gives no colour.
pub fn equation_rgb(math: &MathObject) -> [u8; 3] {
    let rgb = math.rgb.unwrap_or([0.0; 3]);
    let tint = math.tint.unwrap_or(100.0).clamp(0.0, 100.0) / 100.0;
    rgb.map(|v| (255.0 - (255.0 - v.clamp(0.0, 255.0)) * tint).round().clamp(0.0, 255.0) as u8)
}

#[derive(Clone, Copy)]
enum Fill {
    Current,
    Rgb([u8; 3]),
    Nothing,
}

struct Walk {
    stack: Vec<(Affine, Fill, bool)>,
    view: Affine,
    out: Vec<MathShape>,
    /// the outermost `svg` was read
    root: bool,
}

fn parse(svg: &str, frame: &Rect) -> Option<Vec<MathShape>> {
    let mut walk = Walk { stack: vec![(Affine::IDENTITY, Fill::Current, false)], view: Affine::IDENTITY, out: vec![], root: false };
    let mut rest = svg;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let close = rest.find('>')?;
        let tag = &rest[..close];
        rest = &rest[close + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            match name.trim() {
                "g" => {
                    walk.stack.pop()?;
                }
                "svg" | "path" | "rect" => {}
                _ => return None,
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
        let (name, attrs) = (&tag[..name_end], attributes(&tag[name_end..])?);
        let attr = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        match name {
            "svg" => {
                if walk.root {
                    return None; // nested svg: its own view box and size, not read
                }
                walk.root = true;
                let vb: Vec<f64> = attr("viewBox")?.split([' ', ',']).filter(|s| !s.is_empty()).map(|s| s.parse().ok()).collect::<Option<_>>()?;
                let [x, y, w, h] = vb[..] else { return None };
                if w <= 0.0 || h <= 0.0 || frame.width() <= 0.0 || frame.height() <= 0.0 {
                    return None;
                }
                let (sx, sy) = (frame.width() / w, frame.height() / h);
                walk.view = Affine::new(sx, 0.0, 0.0, sy, frame.left - x * sx, frame.top - y * sy);
            }
            "g" => {
                let (outer, fill, hidden) = *walk.stack.last()?;
                let t = match attr("transform") {
                    Some(t) => outer.then_inner(&transform(t)?),
                    None => outer,
                };
                let (fill, hidden) = match attr("fill") {
                    None => (fill, hidden),
                    Some(v) => match paint(v)? {
                        Fill::Nothing => (fill, true),
                        f => (f, hidden),
                    },
                };
                if self_closing {
                    continue;
                }
                walk.stack.push((t, fill, hidden));
            }
            "path" | "rect" => {
                if !walk.root {
                    return None;
                }
                let (outer, mut fill, mut hidden) = *walk.stack.last()?;
                let t = match attr("transform") {
                    Some(t) => outer.then_inner(&transform(t)?),
                    None => outer,
                };
                if let Some(v) = attr("fill") {
                    match paint(v)? {
                        Fill::Nothing => hidden = true,
                        f => fill = f,
                    }
                }
                let t = walk.view.then_inner(&t);
                let paths = if name == "path" { path_data(attr("d")?, &t)? } else { rect(&attrs, &t)? };
                if !hidden && !paths.is_empty() {
                    walk.out.push(MathShape { paths, fill: if let Fill::Rgb(c) = fill { Some(c) } else { None } });
                }
            }
            _ => return None,
        }
    }
    (walk.root && walk.stack.len() == 1).then_some(walk.out)
}

fn attributes(s: &str) -> Option<Vec<(String, String)>> {
    let mut out = vec![];
    let mut rest = s.trim_start();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let key = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = after[1..].find(quote)?;
        out.push((key, after[1..1 + end].to_owned()));
        rest = after[end + 2..].trim_start();
    }
    Some(out)
}

fn paint(value: &str) -> Option<Fill> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("currentColor") {
        return Some(Fill::Current);
    }
    if v.eq_ignore_ascii_case("none") {
        return Some(Fill::Nothing);
    }
    let hex = v.strip_prefix('#')?;
    let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i + n)?, 16).ok();
    match hex.len() {
        6 => Some(Fill::Rgb([digit(0, 2)?, digit(2, 2)?, digit(4, 2)?])),
        3 => Some(Fill::Rgb([digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17])),
        _ => None,
    }
}

fn numbers(s: &str) -> Option<Vec<f64>> {
    s.split([' ', ',']).filter(|p| !p.is_empty()).map(|p| p.parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

/// An SVG transform list: the leftmost function is the outermost.
fn transform(list: &str) -> Option<Affine> {
    let mut out = Affine::IDENTITY;
    let mut rest = list.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        let args = numbers(&rest[open + 1..close])?;
        let one = match (rest[..open].trim(), args.len()) {
            ("translate", 1) => Affine::translate(args[0], 0.0),
            ("translate", 2) => Affine::translate(args[0], args[1]),
            ("scale", 1) => Affine::scale(args[0], args[0]),
            ("scale", 2) => Affine::scale(args[0], args[1]),
            ("matrix", 6) => Affine::new(args[0], args[1], args[2], args[3], args[4], args[5]),
            _ => return None,
        };
        out = out.then_inner(&one);
        rest = rest[close + 1..].trim_start_matches([' ', ',']);
    }
    Some(out)
}

fn rect(attrs: &[(String, String)], t: &Affine) -> Option<Vec<SubPath>> {
    let get = |key: &str| -> Option<f64> {
        match attrs.iter().find(|(k, _)| k == key) {
            Some((_, v)) => v.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
            None => Some(0.0),
        }
    };
    let (x, y, w, h) = (get("x")?, get("y")?, get("width")?, get("height")?);
    let corner = |px: f64, py: f64| {
        let a = t.apply(Point::new(px, py));
        PathPoint { anchor: a, left: a, right: a }
    };
    Some(vec![SubPath { points: vec![corner(x, y), corner(x + w, y), corner(x + w, y + h), corner(x, y + h)], open: false }])
}

/// The outlines of `d` (every command but the arc), points mapped by `t`.
fn path_data(d: &str, t: &Affine) -> Option<Vec<SubPath>> {
    if d.contains(['A','a']) { return None; }
    let mut paths = crate::svg::path::parse(d, t)?;
    for p in &mut paths { p.open = false; }
    Some(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: f64, h: f64) -> Rect {
        Rect { left: 0.0, top: 0.0, right: w, bottom: h }
    }

    #[test]
    fn a_view_box_is_laid_onto_the_frame_and_the_flip_is_kept() {
        // y up inside scale(1,-1): the glyph's top (y=100) lands at the frame's top
        let svg = r#"<svg viewBox="0 -100 200 100"><g fill="currentColor" transform="scale(1,-1)"><path d="M0 0H200V100H0Z"/></g></svg>"#;
        let shapes = parse(svg, &frame(20.0, 10.0)).unwrap();
        assert_eq!(shapes.len(), 1);
        let b = shapes[0].paths[0].bounds().unwrap();
        assert!((b.left - 0.0).abs() < 1e-9 && (b.right - 20.0).abs() < 1e-9 && (b.top - 0.0).abs() < 1e-9 && (b.bottom - 10.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn a_rect_a_nested_group_and_an_own_fill_are_read() {
        let svg = r##"<svg viewBox="0 0 10 10"><g transform="translate(2,0) scale(2)" fill="#115395"><rect width="1" height="1" x="1" y="1"></rect></g></svg>"##;
        let shapes = parse(svg, &frame(10.0, 10.0)).unwrap();
        let b = shapes[0].paths[0].bounds().unwrap();
        assert_eq!(shapes[0].fill, Some([0x11, 0x53, 0x95]));
        assert!((b.left - 4.0).abs() < 1e-9 && (b.right - 6.0).abs() < 1e-9 && (b.top - 2.0).abs() < 1e-9 && (b.bottom - 4.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn quadratic_and_smooth_commands_become_cubics() {
        let paths = path_data("M0 0Q10 20 20 0T40 0Z", &Affine::IDENTITY).unwrap();
        let p = &paths[0].points;
        assert_eq!(p.len(), 3);
        // the first quadratic's control (10,20): cubic handles at 2/3 of the way
        assert!((p[0].right.x - 20.0 / 3.0).abs() < 1e-9 && (p[0].right.y - 40.0 / 3.0).abs() < 1e-9);
        assert!((p[1].left.x - 40.0 / 3.0).abs() < 1e-9 && (p[1].left.y - 40.0 / 3.0).abs() < 1e-9);
        // T reflects the previous control: (30,-20)
        assert!((p[1].right.y + 40.0 / 3.0).abs() < 1e-9 && (p[2].left.y + 40.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn relative_and_implicit_commands_follow_the_svg_rules() {
        let paths = path_data("m10 10 20 0v10h-20z m5 5l1-1", &Affine::IDENTITY).unwrap();
        assert_eq!(paths.len(), 2);
        let a: Vec<(f64, f64)> = paths[0].points.iter().map(|p| (p.anchor.x, p.anchor.y)).collect();
        assert_eq!(a, vec![(10.0, 10.0), (30.0, 10.0), (30.0, 20.0), (10.0, 20.0)]);
        // after z the pen is back at the subpath start: the next relative move starts there
        assert_eq!((paths[1].points[0].anchor.x, paths[1].points[0].anchor.y), (15.0, 15.0));
    }

    #[test]
    fn what_this_reader_does_not_know_is_not_drawn_wrongly() {
        let frame = frame(10.0, 10.0);
        assert!(parse(r#"<svg viewBox="0 0 1 1"><path d="M0 0A1 1 0 0 1 1 1Z"/></svg>"#, &frame).is_none());
        assert!(parse(r##"<svg viewBox="0 0 1 1"><use href="#a"/></svg>"##, &frame).is_none());
        assert!(parse(r#"<svg viewBox="0 0 1 1"><svg viewBox="0 0 1 1"></svg></svg>"#, &frame).is_none());
        assert!(parse(r#"<svg viewBox="0 0 1 1"><g transform="rotate(5)"></g></svg>"#, &frame).is_none());
    }

    #[test]
    fn the_equation_colour_is_the_objects_rgb_lightened_by_its_tint() {
        let mut math = MathObject { id: String::new(), source: None, font_size: None, rgb: None, tint: None, swatch: None, is_mathml: None, svg_data: None };
        assert_eq!(equation_rgb(&math), [0, 0, 0]);
        math.rgb = Some([17.0, 83.0, 149.0]);
        math.tint = Some(100.0);
        assert_eq!(equation_rgb(&math), [17, 83, 149]);
        math.tint = Some(50.0);
        assert_eq!(equation_rgb(&math), [136, 169, 202]);
    }
}
