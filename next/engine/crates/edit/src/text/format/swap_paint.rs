//! Swap Fill and Stroke (Shift+X, the arrow of the toolbox's fill/stroke widget) on text: every run of the range, or of the whole
//! story of each selected text frame (Formatting Affects Text on), trades its own fill and stroke colour and tint.
//!
//! Measured in InDesign 2025 with real input (artifacts/toolbox-c2-20261023, sessions 1, 2 and 4): a frame selected with T on, a text
//! range and a caret all swap; a range or story of several runs swaps run by run (blue/green and purple/orange runs became
//! green/blue and orange/purple, not one pair for the whole range); the frame's own fill and stroke, the stroke weight and every
//! other character attribute stay. The gradient placement (start, length, angle) trades with its paint (toolbox-c3, session 6: a gradient fill
//! over a solid stroke gave the stroke the gradient with the same placement; both gradients traded placements; a solid paint takes the
//! default placement 0 0 / automatic / 0). A caret has no characters: the text typed next gets the swapped paint (that is the control
//! bar's own caret format, `textFormat` at a caret, not this command).
use super::*;

/// The paint of one run as raw attribute values: fill colour, fill tint, stroke colour, stroke tint, then the fill's gradient placement
/// (start, length, angle) and the stroke's ("" = not set).
type Paint = [String; 10];

/// A gradient placement with InDesign's defaults for what is not set: start 0 0, length -1 (automatic), angle 0.
fn place(start: &str, length: &str, angle: &str) -> (String, f64, f64) {
    let n: Vec<f64> = start.split_ascii_whitespace().filter_map(|v| v.parse().ok()).collect();
    let start = if n.len() == 2 { format!("{} {}", n[0], n[1]) } else { "0 0".to_owned() };
    (start, length.trim().parse().unwrap_or(-1.0), angle.trim().parse().unwrap_or(0.0))
}

fn tint(v: &str) -> f64 {
    v.trim().parse::<f64>().ok().filter(|n| n.is_finite()).unwrap_or(-1.0)
}

/// The fill and stroke the character attributes `a` resolve to (an unset fill is [Black], an unset stroke [None]).
fn paint_of(resolved: &Attrs) -> Paint {
    let get = |k: &str| val(resolved, k).to_owned();
    [get("FillColor"), get("FillTint"), get("StrokeColor"), get("StrokeTint"),
     get("GradientFillStart"), get("GradientFillLength"), get("GradientFillAngle"), get("GradientStrokeStart"), get("GradientStrokeLength"), get("GradientStrokeAngle")]
}

/// Every run of `[start, end)` (atoms that share their paragraph and character range) with its two paints swapped.
pub(crate) fn swap_range(w: &mut Work<'_>, id: &str, start: usize, end: usize) -> Result<(), EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let (part, ix, i, id) = locate(w, id)?;
    let s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, start, end)?;
    // run = (first atom, one past the last atom, its resolved paint)
    let mut runs: Vec<(usize, usize, Paint)> = Vec::new();
    let mut key: Option<(usize, usize)> = None;
    for at in start..end {
        let (p, c) = (s.atoms[at].p, s.atoms[at].c);
        if key == Some((p, c)) {
            if let Some(last) = runs.last_mut() { last.1 = at + 1; }
            continue;
        }
        let pa = style_attrs(&s.paragraphs[p]);
        let ca = style_attrs(&s.characters[c]);
        let mut resolved = catalog.character(&pa, &ca)?;
        resolved.extend(ca.clone());
        runs.push((at, at + 1, paint_of(&resolved)));
        key = Some((p, c));
    }
    for (a, b, [fill, fill_tint, stroke, stroke_tint, fgs, fgl, fga, sgs, sgl, sga]) in runs {
        let fill = if fill.is_empty() { "Color/Black".to_owned() } else { fill };
        let stroke = if stroke.is_empty() { "Swatch/None".to_owned() } else { stroke };
        let (fp, sp) = (place(&fgs, &fgl, &fga), place(&sgs, &sgl, &sga));
        if fill == stroke && tint(&fill_tint) == tint(&stroke_tint) && fp == sp { continue; }
        let mut props = BTreeMap::from([
            ("fillColor".to_owned(), Value::from(stroke)),
            ("strokeColor".to_owned(), Value::from(fill)),
            ("fillTint".to_owned(), Value::from(tint(&stroke_tint))),
            ("strokeTint".to_owned(), Value::from(tint(&fill_tint))),
        ]);
        if fp != sp {
            for (key, (start, length, angle)) in [("gradientFill", sp), ("gradientStroke", fp)] {
                props.insert(format!("{key}Start"), Value::from(start));
                props.insert(format!("{key}Length"), Value::from(length));
                props.insert(format!("{key}Angle"), Value::from(angle));
            }
        }
        format(w, &id, a, b, &props)?;
    }
    Ok(())
}

