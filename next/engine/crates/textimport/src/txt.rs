//! Plain text (placetext-c1, `artifacts/placetext-c1-20261023/STATUS.ko.md`): bytes → characters → returns → paragraphs.
use crate::messages::msg;
use crate::{CharacterSet, Options, Paragraph, TextSource, charset};

pub(crate) fn read(bytes: &[u8], options: &Options) -> Result<TextSource, String> {
    let mut warnings = Vec::new();
    let text = decode(bytes, options.character_set.resolve(bytes)).unwrap_or_else(|| {
        // bytes that are no UTF-8: InDesign places nothing (an empty story) and says nothing (placetext-c1 utf8-invalid, utf8-nobom-bad)
        warnings.push(msg("textimport.invalidUtf8", "UTF-8 로 읽을 수 없는 글이어서 빈 글이 들어갑니다 (InDesign과 같습니다)", serde_json::json!({})));
        String::new()
    });
    let text = filter_controls(&text)?;
    // NUL and U+001C come in as U+FEFF (measured: placetext-c1 ctl-00, ctl-1c)
    let text: String = text.chars().map(|c| if c == '\0' || c == '\u{1C}' { '\u{FEFF}' } else { c }).collect();
    let text = normalize_returns(&text);
    let text = strip_returns(&text, options.strip_returns_between_lines, options.strip_returns_between_paragraphs);
    let text = if options.convert_spaces_into_tabs { spaces_into_tabs(&text, options.spaces_into_tabs_count.max(1) as usize) } else { text };
    let text = if options.use_typographers_quotes { typographers_quotes(&text) } else { text };
    let ends_with_return = text.ends_with('\r');
    let body = if ends_with_return { &text[..text.len() - 1] } else { &text[..] };
    Ok(TextSource {
        format: "txt".into(),
        paragraph_styles: Vec::new(),
        character_styles: Vec::new(),
        colors: Vec::new(),
        formatted: false,
        paragraphs: body.split('\r').map(Paragraph::plain).collect(),
        ends_with_return,
        numbering_lists: Vec::new(),
        warnings: warnings.iter().map(|m| m.message.clone()).collect(),
        warning_messages: warnings,
    })
}

/// Control characters, measured one at a time in 121-byte files (placetext-c1 ctl-long-*, ctl-0b-len*): some make InDesign refuse the
/// file ("No filter found for requested operation"), the others are dropped from the text; but a file where those are one character in
/// ten or more (a 7 byte file is refused, an 11 byte one is not) is not taken for text either. NUL, U+001C, U+007F and U+0085 are text.
fn filter_controls(text: &str) -> Result<String, String> {
    const REFUSED: [u32; 11] = [0x01, 0x02, 0x04, 0x05, 0x08, 0x10, 0x12, 0x16, 0x18, 0x19, 0x1E];
    const DROPPED: [u32; 16] = [0x03, 0x06, 0x07, 0x0B, 0x0C, 0x0E, 0x0F, 0x11, 0x13, 0x14, 0x15, 0x17, 0x1A, 0x1B, 0x1D, 0x1F];
    let mut dropped = 0usize;
    for c in text.chars() {
        if REFUSED.contains(&(c as u32)) {
            return Err(format!("글 파일로 읽을 수 없습니다: 제어 문자 U+{:04X}가 들어 있습니다", c as u32));
        }
        dropped += usize::from(DROPPED.contains(&(c as u32)));
    }
    if dropped == 0 {
        return Ok(text.to_owned());
    }
    if dropped * 10 > text.chars().count() {
        return Err("글 파일로 읽을 수 없습니다: 제어 문자가 너무 많습니다".into());
    }
    Ok(text.chars().filter(|c| !DROPPED.contains(&(*c as u32))).collect())
}

/// A byte order mark always decides (the Character Set choice is only for files without one); a UTF-32 mark is none: FF FE is
/// UTF-16 whatever follows (utf32le-bom). `None`: bytes read as UTF-8 that are no UTF-8.
pub(crate) fn decode(bytes: &[u8], set: CharacterSet) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec()).ok();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Some(encoding_rs::UTF_16LE.decode_without_bom_handling(rest).0.into_owned());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Some(encoding_rs::UTF_16BE.decode_without_bom_handling(rest).0.into_owned());
    }
    Some(match set {
        CharacterSet::Ansi => charset::decode_ansi(bytes),
        CharacterSet::Utf8 => return String::from_utf8(bytes.to_vec()).ok(),
        CharacterSet::Utf16 => encoding_rs::UTF_16LE.decode_without_bom_handling(bytes).0.into_owned(),
        other => other.encoding().map_or_else(|| charset::decode_ansi(bytes), |e| e.decode_without_bom_handling(bytes).0.into_owned()),
    })
}

/// CR LF, LF and CR each end one paragraph (U+000D in the story).
fn normalize_returns(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\r');
            }
            '\n' => out.push('\r'),
            _ => out.push(c),
        }
    }
    out
}

/// "Remove at the end of every line": one return between lines becomes a space; "…of every paragraph": a run of
/// returns becomes one return (measured on `wrapped.txt`).
fn strip_returns(text: &str, lines: bool, paragraphs: bool) -> String {
    if !lines && !paragraphs {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut run = 0usize;
    let flush = |out: &mut String, run: &mut usize| {
        match *run {
            0 => {}
            1 => out.push(if lines { ' ' } else { '\r' }),
            n => out.extend(std::iter::repeat_n('\r', if paragraphs { 1 } else { n })),
        }
        *run = 0;
    };
    for c in text.chars() {
        if c == '\r' {
            run += 1;
        } else {
            flush(&mut out, &mut run);
            out.push(c);
        }
    }
    flush(&mut out, &mut run);
    out
}

/// A run of `count` or more spaces becomes one tab.
fn spaces_into_tabs(text: &str, count: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut spaces = 0usize;
    let flush = |out: &mut String, spaces: &mut usize| {
        if *spaces >= count {
            out.push('\t');
        } else {
            out.extend(std::iter::repeat_n(' ', *spaces));
        }
        *spaces = 0;
    };
    for c in text.chars() {
        if c == ' ' {
            spaces += 1;
        } else {
            flush(&mut out, &mut spaces);
            out.push(c);
        }
    }
    flush(&mut out, &mut spaces);
    out
}

/// Straight quotes become English (USA) curly quotes. The quote's side follows the character before it in the file: opening at the
/// start of the text and after a space, a tab, a return, a no-break space, `(`, `[`, `{`, `«` and a curly opening quote; closing after
/// anything else (a letter, a digit, `<`, `-`, `/`, `.`, a dash, an ellipsis, a straight quote ...). Measured with every ASCII context
/// and eleven others (placetext-c1 quotes2); `""` is a pair, and so is `'"'`.
pub(crate) fn typographers_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    for c in text.chars() {
        out.push(smart_quote(prev, c));
        prev = Some(c);
    }
    out
}

/// What a straight quote after `prev` (`None`: at the start of the text) becomes; any other character stays as it is. Typing a quote with
/// "Use Typographer's Quotes" on follows the same rule (InDesign 2025, real key presses: `"a"`, `""` gives `““`, `"-"` gives `“-”`, `'tis`
/// gives `‘tis`, `5'10"` gives `5’10”`), whether autocorrect is on or off.
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
