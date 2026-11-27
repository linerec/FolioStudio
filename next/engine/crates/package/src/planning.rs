//! Decide which files a package contains, their names and the facts shown in the preview.

mod fonts;
mod links;

use crate::names::{self, NameTable};
use crate::{Error, FONTS_DIR, INSTRUCTIONS, Inputs, LINKS_DIR, LOW_PPI, Options, REPORT_JSON, file_name};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

/// One file to copy.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCopy {
    pub source: PathBuf,
    /// Path inside the package, '/'-separated.
    pub dest: String,
    pub bytes: u64,
}

/// A linked file copied into Links (one per source file, however often it is placed).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkFile {
    pub copy: FileCopy,
    /// The file's name on disk, as spelled there.
    pub original_name: String,
    /// Packaged under another name because a different file already has this one.
    pub renamed: bool,
    /// Other source files with the same name and the same bytes, packaged as this one.
    pub same_as: Vec<PathBuf>,
    /// Graphic ids that use it.
    pub owners: Vec<String>,
}

/// One placed link, as the report lists it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRow {
    pub owner: String,
    pub element: String,
    /// File name the document recorded.
    pub name: String,
    /// found | modified | missing | ambiguous | embedded
    pub status: &'static str,
    pub how: Option<&'static str>,
    pub source: Option<PathBuf>,
    /// Path in the package ("Links/…"), when copied.
    pub packaged: Option<String>,
    #[serde(skip)]
    pub file: Option<usize>,
    pub page: Option<usize>,
    pub page_name: Option<String>,
    /// Effective resolution on the page (lower axis).
    pub ppi: Option<f64>,
    pub space: Option<String>,
    pub format: Option<String>,
    /// Recorded location (decoded URI).
    pub recorded: String,
    /// Ambiguous links: the equally good candidates.
    pub candidates: Vec<PathBuf>,
    /// The file on disk spells its Korean name decomposed (NFD, as macOS writes it).
    pub nfd_name: bool,
}

/// A font file copied into Document fonts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFile {
    pub copy: FileCopy,
    /// "Family Style" of the used faces in it.
    pub faces: Vec<String>,
    pub origin: &'static str,
}

/// One family + style the text uses.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRow {
    pub family: String,
    pub style: String,
    /// copied | notCopied | notSelected | missing | substituted
    pub status: &'static str,
    pub code: Option<&'static str>,
    pub reason: Option<String>,
    /// The face that was matched ("Family Style").
    pub face: Option<String>,
    pub postscript: Option<String>,
    /// Path in the package when copied, else the source file's name.
    pub file: Option<String>,
    pub source: Option<PathBuf>,
    pub origin: Option<&'static str>,
    /// Missing / substituted: the face drawn instead.
    pub substitute: Option<String>,
}

/// A font a placed PDF / EPS uses without embedding it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedFont {
    pub link: String,
    pub font: String,
    pub kind: String,
}

/// Everything the package will contain and why.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub document: FileCopy,
    pub pdf_name: String,
    pub links: Vec<LinkFile>,
    pub link_rows: Vec<LinkRow>,
    pub fonts: Vec<FontFile>,
    pub font_rows: Vec<FontRow>,
    pub placed_fonts: Vec<PlacedFont>,
    /// Placed InDesign / Illustrator files: links inside them are not collected.
    pub nested: Vec<String>,
    /// Bytes to copy.
    pub bytes: u64,
    pub ms: f64,
}

impl Plan {
    /// Counts for the window (dialog and result).
    pub fn counts(&self) -> Value {
        let rows = |s: &str| self.link_rows.iter().filter(|r| r.status == s).count();
        let fonts = |s: &str| self.font_rows.iter().filter(|r| r.status == s).count();
        serde_json::json!({
            "document": {"name": self.document.dest, "bytes": self.document.bytes},
            "links": {"placed": self.link_rows.len(), "files": self.links.len(), "bytes": self.links.iter().map(|l| l.copy.bytes).sum::<u64>(),
                      "found": rows("found"), "modified": rows("modified"), "missing": rows("missing"), "ambiguous": rows("ambiguous"),
                      "embedded": rows("embedded"), "renamed": self.links.iter().filter(|l| l.renamed).count(),
                      "sameContent": self.links.iter().map(|l| l.same_as.len()).sum::<usize>(),
                      "rgb": self.link_rows.iter().filter(|r| r.space.as_deref() == Some("Rgb")).count(),
                      "lowRes": self.link_rows.iter().filter(|r| r.ppi.is_some_and(|p| p < LOW_PPI)).count(),
                      "nested": self.nested.len()},
            "fonts": {"used": self.font_rows.len(), "files": self.fonts.len(), "bytes": self.fonts.iter().map(|f| f.copy.bytes).sum::<u64>(),
                      "copied": fonts("copied"), "notCopied": fonts("notCopied"), "notSelected": fonts("notSelected"),
                      "missing": fonts("missing"), "substituted": fonts("substituted"), "placedNotEmbedded": self.placed_fonts.len()},
            "pdfName": self.pdf_name,
            "bytes": self.bytes,
            "ms": (self.ms * 10.0).round() / 10.0,
        })
    }
}

/// Decides what goes into the package. `compare_contents`: when two different files have
/// the same name and size, read both and package identical ones once (else the second is
/// renamed); the window's preview skips the reading.
pub fn plan(inp: &Inputs, opts: &Options, compare_contents: bool) -> Result<Plan, Error> {
    let started = Instant::now();
    let source_bytes = inp.source_bytes()?;
    let doc_name = file_name(inp.source);
    let mut root = NameTable::new();
    for n in [LINKS_DIR, FONTS_DIR, INSTRUCTIONS, REPORT_JSON] {
        root.reserve(n);
    }
    let doc_dest = root.unique(&doc_name);
    let pdf_name = root.unique(&format!("{}.pdf", names::split_ext(&doc_name).0));
    let document = FileCopy { source: inp.source.to_path_buf(), dest: doc_dest, bytes: source_bytes };
    let link_plan = links::plan(inp, opts, compare_contents);
    let (fonts, font_rows) = fonts::plan(inp, opts);
    let bytes = document.bytes
        + link_plan.files.iter().map(|l| l.copy.bytes).sum::<u64>()
        + fonts.iter().map(|f| f.copy.bytes).sum::<u64>();
    Ok(Plan {
        document,
        pdf_name,
        links: link_plan.files,
        link_rows: link_plan.rows,
        fonts,
        font_rows,
        placed_fonts: link_plan.placed_fonts,
        nested: link_plan.nested,
        bytes,
        ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}
