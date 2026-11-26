//! File ▸ Package: one folder a print house can work from, like InDesign's Package.
//!
//! ```text
//! <name>/
//!   <document>.indd       byte copy of the source (the source is only read)
//!   Links/                every found link original; same names, collisions get " (2)"
//!   Document fonts/       the font files the text uses, by InDesign's copy rules
//!   <document>.pdf        PDF/X-4, PDF/X-1a or PDF/X-3 of the export settings (optional; made by the caller's PDF writer)
//!   Instructions.txt      report for the print-house operator (Korean)
//!   package-report.json   the same facts, machine-readable
//! ```
//!
//! [`plan`] owns every decision — which files, under which names, which fonts and why not —
//! and [`run`] carries it out: it copies into a temporary sibling folder (bounded parallel
//! copies, largest first; the PDF is written at the same time), writes both reports from one
//! set of facts, and renames the folder into place without ever replacing an existing one.
//! A failed or cancelled package leaves nothing under the chosen name and removes its
//! temporary folder.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod fonts;
pub mod fsops;
pub mod names;
pub mod protect;
mod report;
pub mod time;

use folio_links::{LinkReport, LinkStatus};
use folio_model::Document;
use folio_model::layout::DocumentLayout;
use folio_text::{FontSource, FontStore};
use names::NameTable;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub const LINKS_DIR: &str = "Links";
pub const FONTS_DIR: &str = "Document fonts";
pub const INSTRUCTIONS: &str = "Instructions.txt";
pub const REPORT_JSON: &str = "package-report.json";

/// The plates of the document (from the separations' ink list).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InkFacts {
    pub plates: Vec<Plate>,
    /// Spot inks the Ink Manager converts to process.
    pub converted: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plate {
    pub name: String,
    pub spot: bool,
    pub ink_type: String,
    /// Document page indexes that use the plate.
    pub pages: Vec<usize>,
    /// Spot swatches printing on this plate through an alias.
    pub aliases: Vec<String>,
}

/// A colour profile of the document and whether this computer has it.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: Option<String>,
    pub file: Option<PathBuf>,
    /// used | display | notNamed | missing | unusable: …
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProfileFacts {
    pub cmyk: Option<Profile>,
    pub rgb: Option<Profile>,
}

/// What the package is made from. Facts are read, never recomputed here: links from the
/// [`LinkReport`], effective resolution from the preflight report, pages from `locations`.
pub struct Inputs<'a> {
    pub doc: &'a Document,
    pub layout: &'a DocumentLayout,
    pub fonts: &'a FontStore,
    pub links: &'a LinkReport,
    pub preflight: &'a folio_preflight::Report,
    /// Item id → page and bounds (`folio_preflight::locate`).
    pub locations: &'a HashMap<String, folio_preflight::Location>,
    /// The document file that was opened (INDD / IDML).
    pub source: &'a Path,
    pub inks: InkFacts,
    pub profiles: ProfileFacts,
}

/// How files are copied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyMethod {
    /// The OS copy call (`CopyFileExW`). The document copy is hashed afterwards.
    #[default]
    Os,
    /// Large-buffer reads and writes, every file hashed on the way (SHA-256 manifest).
    Stream,
}

/// Printing instructions for the report (InDesign's "Printing Instructions").
#[derive(Clone, Debug, Default, Serialize)]
pub struct Instructions {
    pub contact: String,
    pub company: String,
    pub phone: String,
    pub email: String,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub links: bool,
    pub fonts: bool,
    pub pdf: bool,
    /// Also copy non-Adobe CJK fonts (InDesign never does; licences often forbid it).
    pub cjk_fonts: bool,
    /// Parallel copies.
    pub jobs: usize,
    pub copy: CopyMethod,
    /// Roots whose folders may not receive a package (see [`protect`]).
    pub protected_roots: Vec<PathBuf>,
    pub instructions: Instructions,
    pub app_version: String,
    pub engine_version: String,
    /// The window's UTC offset, for the report's date.
    pub utc_offset_minutes: Option<i32>,
    /// PDF settings as chosen (for the report).
    pub pdf_settings: Option<Value>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            links: true,
            fonts: true,
            pdf: false,
            cjk_fonts: false,
            jobs: 4,
            copy: CopyMethod::Os,
            protected_roots: protect::default_protected_roots(),
            instructions: Instructions::default(),
            app_version: String::new(),
            engine_version: String::new(),
            utc_offset_minutes: None,
            pdf_settings: None,
        }
    }
}

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

