//! The bracket handles InDesign 2025 draws on a selected text path (Selection, Direct Selection and Type on a Path tool): a start and an end
//! bracket (a line across the path with a small square on the outer side: the text port) and a centre tick above the middle of the range.
//! Measured with screenshots of the document view (artifacts/typepath-c14-20261023, rules next/oracle/rules/typepath-c14.ko.md):
//! - the lines stand where the bracket distances land on the path the text walks (the stroke's offset path for Top/Bottom alignment) and
//!   reach from `above` over the path to `below` under it: the face's bounding box (`head` yMax, -yMin) times the point size, the largest
//!   over the glyphs of the text; they do not change with the text itself, the flip, the spacing or the text alignment;
//! - "above" is the side the glyph tops are on (the left of travel on screen, `(v.y, -v.x)`).
use folio_model::{Affine, PageItem, layout::FrameText, textpath::{OffsetJoin, PathMeasure}};
use serde::Serialize;

use crate::FontStore;

/// One bracket: its distance along the walked path, the point there and the unit direction of travel, in the item's own coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Handle { pub at: f64, pub point: [f64; 2], pub tangent: [f64; 2] }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BracketHandles {
    pub start: Handle,
    pub end: Handle,
    pub center: Handle,
    /// How far the bracket lines reach to the glyph-top side and to the other side of the path (points).
    pub above: f64,
    pub below: f64,
    pub closed: bool,
    pub flipped: bool,
    /// Length of the walked path (0 for the horizontal baseline the model keeps without a curve).
    pub length: f64,
}

/// What a text path with no characters yet shows (a new Type on a Path press): InDesign's default 12 pt in the document's font; the face is
/// not known here, so the Arial-like box (1.04 above, 0.32 below) stands in.
const EMPTY_SIZE: f64 = 12.0;

/// The extents of the bracket lines from the laid text of the path (`None`: no characters yet).
pub fn extents(laid: Option<&FrameText>, fonts: &FontStore) -> (f64, f64) {
    let (mut above, mut below) = (0.0f64, 0.0f64);
    for glyph in laid.into_iter().flat_map(|f| f.lines.iter()).flat_map(|l| l.glyphs.iter()) {
        let m = fonts.metrics(glyph.font);
        above = above.max(m.bbox_top * glyph.size * glyph.vscale);
        below = below.max(m.bbox_bottom * glyph.size * glyph.vscale);
    }
    if above == 0.0 && below == 0.0 { (1.04 * EMPTY_SIZE, 0.32 * EMPTY_SIZE) } else { (above, below) }
}

/// The handles of `item`'s text path with the brackets at `start` and `end` (the model's values, or those of a drag in progress). A plain horizontal line
/// (left to right, default options) is kept by the model as a baseline with `start` and `end` already counted along x, so its handles stand on that baseline.
pub fn bracket_handles(item: &PageItem, start: f64, end: f64, above: f64, below: f64) -> Option<BracketHandles> {
    let path = item.text_path.as_ref()?;
    let lift = path.path_alignment.height(if path.flipped { -item.stroke.weight } else { item.stroke.weight });
    let Some(curve) = path.curve.as_ref() else {
        let at = |x: f64| Handle { at: x, point: [x, path.baseline - lift], tangent: [1.0, 0.0] };
        return Some(BracketHandles { start: at(start), end: at(end), center: at((start + end) / 2.0), above, below, closed: false, flipped: false, length: 0.0 });
    };
    let measure = PathMeasure::new(curve)?;
    let closed = !curve.open;
    // R13

    let walked = (lift != 0.0)
        .then(|| measure.offset_with(lift, OffsetJoin::of_stroke(item.stroke.join, item.stroke.miter_limit.unwrap_or(4.0)))).flatten()
        .map_or(measure, |o| o.path);
    let at = |d: f64| {
        let (p, v) = walked.at(d);
        let n = v.x.hypot(v.y);
        Handle { at: d, point: [p.x, p.y], tangent: if n > 0.0 { [v.x / n, v.y / n] } else { [1.0, 0.0] } }
    };
    // A closed shape's range may wrap over the path's start (end < start): it then runs on by one more round.
    let end = if closed && end < start { end + walked.length() } else { end };
    Some(BracketHandles { start: at(start), end: at(end), center: at((start + end) / 2.0), above, below, closed, flipped: path.flipped, length: walked.length() })
}

/// A point of the item's own coordinates in the coordinates of the item's parent chain (`transform`: inner → outer).
pub fn through(transform: &Affine, p: [f64; 2]) -> [f64; 2] {
    let q = transform.apply(folio_model::Point::new(p[0], p[1]));
    [q.x, q.y]
}
