//! Data Merge source decoding and field names. No file access or document mutation.
//!
//! InDesign 2025 reads a data source in two ways (probes `datamerge-c2-*`, `datamerge-c4-*`, rules `oracle/rules/datamerge.md`):
//! - without the import options (the scripting `selectDataSource`, and `parse` here): the delimiter comes from the header, a UTF-16
//!   file needs its BOM, a UTF-8 BOM file is refused, a file with other non-ASCII bytes is not guessed (Folio refuses it and asks for
//!   an encoding, where InDesign reads it garbled);
//! - with them (File ▸ Select Data Source ▸ Show Import Options → "Data Source Import Options": Delimiter Comma / Tab / Semicolon,
//!   Encoding ASCII / Unicode / Shift_JIS, Platform Macintosh / PC, Preserve Spaces in Data Source): `parse_with`, with the choices
//!   the dialog fills in beforehand (`detect`) for any it is not given.
use encoding_rs::{SHIFT_JIS, WINDOWS_1252};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub kind: String,
}

/// The choices of the Data Source Import Options dialog, as they were applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub delimiter: Delimiter,
    pub encoding: Encoding,
    pub platform: Platform,
    pub preserve_spaces: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Delimiter { Comma, Tab, Semicolon }

/// `Ascii`: every byte is one character of Windows-1252 (the five bytes it has no character for are a space). `Unicode`: UTF-16 (the BOM tells the byte order, without one it is little
/// endian). `ShiftJis`: a lead byte (81-9F, E0-FC) and a valid trail byte are one Shift_JIS character (one the table has no character
/// for is dropped, so is a lead byte without a valid trail; the user-defined rows F0-F9 are Private Use characters); every other byte is a character of Windows-1252 (half-width katakana
/// bytes too: A1-DF stay Latin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encoding { Ascii, Unicode, ShiftJis }

/// No difference was found between Macintosh and PC in any measured case; the choice is kept (`options`) and read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform { Pc, Macintosh }

/// What the caller chooses; `None` is what the dialog fills in by itself for the file (`detect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub delimiter: Option<Delimiter>,
    pub encoding: Option<Encoding>,
    pub platform: Platform,
    pub preserve_spaces: bool,
}

impl Default for Choice {
    fn default() -> Self { Choice { delimiter: None, encoding: None, platform: Platform::Pc, preserve_spaces: false } }
}

impl Delimiter {
    fn char(self) -> char { match self { Delimiter::Comma => ',', Delimiter::Tab => '\t', Delimiter::Semicolon => ';' } }
    pub fn name(self) -> &'static str { match self { Delimiter::Comma => "comma", Delimiter::Tab => "tab", Delimiter::Semicolon => "semicolon" } }
}
impl Encoding {
    pub fn name(self) -> &'static str { match self { Encoding::Ascii => "ascii", Encoding::Unicode => "unicode", Encoding::ShiftJis => "shiftJis" } }
}
impl Platform {
    pub fn name(self) -> &'static str { match self { Platform::Pc => "pc", Platform::Macintosh => "macintosh" } }
}

