//! Contour-based protrusion at the two ends of a horizontal line.

mod outline;
mod profile;

use outline::{Outline, VerticalGuides};
use profile::{LetterHeight, Profile, Wall, tilt_step};
use ttf_parser::Face;

const BAND_COUNT: usize = 100;
const LAST_BAND: i32 = BAND_COUNT as i32 - 1;
const UNITS_PER_EM: f64 = 10_000.0;
type Bands = [i32; BAND_COUNT];

/// Signed optical protrusion in em. Positive values extend into the margin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub left: f64,
    pub right: f64,
}

/// Returns the optical edges of the character in the selected font face.
pub fn edges(face: &Face<'_>, ch: char, scale: f64) -> Option<Edges> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    // R3
    let scale = (scale * 10_000.0).round() / 10_000.0;
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }

    let outline = Outline::read(face, ch)?;
    let guides = VerticalGuides::from_face(face);
    let glyph = outline.sample(guides.baseline)?;
    let tilt = if ch == 'I' {
        tilt_step(&glyph, guides)
    } else {
        Outline::read(face, 'I')
            .and_then(|stem| stem.sample(guides.baseline))
            .map_or(0, |stem| tilt_step(&stem, guides))
    };

    let letter_height = LetterHeight::of(ch);
    let wall = Wall::new(guides, letter_height, scale);
    let edge = |bands, bearing: f64| {
        let profile = Profile::new(bands, glyph.bottom, glyph.top, guides, letter_height, tilt);
        let adjustment = wall.adjustment(profile, tilt);
        // R8
        (bearing + glyph.width * 0.75)
            .min(scale * bearing - (f64::from(adjustment) + 2853.0) / UNITS_PER_EM)
    };
    Some(Edges {
        left: edge(glyph.left, glyph.left_bearing),
        right: edge(glyph.right, glyph.right_bearing),
    })
}
