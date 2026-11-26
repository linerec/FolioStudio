use super::{Point, Seg, add, cross, dist, handles, left, len, mul, sub, tangent_intersection};

// R3
fn quadratic(a: f64, b: f64, c: f64) -> [f64; 2] {
    if a.abs() <= (b.abs() + c.abs()) * 1e-14 {
        return [-c / b, f64::NAN];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [f64::NAN; 2];
    }
    let q = -0.5 * (b + disc.sqrt().copysign(b));
    [q / a, c / q]
}

fn derivative_coefficients(s: &Seg) -> [Point; 3] {
    let a = sub(s.b, s.a);
    let b = sub(s.c, s.b);
    let c = sub(s.d, s.c);
    [a, mul(sub(b, a), 2.0), add(sub(c, mul(b, 2.0)), a)]
}

fn parallel_parameter(s: &Seg, direction: Point) -> Option<f64> {
    let [c, b, a] = derivative_coefficients(s).map(|p| cross(p, direction));
    quadratic(a, b, c)
        .into_iter()
        .filter(|&t| t > 1e-10 && t < 1.0 - 1e-10)
        .min_by(|a, b| (a - 0.5).abs().total_cmp(&(b - 0.5).abs()))
}

fn displace(s: &Seg, t: f64, height: f64) -> Point {
    add(s.eval(t), mul(left(s.tangent(t)), height))
}

fn offset_speed(s: &Seg, t: f64, h: f64) -> f64 {
    let [_, b, c] = derivative_coefficients(s);
    let v = s.deriv(t);
    len(v).powi(3) + 3.0 * h * cross(v, add(b, mul(c, 2.0 * t)))
}

