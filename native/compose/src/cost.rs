use crate::{HyphenSettings, Settings};
pub const TOL: f64 = 0.000488;
pub const BLOCKED: f64 = 999_000_000.0;
pub const ACROSS_COLUMNS: f64 = 99_900.0;
pub const DICTIONARY_QUALITY: f64 = 64.0;
pub const HYPHEN_WEIGHT_AT_5: f64 = 1.0;
pub const HYPHEN_WEIGHT_TABLE: [f64; 11] = [
    0.0, 0.025, 0.15, 0.325, 0.55, 1.0, 1.325, 1.85, 2.3, 2.975, 3.6,
];

#[derive(Clone, Copy, Debug, Default)]
pub struct LineMetrics {
    pub natural: f64,
    pub space_unit: f64,
    pub last_visible: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
pub struct LineCost {
    pub cost: f64,
    pub category: u8,
}
impl LineCost {
    fn squared(deviation: f64, category: u8) -> Self {
        Self {
            cost: 10.0 + deviation * deviation,
            category,
        }
    }
}

pub(crate) fn character_allowance(s: &Settings, shrink: bool) -> f64 {
    0.5882353 * s.letter_spacing.allowance(shrink) + s.glyph_scaling.allowance(shrink)
}

pub fn justified(m: &LineMetrics, available: f64, s: &Settings) -> LineCost {
    let excess = m.natural - available;
    let spaces = m.space_unit;
    let shrink = excess > TOL;
    let letters = m.natural * character_allowance(s, shrink);
    let words = spaces * s.word_spacing.allowance(shrink);
    if shrink {
        let remaining = excess - letters;
        if remaining - words <= TOL {
            return LineCost::squared(excess, if excess <= 0.15 * spaces { 3 } else { 4 });
        }
        let ratio = if remaining > spaces {
            25.0
        } else {
            remaining / (words + 0.01)
        };
        let deviation = if remaining > 0.6 * spaces {
            2.0 * excess
        } else {
            excess
        };
        let mut line = LineCost::squared(deviation, 5);
        line.cost += 700.0 * ratio * ratio;
        line
    } else {
        let gap = -excess;
        if gap <= letters + words || gap <= 0.6 * spaces {
            LineCost::squared(gap, if gap <= 0.33 * spaces { 3 } else { 2 })
        } else {
            LineCost::squared(1.5 * gap, 1)
        }
    }
}

pub fn adjacency(previous: u8, current: u8, point_size: f64) -> f64 {
    if previous == 0 || current == 0 {
        return 0.0;
    }
    [0.0, 30.0, 100.0, 500.0][usize::from(previous.abs_diff(current).min(3))] * point_size / 12.0
}

pub fn last_line_penalty(natural: f64, available: f64) -> f64 {
    if available <= 0.0 {
        return 0.0;
    }
    if natural <= 0.0 {
        return 1250.0;
    }
    let ratio = natural / available;
    let difference = if ratio < 0.4 && natural < 144.0 {
        0.4 - ratio
    } else if ratio > 0.75 && available - natural < 144.0 {
        ratio - 0.75
    } else {
        0.0
    };
    5000.0 * difference.powi(2)
}

pub fn ragged(
    m: &LineMetrics,
    available: f64,
    line_index: usize,
    end: bool,
    one_segment: bool,
    s: &Settings,
) -> LineCost {
    let excess = m.natural - available;
    if excess > TOL {
        let room = m.natural * character_allowance(s, true)
            + m.space_unit * s.word_spacing.allowance(true);
        let factor = if one_segment && excess < room - TOL {
            20.0
        } else {
            100.0
        };
        LineCost::squared(excess * factor, 5)
    } else if end && s.balance_ragged_lines {
        LineCost::squared(0.85 * excess, 3)
    } else if end {
        let mut line = LineCost::squared(0.0, 0);
        if line_index > 0 {
            if m.natural == 0.0 {
                line.cost += 99_900.0;
            }
            line.cost += last_line_penalty(m.natural, available);
        }
        line
    } else {
        LineCost::squared(
            excess,
            if 0.9 * available - m.natural > TOL {
                2
            } else {
                3
            },
        )
    }
}

pub fn hyphen_weight_value(slider: u32) -> f64 {
    HYPHEN_WEIGHT_TABLE[slider.min(10) as usize]
}
pub fn push_history(previous: u8, hyphen: bool) -> u8 {
    (previous << 1) | u8::from(hyphen)
}
pub fn history_counts(previous: u8) -> (u32, u32, u32) {
    let history = push_history(previous, true);
    let alternate = (0..4).take_while(|i| history & (1 << (2 * i)) != 0).count() as u32;
    (
        history.trailing_ones(),
        1 + (history >> 2).trailing_ones(),
        alternate,
    )
}
pub fn hyphen_penalty(quality: f64, point_size: f64, previous: u8, h: &HyphenSettings) -> f64 {
    if !(1.0..=100.0).contains(&quality) {
        return 0.0;
    }
    let (run, skipped, alternate) = history_counts(previous);
    if h.ladder_limit != 0 && run > h.ladder_limit {
        return BLOCKED;
    }
    let repetition = (15 * run).max(12 * skipped).max(10 * alternate);
    (f64::from(repetition * 15) + (100.0 - quality).powi(3) / 500.0 + 10.0) * point_size / 12.0
        * h.weight
}
