/// The blend curve between two stops. The midpoint belongs to the ending stop.
#[derive(Clone, Copy, Debug)]
pub struct GradientCurve {
    pub exponent: f64,
    pub reversed: bool,
}

impl GradientCurve {
    pub fn new(midpoint: f64) -> Self {
        let m = (midpoint / 100.0).clamp(0.01, 0.99);
        Self { exponent: 0.5f64.ln() / m.max(1.0 - m).ln(), reversed: m < 0.5 }
    }

    pub fn sample(self, position: f64) -> f64 {
        let t = position.clamp(0.0, 1.0);
        if self.reversed { 1.0 - (1.0 - t).powf(self.exponent) } else { t.powf(self.exponent) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_midpoints_and_reflection() {
        for midpoint in [13.0, 25.0, 50.0, 75.0, 87.0] {
            let curve = GradientCurve::new(midpoint);
            assert_eq!(curve.sample(0.0), 0.0);
            assert_eq!(curve.sample(1.0), 1.0);
            assert!((curve.sample(midpoint / 100.0) - 0.5).abs() < 1e-12);
            for position in [0.1, 0.3, 0.6, 0.9] {
                let reflected = GradientCurve::new(100.0 - midpoint).sample(1.0 - position);
                assert!((curve.sample(position) + reflected - 1.0).abs() < 1e-12);
            }
        }
        assert!((GradientCurve::new(25.0).exponent - 2.40942083965).abs() < 1e-10);
    }
}
