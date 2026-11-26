use std::sync::OnceLock;

use super::{Alpha, matte::FULL};

// M5
fn grain() -> &'static [i8; 16384] {
    static GRAIN: OnceLock<[i8; 16384]> = OnceLock::new();
    GRAIN.get_or_init(|| {
        let mut state = 0x4358ebc4u64;
        std::array::from_fn(|_| {
            state = state * 16807 % 0x7fffffff;
            (state >> 8) as i8
        })
    })
}

fn hash(value: u32, multiplier: u32) -> u32 {
    value.wrapping_mul(multiplier).wrapping_add(0x3039) >> 3
}

pub(super) fn matte(values: &mut [i32], w: usize, percent: f64) {
    let level = (percent.clamp(0.0, 100.0) * 255.0 / 100.0).round() as i32;
    if level == 0 {
        return;
    }
    let grain = grain();
    let amp = 512 * level;
    for (y, row) in values.chunks_exact_mut(w).enumerate() {
        let hh = hash(y as u32, 0x64e6d);
        for (x, value) in row.iter_mut().enumerate().filter(|(_, v)| **v != 0) {
            let i = (hash(hh ^ x as u32, 0x4e6d) & 0x3fff) as usize;
            *value = (*value + ((i32::from(grain[i]) * amp) >> 8)).clamp(0, FULL);
        }
    }
}

pub(super) fn feather(a: &mut Alpha, percent: f64) {
    let level = ((percent / 100.0) as f32 * 255.0) as i32 & 0xff;
    if percent <= 0.0 || level == 0 {
        return;
    }
    let grain = grain();
    for (y, row) in a.data.chunks_exact_mut(a.w).enumerate() {
        let hh = hash(y as u32, 0x41c64e6d);
        for (x, pixel) in row.iter_mut().enumerate().filter(|(_, v)| **v != 0) {
            let edge = i32::from((*pixel).min(255 - *pixel));
            let amp = level.min((100 * edge + 6) / 13);
            let i = (hash(hh ^ x as u32, 0x4e6d) & 0x3fff) as usize;
            *pixel = (i32::from(*pixel) + ((i32::from(grain[i]) * amp) >> 7)).clamp(1, 255) as u8;
        }
    }
}
