//! folio-pdf: print output as vector PDF/X-4 (ISO 15930-7:2010), PDF/X-1a:2001 and
//! PDF/X-3:2002, the counterparts of InDesign's "[PDF/X-4:2008]", "[PDF/X-1a:2001]" and
//! "[PDF/X-3:2002]" presets ([`Preset`]). PDF/X-1a and PDF/X-3 are PDF 1.3: transparency is
//! flattened ([`flatten`]), and PDF/X-1a converts RGB, Lab and ICC-based colour to the
//! document CMYK keeping CMYK numbers.
//!
//! The document is written in its own inks: process CMYK as DeviceCMYK, spot inks as
//! Separation spaces (or converted / aliased as the Ink Manager says), mixed inks as DeviceN,
//! Lab as Lab, RGB tagged with the document RGB profile; overprint as OP/op with OPM 1;
//! transparency stays live (groups with opacity and the 16 blend modes, page groups blending
//! in DeviceCMYK). Text is embedded font subsets placed glyph by glyph as `folio-text` laid
//! it out. Linked originals are used when `folio-links` finds them. The output intent embeds
//! the document CMYK profile read from this machine.
//!
//! Work happens in three phases: page content is generated in document order (resources are
//! numbered deterministically), then fonts are subset, images encoded and streams
//! compressed on worker threads, then objects are written.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod options;
pub mod printing;
pub mod book;
mod title;
pub use options::*;
pub mod preset;
mod standards;
pub use preset::PdfPreset;

mod colors;
mod content;
mod draw;
mod text_lines;
mod effects;
mod encodings;
pub mod flatten;
mod fontsub;
mod fonts;
mod graphics;
mod image_color;
mod images;
mod memory;
mod meta;
mod pdfimport;
mod spotconv;
mod registry;

mod export;
mod output;
mod pages;
mod settings;

pub use export::export;
use pages::plan;
use std::path::Path;

pub use folio_links::LinkReport;

#[derive(Debug)]
pub enum ExportError {
    /// No output intent profile (the document CMYK profile must be installed or given).
    OutputProfile(String),
    NoPages,
    Io(String),
    /// An output this export does not make (e.g. a booklet with transparency flattening).
    Unsupported(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::OutputProfile(m) => write!(f, "output intent profile: {m}"),
            ExportError::NoPages => write!(f, "no pages to export"),
            ExportError::Io(m) => write!(f, "I/O: {m}"),
            ExportError::Unsupported(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ExportError {}

pub struct Exported {
    pub pdf: Vec<u8>,
    pub report: Report,
}

/// Font folders InDesign would also use: "Document fonts" beside the document and its parent.
pub fn document_font_dirs(path: &Path) -> Vec<std::path::PathBuf> {
    path.ancestors().skip(1).take(2).map(|d| d.join("Document fonts")).filter(|d| d.is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::devicen_code;

    /// Evaluates the DeviceN tint transform for one input with a tiny PostScript stack machine.
    fn eval(code: &str, inputs: &[f64]) -> Vec<f64> {
        let mut st: Vec<f64> = inputs.to_vec();
        let toks: Vec<&str> = code.trim_matches(|c| c == '{' || c == '}' || c == ' ').split_whitespace().collect();
        let mut i = 0;
        while i < toks.len() {
            match toks[i] {
                "index" => {
                    let n = st.pop().unwrap() as usize;
                    let v = st[st.len() - 1 - n];
                    st.push(v);
                }
                "mul" => {
                    let (b, a) = (st.pop().unwrap(), st.pop().unwrap());
                    st.push(a * b);
                }
                "add" => {
                    let (b, a) = (st.pop().unwrap(), st.pop().unwrap());
                    st.push(a + b);
                }
                "dup" => st.push(*st.last().unwrap()),
                "gt" => {
                    let (b, a) = (st.pop().unwrap(), st.pop().unwrap());
                    st.push(if a > b { 1.0 } else { 0.0 });
                }
                "{pop" => {
                    // "{pop 1} if": the clamp
                    let cond = st.pop().unwrap();
                    if cond != 0.0 {
                        st.pop();
                        st.push(1.0);
                    }
                    i += 2; // skip "1}" "if"
                }
                "roll" => {
                    let (j, n) = (st.pop().unwrap() as usize, st.pop().unwrap() as usize);
                    let len = st.len();
                    st[len - n..].rotate_right(j % n);
                }
                "pop" => {
                    st.pop();
                }
                t => st.push(t.parse().unwrap()),
            }
            i += 1;
        }
        st
    }

    #[test]
    fn devicen_tint_transform_mixes_inks() {
        // Cyan (process) and a spot whose CMYK alternate is 0 60 100 0.
        let code = devicen_code(&[[1.0, 0.0, 0.0, 0.0], [0.0, 0.6, 1.0, 0.0]]);
        let out = eval(&code, &[0.4, 0.5]);
        assert_eq!(out.len(), 4);
        assert!((out[0] - 0.4).abs() < 1e-9 && (out[1] - 0.3).abs() < 1e-9 && (out[2] - 0.5).abs() < 1e-9 && out[3] == 0.0, "{out:?}");
        // clamped at full ink
        let out = eval(&code, &[1.0, 1.0]);
        assert_eq!(out[2], 1.0);
    }

    #[test]
    fn metadata_helpers() {
        assert_eq!(meta::civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(meta::civil(1_792_351_282), (2026, 10, 18, 19, 21, 22));
        let u = meta::uuid(7);
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_eq!(meta::condition_identifier("U.S. Web Coated (SWOP) v2"), Some("CGATS TR 001"));
        assert_eq!(meta::condition_identifier("Coated FOGRA39 (ISO 12647-2:2004)"), Some("FOGRA39"));
        assert_eq!(meta::condition_identifier("My press"), None);
    }

    #[test]
    fn glyph_names() {
        assert_eq!(encodings::glyph_unicode("A"), Some('A'));
        assert_eq!(encodings::glyph_unicode("quotedblleft"), Some('\u{201C}'));
        assert_eq!(encodings::glyph_unicode("uni00E9"), Some('é'));
        assert_eq!(encodings::glyph_unicode("a.sc"), Some('a'));
        assert_eq!(encodings::WIN_ANSI[0x80], 0x20AC);
        assert_eq!(encodings::STANDARD[0x27], 0x2019);
    }
}
