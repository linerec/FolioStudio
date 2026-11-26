//! Importing an INDD or IDML into a project: [`plan`] decides, [`run`] carries it out.
//!
//! * Links found inside the project stay where they are (root-relative); links found
//!   outside are copied to `assets/<document>/` (names fixed for every OS, collisions get
//!   " (2)", files with the same content are stored once, also across documents).
//! * Missing and ambiguous links keep their original URI as a hint only; their previews
//!   (and every other preview the screen draws) become project files under
//!   `assets/<document>/previews/`, named by content hash.
//! * Embedded and pasted files the INDD reader extracted (`--embedded`) become project files
//!   under `assets/<document>/embedded/`.
//! * Font files the package rules allow (fsType, Adobe Fonts, non-Adobe CJK) go to
//!   `fonts/`; the others are recorded by name, version and hash only.
//! * The `.folio` is written last and atomically; until then everything is staged in
//!   `.folio/tmp/`. A failed or cancelled import leaves nothing in the project. The source
//!   document is only read.

mod files;
mod messages;
mod names;
mod planning;
mod references;
mod report;
mod run;

pub use files::sniff_ext;
pub use names::free_document_name;
pub use planning::plan;
pub use report::{Count, FontRow, LinkCounts, Outcome, Progress, Renamed};
pub use run::run;

use crate::format::{FontFacts, RefKind, Reference};
use crate::paths::RelPath;
use folio_links::{LinkReport, LinkStatus};
use folio_model::Document;
use folio_text::FontStore;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "import/tests.rs"]
mod message_tests;

/// What the engine gathered about the source before planning.
pub struct Prepared<'a> {
    pub source: &'a Path,
    /// `indd` or `idml`.
    pub kind: &'a str,
    pub source_sha256: &'a str,
    pub source_bytes: u64,
    /// The package holding the IDML parts (the reader's `document.idml`, or the source IDML).
    pub idml: &'a Path,
    /// The INDD reader's output folder and `result.json` (INDD only).
    pub reader_dir: Option<&'a Path>,
    pub reader_result: Option<&'a Value>,
    pub doc: &'a Document,
    pub fonts: &'a FontStore,
    /// Links resolved from the source's folder and the search folders.
    pub links: &'a LinkReport,
    /// Resolves any other `file:` URI the same way (XMP history, data merge source).
    pub resolve_uri: &'a (dyn Fn(&str) -> LinkStatus + Sync),
}

/// Where an item's file comes from.
#[derive(Clone, Debug)]
enum Src {
    /// Copied from outside the project.
    Copy(PathBuf),
    /// Already in the project.
    Keep(RelPath),
    /// Reader output (moved when on the same volume, else copied).
    Move(PathBuf),
    /// No file (missing link, font that may not be copied: recorded only).
    None,
}

#[derive(Clone, Debug)]
struct Item {
    kind: RefKind,
    src: Src,
    /// Planned destination (folder for hash-named items).
    dest: Option<RelPath>,
    /// The destination name is derived from the content hash (previews).
    hash_named: bool,
    bytes: u64,
    original_name: Option<String>,
    status: Option<&'static str>,
    hint: Option<String>,
    font: Option<FontFacts>,
    /// Font files that are only recorded are hashed where they are.
    hash_only: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct GraphicPlan {
    self_id: String,
    link_self: Option<String>,
    link: Option<usize>,
    preview: Option<usize>,
    embedded: Option<usize>,
}

/// Everything an import will do, and why.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub source: Value,
    pub document: RelPath,
    pub document_exists: bool,
    pub suggestion: Option<RelPath>,
    pub assets: RelPath,
    pub links: LinkCounts,
    pub previews: Count,
    pub embedded: Value,
    pub fonts: Vec<FontRow>,
    pub font_counts: Value,
    pub other_files: Vec<Value>,
    pub renamed: Vec<Renamed>,
    pub long_paths: Vec<String>,
    /// Files and bytes copied or moved into the project (before content de-duplication).
    pub copy: Count,
    pub warnings: Vec<String>,
    pub warning_messages: Vec<Value>,
    /// Differences from InDesign this document uses (principle 2, item 3): the IDML it holds, checked against `limits.generated.json`.
    pub differences: Vec<crate::limits::Hit>,
    pub ms: f64,
    #[serde(skip)]
    items: Vec<Item>,
    #[serde(skip)]
    graphics: Vec<GraphicPlan>,
    /// Other `file:` URIs of the parts → item.
    #[serde(skip)]
    uris: BTreeMap<String, usize>,
}

/// Calls `f(name, text)` for every XML part of an IDML package (or `.folio`).
pub fn for_each_part(package: &Path, f: &mut dyn FnMut(&str, &str)) -> Result<(), String> {
    let file = std::fs::File::open(package).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        let name = e.name().map_err(|e| e.to_string())?.into_owned();
        if !name.ends_with(".xml") || name.starts_with("folio/") {
            continue;
        }
        let mut text = String::new();
        if e.read_to_string(&mut text).is_ok() {
            f(&name, &text);
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}

impl Error {
    fn new(code: &'static str, message: impl Into<String>) -> Error {
        Error { code, message: message.into() }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn io(e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::Interrupted {
        Error::new("cancelled", "가져오기를 취소했습니다")
    } else {
        Error::new("io", e.to_string())
    }
}

/// The `folio.ref` label of a graphic: its Folio id and its references (with path and hash).
pub fn label_value(id: &str, refs: &[(&str, Option<&Reference>)]) -> String {
    let mut v = serde_json::Map::new();
    v.insert("id".into(), json!(id));
    for (k, r) in refs {
        if let Some(r) = r {
            v.insert((*k).into(), json!({"ref": r.id, "path": r.path, "sha256": r.sha256}));
        }
    }
    Value::Object(v).to_string()
}
