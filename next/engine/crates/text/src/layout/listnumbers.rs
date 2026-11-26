use super::structures::format_number;
use folio_model::ListStyle;

#[derive(Clone, Default)]
struct Level {
    number: Option<u32>,
    format: Option<String>,
    higher_seen: bool,
}

#[derive(Clone, Default)]
pub(super) struct NumberingState {
    levels: [Level; 10],
}

impl NumberingState {
    pub fn label(&mut self, style: &ListStyle) -> String {
        let level = (style.level as usize).clamp(1, 9);
        let current = &mut self.levels[level];
        let number = style.restart_at.unwrap_or_else(|| {
            if style.restart_after_higher_level && current.higher_seen {
                style.start_at
            } else {
                current.number.map_or(style.start_at, |n| n + 1)
            }
        });
        current.number = Some(number);
        current.format = Some(style.format.clone());
        current.higher_seen = false;
        for lower in self.levels.iter_mut().skip(level + 1) {
            lower.higher_seen = true;
        }

        let mut out = String::new();
        let mut chars = style.expression.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '^' {
                let reference = match chars.peek().copied() {
                    Some('#') => Some(level),
                    Some(digit @ '1'..='9') => Some(digit as usize - '0' as usize),
                    _ => None,
                };
                if let Some(reference) = reference {
                    chars.next();
                    let value = &self.levels[reference];
                    out.push_str(&format_number(
                        value.number.unwrap_or(1),
                        value.format.as_deref().unwrap_or("1, 2, 3, 4..."),
                    ));
                    continue;
                }
            }
            out.push(ch);
        }
        out
    }
}
