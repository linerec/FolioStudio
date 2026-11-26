//! Condition indicators (Window > Type & Tables > Conditional Text): the marks InDesign draws under text of a condition.
//! Measured from the PDF of InDesign 2025 (scenarios condx-ind-*, rules next/oracle/rules/condx-c2.ko.md):
//! - a wavy underline is a zigzag of whole 1 pt steps (`floor(width)` of them) between 0.5 and 1.5 pt below the baseline,
//!   stroked 0.5 pt wide; a solid or dashed (3 on, 3 off) underline is a line 1.5 pt below the baseline, as wide as the text;
//! - the conditions of a range stack upward in definition order, 1 pt apart, and only the underline conditions take a slot;
//! - the colour is the condition's interface colour in the table of `folio_model::conditions::indicator_rgb`.
//! - a highlight (`UseHighlight`) is an opaque rectangle in the condition's colour behind the range, as wide as the range;
//!   its height is the line's: above the baseline the largest `size x head.yMax` of the faces on the line, below it the
//!   largest `size x -head.yMin` (the same for every range of the line; scenarios condx-hl-*, rules condx-c4.ko.md).
//!   A highlight takes no slot of the underline stack.
//!   `ShowIndicators` draws on screen only; `ShowAndPrintIndicators` also prints.
use folio_model::conditions::{indicator_rgb, Indicators};
use folio_model::geom::{Dash, PartShape};
use folio_model::layout::{Decoration, DecorationKind, TextShape};
use folio_model::{Document, LineCap, LineJoin, Paint, PathPoint, Point, SubPath};

const WEIGHT: f64 = 0.5;

fn line(points: &[(f64, f64)], dash: Option<Dash>) -> TextShape {
    let points = points.iter().map(|&(x, y)| { let a = Point::new(x, y); PathPoint { anchor: a, left: a, right: a } }).collect();
    TextShape {
        paths: vec![SubPath { points, open: true }],
        shape: PartShape::Line { width: WEIGHT, cap: LineCap::Butt, join: LineJoin::Miter, miter_limit: 10.0, dash },
        paint: Paint::None,
        overprint: false,
    }
}

/// The filled rectangle of a highlight: `width` wide and `height` tall from the decoration's origin (its top left).
fn block(width: f64, height: f64) -> TextShape {
    let points = [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)]
        .iter().map(|&(x, y)| { let a = Point::new(x, y); PathPoint { anchor: a, left: a, right: a } }).collect();
    TextShape { paths: vec![SubPath { points, open: false }], shape: PartShape::Fill, paint: Paint::None, overprint: false }
}

