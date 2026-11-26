use std::fmt::Write;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

#[derive(Default)]
struct Path(String);
impl OutlineBuilder for Path {
    fn move_to(&mut self, x: f32, y: f32) { let _ = write!(self.0, "M{x} {y}"); }
    fn line_to(&mut self, x: f32, y: f32) { let _ = write!(self.0, "L{x} {y}"); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { let _ = write!(self.0, "Q{x1} {y1} {x} {y}"); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { let _ = write!(self.0, "C{x1} {y1} {x2} {y2} {x} {y}"); }
    fn close(&mut self) { self.0.push('Z'); }
}
pub(super) fn read(face: &Face<'_>, id: GlyphId) -> (String, Option<[i16; 4]>) {
    let mut path = Path::default();
    let bounds = face.outline_glyph(id, &mut path).map(|b| [b.x_min, b.y_min, b.x_max, b.y_max]);
    (path.0, bounds)
}
