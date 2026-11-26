use super::outline::{SampledGlyph, VerticalGuides};
use super::{Bands, LAST_BAND};

const FIXED_ONE: i32 = 65_536;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LetterHeight {
    Capital,
    Lowercase,
    Other,
}

impl LetterHeight {
    // R6
    pub fn of(ch: char) -> Self {
        match ch {
            'A'..='Z'
            | '0'..='9'
            | 'À'..='Ö'
            | 'Ø'..='Þ'
            | '!'
            | '$'
            | '%'
            | '?'
            | '¡'
            | '£'
            | '¥'
            | '¿' => Self::Capital,
            'a'..='z' | 'ß'..='ö' | 'ø'..='ÿ' | '¢' => Self::Lowercase,
            _ => Self::Other,
        }
    }
}

// R6
pub(super) fn tilt_step(stem: &SampledGlyph, guides: VerticalGuides) -> i32 {
    let height = guides.capital - guides.baseline;
    let low = (guides.baseline + height / 3) as usize;
    let high = (guides.baseline + 2 * height / 3) as usize;
    let span = (high - low) as i64;
    let left = i64::from(stem.left[high]) - i64::from(stem.left[low]);
    let right = i64::from(stem.right[low]) - i64::from(stem.right[high]);
    if span == 0 || (left - right).abs() > 20 {
        return 0;
    }

    let slope = 6553 * left / (10 * span);
    if !(-13_107..=55_705).contains(&slope) {
        return 0;
    }
    let shift = |height: i32| (height * slope as i32 + FIXED_ONE / 16).div_euclid(FIXED_ONE);
    shift(200) - shift(100)
}

pub(super) struct Profile {
    bands: Bands,
}

impl Profile {
    // R6
    pub fn new(
        mut bands: Bands,
        bottom: i32,
        top: i32,
        guides: VerticalGuides,
        letter: LetterHeight,
        tilt: i32,
    ) -> Self {
        let VerticalGuides {
            baseline,
            lowercase,
            capital,
            zone,
        } = guides;
        let lower_height = lowercase - baseline;
        let short = top < baseline + 2 * lower_height / 3 || bottom > baseline + lower_height / 3;
        let rise_at_baseline = (baseline - zone - 1..=baseline + zone).contains(&bottom)
            || letter != LetterHeight::Other;
        let fall_at_top =
            |guide, class| (guide - zone..=guide + zone + 1).contains(&top) || letter == class;
        let fall_at_capital = fall_at_top(capital, LetterHeight::Capital);
        let fall_at_lowercase = fall_at_top(lowercase, LetterHeight::Lowercase);
        let fall_zone = |guide, band| (guide - zone - 3..=guide + zone + 1).contains(&band);

        for band in (baseline.max(bottom) + 1)..=LAST_BAND {
            let near_guide =
                rise_at_baseline && (baseline - zone - 1..=baseline + zone + 3).contains(&band);
            let step = slope_limit(short, near_guide) + tilt;
            let extended = bands[(band - 1) as usize] + step;
            let current = &mut bands[band as usize];
            *current = if band > top {
                extended
            } else {
                (*current).min(extended)
            };
        }
        for band in (0..capital.min(top)).rev() {
            let near_guide = (fall_at_capital && fall_zone(capital, band))
                || (fall_at_lowercase && fall_zone(lowercase, band));
            let step = slope_limit(short, near_guide) - tilt;
            let extended = bands[(band + 1) as usize] + step;
            let current = &mut bands[band as usize];
            *current = if band < bottom {
                extended
            } else {
                (*current).min(extended)
            };
        }
        Self { bands }
    }

    fn shrink(
        &self,
        scale: i32,
        guides: VerticalGuides,
        first: i32,
        last: i32,
        tilt: i32,
    ) -> Bands {
        let baseline = guides.baseline;
        let mut bands = std::array::from_fn(|band| {
            let offset = (band as i32 - baseline) as f64 * f64::from(FIXED_ONE) / f64::from(scale);
            // R7
            let nearest = (offset * f64::from(FIXED_ONE) + 0.5).floor() / f64::from(FIXED_ONE);
            let source = (baseline + nearest as i32).clamp(0, LAST_BAND);
            scaled(self.bands[source as usize], scale)
        });
        let lower_height = guides.lowercase - baseline;
        if last < baseline + 2 * lower_height / 3 {
            for band in (last + 1)..=LAST_BAND {
                bands[band as usize] =
                    bands[band as usize].min(bands[(band - 1) as usize] + 20 + tilt);
            }
        }
        if first > baseline + lower_height / 3 {
            for band in (0..first).rev() {
                bands[band as usize] =
                    bands[band as usize].min(bands[(band + 1) as usize] + 20 - tilt);
            }
        }
        bands
    }
}

