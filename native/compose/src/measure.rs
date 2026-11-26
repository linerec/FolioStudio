use crate::{Item, Settings, cost::LineMetrics, is_trailing_blank, is_word_space};

pub(crate) fn visible_length(items: &[Item]) -> usize {
    items
        .iter()
        .rposition(|i| !is_trailing_blank(i.ch))
        .map_or(0, |i| i + 1)
}

pub(crate) fn natural_width(visible: &[Item]) -> f64 {
    visible.split_last().map_or(0.0, |(last, rest)| {
        rest.iter().map(|i| i.advance).sum::<f64>() + last.end_advance
    })
}

#[derive(Default, Clone, Copy)]
struct Prefix {
    width: f64,
    spaces: f64,
    visible_end: usize,
}

pub(crate) struct Measures<'a> {
    items: &'a [Item],
    prefix: Vec<Prefix>,
    space_factor: f64,
}
impl<'a> Measures<'a> {
    pub fn new(items: &'a [Item], s: &Settings) -> Self {
        let mut prefix = Vec::with_capacity(items.len() + 1);
        let mut sum = Prefix::default();
        prefix.push(sum);
        for (i, item) in items.iter().enumerate() {
            sum.width += item.advance;
            if is_word_space(item.ch) {
                sum.spaces += item.advance;
            }
            if !is_trailing_blank(item.ch) {
                sum.visible_end = i + 1;
            }
            prefix.push(sum);
        }
        Self {
            items,
            prefix,
            space_factor: 100.0 / s.word_spacing.desired.max(1.0),
        }
    }
    pub fn line(&self, start: usize, end: usize) -> LineMetrics {
        let visible = self.prefix[end].visible_end;
        if visible <= start {
            return LineMetrics::default();
        }
        let last = visible - 1;
        LineMetrics {
            natural: self.prefix[last].width - self.prefix[start].width
                + self.items[last].end_advance,
            space_unit: (self.prefix[last].spaces - self.prefix[start].spaces) * self.space_factor,
            last_visible: Some(last),
        }
    }
}
