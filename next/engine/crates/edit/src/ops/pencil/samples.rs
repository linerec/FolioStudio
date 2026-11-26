use super::bezier::Point;
use std::f64::consts::PI;

pub(super) fn lengths(points: &[Point]) -> Vec<f64> {
    let mut out = vec![0.; points.len()];
    for (i, p) in points.windows(2).enumerate() {
        out[i + 1] = out[i] + (p[1] - p[0]).length();
    }
    out
}

pub(super) fn window(lengths: &[f64], center: usize, radius: f64) -> (usize, usize) {
    let s = lengths[center];
    let lo = lengths[..center]
        .partition_point(|&x| x <= s - radius)
        .saturating_sub(1);
    let hi = (center + 1 + lengths[center + 1..].partition_point(|&x| x < s + radius)).min(lengths.len() - 1);
    (lo, hi)
}

fn corner_indices(p: &[Point], tol: f64) -> Vec<usize> {
    let s = lengths(p);
    let mut cos = vec![-1.; p.len()];
    for i in 1..p.len() - 1 {
        let (lo, hi) = window(&s, i, tol);
        if s[lo] > s[i] - tol || s[hi] < s[i] + tol {
            continue;
        }
        let sum = |slice: &[Point]| slice.iter().fold(Point::ZERO, |a, &v| a + (v - p[i]).unit());
        let a = sum(&p[lo..i]);
        let b = sum(&p[i + 1..=hi]);
        let norm = a.length() * b.length();
        cos[i] = if norm == 0. { 0. } else { a.dot(b) / norm };
    }
    (1..p.len() - 1)
        .filter(|&i| cos[i] >= cos[i - 1] && cos[i] > cos[i + 1] && cos[i] > -0.5)
        .collect()
}

fn sine_transform(values: &[Point]) -> Vec<Point> {
    let n = (values.len() + 1) as f64;
    let scale = (2. / n).sqrt();
    (1..=values.len())
        .map(|k| {
            values.iter().enumerate().fold(Point::ZERO, |sum, (i, &v)| {
                sum + v * (PI * (i + 1) as f64 * k as f64 / n).sin()
            }) * scale
        })
        .collect()
}

fn low_pass(points: &mut [Point], sigma: f64) {
    if points.len() < 3 || sigma >= 1. {
        return;
    }
    let steps = points.len() - 1;
    let a = points[0];
    let b = points[steps];
    let chord = |i: usize| a.between(b, i as f64 / steps as f64);
    let offsets = (1..steps).map(|i| points[i] - chord(i)).collect::<Vec<_>>();
    let mut modes = sine_transform(&offsets);
    let target = (1. - sigma) * modes.iter().map(|v| v.dot(*v)).sum::<f64>();
    if target <= 0. {
        return;
    }
    let mut energy = 0.;
    let mut cutoff_index = 1;
    for (k, mode) in modes.iter().enumerate() {
        energy += mode.dot(*mode);
        cutoff_index = k + 1;
        if energy >= target {
            break;
        }
    }
    let frequency = |k: usize| 1. - (k as f64 * PI / steps as f64).cos();
    let fk = frequency(cutoff_index);
    let alpha = 10. / (fk - frequency(cutoff_index - 1)).powi(2);
    let peak = modes[cutoff_index - 1];
    let r = (1. - (energy - target) / peak.dot(peak)).sqrt().max(10f64.powf(-0.5));
    let cutoff = fk - (-20. * r.log10() / alpha).sqrt();
    if !cutoff.is_finite() {
        return;
    }
    for (i, v) in modes.iter_mut().enumerate() {
        let delta = (frequency(i + 1) - cutoff).max(0.);
        let db = -alpha * delta * delta;
        *v = *v * if db < -1000. { 0. } else { 10f64.powf(db / 20.) };
    }
    for (k, v) in sine_transform(&modes).into_iter().enumerate() {
        points[k + 1] = chord(k + 1) + v;
    }
}

fn resample(points: &[Point], sigma: f64) -> Vec<Point> {
    let count = points.len() - 1;
    let extra = count.next_power_of_two() - count;
    let mut midpoints = (1..=extra)
        .map(|c| (c as f64 * count as f64 / (extra + 1) as f64).round() as usize)
        .peekable();
    let mut result = vec![points[0]];
    for i in 1..=count {
        if midpoints.peek() == Some(&i) {
            result.push((points[i - 1] + points[i]) * 0.5);
            midpoints.next();
        }
        result.push(points[i]);
    }
    low_pass(&mut result, sigma);
    result
}

pub(super) struct Samples {
    pub points: Vec<Point>,
    pub lengths: Vec<f64>,
    pub corners: Vec<usize>,
}

impl Samples {
    pub fn prepare(input: &[Point], tol: f64, sigma: f64) -> Self {
        let s = lengths(input);
        let mut points = input
            .iter()
            .enumerate()
            .filter(|(i, _)| *i == 0 || s[*i] != s[i - 1])
            .map(|(_, p)| *p)
            .collect::<Vec<_>>();
        let mut corners = Vec::new();
        if points.len() >= 2 {
            for k in [0.4749662150439437, -0.49865042557025946].repeat(2) {
                let mut prev = points[0];
                for i in 1..points.len() - 1 {
                    let old = points[i];
                    points[i] = old + ((prev + points[i + 1]) * 0.5 - old) * k;
                    prev = old;
                }
            }
            corners = corner_indices(&points, tol);
            if sigma > 1e-6 {
                let mut start = 0;
                let mut result = vec![points[0]];
                let mut new_corners = Vec::new();
                for &end in corners.iter().chain(std::iter::once(&(points.len() - 1))) {
                    result.extend(resample(&points[start..=end], sigma).into_iter().skip(1));
                    if end < points.len() - 1 {
                        new_corners.push(result.len() - 1);
                    }
                    start = end;
                }
                points = result;
                corners = new_corners;
            }
        }
        Self {
            lengths: lengths(&points),
            points,
            corners,
        }
    }
}
