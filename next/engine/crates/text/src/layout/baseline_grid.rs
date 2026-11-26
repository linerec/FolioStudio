use super::FrameRef;
use folio_model::{ParagraphAttrs, TextPreferences};

#[derive(Clone, Copy, Debug)]
pub(super) struct Grid { origin: f64, step: f64 }

impl Grid {
    pub(super) fn for_frame(prefs: &TextPreferences, frame: &FrameRef<'_>) -> Option<Self> {
        let page = frame.page_rect?;
        let t = frame.to_spread;
        if t.b.abs() > 1e-6 || t.c.abs() > 1e-6 || t.d <= 0.0 || prefs.baseline_division <= 0.0 {
            return None;
        }
        let margin = if prefs.baseline_relative_to.starts_with("TopOfMargin") { frame.margins[0] } else { 0.0 };
        Some(Self { origin: (page.top + margin + prefs.baseline_start - t.ty) / t.d, step: prefs.baseline_division / t.d })
    }

    /// A vertical frame's grid in its virtual horizontal frame (`crate::vertical`): the grid runs down the page, its lines
    /// counted from the page's right edge (the right margin for "Top of Margin"), and the virtual y is the frame's right
    /// edge leftwards (vtext-c2, InDesign 2025: 24 pt from the page's right edge, oracle vtext2/vt2-grid).
    pub(super) fn for_vertical_frame(prefs: &TextPreferences, frame: &FrameRef<'_>) -> Option<Self> {
        let page = frame.page_rect?;
        let t = frame.to_spread;
        if t.b.abs() > 1e-6 || t.c.abs() > 1e-6 || t.a <= 0.0 || prefs.baseline_division <= 0.0 {
            return None;
        }
        let area = folio_model::paths_bounds(&frame.item.paths)?;
        let margin = if prefs.baseline_relative_to.starts_with("TopOfMargin") { frame.margins[3] } else { 0.0 };
        let first = (page.right - margin - prefs.baseline_start - t.tx) / t.a;
        Some(Self { origin: area.left + area.right - first, step: prefs.baseline_division / t.a })
    }

    pub(super) fn for_line(self, attrs: &ParagraphAttrs, line: usize) -> Option<Self> {
        (attrs.align_to_baseline && (!attrs.grid_align_first_line_only || line == 0)).then_some(self)
    }

    pub(super) fn place(self, baseline: f64) -> f64 {
        self.origin + ((baseline - self.origin - 1e-6) / self.step).ceil().max(0.0) * self.step
    }
}
