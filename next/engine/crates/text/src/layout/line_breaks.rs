//! Map saved line starts and pinned character positions to composition items.

use super::*;

impl ParaCtx<'_> {
    pub(super) fn saved_starts(&self, forced: Option<&[ForcedLine]>) -> (Vec<usize>, Vec<usize>) {
        let shaped = self.shaped;
        let starts: Vec<usize> = forced
            .map(|f| {
                f.iter()
                    .skip(1)
                    .map(|l| {
                        let j = self.index.partition_point(|&x| x < l.start);
                        shaped.clusters.partition_point(|c| c.chars.start < j)
                    })
                    .collect()
            })
            .unwrap_or_default();
        // InDesign broke these words with a hyphen (a Latin letter on both sides).
        let latin = |k: usize| shaped.clusters.get(k).is_some_and(|c| c.kind == ClusterKind::Glyphs && folio_compose::breaks::is_latin_letter(c.ch));
        let word_breaks: Vec<usize> = starts.iter().copied().filter(|&k| k > 0 && latin(k - 1) && latin(k)).collect();
        (starts, word_breaks)
    }
}

impl Prepared {
    fn item_indices(&self, shaped: &ShapedParagraph) -> Vec<usize> {
        let n = self.items.len();
        let mut item_of = vec![n; shaped.clusters.len() + 1];
        for (i, o) in self.origin.iter().enumerate().rev() {
            if let Some(c) = o {
                item_of[*c] = i;
            }
        }
        item_of
    }

    pub(super) fn saved_breaks(
        &self, shaped: &ShapedParagraph, forced: Option<&[ForcedLine]>, starts: &[usize],
    ) -> (Vec<usize>, Vec<ForcedLine>) {
        let n = self.items.len();
        // The forced breaks as item indices, with the saved line each line follows.
        let mut forced_breaks: Vec<usize> = Vec::new();
        let mut forced_lines: Vec<ForcedLine> = Vec::new();
        if let Some(f) = forced {
            let item_of = self.item_indices(shaped);
            forced_lines.push(f[0]);
            for (k, l) in starts.iter().zip(&f[1..]) {
                let b = item_of[*k];
                if b > forced_breaks.last().copied().unwrap_or(0) && b < n {
                    forced_breaks.push(b);
                    forced_lines.push(*l);
                }
            }
        }
        (forced_breaks, forced_lines)
    }

    pub(super) fn pinned_items(&self, p: &ParaCtx<'_>) -> Vec<usize> {
        let n = self.items.len();
        let shaped = p.shaped;
        // Nested line styles: the item index each pinned line starts at.
        if p.pins.is_empty() { Vec::new() } else {
            let item_of = self.item_indices(shaped);
            let mut items: Vec<usize> = Vec::new();
            for &j in p.pins {
                let b = item_of[shaped.clusters.partition_point(|c| c.chars.start < j)];
                if b > items.last().copied().unwrap_or(0) && b < n {
                    items.push(b);
                }
            }
            items
        }
    }
}