/// The wire names (`comma`, `ascii`, `pc`, …; empty is "not chosen") of the choices, as the request and the command carry them.
pub fn choice(delimiter: &str, encoding: &str, platform: &str, preserve_spaces: bool) -> Result<Choice, String> {
    Ok(Choice {
        delimiter: match delimiter { "" => None, "comma" => Some(Delimiter::Comma), "tab" => Some(Delimiter::Tab), "semicolon" => Some(Delimiter::Semicolon),
            other => return Err(format!("delimiter is comma, tab or semicolon, not {other}")) },
        encoding: match encoding { "" => None, "ascii" => Some(Encoding::Ascii), "unicode" => Some(Encoding::Unicode), "shiftJis" => Some(Encoding::ShiftJis),
            other => return Err(format!("encoding is ascii, unicode or shiftJis, not {other}")) },
        platform: match platform { "" | "pc" => Platform::Pc, "macintosh" => Platform::Macintosh, other => return Err(format!("platform is pc or macintosh, not {other}")) },
        preserve_spaces,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub encoding: String,
    pub delimiter: String,
    pub fields: Vec<Field>,
    pub records: Vec<Vec<String>>,
    /// The import options that were applied (what the dialog shows).
    pub options: Options,
}

const MAX_BYTES: usize = 16 * 1024 * 1024;

fn is_lead(b: u8) -> bool { matches!(b, 0x81..=0x9F | 0xE0..=0xFC) }
fn is_trail(b: u8) -> bool { matches!(b, 0x40..=0x7E | 0x80..=0xFC) }

/// The Shift_JIS character of a lead and a trail byte, when the table has one (the pair is dropped when it has none).
fn pair(lead: u8, trail: u8) -> Option<String> {
    let bytes = [lead, trail];
    SHIFT_JIS.decode_without_bom_handling_and_without_replacement(&bytes).map(|text| text.into_owned())
}

/// Whether the first high byte of the file starts a Shift_JIS pair (measured: that decides the dialog's pre-selection; the `ë` of a
/// Windows-1252 file is `EB 2C`, a comma after a lead byte, and the file shows ASCII although a later `ö` is `F6 6C`).
fn first_high_byte_starts_pair(bytes: &[u8]) -> bool {
    match bytes.iter().position(|&b| b >= 0x80) {
        Some(i) => is_lead(bytes[i]) && bytes.get(i + 1).is_some_and(|&t| is_trail(t)),
        None => false,
    }
}

/// The encoding the dialog shows for the file before anything is chosen (measured): Unicode for a UTF-16 file (a BOM, or the byte
/// pattern of little endian text without one), Shift_JIS when the first high byte starts a valid pair (UTF-8 text does), else ASCII.
pub fn detect_encoding(bytes: &[u8]) -> Encoding {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) || (bytes.len() >= 4 && bytes[1] == 0 && bytes[0] != 0 && bytes[3] == 0) { Encoding::Unicode }
    else if first_high_byte_starts_pair(bytes) { Encoding::ShiftJis }
    else { Encoding::Ascii }
}

fn decode_unicode(bytes: &[u8]) -> Result<(String, &'static str), String> {
    let (body, be) = if bytes.starts_with(&[0xff, 0xfe]) { (&bytes[2..], false) } else if bytes.starts_with(&[0xfe, 0xff]) { (&bytes[2..], true) } else { (bytes, false) };
    if body.len() % 2 != 0 { return Err("Incomplete UTF-16 code unit".into()); }
    let words: Vec<u16> = body.chunks_exact(2).map(|b| if be { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) }).collect();
    Ok((String::from_utf16(&words).map_err(|_| "Invalid UTF-16")?, if be { "UTF-16BE" } else { "UTF-16LE" }))
}

/// One byte as Windows-1252 (the five bytes it has no character for read as a space: measured, `0x9D` in a merged text).
fn latin(b: u8, out: &mut String) {
    if matches!(b, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D) { out.push(' '); } else { out.push_str(&WINDOWS_1252.decode_without_bom_handling(&[b]).0); }
}

fn decode_shift_jis(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 { out.push(b as char); i += 1; }
        else if is_lead(b) {
            match bytes.get(i + 1).copied().filter(|&t| is_trail(t)) {
                Some(t) => {
                    if let Some(text) = pair(b, t) { out.push_str(&text); }
                    i += 2;
                }
                None => i += 1,
            }
        } else { latin(b, &mut out); i += 1; }
    }
    out
}

fn decode(bytes: &[u8], encoding: Encoding) -> Result<(String, &'static str), String> {
    match encoding {
        Encoding::Unicode => decode_unicode(bytes),
        Encoding::Ascii => Ok((bytes.iter().fold(String::with_capacity(bytes.len()), |mut s, &b| { if b < 0x80 { s.push(b as char) } else { latin(b, &mut s) } s }), "ASCII")),
        Encoding::ShiftJis => Ok((decode_shift_jis(bytes), "Shift_JIS")),
    }
}

/// Read one physical line. Quoting protects delimiters, but never a record separator.
fn row(line: &str, delimiter: char, preserve: bool) -> Vec<String> {
    let mut values = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    let fin = |v: &str| if preserve { v.to_owned() } else { v.trim().to_owned() };
    while let Some(c) = chars.next() {
        if c == '"' && quoted {
            if chars.peek() == Some(&'"') { value.push('"'); chars.next(); }
            else { quoted = false; }
        } else if c == '"' && value.trim().is_empty() {
            quoted = true;
        } else if c == delimiter && !quoted {
            values.push(fin(&value)); value.clear();
        } else { value.push(c); }
    }
    values.push(fin(&value));
    values
}

