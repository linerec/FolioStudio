//! Tab leaders are decoration glyphs: they do not change text or caret positions.
use crate::shape::FaceTable;
use folio_model::{CharacterAttrs, ParagraphAttrs};
use folio_model::layout::{FontId, PlacedGlyph};

pub(super) fn place(
    table: &FaceTable, para: &ParagraphAttrs, attrs: &CharacterAttrs,
    font: FontId, size: f64, x: f64, y: f64, width: f64, left: f64,
) -> Vec<PlacedGlyph> {
    let Some(stop) = para.tabs.iter().filter(|s| s.position > x - left + super::EPS)
        .min_by(|a, b| a.position.total_cmp(&b.position)) else { return Vec::new() };
    let pattern: Vec<_> = stop.leader.chars().filter_map(|c| table.glyph(font, c))
        .map(|(g, a)| (g, (a * 1000.0).round() / 1000.0)).collect();
    let hscale = attrs.horizontal_scale / 100.0;
    let advance: f64 = pattern.iter().map(|(_, a)| a * size * hscale).sum();
    if !advance.is_finite() || advance <= super::EPS || width < advance { return Vec::new() }
    let mut result = Vec::new();
    let mut at = x;
    for repeat in 0..((width / advance).ceil() as usize).min(10000) {
        if at + advance > x + width + super::EPS { break }
        for &(glyph, a) in &pattern {
            result.push(PlacedGlyph {
                font, glyph, x: at, y, size, hscale,
                path_transform: None,
                vscale: attrs.vertical_scale / 100.0, skew: attrs.skew, fill: attrs.fill.clone(),
                stroke: attrs.stroke.clone(), stroke_weight: attrs.stroke_weight,
                overprint_fill: attrs.overprint_fill, overprint_stroke: attrs.overprint_stroke,
                fill_gradient: attrs.placed_gradients().0, stroke_gradient: attrs.placed_gradients().1,
            });
            at += a * size * hscale;
        }
        if repeat == 0 { at = left + ((x - left) / advance).ceil() * advance + advance; }
    }
    result
}