/// Effective resolution below which images are listed (preflight's warning threshold).
pub const LOW_PPI: f64 = 300.0;

/// A package that could not be made. `code`: exists, protected, noParent, space, cancelled,
/// source, io.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
    pub message_id: Option<&'static str>,
    pub message_args: Value,
    /// `exists`: a free folder name next to the requested one.
    pub suggestion: Option<String>,
    /// A temporary folder that could not be removed.
    pub leftover: Option<PathBuf>,
}

impl Error {
    fn new(code: &'static str, message: impl Into<String>) -> Error {
        Error { code, message: message.into(), message_id: None, message_args: serde_json::json!({}), suggestion: None, leftover: None }
    }

    fn localized(mut self, id: &'static str, args: Value) -> Self {
        self.message_id = Some(id);
        self.message_args = args;
        self
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Document page index → page name.
pub fn page_names(doc: &Document) -> HashMap<usize, String> {
    doc.spreads.iter().flat_map(|s| s.pages.iter()).map(|p| (p.index, p.name.clone())).collect()
}

fn has_conjoining_jamo(s: &str) -> bool {
    s.chars().any(|c| ('\u{1100}'..='\u{11FF}').contains(&c))
}

/// Decides what goes into the package. `compare_contents`: when two different files have
/// the same name and size, read both and package identical ones once (else the second is
/// renamed); the window's preview skips the reading.
pub fn plan(inp: &Inputs, opts: &Options, compare_contents: bool) -> Result<Plan, Error> {
    let started = Instant::now();
    let meta = std::fs::metadata(inp.source).map_err(|e| Error::new("source", format!("원본 문서를 읽을 수 없습니다: {}: {e}", inp.source.display())))?;
    let doc_name = file_name(inp.source);
    let mut root = NameTable::new();
    for n in [LINKS_DIR, FONTS_DIR, INSTRUCTIONS, REPORT_JSON] {
        root.reserve(n);
    }
    let doc_dest = root.unique(&doc_name);
    let pdf_name = root.unique(&format!("{}.pdf", names::split_ext(&doc_name).0));
    let document = FileCopy { source: inp.source.to_path_buf(), dest: doc_dest, bytes: meta.len() };
    let (links, link_rows, nested, placed_fonts) = plan_links(inp, opts, compare_contents);
    let (fonts, font_rows) = plan_fonts(inp, opts);
    let bytes = document.bytes + links.iter().map(|l| l.copy.bytes).sum::<u64>() + fonts.iter().map(|f| f.copy.bytes).sum::<u64>();
    Ok(Plan { document, pdf_name, links, link_rows, fonts, font_rows, placed_fonts, nested, bytes, ms: started.elapsed().as_secs_f64() * 1000.0 })
}

type LinkPlan = (Vec<LinkFile>, Vec<LinkRow>, Vec<String>, Vec<PlacedFont>);

fn plan_links(inp: &Inputs, opts: &Options, compare_contents: bool) -> LinkPlan {
    let pages = page_names(inp.doc);
    let frames = folio_links::owner_frames(inp.doc);
    let mut files: Vec<LinkFile> = Vec::new();
    let mut by_path: HashMap<PathBuf, usize> = HashMap::new();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut table = NameTable::new();
    let mut rows = Vec::new();
    let mut nested: Vec<String> = Vec::new();
    let mut placed_fonts: Vec<PlacedFont> = Vec::new();
    for e in &inp.links.links {
        let name = folio_links::parse_uri(&e.recorded.uri).and_then(|l| l.file_name().map(str::to_owned)).unwrap_or_else(|| e.owner.clone());
        let fact = inp.preflight.images.get(&e.owner);
        let frame = fact.map(|f| f.frame.clone()).or_else(|| frames.get(&e.owner).cloned());
        let loc = frame.as_ref().and_then(|f| inp.locations.get(f));
        let info = e.info.as_ref().and_then(|r| r.as_ref().ok());
        let found = match &e.status {
            Some(LinkStatus::Found { path, .. }) => Some(path.as_path()),
            _ => None,
        };
        let mut row = LinkRow {
            owner: e.owner.clone(),
            element: e.element.clone(),
            name: name.clone(),
            status: e.state_word(),
            how: e.how_word(),
            source: found.map(Path::to_path_buf),
            packaged: None,
            file: None,
            page: loc.map(|l| l.page),
            page_name: loc.and_then(|l| pages.get(&l.page).cloned()),
            ppi: fact.map(|f| f.ppi[0].min(f.ppi[1])),
            space: info.map(|i| format!("{:?}", i.color_space)),
            format: info.map(|i| format!("{:?}", i.format)),
            recorded: folio_links::uri::percent_decode(&e.recorded.uri),
            candidates: match &e.status {
                Some(LinkStatus::Ambiguous(c)) => c.clone(),
                _ => Vec::new(),
            },
            nfd_name: found.is_some_and(|p| has_conjoining_jamo(&file_name(p))),
        };
        if let Some(i) = info {
            for f in i.fonts.iter().filter(|f| !f.embedded) {
                if !placed_fonts.iter().any(|p| p.link == name && p.font == f.name) {
                    placed_fonts.push(PlacedFont { link: name.clone(), font: f.name.clone(), kind: f.kind.clone() });
                }
            }
        }
        if let Some(path) = found {
            let ext = path.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "indd" | "indt" | "idml" | "ai") && !nested.contains(&name) {
                nested.push(name.clone());
            }
        }
        if let (true, Some(path)) = (opts.links, found) {
            let key = protect::normalize(path);
            let idx = match by_path.get(&key) {
                Some(&i) => i,
                None => {
                    let disk_name = file_name(path);
                    let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
                    let nk = folio_links::uri::name_key(&disk_name);
                    let same = if compare_contents {
                        by_name.get(&nk).and_then(|c| {
                            c.iter().copied().find(|&j| files[j].copy.bytes == bytes && fsops::same_content(&files[j].copy.source, path).unwrap_or(false))
                        })
                    } else {
                        None
                    };
                    let i = match same {
                        Some(j) => {
                            files[j].same_as.push(path.to_path_buf());
                            j
                        }
                        None => {
                            let packaged = table.unique(&disk_name);
                            files.push(LinkFile {
                                copy: FileCopy { source: path.to_path_buf(), dest: format!("{LINKS_DIR}/{packaged}"), bytes },
                                renamed: packaged != disk_name,
                                original_name: disk_name,
                                same_as: Vec::new(),
                                owners: Vec::new(),
                            });
                            by_name.entry(nk).or_default().push(files.len() - 1);
                            files.len() - 1
                        }
                    };
                    by_path.insert(key, i);
                    i
                }
            };
            files[idx].owners.push(e.owner.clone());
            row.file = Some(idx);
            row.packaged = Some(files[idx].copy.dest.clone());
        }
        rows.push(row);
    }
    (files, rows, nested, placed_fonts)
}

fn norm_family(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn plan_fonts(inp: &Inputs, opts: &Options) -> (Vec<FontFile>, Vec<FontRow>) {
    let store = inp.fonts;
    // the preflight's own reading of missing vs substituted (one owner of that rule)
    let facts = folio_preflight::font_facts(store, inp.doc, inp.layout);
    struct Group {
        path: PathBuf,
        source: FontSource,
        faces: Vec<(u32, bool)>,
        names: Vec<String>,
        rows: Vec<usize>,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_file: HashMap<PathBuf, usize> = HashMap::new();
    let mut rows: Vec<FontRow> = Vec::new();
    for (family, style) in &inp.doc.fonts_used {
        let mut row = FontRow {
            family: family.clone(),
            style: style.clone(),
            status: "copied",
            code: None,
            reason: None,
            face: None,
            postscript: None,
            file: None,
            source: None,
            origin: None,
            substitute: None,
        };
        let Some((face, cjk_glyphs)) = fonts::exact_face(store, family, style, &inp.doc.font_table) else {
            let listed = |l: &[(String, String, String)]| l.iter().find(|(f, s, _)| f == family && s == style).map(|t| t.2.clone());
            let (status, sub) = if let Some(s) = listed(&facts.substituted) {
                ("substituted", s)
            } else if let Some(s) = listed(&facts.missing) {
                ("missing", s)
            } else {
                // not set by the layout (e.g. only in empty runs): classify the same way
                let r = store.resolve(family, style, &inp.doc.font_table);
                let sub = store.face_info(r.font).map(|i| format!("{} {}", i.family, i.style)).unwrap_or_else(|| "(대체 글꼴 없음)".into());
                let installed = store.faces().iter().any(|i| norm_family(&i.family) == norm_family(family));
                (if installed { "substituted" } else { "missing" }, sub)
            };
            row.status = status;
            row.substitute = Some(sub);
            rows.push(row);
            continue;
        };
        row.face = Some(format!("{} {}", face.family, face.style));
        row.postscript = Some(face.postscript.clone());
        row.source = Some((*face.path).clone());
        row.origin = Some(fonts::origin_word(face.source));
        row.file = Some(file_name(&face.path));
        if !opts.fonts {
            row.status = "notSelected";
            rows.push(row);
            continue;
        }
        let cjk_script = cjk_glyphs
            || inp.doc.font_table.iter().any(|f| &f.family == family && &f.style == style && f.writing_script.is_some_and(fonts::is_cjk_script));
        let path = (*face.path).clone();
        let g = *by_file.entry(path.clone()).or_insert_with(|| {
            groups.push(Group { path, source: face.source, faces: Vec::new(), names: Vec::new(), rows: Vec::new() });
            groups.len() - 1
        });
        let group = &mut groups[g];
        if let Some(f) = group.faces.iter_mut().find(|f| f.0 == face.index) {
            f.1 |= cjk_script;
        } else {
            group.faces.push((face.index, cjk_script));
        }
        let label = format!("{} {}", face.family, face.style);
        if !group.names.contains(&label) {
            group.names.push(label);
        }
        group.rows.push(rows.len());
        rows.push(row);
    }
    let mut table = NameTable::new();
    let mut files = Vec::new();
    for g in groups {
        match fonts::decide(&g.path, g.source, &g.faces, opts.cjk_fonts) {
            Ok(_) => {
                let name = table.unique(&file_name(&g.path));
                let dest = format!("{FONTS_DIR}/{name}");
                let bytes = std::fs::metadata(&g.path).map_or(0, |m| m.len());
                for &r in &g.rows {
                    rows[r].file = Some(dest.clone());
                }
                files.push(FontFile { copy: FileCopy { source: g.path, dest, bytes }, faces: g.names, origin: fonts::origin_word(g.source) });
            }
            Err(skip) => {
                for &r in &g.rows {
                    rows[r].status = "notCopied";
                    rows[r].code = Some(skip.code);
                    rows[r].reason = Some(skip.reason.clone());
                }
            }
        }
    }
    (files, rows)
}

/// A PDF made into the package by the caller.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOutcome {
    pub bytes: u64,
    pub pages: usize,
    pub ms: f64,
    pub standard: String,
    /// InDesign's preset of the same settings, e.g. "[PDF/X-1a:2001]".
    pub preset: String,
    pub output_intent: String,
    pub fonts_embedded: usize,
    /// "Family Style → substitute" of fonts drawn with a substitute.
    pub fonts_substituted: Vec<String>,
    pub warnings: Vec<PdfWarning>,
    /// Transparency flattening (PDF/X-1a, PDF/X-3): pages flattened and resolutions.
    pub flatten: Option<PdfFlatten>,
}

/// What the flattener did to the package's PDF.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PdfFlatten {
    pub pages: usize,
    pub line_art_ppi: f64,
    pub smooth_ppi: f64,
    /// Placed PDFs flattened from their low-resolution preview.
    pub preview_graphics: usize,
}

/// A warning of the PDF writer: `kind` font, link, image, colour, effect, text, document
/// (what reading the document could not model).
#[derive(Clone, Debug, Default, Serialize)]
pub struct PdfWarning {
    pub kind: String,
    pub message: String,
}

/// Writes the PDF to the given path (inside the temporary folder).
pub type PdfMaker<'a> = &'a (dyn Fn(&Path) -> Result<PdfOutcome, String> + Sync);

/// Progress of [`run`]: bytes and files copied, and the PDF's state.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// copy | pdf | report | finish
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
    pub files: usize,
    pub files_total: usize,
    /// running | done | failed; absent without a PDF.
    pub pdf: Option<&'static str>,
}