fn cusp_parameters(s: &Seg, h: f64, cuts: &mut Vec<f64>) {
    let mut previous = offset_speed(s, 0.0, h);
    for i in 1..=32 {
        let mut lo = (i - 1) as f64 / 32.0;
        let mut hi = i as f64 / 32.0;
        let next = offset_speed(s, hi, h);
        if previous.signum() != next.signum() {
            for _ in 0..48 {
                let m = (lo + hi) * 0.5;
                if offset_speed(s, m, h).signum() == previous.signum() {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            let t = (lo + hi) * 0.5;
            if t > 1e-8 && t < 1.0 - 1e-8 {
                cuts.push(t);
            }
        }
        previous = next;
    }
}

fn fit(s: &Seg, h: f64, tolerance: f64, turning_back: bool) -> Option<Seg> {
    let u = s.start_dir()?;
    let v = s.end_dir()?;
    let a = add(s.a, mul(left(u), h));
    let d = add(s.d, mul(left(v), h));
    let (along, intersection) = tangent_intersection(a, u, d, v)?;
    let forward = offset_speed(s, 0.5, h);
    if along * forward <= 0.0 {
        return None;
    }
    let x = sub(a, intersection);
    let y = sub(d, intersection);
    let area = cross(x, y);
    if area.abs() < 1e-14 {
        return None;
    }
    let mut support = [0.0; 3];
    let mut weights = [0.0; 3];
    for (i, (wx, wy)) in handles::directions().into_iter().enumerate() {
        let dir = add(mul(x, wx), mul(y, wy));
        let t = parallel_parameter(s, dir)?;
        support[i] = (cross(dir, sub(displace(s, t, h), intersection)) / area).abs();
        weights[i] = area.abs() / len(dir);
    }
    if !support.iter().all(|n| (0.0..=1.0).contains(n)) {
        return None;
    }
    let (k1, k2, error) = handles::select_handles(support, weights, tolerance)?;
    if error >= tolerance {
        return None;
    }
    let curve = Seg {
        a,
        b: add(intersection, mul(x, k1)),
        c: add(intersection, mul(y, k2)),
        d,
    };
    let tangent_reach = 0.2 * dist(a, d);
    for i in 1..12 {
        let t = i as f64 / 12.0;
        let q = displace(s, t, h);
        let mut p = t;
        for _ in 0..10 {
            let g = curve.deriv(p);
            let r = sub(curve.eval(p), q);
            let step = (r.x * g.x + r.y * g.y) / (g.x * g.x + g.y * g.y);
            if !step.is_finite() {
                return None;
            }
            p = (p - step).clamp(0.0, 1.0);
        }
        if dist(q, curve.eval(p)) > tolerance {
            return None;
        }
        let tangent_error = cross(s.tangent(t), curve.tangent(p)).abs();
        if turning_back && tangent_error * tangent_reach > tolerance {
            return None;
        }
    }
    Some(curve)
}

fn fit_convex(s: &Seg, h: f64, tolerance: f64, depth: u8, turning_back: bool, out: &mut Vec<Seg>) {
    let Some(u) = s.start_dir() else {
        return;
    };
    let Some(v) = s.end_dir() else {
        return;
    };
    let mut turn = cross(u, v).atan2(u.x * v.x + u.y * v.y);
    let [_, b, c] = derivative_coefficients(s);
    let curvature = cross(s.deriv(0.5), add(b, c));
    if turn * curvature < 0.0 {
        turn += std::f64::consts::TAU.copysign(curvature);
    }
    let exceeds_quarter_turn = turn.abs() > std::f64::consts::FRAC_PI_2 + 0.001;
    let turning_back = turning_back || exceeds_quarter_turn;
    if exceeds_quarter_turn && depth < 12 {
        let pieces = (turn.abs() / std::f64::consts::FRAC_PI_2).ceil();
        let angle = u.y.atan2(u.x) + turn / pieces;
        if let Some(t) = parallel_parameter(s, Point::new(angle.cos(), angle.sin())) {
            let (l, r) = s.split(t);
            fit_convex(&l, h, tolerance, depth + 1, turning_back, out);
            fit_convex(&r, h, tolerance, depth + 1, turning_back, out);
            return;
        }
    }
    let a = add(s.a, mul(left(u), h));
    let d = add(s.d, mul(left(v), h));
    let chord = dist(s.a, s.d);
    let bend = cross(u, v).abs();
    if depth >= 12 || (bend < 0.2 && 0.75 * chord.max(dist(a, d)) * bend < tolerance) {
        let scale = if chord > 0.0 { dist(a, d) / chord } else { 1.0 };
        out.push(Seg {
            a,
            b: add(a, mul(sub(s.b, s.a), scale)),
            c: add(d, mul(sub(s.c, s.d), scale)),
            d,
        });
    } else if let Some(curve) = fit(s, h, tolerance, turning_back) {
        out.push(curve);
    } else if dist(a, d) < 3.0 * tolerance {
        let forward = offset_speed(s, 0.5, h);
        let handle = (dist(a, d) / 3.0).copysign(forward);
        out.push(Seg {
            a,
            b: add(a, mul(u, handle)),
            c: sub(d, mul(v, handle)),
            d,
        });
    } else {
        let (l, r) = s.split(0.5);
        fit_convex(&l, h, tolerance, depth + 1, turning_back, out);
        fit_convex(&r, h, tolerance, depth + 1, turning_back, out);
    }
}

pub(super) fn curve(s: &Seg, h: f64) -> Vec<Seg> {
    if h == 0.0 {
        return vec![*s];
    }
    if s.is_line() {
        let Some(u) = s.start_dir() else {
            return Vec::new();
        };
        let shift = mul(left(u), h);
        return vec![Seg::line(add(s.a, shift), add(s.d, shift))];
    }
    let [a, b, c] = derivative_coefficients(s);
    let mut cuts = quadratic(cross(b, c), 2.0 * cross(a, c), cross(a, b))
        .into_iter()
        .filter(|&t| t > 1e-8 && t < 1.0 - 1e-8)
        .collect::<Vec<_>>();
    cusp_parameters(s, h, &mut cuts);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
    cuts.push(1.0);
    let mut start = 0.0;
    let mut result = Vec::new();
    let tolerance = (h.abs() * 0.5).clamp(0.01, 0.25);
    for end in cuts {
        fit_convex(&s.sub(start, end), h, tolerance, 0, false, &mut result);
        start = end;
    }
    result
}
