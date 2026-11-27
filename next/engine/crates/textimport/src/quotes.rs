//! Typographer’s quotes in imported text, including formatted stories and inline content.
use crate::{Inline, Paragraph};

pub(super) fn story(paragraphs: &mut [Paragraph]) {
    paragraph_text(paragraphs);
    for run in paragraphs.iter_mut().flat_map(|p| &mut p.runs) {
        match &mut run.inline {
            Some(Inline::Table(table)) => {
                for cell in &mut table.cells {
                    paragraph_text(&mut cell.paragraphs);
                }
            }
            Some(Inline::Footnote(note) | Inline::Endnote(note)) => {
                *note = text(note);
                // Formatted note runs each start their own quote context.
                for part in &mut run.note {
                    part.text = text(&part.text);
                }
            }
            Some(Inline::Picture(_)) | None => {}
        }
    }
}

fn paragraph_text(paragraphs: &mut [Paragraph]) {
    for paragraph in paragraphs {
        // A paragraph starts after a return. Formatting changes keep the previous input character.
        let mut previous = None;
        for run in &mut paragraph.runs {
            run.text = convert(&run.text, &mut previous);
        }
    }
}

/// Convert an independent piece of text using the preceding character from the input.
pub(super) fn text(input: &str) -> String {
    convert(input, &mut None)
}

fn convert(input: &str, previous: &mut Option<char>) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        out.push(smart_quote(*previous, c));
        *previous = Some(c);
    }
    out
}

/// Choose the English typographer’s quote after `prev`, or at the start of text for `None`.
/// Import supplies the preceding input character. Interactive typing supplies the character in the story.
pub fn smart_quote(prev: Option<char>, c: char) -> char {
    let opening = prev.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | '{' | '\u{AB}' | '\u{2018}' | '\u{201C}'));
    match c {
        '"' if opening => '\u{201C}',
        '"' => '\u{201D}',
        '\'' if opening => '\u{2018}',
        '\'' => '\u{2019}',
        other => other,
    }
}
