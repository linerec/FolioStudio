use super::structures::translate_line;
use super::{Column, EPS, Flow, FlowFrame, FlowLine, Paragraph, ParagraphAttrs, first_baseline};
use folio_model::paragraph_columns::{ColumnLayout, ParagraphColumns};

mod balance;
mod depth;
mod regions;
mod wrapping;

pub use depth::partition;
pub(super) use balance::prefix_bottom;
pub(super) use regions::{begin, effective, finish};

pub(super) fn justify(flow: &mut Flow<'_>) -> Vec<bool> {
    use folio_model::VerticalJustification::{Bottom, Center};
    let mut handled = vec![false; flow.frames.len()];
    for (fi, frame) in flow.frames.iter_mut().enumerate() {
        let fraction = match frame.frame.vertical_justification {
            Center => 0.5,
            Bottom => 1.0,
            _ => continue,
        };
        if frame.cols.len() <= frame.base_columns
            || frame.lines.is_empty()
            || flow
                .saved_columns
                .iter()
                .any(|&(saved_fi, _)| saved_fi == fi)
        {
            continue;
        }
        // R15
        if let Some(&(_, span)) = flow
            .keep
            .spans
            .range((fi, 0)..=(fi, usize::MAX))
            .next_back()
        {
            let tail = span + 1..frame.cols.len();
            if tail.len() <= frame.base_columns {
                let columns: Vec<_> = tail
                    .filter(|&ci| frame.cols[ci].previous.is_some())
                    .collect();
                if columns.len() > 1 {
                    let rows = balance::rows_in(frame, &columns);
                    balance::balance(frame, &rows, &columns);
                }
            }
        }
        // R14
        let bottom = frame
            .cols
            .iter()
            .map(|c| c.bottom)
            .fold(f64::INFINITY, f64::min);
        let used = frame
            .lines
            .iter()
            .map(|l| l.line.baseline)
            .fold(f64::NEG_INFINITY, f64::max);
        let dy = (bottom - used).max(0.0) * fraction;
        if dy > 0.0 {
            for line in &mut frame.lines {
                translate_line(&mut line.line, 0.0, dy);
            }
        }
        handled[fi] = true;
    }
    handled
}
