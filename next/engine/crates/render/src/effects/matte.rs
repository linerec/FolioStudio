use super::{Alpha, noise};

pub(super) const MAX_BLUR: usize = 250;
pub(super) const FULL: i32 = 255 * 256;

// M2, M4
const HALF: i32 = FULL / 2;

// R4
pub(super) fn split(radius: f32, share: f32) -> (usize, usize) {
    let total = (radius.max(0.0) as usize).min(MAX_BLUR);
    let percent = share.clamp(0.0, 100.0).floor();
    let spread = ((percent / 100.0 * total as f32 + 0.5) as usize).min(total);
    (spread, total - spread)
}

// M1
#[allow(clippy::approx_constant, reason = "the rounded factor sets the integer diagonal costs")]
fn chamfer(step: i32, cap: i32) -> [(isize, isize, i32); 24] {
    let costs = [
        [0, step, 2 * step],
        [
            step,
            (f64::from(step) * 1.41421) as i32,
            (f64::from(step) * 2.23607) as i32,
        ],
        [
            2 * step,
            (f64::from(step) * 2.23607) as i32,
            (f64::from(step) * 2.82843) as i32,
        ],
    ];
    let mut offsets = [(0, 0, 0); 24];
    let mut entries = offsets.iter_mut();
    for dy in -2isize..=2 {
        for dx in -2isize..=2 {
            if dx != 0 || dy != 0 {
                *entries.next().unwrap() =
                    (dx, dy, costs[dy.unsigned_abs()][dx.unsigned_abs()].min(cap));
            }
        }
    }
    offsets
}

fn sweep(
    a: &Alpha,
    field: &mut [i32],
    radius: usize,
    weights: &[(isize, isize, i32); 24],
    beyond: i32,
    active: impl Fn(u8, i32) -> bool,
    update: impl Fn(u8, i32, i32, i32) -> i32,
) {
    let w = a.w as isize;
    let h = a.h as isize;
    let len = field.len();
    for reverse in [false, true]
        .into_iter()
        .take(if radius > 2 { 2 } else { 1 })
    {
        for i in 0..len {
            let i = if reverse { len - 1 - i } else { i };
            let source = a.data[i];
            let mut value = field[i];
            if !active(source, value) {
                continue;
            }
            let x = (i % a.w) as isize;
            let y = (i / a.w) as isize;
            for &(dx, dy, cost) in weights {
                let (nx, ny) = (x + dx, y + dy);
                let neighbor = if nx >= 0 && nx < w && ny >= 0 && ny < h {
                    field[(ny * w + nx) as usize]
                } else {
                    beyond
                };
                value = update(source, value, neighbor, cost);
            }
            field[i] = value;
        }
    }
}

// M2
pub(super) fn spread(a: &mut Alpha, radius: usize, dilate: bool) {
    if radius == 0 {
        return;
    }
    let step = FULL / (radius as i32 + 1);
    let invert = |v: u8| if dilate { 255 - v } else { v };
    let mut field: Vec<_> = a.data.iter().map(|&v| i32::from(invert(v)) * 256).collect();
    let beyond = if dilate { FULL } else { 0 };
    sweep(
        a,
        &mut field,
        radius,
        &chamfer(step, FULL),
        beyond,
        |_, value| value != 0,
        |_, value, neighbor, cost| value.min(neighbor + cost),
    );
    for (pixel, value) in a.data.iter_mut().zip(field) {
        let coarse = ((value - (FULL - step)).clamp(0, step) * 255 / step) as u8;
        *pixel = invert(coarse);
    }
}

// M3
fn triangle_line(
    len: usize,
    n: usize,
    sample: impl Fn(usize) -> i64,
    mut emit: impl FnMut(usize, i64),
) {
    let at = |i: usize| if i < len { sample(i) } else { 0 };
    let mut sum: i64 = (0..n).map(|i| (n - i) as i64 * at(i)).sum();
    let mut left = at(0);
    let mut right: i64 = (1..=n).map(at).sum();
    for x in 0..len {
        emit(x, sum);
        sum += right - left;
        let next = at(x + 1);
        left += next - (x + 1).checked_sub(n).map_or(0, at);
        right += at(x + n + 1) - next;
    }
}

fn triangle(a: &Alpha, radius: usize) -> Vec<i32> {
    let n = radius.clamp(2, MAX_BLUR);
    let norm = (n * n) as i64;
    let mut rows = vec![0; a.data.len()];
    for (input, output) in a.data.chunks_exact(a.w).zip(rows.chunks_exact_mut(a.w)) {
        triangle_line(
            a.w,
            n,
            |i| i64::from(input[i]),
            |i, sum| {
                output[i] = ((256 * sum + norm / 2) / norm) as i32;
            },
        );
    }
    let mut values = vec![0; a.data.len()];
    for x in 0..a.w {
        triangle_line(
            a.h,
            n,
            |y| i64::from(rows[y * a.w + x]),
            |y, sum| {
                values[y * a.w + x] = (sum / norm) as i32;
            },
        );
    }
    values
}

// M4
fn precise(a: &Alpha, radius: usize, beyond: i32) -> Vec<i32> {
    let step = HALF / (radius as i32 + 1);
    let mut field: Vec<_> = a
        .data
        .iter()
        .map(|&v| match v {
            0 => -HALF,
            255 => HALF,
            v => i32::from(v) * step / 255,
        })
        .collect();
    sweep(
        a,
        &mut field,
        radius,
        &chamfer(step, HALF),
        beyond,
        |source, _| source == 0 || source == 255,
        |source, value, neighbor, cost| match source {
            0 => value.max(neighbor.min(step) - cost),
            255 => value.min(neighbor.max(0) + cost),
            _ => value,
        },
    );
    for value in &mut field {
        *value = (*value + HALF).clamp(0, FULL);
    }
    field
}

pub(super) fn values(a: &Alpha, radius: usize, use_precise: bool) -> Vec<i32> {
    if radius == 0 {
        a.data.iter().map(|&v| i32::from(v) * 256).collect()
    } else if use_precise {
        precise(a, radius, -HALF)
    } else {
        triangle(a, radius)
    }
}

// R5 (M4)
pub(super) fn precise_glow(a: &mut Alpha, radius: usize) -> Vec<i32> {
    for pixel in &mut a.data {
        *pixel = 255 - *pixel;
    }
    let mut values = precise(a, radius, HALF);
    for value in &mut values {
        *value = FULL - *value;
    }
    values
}

// M5
pub(super) fn finish(a: &mut Alpha, mut values: Vec<i32>, percent: f64) {
    noise::matte(&mut values, a.w, percent);
    for (pixel, value) in a.data.iter_mut().zip(values) {
        *pixel = (value >> 8) as u8;
    }
}