/// A copy that failed because its source could not be read (the package is still made and
/// the report lists it).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub dest: String,
    pub source: PathBuf,
    pub error: String,
}

/// One file written into the package.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    pub path: String,
    pub bytes: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    pub plan_ms: f64,
    pub copy_ms: f64,
    pub pdf_ms: f64,
    pub report_ms: f64,
    pub finish_ms: f64,
    pub total_ms: f64,
    /// Copy throughput, MB/s.
    pub mb_per_s: f64,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub dir: PathBuf,
    pub files: Vec<Written>,
    pub bytes: u64,
    pub failures: Vec<Failure>,
    pub pdf: Option<Result<PdfOutcome, String>>,
    pub document_sha256: Option<String>,
    pub timings: Timings,
    /// The JSON report as written.
    pub report: Value,
}

/// A free folder name next to `dest`: "<name> 2", "<name> 3", …
pub fn suggest_name(dest: &Path) -> Option<String> {
    let parent = dest.parent()?;
    let name = file_name(dest);
    (2..1000).map(|n| format!("{name} {n}")).find(|c| std::fs::symlink_metadata(parent.join(c)).is_err())
}

/// Checks a destination before anything is written: it must not exist, its folder must,
/// and it must not be inside a guarded folder ([`protect::check`]).
pub fn check_destination(dest: &Path, inp: &Inputs, opts: &Options) -> Result<(), Error> {
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| Error::new("noParent", "패키지 폴더 위치가 올바르지 않습니다").localized("package.destination.invalid", serde_json::json!({})))?;
    if dest.file_name().is_none_or(|n| n.to_string_lossy().trim().is_empty()) {
        return Err(Error::new("noParent", "패키지 폴더 이름이 없습니다").localized("package.destination.unnamed", serde_json::json!({})));
    }
    if !parent.is_dir() {
        return Err(Error::new("noParent", format!("저장할 폴더가 없습니다: {}", parent.display())).localized("package.destination.missingParent", serde_json::json!({"path": parent.display().to_string()})));
    }
    if std::fs::symlink_metadata(dest).is_ok() {
        let mut e = Error::new("exists", format!("같은 이름의 폴더(또는 파일)가 이미 있습니다: {}. 다른 이름을 입력하세요.", dest.display())).localized("package.destination.exists", serde_json::json!({"path": dest.display().to_string()}));
        e.suggestion = suggest_name(dest);
        return Err(e);
    }
    match protect::check(dest, inp.source, &inp.links.search.roots, &opts.protected_roots) {
        Ok(_) => Ok(()),
        Err(protect::Refusal::SourceDocument(p)) => Err(Error::new("source", format!("원본 문서 파일({})에는 쓸 수 없습니다. 다른 위치를 고르세요.", p.display())).localized("package.destination.source", serde_json::json!({"path": p.display().to_string()}))),
        Err(protect::Refusal::Protected { root }) => {
            Err(Error::new("protected", format!("보호된 위치({}) 안에는 패키지를 만들 수 없습니다(설정에서 지정한 보호 폴더). 다른 위치를 고르세요.", root.display())).localized("package.destination.protected", serde_json::json!({"path": root.display().to_string()})))
        }
    }
}

