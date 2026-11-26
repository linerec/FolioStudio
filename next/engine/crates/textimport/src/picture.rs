//! A picture a Word file holds (`w:drawing` with a `pic:pic`). InDesign 2025 embeds it where it stands in the text, as an inline
//! frame the size Word gave it, with the file's bytes inside the document (oracle `docxtext-c3-pic*`; measured in
//! `artifacts/docxtext-c3-20261023/measurement/g1.json`): the frame is `wp:extent` in points, the picture is stretched to it (a crop is
//! ignored), a floating picture (`wp:anchor`) comes in inline too, and the description (`wp:docPr/@descr`) becomes the alternate text.
use serde::{Deserialize, Serialize};

/// A picture file: its bytes and the size and text Word gave it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    /// The picture file's bytes in base64, 76 characters to the line (the way a document holds embedded `Contents`).
    pub data: String,
    /// The file format the bytes are in: "PNG", "JPEG", "GIF" or "TIFF".
    pub format: String,
    /// The size Word gave the picture (`wp:extent`), in points.
    pub width: f64,
    pub height: f64,
    /// The alternate text (`wp:docPr/@descr`), none when it is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

/// The format of a picture file by its first bytes; none for a format InDesign's Place of a Word file is not measured for here (EMF, WMF, BMP, SVG ...).
pub(crate) fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("PNG")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("JPEG")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("GIF")
    } else if bytes.starts_with(&[b'I', b'I', 42, 0]) || bytes.starts_with(&[b'M', b'M', 0, 42]) {
        Some("TIFF")
    } else {
        None
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 in 76-column lines, as InDesign writes `Contents`.
pub(crate) fn base64_lines(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4 * 77 / 76 + 4);
    let mut col = 0;
    for g in data.chunks(3) {
        if col == 76 {
            out.push('\n');
            col = 0;
        }
        let b = [g[0], g.get(1).copied().unwrap_or(0), g.get(2).copied().unwrap_or(0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= g.len() { ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
        col += 4;
    }
    out
}

/// The bytes of base64 text (white space between the characters is left out); none for text that is not base64.
pub fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | ALPHABET.iter().position(|a| *a == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_in_76_columns() {
        for n in [0usize, 1, 2, 3, 4, 57, 58, 151, 300] {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 3) as u8).collect();
            let text = base64_lines(&data);
            assert!(text.lines().all(|l| l.len() <= 76), "{n}");
            assert_eq!(decode_base64(&text).unwrap(), data, "{n}");
        }
        assert_eq!(base64_lines(b"Man"), "TWFu");
        assert_eq!(base64_lines(b"Ma"), "TWE=");
        assert!(decode_base64("a*b").is_none());
    }

    #[test]
    fn formats_are_told_by_their_first_bytes() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some("PNG"));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("JPEG"));
        assert_eq!(sniff(b"GIF89a.."), Some("GIF"));
        assert_eq!(sniff(b"BM\0\0"), None);
    }
}
