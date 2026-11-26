mod diagnostics;
mod fit;
mod score;
mod search;
mod slots;

use crate::SOFT_HYPHEN;
use crate::{
    Item, Settings,
    breaks::{HyphenBreak, hyphen_break, opportunities, word_limit_penalty},
    measure::Measures,
};
pub use diagnostics::{explain, explain_slots, path_cost};
pub use fit::{first_fit, first_unit};
pub use slots::{MAX_SLOTS, Slots};

pub const LONG_PARAGRAPH: usize = 8001;
pub const SHORT_PARAGRAPH: usize = 3;

#[derive(Default, Clone, Copy)]
struct State {
    row: usize,
    grade: u8,
    history: u8,
}
#[derive(Debug, Clone, Copy)]
pub struct LineReport {
    pub start: usize,
    pub end: usize,
    pub natural: f64,
    pub measure: f64,
    pub space_unit: f64,
    pub category: u8,
    pub cost: f64,
    pub hyphen_penalty: f64,
}

struct Stop {
    at: usize,
    unit: f64,
    forced: bool,
    hyphen: Option<HyphenBreak>,
    word_limit: f64,
    ordinary_before: usize,
    count: usize,
}
struct Paragraph<'a> {
    items: &'a [Item],
    settings: &'a Settings,
    widths: Measures<'a>,
    uniform_size: Option<f64>,
    stops: Vec<Stop>,
}
impl<'a> Paragraph<'a> {
    fn new(items: &'a [Item], settings: &'a Settings, extra: &[usize]) -> Self {
        let widths = Measures::new(items, settings);
        let mut positions = opportunities(items, settings);
        positions.push(items.len());
        let original = if extra.is_empty() {
            None
        } else {
            let original = positions.clone();
            positions.extend(extra.iter().copied().filter(|&i| i > 0 && i < items.len()));
            positions.sort_unstable();
            positions.dedup();
            Some(original)
        };
        let mut stops = Vec::with_capacity(positions.len() + 1);
        let mut ordinary = 0;
        let mut count = 0;
        let mut prev = 0;
        for (idx, at) in [0].into_iter().chain(positions.iter().copied()).enumerate() {
            count += items[prev..at]
                .iter()
                .filter(|i| i.ch != SOFT_HYPHEN)
                .count();
            let hyphen = if at == items.len() {
                None
            } else {
                hyphen_break(items, at, settings)
            };
            let word_limit = if hyphen.is_some_and(|h| h.discretionary) {
                word_limit_penalty(items, at, 12.0, settings)
            } else {
                0.0
            };
            let end = match &original {
                Some(ops) => ops.get(ops.partition_point(|&p| p <= at)),
                None => positions.get(idx),
            }
            .copied()
            .unwrap_or(items.len());
            let unit = fit::unit_width(items, at, end, settings, |a, b| widths.line(a, b).natural);
            stops.push(Stop {
                at,
                unit,
                forced: at > 0 && items[at - 1].ch == '\n',
                hyphen,
                word_limit,
                ordinary_before: ordinary,
                count,
            });
            if hyphen.is_none() {
                ordinary = idx;
            }
            prev = at;
        }
        let uniform_size = items
            .first()
            .map(|item| item.point_size)
            .filter(|size| items.iter().all(|item| item.point_size == *size));
        Self {
            items,
            settings,
            widths,
            stops,
            uniform_size,
        }
    }
    fn end(&self) -> usize {
        self.stops.len() - 1
    }
    fn line_slots(&self, end: usize, mut slots: Slots) -> Slots {
        if end == self.end() {
            slots.w[0] -= self.settings.last_line_indent;
        }
        slots
    }
}

pub fn compose(items: &[Item], settings: &Settings, width: &dyn Fn(usize) -> f64) -> Vec<usize> {
    compose_rooms(items, settings, &|row, _| width(row))
}
pub fn compose_rooms(
    items: &[Item],
    settings: &Settings,
    room: &dyn Fn(usize, f64) -> f64,
) -> Vec<usize> {
    compose_slots(items, settings, &|r, u| Slots::one(room(r, u))).0
}
pub fn compose_slots(
    items: &[Item],
    settings: &Settings,
    room: &dyn Fn(usize, f64) -> Slots,
) -> (Vec<usize>, Vec<Vec<usize>>) {
    if items.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if !(SHORT_PARAGRAPH..LONG_PARAGRAPH).contains(&items.len())
        || !settings.alignment.is_justified() && items.iter().any(|i| i.ch == '\t')
    {
        let breaks = first_fit(items, settings, &|r| room(r, 0.0).w[0]);
        let splits = vec![vec![]; breaks.len() + 1];
        return (breaks, splits);
    }
    let paragraph = Paragraph::new(items, settings, &[]);
    let ends = search::line_ends(&paragraph, room);
    let mut start = 0;
    let mut splits = Vec::with_capacity(ends.len());
    for (row, &end) in ends.iter().enumerate() {
        let slots = paragraph.line_slots(end, room(row, paragraph.stops[start].unit));
        splits.push(if slots.n > 1 {
            let justified = settings.alignment.is_justified() && end != paragraph.end();
            let plan = paragraph.slot_plan(start, end, row, slots, justified);
            plan.splits(&paragraph, start, slots.len())
        } else {
            Vec::new()
        });
        start = end;
    }
    (
        ends.into_iter()
            .filter(|&end| end != paragraph.end())
            .map(|end| paragraph.stops[end].at)
            .collect(),
        splits,
    )
}
