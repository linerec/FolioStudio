use crate::{
    Alignment, Item, SOFT_HYPHEN, Settings,
    cost::TOL,
    is_word_space,
    measure::{natural_width, visible_length},
};

#[derive(Debug)]
pub struct PlacedLine {
    pub start: usize,
    pub end: usize,
    pub x: Vec<f64>,
    pub width: Vec<f64>,
    pub word_spacing: f64,
    pub letter_extra: f64,
    pub glyph_scale: f64,
    pub letter_fraction: f64,
}

#[derive(Default)]
struct Shares {
    word: f64,
    letter: f64,
    glyph: f64,
}

fn distribute(delta: f64, words: f64, gaps: f64, natural: f64, s: &Settings) -> Shares {
    if delta.abs() <= 1e-9 {
        return Shares::default();
    }
    let shrink = delta < 0.0;
    let sign = delta.signum();
    let demand = delta.abs();
    let word = s.word_spacing.allowance(shrink);
    let letter = s.letter_spacing.allowance(shrink);
    let glyph = s.glyph_scaling.allowance(shrink);
    let word_room = word * words;
    if demand <= word_room {
        return Shares {
            word: word * delta / (word_room + 1e-7),
            ..Shares::default()
        };
    }
    let other_room = letter * gaps + glyph * natural;
    if demand - word_room <= other_room && other_room > 0.0 {
        let ratio = sign * (demand - word_room) / (other_room + 1e-7);
        return Shares {
            word: sign * word,
            letter: ratio * letter,
            glyph: ratio * glyph,
        };
    }
    let remaining = demand - glyph * natural;
    let (word_share, letter_share) = if shrink {
        let (mut w, mut l) = (word, letter);
        if w * words + l * gaps == 0.0 {
            if words > 0.0 {
                w = 1.0;
            } else {
                l = 1.0;
            }
        }
        let budget = w * words + l * gaps;
        if budget <= 0.0 {
            (0.0, 0.0)
        } else {
            let k = remaining / budget;
            if w * k > 0.8 && gaps > 0.0 {
                (0.8, (remaining - 0.8 * words) / gaps)
            } else {
                (
                    if words > 0.0 { w * k } else { 0.0 },
                    if gaps > 0.0 { l * k } else { 0.0 },
                )
            }
        }
    } else if word_room > 0.0 {
        let ratio = remaining / (word_room + letter * gaps);
        (ratio * word, if gaps > 0.0 { ratio * letter } else { 0.0 })
    } else if words > 0.0 {
        ((remaining - letter * gaps) / words, letter)
    } else if gaps > 0.0 {
        (0.0, remaining / gaps)
    } else {
        (0.0, 0.0)
    };
    Shares {
        word: sign * word_share,
        letter: sign * letter_share,
        glyph: sign * glyph,
    }
}

pub fn place_line(
    items: &[Item],
    start: usize,
    end: usize,
    available: f64,
    s: &Settings,
    last: bool,
) -> PlacedLine {
    let line = items.get(start..end).unwrap_or_default();
    let visible = &line[..visible_length(line)];
    let gaps = &visible[..visible.len().saturating_sub(1)];
    let natural = natural_width(visible);
    let delta = available - natural;
    let first_size = line.first().map_or(0.0, |i| i.point_size);
    let (base_unit, base_size) =
        gaps.iter()
            .find(|i| is_word_space(i.ch))
            .map_or((first_size * 0.25, first_size), |i| {
                (
                    i.end_advance * 10000.0
                        / (s.word_spacing.desired.max(1.0) * s.glyph_scaling.desired.max(1.0)),
                    i.point_size,
                )
            });
    let unit = |i: &Item| {
        if i.unit > 0.0 {
            i.unit
        } else {
            base_unit * i.point_size / base_size.max(1.0)
        }
    };
    let mut words = 0.0;
    let mut letters = 0.0;
    let mut gap_count = 0;
    let mut has_space = false;
    for item in gaps {
        if is_word_space(item.ch) {
            words += unit(item);
            has_space = true;
        }
        if item.ch != SOFT_HYPHEN {
            letters += unit(item);
            gap_count += 1;
        }
    }
    let mut align = s.alignment;
    let overfull = delta < -TOL;
    let mut justify =
        overfull || align.is_justified() && (!last || align == Alignment::FullyJustified);
    if justify && !overfull && !has_space {
        align = s.single_word;
        justify = align.is_justified();
    }
    let shares = if justify {
        distribute(delta, words, letters, natural, s)
    } else {
        Shares::default()
    };
    let offset = if justify {
        0.0
    } else {
        match align {
            Alignment::Center | Alignment::CenterJustified => delta / 2.0,
            Alignment::Right | Alignment::RightJustified => delta,
            _ => 0.0,
        }
    };
    let tab = if justify {
        None
    } else {
        visible.iter().rposition(|i| i.ch == '\t')
    };
    let mut cursor = if tab.is_some() { 0.0 } else { offset };
    let mut placed = PlacedLine {
        start,
        end,
        x: Vec::with_capacity(line.len()),
        width: Vec::with_capacity(line.len()),
        word_spacing: s.word_spacing.desired + 100.0 * shares.word,
        glyph_scale: 1.0 + shares.glyph,
        letter_fraction: shares.letter,
        letter_extra: if gap_count > 0 {
            shares.letter * letters / f64::from(gap_count)
        } else {
            0.0
        },
    };
    for (i, item) in line.iter().enumerate() {
        let last_visible = i + 1 == visible.len();
        let base = if last_visible {
            item.end_advance
        } else {
            item.advance
        };
        let width = if Some(i) == tab {
            base + offset
        } else if i >= visible.len() || (last_visible && is_word_space(item.ch)) {
            base
        } else if i < gaps.len() && is_word_space(item.ch) {
            base * placed.glyph_scale + shares.word * unit(item)
        } else {
            base * placed.glyph_scale
        };
        placed.x.push(cursor);
        placed.width.push(width);
        cursor += width;
        if i < gaps.len() && item.ch != SOFT_HYPHEN {
            cursor += shares.letter * unit(item);
        }
    }
    placed
}