/// Warnings for a destination that is allowed: inside the source document's folder or a
/// link search folder that lies in a Synology Drive sync folder ([`protect::Caution`]).
pub fn destination_cautions(dest: &Path, inp: &Inputs, opts: &Options) -> Vec<String> {
    protect::check(dest, inp.source, &inp.links.search.roots, &opts.protected_roots)
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            format!(
                "원본이 있는 폴더({}) 안입니다. 이 폴더는 Synology Drive 동기화 폴더({}) 안이라 패키지 전체가 NAS로 동기화되고 원본과 사본이 섞일 수 있습니다.",
                c.guarded.display(),
                c.sync_root.display()
            )
        })
        .collect()
}

enum Kind {
    Document,
    Link(usize),
    Font(usize),
}

struct Job<'p> {
    kind: Kind,
    copy: &'p FileCopy,
}

#[derive(Default)]
struct Copied {
    bytes: u64,
    sha256: Option<String>,
    failure: Option<String>,
}

/// Removes the temporary folder if the run panics or returns early.
struct StagingGuard(Option<PathBuf>);

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn source_stamp(p: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    std::fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()))
}

/// Copies one file and checks it: the destination has the source's size, and the source did
/// not change while it was copied (else one retry). `Err` = stop the package (destination
/// trouble or cancel); `Ok` with `failure` = the source could not be read.
fn copy_one(job: &Job, dst: &Path, method: CopyMethod, hash: bool, buf: &mut Vec<u8>, done: &AtomicU64, stop: &AtomicBool) -> Result<Copied, std::io::Error> {
    let src = &job.copy.source;
    for attempt in 0..2 {
        let before = source_stamp(src);
        if let Err(e) = std::fs::File::open(src) {
            return Ok(Copied { failure: Some(format!("원본을 열 수 없음: {e}")), ..Default::default() });
        }
        let r = match method {
            CopyMethod::Os => fsops::copy_os(src, dst, done, stop).map(|n| (n, None)),
            CopyMethod::Stream => {
                if buf.is_empty() {
                    buf.resize(fsops::STREAM_BUFFER, 0);
                }
                fsops::copy_stream(src, dst, buf, done, stop).map(|(n, h)| (n, Some(h)))
            }
        };
        let (n, mut sha) = match r {
            Ok(v) => v,
            Err(e) if fsops::is_cancelled(&e) => return Err(e),
            Err(e) => {
                // the source went away or became unreadable: list it; anything else is ours
                if std::fs::File::open(src).is_err() {
                    let _ = std::fs::remove_file(dst);
                    return Ok(Copied { failure: Some(format!("원본을 읽는 중 오류: {e}")), ..Default::default() });
                }
                return Err(std::io::Error::new(e.kind(), format!("{} 복사 실패: {e}", job.copy.dest)));
            }
        };
        let after = source_stamp(src);
        let written = std::fs::metadata(dst)?.len();
        if before == after && after.is_some_and(|a| a.0 == n) && written == n {
            fsops::make_writable(dst);
            if hash && sha.is_none() {
                sha = Some(fsops::sha256_file(dst, buf_or_alloc(buf), stop)?);
            }
            return Ok(Copied { bytes: n, sha256: sha, failure: None });
        }
        // changed while copying (or a short copy): drop it and try once more
        done.fetch_sub(n, Ordering::Relaxed);
        let _ = std::fs::remove_file(dst);
        if attempt == 1 {
            return Ok(Copied { failure: Some("복사하는 동안 원본이 바뀌었거나 크기가 맞지 않음".into()), ..Default::default() });
        }
    }
    // Both attempts return on success, error, or the final source-change check.
    unreachable!()
}

