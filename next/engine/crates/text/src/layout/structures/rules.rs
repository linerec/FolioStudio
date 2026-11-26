use super::{Column, FlowLine, Paint, ParagraphRule, RuleWidth};

pub(super) fn paint(rule: &ParagraphRule, text: impl FnOnce() -> Paint) -> Paint {
    rule.paint.clone().unwrap_or_else(text)
}

pub(super) fn span(rule: &ParagraphRule, column: &Column, line: &FlowLine) -> (f64, f64) {
    let (left, right) = match rule.width {
        RuleWidth::Column => (column.left, column.right),
        RuleWidth::Text => line.text,
    };
    (left + rule.left_indent, right - rule.right_indent)
}