fn slope_limit(short: bool, near_guide: bool) -> i32 {
    if short {
        250
    } else if near_guide {
        500
    } else {
        90
    }
}

fn scaled(value: i32, fixed: i32) -> i32 {
    (i64::from(value) * i64::from(fixed) / i64::from(FIXED_ONE)) as i32
}

pub(super) struct Wall {
    guides: VerticalGuides,
    letter: LetterHeight,
    scale: f64,
}

impl Wall {
    pub fn new(guides: VerticalGuides, letter: LetterHeight, scale: f64) -> Self {
        Self {
            guides,
            letter,
            scale,
        }
    }

    // R7, R8
    pub fn adjustment(&self, profile: Profile, tilt: i32) -> i32 {
        let VerticalGuides {
            baseline,
            mut lowercase,
            mut capital,
            mut zone,
        } = self.guides;
        let inverse = if self.scale > 1.0 {
            self.scale.recip()
        } else {
            1.0
        };
        let shrink = (self.scale.min(inverse) * f64::from(FIXED_ONE)).round() as i32;
        let move_band = |band| baseline + scaled(band - baseline, shrink);
        let mut first = move_band(0);
        let mut last = move_band(LAST_BAND);
        let bands = if self.scale < 1.0 {
            profile.shrink(shrink, self.guides, first, last, tilt)
        } else {
            if self.scale > 1.0 {
                lowercase = move_band(lowercase);
                capital = move_band(capital);
                zone = move_band(baseline + zone) - baseline;
            }
            profile.bands
        };
        (first, last) = (
            (last - 2 * zone).min(first).max(0),
            (first + 2 * zone).max(last).min(LAST_BAND),
        );

        let mean_first = if self.letter == LetterHeight::Other {
            first
        } else {
            first.max(baseline)
        };
        let mean_last = match self.letter {
            LetterHeight::Lowercase => last.min(lowercase).min(capital),
            _ => last.min(capital),
        };
        let distance = |value| (f64::from(value) * inverse) as i32;
        let wall = distance(3030);
        let representative =
            mean_gap((mean_first..=mean_last).map(|band| bands[band as usize] + wall));
        let target = distance(1171) - representative;
        let target = target - i32::from(target < 0);
        let minimum = (first..=last)
            .map(|band| bands[band as usize] + wall)
            .min()
            .unwrap_or(0);
        let adjustment = target.max(distance(-2000) - minimum);
        (f64::from(adjustment) / inverse) as i32
    }
}

fn mean_gap(gaps: impl Iterator<Item = i32>) -> i32 {
    let (sum, count) = gaps.fold((0_i64, 0), |(sum, count), gap| {
        let magnitude = gap.abs();
        let rounded = if magnitude <= 29_990 {
            (magnitude + 5) / 10 * 10
        } else {
            magnitude
        };
        let root = (f64::from(rounded).sqrt() * f64::from(FIXED_ONE)).round() as i64;
        (sum + root * i64::from(gap.signum()), count + 1)
    });
    if count == 0 {
        return 0;
    }
    let average = (sum / (count * i64::from(FIXED_ONE))) as i32;
    average * average.abs()
}

#[cfg(test)]
mod tests {
    use super::mean_gap;

    #[test]
    fn representative_gap_truncates_the_signed_mean_before_squaring() {
        // R8
        assert_eq!(mean_gap([100, 50].into_iter()), 64);
        assert_eq!(mean_gap([-100, -50].into_iter()), -64);
        assert_eq!(mean_gap([100, -50].into_iter()), 1);
        assert_eq!(mean_gap([0, 105].into_iter()), 25);
        assert_eq!(mean_gap(std::iter::empty()), 0);
    }
}
