use super::{LineReport, Paragraph, Slots, State};
use crate::Composer;
use crate::breaks::{is_hangul, is_latin_letter};
use crate::cost::{self, LineCost};

impl Paragraph<'_> {
    pub(super) fn base_cost(
        &self,
        start: usize,
        end: usize,
        row: usize,
        width: f64,
        justified: bool,
    ) -> LineCost {
        let first = self.stops[start].at;
        let last = self.stops[end].at;
        let metrics = self.widths.line(first, last);
        if self.settings.composer == Composer::JapaneseParagraph
            && metrics.natural > width + cost::TOL
            && last >= first + 3
            && self.items.get(last).is_some_and(|item| is_hangul(item.ch))
            && is_hangul(self.items[last - 1].ch)
            && self.items[last - 2].ch == ' '
            && is_latin_letter(self.items[last - 3].ch)
        {
            return LineCost {
                cost: cost::BLOCKED,
                category: 5,
            };
        }
        if justified {
            cost::justified(&metrics, width, self.settings)
        } else {
            cost::ragged(
                &metrics,
                width,
                row,
                end == self.end() || self.stops[end].forced,
                end - start == 1,
                self.settings,
            )
        }
    }
    pub(super) fn score(
        &self,
        from: usize,
        to: usize,
        state: State,
        slots: Slots,
        justified: bool,
    ) -> (LineReport, State) {
        let start = &self.stops[from];
        let end = &self.stops[to];
        let slots = self.line_slots(to, slots);
        let measure = slots.w[0];
        let metrics = self.widths.line(start.at, end.at);
        let size = self.uniform_size.unwrap_or_else(|| {
            self.items[start.at..end.at]
                .iter()
                .map(|i| i.point_size)
                .fold(0.0, f64::max)
        });
        let h = &self.settings.hyphen;
        let mut penalty = if slots.column_start && !h.across_columns && state.history & 1 != 0 {
            cost::ACROSS_COLUMNS * size / 12.0
        } else {
            0.0
        };
        let mut hyphen_penalty = 0.0;
        if let Some(hyphen) = end.hyphen {
            hyphen_penalty = cost::hyphen_penalty(hyphen.quality, size, state.history, h);
            penalty += hyphen_penalty;
            if hyphen.discretionary {
                penalty += end.word_limit * size / 12.0;
                if end.count - start.count < h.after_first.max(h.before_last) {
                    penalty += cost::BLOCKED;
                }
            }
            if !justified
                && slots.len() == 1
                && measure < metrics.natural + h.zone
                && end.ordinary_before > from
            {
                let previous = self.stops[end.ordinary_before].at;
                if measure < self.widths.line(start.at, previous).natural + h.zone {
                    penalty *= 2.0;
                }
            }
        }
        let mut line = if slots.len() > 1 {
            self.slot_plan(from, to, state.row, slots, justified).score
        } else {
            self.base_cost(from, to, state.row, measure, justified)
        };
        if justified {
            line.cost += cost::adjacency(state.grade, line.category, size);
        }
        line.cost += penalty;
        (
            LineReport {
                start: start.at,
                end: end.at,
                natural: metrics.natural,
                measure,
                space_unit: metrics.space_unit,
                category: line.category,
                cost: line.cost,
                hyphen_penalty,
            },
            State {
                row: state.row + 1,
                grade: if end.forced { 0 } else { line.category },
                history: cost::push_history(state.history, end.hyphen.is_some()),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Alignment, Item, Settings, compose};

    #[test]
    fn japanese_paragraph_keeps_a_hangul_syllable_only_when_it_fits() {
        let latin = "supercalifragilisticexpialidocious";
        let text = format!("{latin} 단어도 여러 줄에 놓습니다.");
        let items: Vec<_> = text
            .chars()
            .map(|ch| {
                let advance = match ch {
                    ' ' => 3.867,
                    _ if is_latin_letter(ch) => 158.259 / latin.len() as f64,
                    _ => 11.0,
                };
                Item::new(ch, advance, 0.0, 11.0)
            })
            .collect();
        for alignment in [Alignment::Left, Alignment::LeftJustified] {
            let japanese = Settings {
                alignment,
                composer: Composer::JapaneseParagraph,
                ..Settings::default()
            };
            let roman = Settings {
                composer: Composer::RomanParagraph,
                ..japanese
            };
            assert_eq!(compose(&items, &japanese, &|_| 173.0)[0], latin.len() + 1);
            assert_eq!(compose(&items, &japanese, &|_| 174.0)[0], latin.len() + 2);
            assert_eq!(compose(&items, &roman, &|_| 173.0)[0], latin.len() + 2);
        }
    }
}