/// Swap Fill and Stroke with T on and text frames selected: the whole story of each (threaded frames and overflow included, like
/// `formatFrameText`).
pub(crate) fn swap_frames(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    let stories = frame_text::stories(w, ids)?;
    if stories.is_empty() {
        return Err(EditError::new("notText", "none of the items holds text"));
    }
    for story in stories {
        let (part, ix, i, id) = locate(w, &story)?;
        let length = Story::read(w.text(&part)?, &ix, i)?.atoms.len();
        if length > 0 { swap_range(w, &id, 0, length)?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Editor};

    // the measured case: blue/green and purple/orange runs (InDesign 2025, DOM textStyleRanges after Shift+X with T on)
    const STORY: &str = r#"<Story Self="s"><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Body"><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/None" FillColor="Color/Blue" StrokeColor="Color/Green" StrokeTint="50" GradientFillStart="10 20" GradientFillLength="150" GradientFillAngle="30"><Content>Hello </Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/None" FillColor="Color/Purple" StrokeColor="Color/Orange"><Content>Folio text</Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/None"><Content>x</Content></CharacterStyleRange></ParagraphStyleRange></Story>"#;
    const SPREAD: &str = r#"<Spread Self="sp"><TextFrame Self="f1" ParentStory="s" ContentType="TextType"/><Rectangle Self="r"/></Spread>"#;

    fn editor() -> Editor {
        let styles = r#"<Styles><ParagraphStyle Self="ParagraphStyle/Body" Name="Body" PointSize="12" FontStyle="Regular" LeftIndent="0"><Properties><AppliedFont type="string">Arial</AppliedFont></Properties></ParagraphStyle><CharacterStyle Self="CharacterStyle/None" Name="[None]"/></Styles>"#;
        let colors: String = ["Blue", "Green", "Purple", "Orange", "Black"].iter()
            .map(|c| format!(r#"<Color Self="Color/{c}" Name="{c}" Model="Process" Space="RGB" ColorValue="0 0 255"/>"#)).collect();
        let graphic = format!("<Graphic>{colors}</Graphic>");
        Editor::new(BTreeMap::from([
            ("Resources/Styles.xml".into(), styles.into()),
            ("Resources/Graphic.xml".into(), graphic),
            ("Stories/Story_s.xml".into(), STORY.into()),
            ("Spreads/Spread_sp.xml".into(), SPREAD.into()),
        ]))
    }
    fn paints(e: &mut Editor) -> Vec<(String, String)> {
        let f = e.text_format("s", 0, 6).unwrap();
        let g = e.text_format("s", 6, 16).unwrap();
        let h = e.text_format("s", 16, 17).unwrap();
        [f, g, h].iter().map(|v| (v["fillColor"].as_str().unwrap_or("?").to_owned(), v["strokeColor"].as_str().unwrap_or("?").to_owned())).collect()
    }

    #[test]
    fn swaps_run_by_run_with_tints() {
        let mut e = editor();
        e.apply("swap", &[Command::Registered(crate::commands::RegisteredCommand::SwapTextPaint { story: "s".into(), start: 0, end: 17 })]).unwrap();
        let p = paints(&mut e);
        assert_eq!(p[0], ("Color/Green".to_owned(), "Color/Blue".to_owned()));
        assert_eq!(p[1], ("Color/Orange".to_owned(), "Color/Purple".to_owned()));
        // an unset fill ([Black]) and stroke ([None]) trade too
        assert_eq!(p[2], ("Swatch/None".to_owned(), "Color/Black".to_owned()));
        let tints = e.text_format("s", 0, 6).unwrap();
        assert_eq!(tints["fillTint"].as_f64(), Some(50.0));
        assert_eq!(tints["strokeTint"].as_f64(), Some(-1.0));
    }

    #[test]
    fn a_range_swaps_only_its_runs_and_twice_is_the_start() {
        let mut e = editor();
        e.apply("swap", &[Command::Registered(crate::commands::RegisteredCommand::SwapTextPaint { story: "s".into(), start: 6, end: 16 })]).unwrap();
        let p = paints(&mut e);
        assert_eq!(p[0], ("Color/Blue".to_owned(), "Color/Green".to_owned()));
        assert_eq!(p[1], ("Color/Orange".to_owned(), "Color/Purple".to_owned()));
        e.apply("swap", &[Command::Registered(crate::commands::RegisteredCommand::SwapTextPaint { story: "s".into(), start: 6, end: 16 })]).unwrap();
        assert_eq!(paints(&mut e)[1], ("Color/Purple".to_owned(), "Color/Orange".to_owned()));
    }

    #[test]
    fn frames_swap_their_story_and_leave_other_items() {
        let mut e = editor();
        e.apply("swap", &[Command::Registered(crate::commands::RegisteredCommand::SwapFrameTextPaint { ids: vec!["f1".into(), "r".into()] })]).unwrap();
        assert_eq!(paints(&mut e)[0], ("Color/Green".to_owned(), "Color/Blue".to_owned()));
        let none = e.apply("swap", &[Command::Registered(crate::commands::RegisteredCommand::SwapFrameTextPaint { ids: vec!["r".into()] })]);
        assert!(none.is_err());
    }

    #[test]
    fn the_gradient_placement_goes_with_its_paint() {
        // InDesign 2025 (toolbox-c3 session 6): a fill placed 10 20 / 150 / 30 over a solid stroke: after Shift+X the stroke has it and the fill is back to the default placement
        let mut e = editor();
        let swap = || [Command::Registered(crate::commands::RegisteredCommand::SwapTextPaint { story: "s".into(), start: 0, end: 17 })];
        e.apply("swap", &swap()).unwrap();
        let f = e.text_format("s", 0, 6).unwrap();
        assert_eq!(f["gradientStrokeStart"], serde_json::json!([10.0, 20.0]));
        assert_eq!((f["gradientStrokeLength"].as_f64(), f["gradientStrokeAngle"].as_f64()), (Some(150.0), Some(30.0)));
        assert_eq!((f["gradientFillStart"].clone(), f["gradientFillLength"].as_f64(), f["gradientFillAngle"].as_f64()), (serde_json::json!([0.0, 0.0]), Some(-1.0), Some(0.0)));
        e.apply("swap", &swap()).unwrap();
        let f = e.text_format("s", 0, 6).unwrap();
        assert_eq!((f["gradientFillStart"].clone(), f["gradientFillLength"].as_f64(), f["gradientFillAngle"].as_f64()), (serde_json::json!([10.0, 20.0]), Some(150.0), Some(30.0)));
    }
}