/// The indicators of runs of text (`(conditions, left, right, baseline)`), in the order of the runs.
/// `extent` is how far the line's faces reach above and below the baseline (a highlight's height).
pub(super) fn decorations(doc: &Document, runs: &[(&[String], f64, f64, f64, usize)], extent: (f64, f64)) -> Vec<Decoration> {
    let mut out = Vec::new();
    for &(ids, x0, x1, baseline, _) in runs {
        let mut slot = 0.0;
        // Several highlight conditions share the one rectangle: each takes an equal part of its height, the first
        // (in definition order) on top (condx-hl-both).
        let highlights = doc.conditions.iter().filter(|c| ids.contains(&c.id) && c.indicator_method == "UseHighlight").count();
        let mut part = 0;
        for c in doc.conditions.iter().filter(|c| ids.contains(&c.id)) {
            let Some(rgb) = indicator_rgb(&c.indicator_color) else { continue };
            let width = x1 - x0;
            if c.indicator_method == "UseHighlight" {
                let height = (extent.0 + extent.1) / highlights as f64;
                if width > 0.0 && height > 0.0 {
                    out.push(Decoration {
                        kind: DecorationKind::Highlight, x0, x1, y: baseline - extent.0 + height * part as f64, thickness: height,
                        shapes: vec![block(width, height)], rgb: Some(rgb), screen_only: doc.condition_indicators != Indicators::ShowAndPrint,
                        ..Default::default()
                    });
                }
                part += 1;
                continue;
            }
            let (y, shape) = match c.underline_appearance.as_str() {
                "Solid" => (baseline + 1.5 - slot, line(&[(0.0, 0.0), (width, 0.0)], None)),
                "Dashed" => (baseline + 1.5 - slot, line(&[(0.0, 0.0), (width, 0.0)], Some(Dash { array: vec![3.0], phase: 0.0 }))),
                _ => {
                    let steps = width.floor() as usize;
                    if steps == 0 { slot += 1.0; continue; }
                    (baseline + 0.5 - slot, line(&(0..=steps).map(|i| (i as f64, (i % 2) as f64)).collect::<Vec<_>>(), None))
                }
            };
            slot += 1.0;
            out.push(Decoration {
                kind: DecorationKind::Underline, x0, x1, y, thickness: WEIGHT, shapes: vec![shape],
                rgb: Some(rgb), screen_only: doc.condition_indicators != Indicators::ShowAndPrint, ..Default::default()
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::conditions::Condition;

    fn cond(id: &str, color: &str, method: &str, appearance: &str) -> Condition {
        Condition { id: id.into(), name: id.into(), visible: true, indicator_method: method.into(), underline_appearance: appearance.into(), indicator_color: color.into() }
    }

    fn doc(mode: Indicators, conditions: Vec<Condition>) -> Document {
        Document { conditions, condition_indicators: mode, ..Default::default() }
    }

    fn points(d: &Decoration) -> Vec<(f64, f64)> {
        d.shapes[0].paths[0].points.iter().map(|p| (p.anchor.x, p.anchor.y)).collect()
    }

    /// condx-ind-print: 'pha ' of Print (Red) and Web (LightBlue) over a baseline at 46.92 (InDesign's PDF).
    #[test]
    fn wavy_marks_are_whole_one_point_steps_stacked_upward_in_definition_order() {
        let d = doc(Indicators::ShowAndPrint, vec![cond("Condition/Print", "Red", "UseUnderline", "Wavy"), cond("Condition/Web", "LightBlue", "UseUnderline", "Wavy")]);
        let ids = ["Condition/Web".to_owned(), "Condition/Print".to_owned()];
        let out = decorations(&d, &[(&ids, 51.67, 75.02, 46.92, 0)], (0.0, 0.0));
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].rgb, out[1].rgb), (Some([255, 0, 0]), Some([79, 153, 255])));
        assert!((out[0].y - 47.42).abs() < 1e-9 && (out[1].y - 46.42).abs() < 1e-9);
        let p = points(&out[0]);
        assert_eq!(p.len(), 24, "floor(23.35) steps");
        assert_eq!((p[0], p[1], p[2]), ((0.0, 0.0), (1.0, 1.0), (2.0, 0.0)));
        assert!(!out[0].screen_only);
    }

    #[test]
    fn solid_and_dashed_marks_are_lines_below_the_wave_slots_and_highlights_take_no_slot() {
        let d = doc(Indicators::ShowAndPrint, vec![
            cond("Condition/A", "Red", "UseUnderline", "Solid"), cond("Condition/B", "Blue", "UseHighlight", "Wavy"), cond("Condition/C", "Green", "UseUnderline", "Dashed")]);
        let ids = ["Condition/A".to_owned(), "Condition/B".to_owned(), "Condition/C".to_owned()];
        let out = decorations(&d, &[(&ids, 10.0, 30.0, 100.0, 0)], (0.0, 0.0));
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].y, out[1].y), (101.5, 100.5));
        assert_eq!(points(&out[0]), [(0.0, 0.0), (20.0, 0.0)]);
        let PartShape::Line { dash, .. } = &out[1].shapes[0].shape else { panic!("a stroke") };
        assert_eq!(dash.as_ref().map(|d| d.array.clone()), Some(vec![3.0]));
    }

    /// condx-hl-arial-12 / condx-hl-both: the rectangle reaches the line's extent above and below the baseline;
    /// two highlight conditions split it in halves, the first on top.
    #[test]
    fn highlights_are_the_lines_extent_split_between_the_highlight_conditions() {
        let one = doc(Indicators::ShowAndPrint, vec![cond("Condition/Print", "Red", "UseHighlight", "Wavy")]);
        let ids = ["Condition/Print".to_owned()];
        let out = decorations(&one, &[(&ids, 51.67, 75.025, 797.2637, 0)], (12.668, 4.518));
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].kind, out[0].rgb, out[0].x0), (DecorationKind::Highlight, Some([255, 0, 0]), 51.67));
        assert!((out[0].y - (797.2637 - 12.668)).abs() < 1e-9 && (out[0].thickness - 17.186).abs() < 1e-9);
        let both = doc(Indicators::ShowAndPrint, vec![cond("Condition/Print", "Red", "UseHighlight", "Wavy"), cond("Condition/Web", "LightBlue", "UseHighlight", "Wavy")]);
        let ids = ["Condition/Web".to_owned(), "Condition/Print".to_owned()];
        let out = decorations(&both, &[(&ids, 0.0, 10.0, 100.0, 0)], (14.0, 6.0));
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].rgb, out[1].rgb), (Some([255, 0, 0]), Some([79, 153, 255])));
        assert_eq!((out[0].y, out[0].thickness, out[1].y, out[1].thickness), (86.0, 10.0, 96.0, 10.0));
    }

    #[test]
    fn marks_print_only_in_show_and_print_mode() {
        let c = vec![cond("Condition/A", "Red", "UseUnderline", "Wavy")];
        let ids = ["Condition/A".to_owned()];
        assert!(decorations(&doc(Indicators::Show, c.clone()), &[(&ids, 0.0, 5.0, 0.0, 0)], (0.0, 0.0))[0].screen_only);
        assert!(!decorations(&doc(Indicators::ShowAndPrint, c), &[(&ids, 0.0, 5.0, 0.0, 0)], (0.0, 0.0))[0].screen_only);
    }
}
