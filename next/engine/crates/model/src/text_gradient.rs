//! Gradients on text (InDesign 2025, measured with the DOM and a PDF export: `artifacts/gradtext-c1-20261023/probes`).
//!
//! A character's fill or stroke may be a gradient swatch. Where the gradient sits is not stored per
//! glyph but per range, as three IDML attributes of the character range (`GradientFillStart`,
//! `GradientFillLength`, `GradientFillAngle`; the `GradientStroke…` set for the stroke):
//!
//! - **Automatic** (`start` 0 0 and `length` −1, the defaults): every line of every column is painted with
//!   the whole gradient across the column's text area: a linear gradient runs from the column's left to its
//!   right edge (the angle is not used), a radial one is centred in the column with half the column's width
//!   as its radius. Indents, alignment and the length of the range do not change it.
//! - **Explicit** (any other value): one gradient for the range inside its text frame. `start` is measured from the
//!   top left of the frame's text area (the frame less its inset; the first column's top left, however many
//!   columns there are), `length` and `angle` (degrees, counter-clockwise) as for an object. A frame that
//!   continues in a threaded frame has its own origin in each frame. A turned frame turns the gradient with it.
//!   A start with the length still −1 gives a gradient of length 0.

use crate::{GradientGeometry, Point, Rect};
use serde::{Deserialize, Serialize};

/// Where the gradient of a character range's fill or stroke sits (see the module).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextGradient {
    /// From the top left of the frame's text area.
    pub start: Point,
    /// Points; negative with the start at 0 0 = automatic.
    pub length: f64,
    /// Degrees, counter-clockwise.
    pub angle: f64,
}

impl Default for TextGradient {
    fn default() -> Self {
        Self { start: Point::new(0.0, 0.0), length: -1.0, angle: 0.0 }
    }
}

/// The text area a line's gradient is measured against, in the frame's inner coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GradientArea {
    /// The column the line is in.
    pub column: Rect,
    /// Top left of the frame's text area (an explicit gradient's origin).
    pub origin: Point,
}

/// The gradient a glyph is painted with, and the area it is measured against once the frame is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacedGradient {
    pub spec: TextGradient,
    /// `None` where the layout gives no area (table cells, footnotes): the gradient shows as its flat stand-in.
    pub area: Option<GradientArea>,
}

impl TextGradient {
    /// The attributes' defaults: nothing is written to IDML.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Automatic placement: the start at 0 0 and no length.
    pub fn is_auto(&self) -> bool {
        self.start == Point::new(0.0, 0.0) && self.length < 0.0
    }

    /// The gradient's geometry in the frame's inner coordinates.
    pub fn place(&self, area: &GradientArea, radial: bool) -> GradientGeometry {
        if self.is_auto() {
            let c = &area.column;
            return if radial {
                GradientGeometry { start: Point::new((c.left + c.right) / 2.0, (c.top + c.bottom) / 2.0), length: (c.right - c.left) / 2.0, ..GradientGeometry::default() }
            } else {
                GradientGeometry { start: Point::new(c.left, c.bottom), length: c.right - c.left, angle: 0.0, ..GradientGeometry::default() }
            };
        }
        GradientGeometry {
            start: Point::new(area.origin.x + self.start.x, area.origin.y + self.start.y),
            length: self.length.max(0.0),
            angle: self.angle,
            ..GradientGeometry::default()
        }
    }
}

impl crate::CharacterAttrs {
    /// What a glyph of these attributes carries for its fill and stroke: the gradient's placement, for a gradient paint.
    pub fn placed_gradients(&self) -> (Option<PlacedGradient>, Option<PlacedGradient>) {
        let of = |paint: &crate::Paint, spec: TextGradient| matches!(paint, crate::Paint::Gradient { .. }).then_some(PlacedGradient { spec, area: None });
        (of(&self.fill, self.fill_gradient), of(&self.stroke, self.stroke_gradient))
    }
}

impl PlacedGradient {
    /// The geometry, when the frame's layout gave the area.
    pub fn geometry(&self, radial: bool) -> Option<GradientGeometry> {
        self.area.as_ref().map(|a| self.spec.place(a, radial))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> GradientArea {
        GradientArea { column: Rect { left: 10.0, top: 20.0, right: 170.0, bottom: 80.0 }, origin: Point::new(10.0, 20.0) }
    }

    /// InDesign 2025 (probe2 C1, C2, C3): the whole column, whatever the angle (probe2 C4).
    #[test]
    fn automatic_linear_spans_the_column() {
        let g = TextGradient { angle: 90.0, ..TextGradient::default() }.place(&area(), false);
        assert_eq!((g.start.x, g.length, g.angle), (10.0, 160.0, 0.0));
    }

    /// InDesign 2025 (probe3 D6): the centre of the column, half its width.
    #[test]
    fn automatic_radial_is_centred() {
        let g = TextGradient::default().place(&area(), true);
        assert_eq!((g.start.x, g.start.y, g.length), (90.0, 50.0, 80.0));
    }

    /// InDesign 2025 (probe2 C5/C6, probe3 D1): from the text area's top left; a start with no length is a point.
    #[test]
    fn explicit_is_measured_from_the_text_area() {
        let g = TextGradient { start: Point::new(10.0, 5.0), length: 100.0, angle: 45.0 }.place(&area(), false);
        assert_eq!((g.start.x, g.start.y, g.length, g.angle), (20.0, 25.0, 100.0, 45.0));
        let g = TextGradient { start: Point::new(50.0, 0.0), ..TextGradient::default() }.place(&area(), false);
        assert_eq!(g.length, 0.0);
    }
}
