use folio_model::{FeatherCorners, FeatherEffect};

use super::super::{Alpha, noise, squared_distance_field};
use super::{diffusion, square};

// R15
pub(super) fn apply(a: &mut Alpha, ppu: f64, f: &FeatherEffect) {
    let resolution = (ppu * 72.0) as f32;
    let px = |pt: f64| (f64::from(pt as f32 * resolution) / 72.0) as usize;
    let width = f.width.max(0.0);
    let choke = f.choke.clamp(0.0, 1.0);
    let choke_px = px(choke * width);
    let feather_px = px((1.0 - choke) * width);
    if choke_px > 0 {
        ramp(a, choke_px, true, false);
    }
    if feather_px > 0 {
        match f.corners {
            FeatherCorners::Sharp => ramp(a, feather_px, true, true),
            FeatherCorners::Rounded => {
                ramp(a, feather_px, true, false);
                ramp(a, feather_px, false, true);
            }
            FeatherCorners::Diffusion => diffusion::apply(a, feather_px),
        }
    }
    noise::feather(a, f.noise);
}

// R16
fn rounded_distance(mut squared: u32) -> u32 {
    if squared < 1024 {
        return (8.0 * f64::from(squared).sqrt()).round() as u32 / 8;
    }
    let mut scale = 1;
    while squared >= 65536 {
        squared /= 4;
        scale *= 2;
    }
    (8.0 * f64::from(squared / 64).sqrt()).round() as u32 * scale
}

fn ramp(a: &mut Alpha, radius: usize, inside: bool, curved: bool) {
    if !inside && !a.data.contains(&255) {
        return;
    }
    let seed = if inside { 0 } else { 255 };
    let field = squared_distance_field(a, inside, |v| v == seed);
    let limit = (radius as f64).powi(2);
    for (value, squared) in a.data.iter_mut().zip(field) {
        let end = if inside { 255 } else { 0 };
        let level = if squared >= limit {
            end
        } else if curved {
            let v = ((u64::from(rounded_distance(squared as u32)) * 255 / radius as u64).min(255))
                as u32;
            255 - square(if inside { 255 - v } else { v })
        } else {
            255 - end
        };
        *value = if inside {
            (*value).min(level)
        } else {
            (*value).max(level)
        };
    }
}
