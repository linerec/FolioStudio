use super::{LineReport, Paragraph, Slots, State};
use crate::{Alignment, Item, Settings};

impl Paragraph<'_> {
    pub(super) fn best_line_mode(
        &self,
        start: usize,
        end: usize,
        state: State,
        slots: Slots,
        justify_overflow: bool,
    ) -> (LineReport, State) {
        let justified = self.settings.alignment.is_justified();
        let full = self.score(start, end, state, slots, justified);
        if !justified || end != self.end() || self.settings.alignment == Alignment::FullyJustified {
            return full;
        }
        let ragged = self.score(start, end, state, slots, false);
        if (justify_overflow && ragged.0.category == 5) || full.0.cost < ragged.0.cost {
            full
        } else {
            ragged
        }
    }
}
pub fn explain_slots(
    items: &[Item],
    settings: &Settings,
    room: &dyn Fn(usize, f64) -> Slots,
    breaks: &[usize],
) -> Vec<LineReport> {
    if items.is_empty() {
        return Vec::new();
    }
    let p = Paragraph::new(items, settings, breaks);
    let mut state = State::default();
    let mut start = 0;
    let mut reports = Vec::new();
    for at in breaks
        .iter()
        .copied()
        .filter(|&b| b > 0 && b < items.len())
        .chain([items.len()])
    {
        let end = p.stops.partition_point(|stop| stop.at < at);
        if end <= start {
            continue;
        }
        let slots = room(state.row, p.stops[start].unit);
        let (report, next) = p.best_line_mode(start, end, state, slots, false);
        reports.push(report);
        state = next;
        start = end;
    }
    reports
}
pub fn explain(
    items: &[Item],
    settings: &Settings,
    width: &dyn Fn(usize) -> f64,
    breaks: &[usize],
) -> Vec<LineReport> {
    explain_slots(items, settings, &|r, _| Slots::one(width(r)), breaks)
}
pub fn path_cost(
    items: &[Item],
    settings: &Settings,
    width: &dyn Fn(usize) -> f64,
    breaks: &[usize],
) -> f64 {
    explain(items, settings, width, breaks)
        .iter()
        .map(|l| l.cost)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustive_final_line_uses_justification_when_its_slot_overflows() {
        let items = "abc"
            .chars()
            .map(|ch| Item::new(ch, 10.0, 0.0, 12.0))
            .collect::<Vec<_>>();
        let settings = Settings::default();
        let paragraph = Paragraph::new(&items, &settings, &[]);
        let state = State::default();
        let slots = Slots::of(&[29.99, 29.99]);
        let score = |justified| {
            paragraph
                .score(0, paragraph.end(), state, slots, justified)
                .0
        };
        let ragged = score(false);
        let full = score(true);
        assert_eq!(ragged.category, 5);
        assert!(ragged.cost < full.cost);
        let (chosen, _) = paragraph.best_line_mode(0, paragraph.end(), state, slots, true);
        assert_eq!(chosen.cost, full.cost);
    }
}