fn buf_or_alloc(buf: &mut Vec<u8>) -> &mut [u8] {
    if buf.is_empty() {
        buf.resize(fsops::STREAM_BUFFER, 0);
    }
    buf
}

/// Makes the package at `dest` (which must not exist). See the crate documentation.
pub fn run(
    plan: &Plan,
    inp: &Inputs,
    opts: &Options,
    dest: &Path,
    pdf: Option<PdfMaker>,
    progress: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Outcome, Error> {
    let started = Instant::now();
    check_destination(dest, inp, opts)?;
    let parent = dest.parent().unwrap_or(Path::new("."));
    const MARGIN: u64 = 64 << 20;
    if let Some(avail) = fsops::free_space(parent)
        && avail < plan.bytes + MARGIN
    {
        return Err(Error::new(
            "space",
            format!("디스크 공간이 부족합니다: 필요 약 {:.0} MB, 남은 공간 {:.0} MB", (plan.bytes + MARGIN) as f64 / 1e6, avail as f64 / 1e6),
        ));
    }
    let staging = fsops::create_staging(dest).map_err(|e| Error::new("io", format!("임시 폴더를 만들 수 없습니다: {e}")))?;
    let mut guard = StagingGuard(Some(staging.clone()));
    let built = build(plan, inp, opts, dest, &staging, pdf, progress, cancel);
    let result = built.and_then(|mut out| {
        let t = Instant::now();
        progress(&Progress { stage: "finish", done: plan.bytes, total: plan.bytes, files: out.files.len(), files_total: out.files.len(), pdf: None });
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::new("cancelled", "취소했습니다"));
        }
        fsops::rename_no_replace(&staging, dest).map_err(|e| {
            if fsops::is_exists(&e) || std::fs::symlink_metadata(dest).is_ok() {
                let mut err = Error::new("exists", format!("같은 이름의 폴더가 그사이 생겼습니다: {}. 다른 이름을 입력하세요.", dest.display()));
                err.suggestion = suggest_name(dest);
                err
            } else {
                Error::new("io", format!("패키지 폴더 이름을 바꿀 수 없습니다: {e}"))
            }
        })?;
        out.timings.finish_ms = t.elapsed().as_secs_f64() * 1000.0;
        out.timings.total_ms = started.elapsed().as_secs_f64() * 1000.0 + plan.ms;
        Ok(out)
    });
    match result {
        Ok(out) => {
            guard.0 = None;
            Ok(out)
        }
        Err(mut e) => {
            guard.0 = None;
            if std::fs::remove_dir_all(&staging).is_err() && staging.exists() {
                e.leftover = Some(staging.clone());
                e.message = format!("{} (임시 폴더를 지우지 못했습니다: {})", e.message, staging.display());
            }
            Err(e)
        }
    }
}

