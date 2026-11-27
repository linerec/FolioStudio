//! Find a line's horizontal slot and advance past obstructions within its column.

use super::*;

pub(super) enum Band {
    Fits {
        left: f64,
        right: f64,
        next: Option<(f64, f64)>,
        first_band: Option<(f64, f64)>,
    },
    Retry,
    NextColumn,
}

impl Flow<'_> {
    pub(super) fn fit_band(
        &mut self, c: &Column, m: &LineMetrics, b: f64, need: f64, forced: bool, nudge: &mut f64,
    ) -> Band {
        let ff = self.frame().expect("column implies frame");
        // The first line of an area: its band runs from the area top (and the
        // steps taken down) to its first baseline.
        let first_in_area = self.last.is_none() && self.jumped_from.is_none() && m.above_line <= 0.0;
        // A line meets a wrap over its leading, from its baseline up, but not
        // above the area's top (the frame top or the wrap just jumped); its
        // descenders may overlap. Measured on #347: leading 95.2% of line ends,
        // ascent 94.6%, em box 94.6%, either with the descent ≤ 90.7%.
        let area_top = self.region_top.unwrap_or(c.top);
        // Vertical text: the band is the line's em box, across the Roman baseline (vtext-c2, InDesign 2025: a
        // line whose baseline stops 0.64 pt short of a wrap but whose em box crosses it is wrapped, oracle
        // vtext2/vt2-wrap-show; the rows of an oval frame start where the oval crosses the em box,
        // vtext2/vt2-oval-show, not the leading above the baseline).
        let band_end = if self.vertical { b + m.embox_below } else { b };
        let band_top = if self.vertical { b - m.embox } else { band_end - m.leading };
        let top = if first_in_area { area_top + *nudge } else { band_top.max(area_top) };
        match wrap_slot(c, ff.shape, ff.wraps, top, band_end, ff.verso, need) {
            // A line holds at least the text up to its first break opportunity:
            // InDesign leaves a narrower slot beside a wrap (#347: an empty
            // paragraph sits in a 4.8 pt slot) and does not set a word wider
            // than the column (it is overset; InDesign 2025 experiments, also
            // 0.1 pt too wide). See `first_unit`.
            Slot::Fits(l, r, next) if forced || r - l >= need - TOL => {
                let first_band = (first_in_area && ff.shape.is_some()).then_some((area_top, b - top));
                Band::Fits { left: l, right: r, next, first_band }
            }
            Slot::Fits(..) => Band::NextColumn,
            Slot::Step => {
                *nudge += 1.0;
                Band::Retry
            }
            Slot::JumpBelow(y) if m.leading > EPS => {
                *nudge = 0.0;
                if self.last.is_none() {
                    self.region_top = Some(y);
                    self.jumped_from = Some(b);
                    return Band::Retry;
                }
                let steps = ((y - band_top) / m.leading).ceil().max(1.0);
                self.last = Some(b + (steps - 1.0) * m.leading);
                Band::Retry
            }
            Slot::SkipRow => {
                self.last = Some(b);
                *nudge = 0.0;
                Band::Retry
            }
            // A line with a line before it in the column and a wrap blocking its whole width goes
            // down by whole leadings until its leading band clears the wrap (InDesign 2025, oracle
            // wrapmid: wrap bottom 110.5 -> 128.74, not 119.24).
            Slot::Below(y) if self.last.is_some() && ff.shape.is_none() && m.leading > EPS && y > self.region_top.unwrap_or(c.top) + EPS => {
                *nudge = 0.0;
                let steps = ((y - band_top) / m.leading).ceil().max(1.0);
                self.last = Some(b + (steps - 1.0) * m.leading);
                Band::Retry
            }
            Slot::Below(y) if y > self.region_top.unwrap_or(c.top) + EPS => {
                *nudge = 0.0;
                self.region_top = Some(y);
                self.jumped_from = Some(b);
                self.last = None;
                Band::Retry
            }
            // Next column, or no progress below the wrap.
            _ => Band::NextColumn,
        }
    }
}
