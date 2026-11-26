use crate::{
    Composer, Item, SOFT_HYPHEN, Settings, cost::ACROSS_COLUMNS, is_trailing_blank,
    measure::visible_length,
};

pub fn is_hangul(c: char) -> bool {
    matches!(c, '\u{ac00}'..='\u{d7a3}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}')
}
fn is_cjk(c: char) -> bool {
    is_hangul(c)
        || matches!(c, '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{3040}'..='\u{30ff}')
}
pub fn is_latin_letter(c: char) -> bool {
    c < '\u{2000}' && c.is_alphabetic() && !is_cjk(c)
}

pub fn can_break_before(items: &[Item], j: usize, settings: &Settings) -> bool {
    if j == 0 || j >= items.len() {
        return false;
    }
    let (left, right) = (items[j - 1].ch, items[j].ch);
    if left == '\n' {
        return true;
    }
    if right == SOFT_HYPHEN {
        return false;
    }
    if left == SOFT_HYPHEN {
        return settings.hyphenation;
    }
    if is_trailing_blank(right) {
        return false;
    }
    if matches!(left, ' ' | '\t') {
        return true;
    }
    if settings.composer == Composer::RomanParagraph
        && is_hangul(right)
        && (matches!(left, '」' | '』') || (left == '「' && j >= 2 && items[j - 2].ch == ' '))
    {
        return true;
    }
    let closes = |c| ".,)]}!?:;%’”…·」』、。〉》〕】），．！？：；％".contains(c);
    if closes(right) || "〈《「『【〔〖〘〚〝（［｛".contains(left) {
        return false;
    }
    if is_hangul(right)
        && closes(left)
        && matches!(left, '\u{3000}'..='\u{303f}' | '\u{ff00}'..='\u{ffef}')
    {
        return settings.hyphenation;
    }
    if matches!(right, '"' | '\'') && !is_trailing_blank(left) {
        return false;
    }
    if is_trailing_blank(left) || matches!(left, '-' | '‐' | '–' | '—') {
        return true;
    }
    if is_hangul(left) && is_hangul(right) {
        return settings.hyphenation;
    }
    if matches!(left, '/' | '\\' | '@' | '.') && right.is_alphabetic() {
        return !(left == '.' && j >= 2 && is_cjk(items[j - 2].ch) && !is_cjk(right));
    }
    is_cjk(left) || is_cjk(right)
}

pub fn opportunities(items: &[Item], settings: &Settings) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut start = 0;
    for end in (1..items.len())
        .filter(|&j| can_break_before(items, j, settings))
        .chain([items.len()])
    {
        let final_segment = end == items.len();
        if end - start + usize::from(final_segment) > 32
            && (start == 0 || items[start - 1].ch != SOFT_HYPHEN)
        {
            let word_end = start + visible_length(&items[start..end]);
            if start < word_end && items[start..word_end].iter().all(|i| is_hangul(i.ch)) {
                let limit = if final_segment { word_end } else { end };
                let first = start.saturating_add(settings.hyphen.after_first.max(1));
                let last = limit
                    .saturating_sub(settings.hyphen.before_last.max(1))
                    .min(word_end - 1);
                positions.extend(first..=last);
            }
        }
        if !final_segment {
            positions.push(end);
        }
        start = end;
    }
    positions
}

#[derive(Clone, Copy, Debug)]
pub struct HyphenBreak {
    pub quality: f64,
    pub discretionary: bool,
}
fn is_dash(c: char) -> bool {
    matches!(c, '-' | '\u{58a}' | '\u{5be}' | '\u{1400}' | '\u{1806}' | '\u{2010}'..='\u{2015}' |
        '\u{2053}' | '\u{207b}' | '\u{208b}' | '\u{2212}' | '\u{2e17}' | '\u{2e1a}' |
        '\u{2e3a}'..='\u{2e3b}' | '\u{2e40}' | '\u{2e5d}' | '\u{301c}' | '\u{3030}' |
        '\u{30a0}' | '\u{fe31}'..='\u{fe32}' | '\u{fe58}' | '\u{fe63}' | '\u{ff0d}' |
        '\u{10d6e}' | '\u{10ead}')
}
pub fn hyphen_break(items: &[Item], j: usize, settings: &Settings) -> Option<HyphenBreak> {
    let before = items.get(..j)?;
    match before.last()?.ch {
        '\n' => return None,
        SOFT_HYPHEN => {
            return Some(HyphenBreak {
                quality: settings.hyphen.quality,
                discretionary: true,
            });
        }
        _ => {}
    }
    let last = visible_length(before).checked_sub(1)?;
    let c = before[last].ch;
    let quality = if is_dash(c) {
        if last + 1 != j {
            75.0
        } else if matches!(c, '-' | '\u{5be}') {
            90.0
        } else {
            50.0
        }
    } else if c == '_' {
        25.0
    } else {
        return None;
    };
    Some(HyphenBreak {
        quality,
        discretionary: false,
    })
}

pub fn word_limit_penalty(items: &[Item], j: usize, size: f64, settings: &Settings) -> f64 {
    let h = &settings.hyphen;
    if h.capitalized && h.last_word || j == 0 || j > items.len() || items[j - 1].ch != SOFT_HYPHEN {
        return 0.0;
    }
    let in_word = |i: &Item| i.ch.is_alphabetic() || i.ch == SOFT_HYPHEN;
    let start = items[..j]
        .iter()
        .rposition(|i| !in_word(i))
        .map_or(0, |k| k + 1);
    let end = j + items[j..].iter().take_while(|i| in_word(i)).count();
    let capital = !h.capitalized
        && items[start..end]
            .iter()
            .find(|i| i.ch != SOFT_HYPHEN)
            .is_some_and(|i| i.ch.is_uppercase());
    let last = !h.last_word
        && items[start..end].iter().any(|i| i.ch.is_alphabetic())
        && !items[end..].iter().any(|i| i.ch.is_alphabetic());
    f64::from(u8::from(capital) + u8::from(last)) * ACROSS_COLUMNS * size / 12.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_paragraph_can_break_after_corner_quotes() {
        let roman = Settings {
            hyphenation: false,
            ..Settings::default()
        };
        let japanese = Settings {
            composer: Composer::JapaneseParagraph,
            ..roman
        };
        for (text, allowed) in [
            (" 「작", true),
            ("는「작", false),
            ("야」를", true),
            ("야』를", true),
            ("야）를", false),
        ] {
            let items: Vec<_> = text.chars().map(|ch| Item::new(ch, 1.0, 0.0, 11.0)).collect();
            assert_eq!(can_break_before(&items, 2, &roman), allowed, "{text}");
            assert!(!can_break_before(&items, 2, &japanese), "{text}");
        }
    }
}