fn fields(headers: Vec<String>) -> Result<Vec<Field>, String> {
    let mut used = BTreeSet::new();
    headers.into_iter().map(|header| {
        let (kind, base) = if let Some(name) = header.strip_prefix('@') { ("image", name) }
            else if let Some(name) = header.strip_prefix('#') { ("qrcode", name) }
            else { ("text", header.as_str()) };
        if base.is_empty() { return Err("Data source contains an empty field name".into()); }
        let mut name = base.to_owned();
        let mut suffix = 1;
        while used.contains(&(kind, name.clone())) { name = format!("{base}{suffix}"); suffix += 1; }
        used.insert((kind, name.clone()));
        Ok(Field { name, kind: kind.into() })
    }).collect()
}

impl Source {
    /// `DataSourceFileType` InDesign writes in the `DataMerge` preference: `CommaSeparated`, `TabDelimited` or `SemicolonSeparated` for
    /// an ASCII file; none for a UTF-16 or Shift_JIS one (measured, probes `datamerge-c4-a`, `-m`).
    pub fn file_type(&self) -> Option<&'static str> {
        if self.options.encoding != Encoding::Ascii { return None; }
        Some(match self.options.delimiter { Delimiter::Comma => "CommaSeparated", Delimiter::Tab => "TabDelimited", Delimiter::Semicolon => "SemicolonSeparated" })
    }
}

/// The delimiter the dialog shows beforehand: a comma in the header, else a tab, else a semicolon, else a comma.
fn detect_delimiter(header: &str) -> Delimiter {
    if header.contains(',') { Delimiter::Comma } else if header.contains('\t') { Delimiter::Tab } else if header.contains(';') { Delimiter::Semicolon } else { Delimiter::Comma }
}

/// Without import options: the delimiter from the header, a UTF-16 file by its BOM, a UTF-8 BOM file or other non-ASCII bytes refused.
pub fn parse(bytes: &[u8]) -> Result<Source, String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err("UTF-8 BOM data sources are not accepted; use UTF-16 with BOM".into());
    }
    let unicode = bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]);
    if !unicode && !bytes.is_ascii() {
        return Err("Non-ASCII sources without a UTF-16 BOM need an encoding: choose ASCII, Unicode or Shift_JIS in the import options, or use UTF-16".into());
    }
    parse_with(bytes, &Choice { encoding: Some(if unicode { Encoding::Unicode } else { Encoding::Ascii }), ..Choice::default() })
}

/// With the choices of the Data Source Import Options dialog; what is not chosen is what the dialog fills in for the file.
pub fn parse_with(bytes: &[u8], choice: &Choice) -> Result<Source, String> {
    if bytes.len() > MAX_BYTES { return Err("Data source exceeds 16 MiB".into()); }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        // measured: InDesign refuses it before the dialog comes up ("has no records or is not a supported file format")
        return Err("UTF-8 BOM data sources are not accepted; use UTF-16 with BOM".into());
    }
    let encoding = choice.encoding.unwrap_or_else(|| detect_encoding(bytes));
    let (text, name) = decode(bytes, encoding)?;
    if text.contains('\0') { return Err("NUL is not supported in a data source".into()); }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = text.split('\n');
    let header = lines.next().ok_or("Data source has no header")?;
    let delimiter = choice.delimiter.unwrap_or_else(|| detect_delimiter(header));
    let options = Options { delimiter, encoding, platform: choice.platform, preserve_spaces: choice.preserve_spaces };
    if header.is_empty() {
        return Ok(Source { encoding: name.into(), delimiter: delimiter.char().to_string(),
            fields: vec![Field { name: "record".into(), kind: "text".into() }], records: vec![], options });
    }
    // a header is always read without its spaces
    let fields = fields(row(header, delimiter.char(), false))?;
    let mut records = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        if records.len() >= 100_000 { return Err("Data source exceeds 100000 records".into()); }
        let mut values = row(line, delimiter.char(), choice.preserve_spaces);
        values.resize(fields.len(), String::new()); values.truncate(fields.len());
        records.push(values);
    }
    if records.is_empty() { return Err("Data source has no records".into()); }
    Ok(Source { encoding: name.into(), delimiter: delimiter.char().to_string(), fields, records, options })
}