// This internal phase keeps the public run inputs plus its separate staging path.
#[allow(clippy::too_many_arguments)]
fn build(
    plan: &Plan,
    inp: &Inputs,
    opts: &Options,
    dest: &Path,
    staging: &Path,
    pdf: Option<PdfMaker>,
    progress: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Outcome, Error> {
    let io = |what: &str, e: std::io::Error| Error::new("io", format!("{what}: {e}"));
    if !plan.links.is_empty() {
        std::fs::create_dir(staging.join(LINKS_DIR)).map_err(|e| io("Links 폴더를 만들 수 없습니다", e))?;
    }
    if !plan.fonts.is_empty() {
        std::fs::create_dir(staging.join(FONTS_DIR)).map_err(|e| io("Document fonts 폴더를 만들 수 없습니다", e))?;
    }
    let mut jobs: Vec<Job> = Vec::with_capacity(1 + plan.links.len() + plan.fonts.len());
    jobs.push(Job { kind: Kind::Document, copy: &plan.document });
    jobs.extend(plan.links.iter().enumerate().map(|(i, l)| Job { kind: Kind::Link(i), copy: &l.copy }));
    jobs.extend(plan.fonts.iter().enumerate().map(|(i, f)| Job { kind: Kind::Font(i), copy: &f.copy }));
    // largest first: the parallel copies finish together
    jobs.sort_by_key(|j| std::cmp::Reverse(j.copy.bytes));
    let results: Vec<Mutex<Option<Copied>>> = jobs.iter().map(|_| Mutex::new(None)).collect();
    let fatal: Mutex<Option<String>> = Mutex::new(None);
    let (done, next, finished) = (AtomicU64::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
    let stop = AtomicBool::new(false);
    let workers = opts.jobs.clamp(1, 16).min(jobs.len().max(1));
    let total = plan.bytes;
    let files_total = jobs.len();
    let copy_started = Instant::now();
    let mut copy_ms: Option<f64> = None;
    let pdf_path = staging.join(&plan.pdf_name);
    let mut pdf_result: Option<(Result<PdfOutcome, String>, f64)> = None;

    std::thread::scope(|s| {
        let pdf_handle = pdf.filter(|_| opts.pdf).map(|make| {
            let pdf_path = &pdf_path;
            s.spawn(move || {
                let t = Instant::now();
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| make(pdf_path))).unwrap_or_else(|_| Err("PDF 작성 중 엔진 오류".into()));
                (r, t.elapsed().as_secs_f64() * 1000.0)
            })
        });
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        for _ in 0..workers {
            let tx = tx.clone();
            let (jobs, results, fatal, done, next, finished, stop) = (&jobs, &results, &fatal, &done, &next, &finished, &stop);
            let copy = opts.copy;
            s.spawn(move || {
                fsops::lower_thread_priority();
                let mut buf: Vec<u8> = Vec::new();
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else { break };
                    let dst = staging.join(&job.copy.dest);
                    // the document is always hashed; with stream copies every file is
                    let hash = matches!(job.kind, Kind::Document);
                    match copy_one(job, &dst, copy, hash, &mut buf, done, stop) {
                        Ok(c) => {
                            if c.failure.is_some() && matches!(job.kind, Kind::Document) {
                                *fatal.lock().expect("the fatal-error slot is only assigned while locked") = Some(format!("원본 문서를 복사할 수 없습니다: {}", c.failure.as_deref().unwrap_or("")));
                                stop.store(true, Ordering::Relaxed);
                            }
                            *results[i].lock().expect("each worker only assigns its own result slot") = Some(c);
                        }
                        Err(e) => {
                            if !fsops::is_cancelled(&e) {
                                let mut f = fatal.lock().expect("the fatal-error slot is only assigned while locked");
                                if f.is_none() {
                                    *f = Some(e.to_string());
                                }
                            }
                            stop.store(true, Ordering::Relaxed);
                        }
                    }
                    finished.fetch_add(1, Ordering::Relaxed);
                }
                let _ = tx.send(());
            });
        }
        drop(tx);
        // the window hears about progress about ten times a second
        let mut running = workers;
        let mut pdf_state = pdf_handle.as_ref().map(|_| "running");
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(()) => running -= 1,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => running = 0,
            }
            if cancel.load(Ordering::Relaxed) {
                stop.store(true, Ordering::Relaxed);
            }
            if running == 0 && copy_ms.is_none() {
                copy_ms = Some(copy_started.elapsed().as_secs_f64() * 1000.0);
            }
            if let Some(h) = &pdf_handle
                && h.is_finished()
                && pdf_state == Some("running")
            {
                pdf_state = Some("done");
            }
            let stage = if running > 0 { "copy" } else { "pdf" };
            progress(&Progress { stage, done: done.load(Ordering::Relaxed), total, files: finished.load(Ordering::Relaxed), files_total, pdf: pdf_state });
            if running == 0 && pdf_handle.as_ref().is_none_or(|h| h.is_finished()) {
                break;
            }
        }
        if let Some(h) = pdf_handle {
            pdf_result = Some(h.join().unwrap_or_else(|_| (Err("PDF 작성 중 엔진 오류".into()), 0.0)));
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::new("cancelled", "취소했습니다"));
    }
    if let Some(msg) = fatal.into_inner().expect("all workers finished without panicking while holding the fatal-error slot") {
        return Err(Error::new("io", msg));
    }

    // gather what was written
    let mut written: Vec<Written> = Vec::new();
    let mut failures: Vec<Failure> = Vec::new();
    let mut document_sha256 = None;
    let mut copied_links = vec![false; plan.links.len()];
    let mut copied_fonts = vec![false; plan.fonts.len()];
    for (job, r) in jobs.iter().zip(results) {
        let Some(c) = r.into_inner().expect("all workers finished without panicking while holding a result slot") else {
            return Err(Error::new("io", format!("{} 를 복사하지 못했습니다", job.copy.dest)));
        };
        if let Some(err) = c.failure {
            failures.push(Failure { dest: job.copy.dest.clone(), source: job.copy.source.clone(), error: err });
            continue;
        }
        match job.kind {
            Kind::Document => document_sha256 = c.sha256.clone(),
            Kind::Link(i) => copied_links[i] = true,
            Kind::Font(i) => copied_fonts[i] = true,
        }
        written.push(Written { path: job.copy.dest.clone(), bytes: c.bytes, sha256: c.sha256 });
    }
    written.sort_by(|a, b| a.path.cmp(&b.path));
    let (pdf_outcome, pdf_ms) = match pdf_result {
        Some((r, ms)) => (Some(r), ms),
        None => (None, 0.0),
    };
    if let Some(Ok(p)) = &pdf_outcome {
        written.push(Written { path: plan.pdf_name.clone(), bytes: p.bytes, sha256: None });
    } else if pdf_path.exists() {
        let _ = std::fs::remove_file(&pdf_path);
    }

    // reports, from one set of facts
    progress(&Progress { stage: "report", done: total, total, files: files_total, files_total, pdf: None });
    let t = Instant::now();
    let copied_bytes: u64 = written.iter().map(|w| w.bytes).sum();
    let copy_ms = copy_ms.unwrap_or_default();
    let mut timings = Timings {
        plan_ms: plan.ms,
        copy_ms,
        pdf_ms,
        mb_per_s: if copy_ms > 0.0 { (copied_bytes as f64 / 1e6) / (copy_ms / 1000.0) } else { 0.0 },
        ..Default::default()
    };
    let facts = report::Facts {
        plan,
        inp,
        opts,
        dest,
        written: &written,
        failures: &failures,
        copied_links: &copied_links,
        copied_fonts: &copied_fonts,
        pdf: pdf_outcome.as_ref(),
        document_sha256: document_sha256.as_deref(),
        created: time::now(opts.utc_offset_minutes),
        timings: &timings,
    };
    let json = report::json(&facts);
    let text = report::text(&json);
    let json_bytes = serde_json::to_vec_pretty(&json).map_err(|e| Error::new("io", e.to_string()))?;
    std::fs::write(staging.join(REPORT_JSON), &json_bytes).map_err(|e| io("보고서를 쓸 수 없습니다", e))?;
    let text_bytes = report::text_file_bytes(&text);
    std::fs::write(staging.join(INSTRUCTIONS), &text_bytes).map_err(|e| io("보고서를 쓸 수 없습니다", e))?;
    written.push(Written { path: REPORT_JSON.into(), bytes: json_bytes.len() as u64, sha256: None });
    written.push(Written { path: INSTRUCTIONS.into(), bytes: text_bytes.len() as u64, sha256: None });
    timings.report_ms = t.elapsed().as_secs_f64() * 1000.0;
    let bytes = written.iter().map(|w| w.bytes).sum();
    Ok(Outcome { dir: dest.to_path_buf(), files: written, bytes, failures, pdf: pdf_outcome, document_sha256, timings, report: json })
}

#[cfg(test)]
mod tests;
