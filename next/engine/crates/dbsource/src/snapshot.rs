//! The result of a query as the snapshot CSV the document keeps in the project (`assets/<document>/datamerge/`), and as the
//! data source Folio merges with: the source is always read back from the snapshot bytes, so what Folio merges and what
//! InDesign reads from the exported IDML are the same file.
//!
//! Encoding: UTF-16 little endian with a BOM. InDesign 2025 refuses a UTF-8 file with a BOM and does not guess other
//! non-ASCII bytes (measured, `oracle/rules/datamerge.md`, datamerge-c2/c4); a UTF-16 file with a BOM it reads without asking.
//! Line breaks inside a value cannot be in a data source (a line is a record): they become U+2028, InDesign's forced line break.
use crate::error::{DbError, Kind, Result};

/// A result: column names and rows (`None` is SQL NULL).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl Table {
    /// The limits (design 0): rows and bytes.
    pub fn check_limits(&self) -> Result<()> {
        if self.rows.len() > crate::MAX_ROWS { return Err(DbError::new(Kind::Limit, format!("more than {} rows", crate::MAX_ROWS))); }
        Ok(())
    }
}

fn cell(value: &str) -> String {
    let v: String = value.replace("\r\n", "\u{2028}").replace(['\r', '\n'], "\u{2028}");
    if v.contains([',', '"']) || v.starts_with(char::is_whitespace) || v.ends_with(char::is_whitespace) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else { v }
}

/// The CSV text (comma separated, CRLF lines). A column name that is empty becomes `column N` (InDesign refuses an empty
/// field name); the `@` and `#` prefixes keep their meaning (image and QR code fields).
pub fn csv_text(table: &Table) -> String {
    let mut out = String::new();
    let header: Vec<String> = table.columns.iter().enumerate()
        .map(|(i, c)| if c.trim().is_empty() { format!("column {}", i + 1) } else { c.clone() }).map(|c| cell(&c)).collect();
    out.push_str(&header.join(","));
    out.push_str("\r\n");
    for row in &table.rows {
        let line: Vec<String> = (0..table.columns.len()).map(|i| row.get(i).cloned().flatten().map(|v| cell(&v)).unwrap_or_default()).collect();
        // a record of only empty values would be read as no record: InDesign skips empty lines too
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    }
    out
}

/// The snapshot file: UTF-16 LE with a BOM.
pub fn csv_bytes(table: &Table) -> Vec<u8> {
    let text = csv_text(table);
    let mut out = Vec::with_capacity(2 + text.len() * 2);
    out.extend_from_slice(&[0xff, 0xfe]);
    for unit in text.encode_utf16() { out.extend_from_slice(&unit.to_le_bytes()); }
    out
}

/// The data source of a snapshot, read as InDesign reads it unasked.
pub fn source(bytes: &[u8]) -> Result<folio_idml::datamerge::Source> {
    folio_idml::datamerge::parse(bytes).map_err(|e| DbError::new(Kind::Invalid, e))
}

/// SHA-256 (hex) of the snapshot: the "result hash" the document remembers and a later run compares with.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_through_the_data_source_reader() {
        let table = Table {
            columns: vec!["번호".into(), "제목".into(), "@사진".into(), "#QR".into(), "".into()],
            rows: vec![
                vec![Some("1".into()), Some("가을, \"특집\"".into()), Some("images/a.jpg".into()), Some("https://example.com/1".into()), None],
                vec![Some("2".into()), Some("첫 줄\n둘째 줄".into()), None, Some(" 앞뒤 공백 ".into()), Some("x".into())],
            ],
        };
        let bytes = csv_bytes(&table);
        assert_eq!(&bytes[..2], &[0xff, 0xfe]);
        let s = source(&bytes).unwrap();
        let names: Vec<(&str, &str)> = s.fields.iter().map(|f| (f.name.as_str(), f.kind.as_str())).collect();
        assert_eq!(names, [("번호", "text"), ("제목", "text"), ("사진", "image"), ("QR", "qrcode"), ("column 5", "text")]);
        assert_eq!(s.records[0], ["1", "가을, \"특집\"", "images/a.jpg", "https://example.com/1", ""]);
        // a line break is a forced line break; spaces around a value go (InDesign trims them unless Preserve Spaces)
        assert_eq!(s.records[1], ["2", "첫 줄\u{2028}둘째 줄", "", "앞뒤 공백", "x"]);
        assert_eq!(s.file_type(), None, "a UTF-16 source has no DataSourceFileType");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
