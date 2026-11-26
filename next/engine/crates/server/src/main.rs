#![cfg_attr(not(test), warn(clippy::unwrap_used))]

//! folio-engine: the Folio Studio engine process.
//!
//! Requests arrive on stdin, one JSON object per line:
//!   {"id":1,"cmd":"open","path":"D:\\…\\issue.indd"}
//!   {"id":2,"cmd":"render","page":0,"scale":1.5}
//!   {"id":3,"cmd":"info"}
//!   {"id":4,"cmd":"links","roots":["D:\images"]}   links panel (search folders for missing files)
//!   {"id":5,"cmd":"preflight"}                     preflight panel
//!   {"id":6,"cmd":"export","path":"…\issue.pdf","bleed":true,"marks":{…},"spreads":false,"pages":null}
//!   {"id":7,"cmd":"inks"}                          plates of the document (separations panel)
//!   {"id":8,"cmd":"render","page":0,"scale":1.5,"view":{"mode":"separations","inks":["Black"]}}
//!                                                  ink view: overprint | separations | inkLimit (+ "limit")
//!   {"id":2,"cmd":"render","page":0,"scale":1.5,"display":{"view":"OPTIMIZED","ignoreLocal":false}}
//!                                                  Display Performance: Fast Display draws graphics as grey boxes (screen only)
//!   {"id":9,"cmd":"inkAt","page":0,"x":100,"y":200} ink per plate at a point (points from the page's top left)
//!   {"id":10,"cmd":"packagePlan", …} / {"id":11,"cmd":"package","dir":"…","pdf":true,…} / {"cmd":"packageCancel"}
//!                                                  File ▸ Package (see package.rs)
//!   {"id":12,"cmd":"documentInfo"}                 units, layers, swatches, masters, pages, styles
//!   {"id":13,"cmd":"pageItems","page":0}           a page's items, guides and columns (see inspect.rs)
//! Replies and events go to stdout as frames: u32 little-endian header length, the header
//! as UTF-8 JSON, then `bodyBytes` raw bytes (RGBA pixels for renders). Pixels are sent as
//! they are, without image encoding. Logs go to stderr.

mod attach;
mod attach_paths;
#[path = "bin/mcp/attached.rs"]
mod attached;
// the project MCP server's path checks (mcp-c1), reused for an attached connection's paths
#[path = "bin/mcp/boundary.rs"]
#[allow(dead_code)]
mod boundary;
mod clip;
mod clippaths;
mod conveyor;
mod edit;
mod mathml;
mod place;
mod pdfplace;
mod epsplace;
mod svgplace;
mod glyphfill;
mod rasterplace;
mod psdlayers;
mod textvars;
mod textpath_handles;
mod source_queries;
mod pages;
mod inspect;
mod package;
mod print;
mod pdf_presets;
mod project;
mod sep;
mod text_edit;
mod registered_queries;
mod requests;
mod view_state;
mod wire;
mod dialog_state;
mod dock_action;
mod document_presets;
mod user_settings;
mod renderer_preferences;
mod urlcheck;
mod recent_files;
mod panel_state;
mod workspace_layout;
mod drawing_settings;
mod flatpreview;
mod proof_setup;
mod preflight_files;
mod stroke_files;
mod toc;
mod placeholder;
mod listconvert;
mod xrefloc;
mod bookindex;
mod layoutfit;
mod outlines;
mod color_setup;
mod printing;
mod glyphs;
mod glyph_sets;
mod history_doc;
mod library;
mod book;
mod pdfcomments;
mod spell;
mod parent_links;
mod datamerge;
mod datamerge_link;
mod dbmerge;
mod datamerge_expr;
mod xlink;
mod table_view;
mod known_limits;
mod engine_messages;
#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod robust_tests;

use folio_model::Document;
use folio_model::layout::DocumentLayout;
use folio_model::Rect;
use folio_render::{GlyphCache, ImageCache, Scene, render_page_display};
use folio_color::{Colors, ProfileIndex};
use folio_text::FontStore;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

struct Session {
    view: Arc<Mutex<view_state::ViewState>>,
    dialog: Arc<Mutex<Value>>,
    doc: Document,
    layout: DocumentLayout,
    colors: folio_color::Colors,
    /// Shared with the sessions an edit makes from this one.
    fonts: Arc<FontStore>,
    glyph_cache: Arc<GlyphCache>,
    images: Arc<ImageCache>,
    summary: Value,
    /// The document file that was opened, its folder (relative links, Links folder,
    /// Document fonts) and name.
    source_path: PathBuf,
    source_dir: Option<PathBuf>,
    source_name: String,
    /// Installed profile files of the document's CMYK / RGB profiles, when found.
    cmyk_profile: Option<PathBuf>,
    rgb_profile: Option<PathBuf>,
    print: Mutex<Option<print::PrintState>>,
    lazy: print::Lazy,
    /// Separations preview: the document's plates, images decoded as ink, recent planes.
    inks: OnceLock<folio_render::sep::DocInks>,
    ink_images: folio_render::sep::InkImages,
    planes: sep::PlaneCache,
    /// A `.folio` document: its project, references and lock.
    folio: Option<Arc<project::FolioSession>>,
    /// Facts the inspect commands share (built on their first use).
    inspect: OnceLock<inspect::Index>,
    /// The parts being edited and their history, shared along the sessions edits make;
    /// None when the document cannot be edited (INDD opened read-only, IDML).
    edit: Option<edit::Shared>,
}

impl Session {
    fn locations(&self) -> &std::collections::HashMap<String, folio_preflight::Location> {
        self.lazy.locations.get_or_init(|| folio_preflight::locate(&self.doc, &self.layout))
    }

    /// Current links; resolved without search folders the first time.
    fn links(&self) -> Arc<folio_links::LinkReport> {
        let mut state = self.print.lock().expect("shared state must remain unpoisoned after each operation");
        state.get_or_insert_with(|| print::build_links(&self.doc, self.source_dir.as_deref(), &[])).links.clone()
    }

    fn doc_inks(&self) -> &folio_render::sep::DocInks {
        self.inks.get_or_init(|| folio_render::sep::document_inks(&self.doc, &self.layout, &self.colors))
    }

    /// Ink planes of a page area at `scale`: cached, or rendered and kept. Also whether it
    /// was cached.
    fn ink_planes(&self, page: usize, scale: f64, region: Option<Rect>, colors: &Colors) -> Result<(Arc<folio_render::sep::InkPlanes>, Rect, bool), String> {
        let area = folio_render::page_region(&self.doc, page, region).ok_or_else(|| format!("invalid region for page {page}"))?;
        if let Some(hit) = self.planes.get(page, scale, &area, colors.key()) {
            return Ok((hit, area, true));
        }
        let scene = Scene { doc: &self.doc, colors, layout: &self.layout, glyphs: &*self.fonts, glyph_cache: &self.glyph_cache, images: &self.images };
        let planes = folio_render::sep::render_inks(&scene, &self.ink_images, self.doc_inks(), page, scale, region).ok_or_else(|| format!("page {page} cannot be drawn at this size"))?;
        let planes = Arc::new(planes);
        self.planes.put(page, scale, area, planes.clone(), colors.key());
        Ok((planes, area, false))
    }

    /// A render in an ink view: the header and RGBA pixels. `proof` is View ▸ Proof Colors' conversions while it is on: Overprint Preview
    /// and Separations Preview draw through the proof device (RGB content is separated into its inks; measured in InDesign 2025 for CMYK, RGB
    /// and gray devices), the Ink Limit view and the Flattener Preview do not (InDesign's Ink Limit view follows the device in a way not explained).
    fn render_view(&self, page: usize, scale: f64, region: Option<Rect>, view: &Value, proof: Option<&Colors>) -> Result<(Value, Vec<u8>), String> {
        if view["mode"] == "flattener" {
            return flatpreview::render(self, page, scale, region, view);
        }
        let started = Instant::now();
        let inks = self.doc_inks();
        let v = sep::parse_view(view, inks)?;
        let colors = if matches!(v, folio_render::sep::View::InkLimit { .. }) { &self.colors } else { proof.unwrap_or(&self.colors) };
        let (planes, _, cached) = self.ink_planes(page, scale, region, colors)?;
        let rgba = folio_render::sep::composite_with(&planes, inks, colors, &v, sep::parse_look(view));
        let st = &planes.stats;
        let header = json!({"ok": true, "type": "page", "page": page, "width": planes.width, "height": planes.height,
                            "region": region.map(|r| [r.left, r.top, r.right, r.bottom]), "view": sep::view_name(&v),
                            "stats": {"ms": started.elapsed().as_secs_f64() * 1000.0, "inkMs": st.elapsed_ms, "cached": cached,
                                      "items": st.items, "glyphs": st.glyphs, "images": st.images, "missingImages": st.missing_images,
                                      "overprints": st.overprints, "layers": st.layers}});
        Ok((header, rgba))
    }

    fn preflight(&self, image_ink: bool) -> folio_preflight::Report {
        let links = self.links();
        let options = folio_preflight::Options {
            scope: self.doc.preflight_settings.clone(),
            image_ink,
            fonts: Some(folio_preflight::font_facts(&self.fonts, &self.doc, &self.layout)),
            ..self.doc.preflight_profiles.profiles.iter().find(|p|p.name == self.doc.preflight_profiles.active).and_then(|p|folio_preflight::profiles::options(p).ok()).unwrap_or_default()
        };
        folio_preflight::preflight(&self.doc, &self.layout, &links, &options)
    }
}

/// System fonts are scanned once, at startup, while the user is still choosing a file;
/// later documents reuse the store of the previous one.
struct Fonts {
    startup: Mutex<Option<std::thread::JoinHandle<FontStore>>>,
    spare: Mutex<Option<FontStore>>,
}

impl Fonts {
    fn take(&self) -> FontStore {
        if let Some(store) = self.spare.lock().expect("shared state must remain unpoisoned after each operation").take() {
            return store;
        }
        if let Some(handle) = self.startup.lock().expect("shared state must remain unpoisoned after each operation").take()
            && let Ok(store) = handle.join()
        {
            return store;
        }
        FontStore::discover(&[])
    }
}

/// Installed ICC profiles by description, scanned once at startup like the fonts (in the
/// background) and shared by every document.
struct Profiles {
    startup: Mutex<Option<std::thread::JoinHandle<ProfileIndex>>>,
    index: OnceLock<Arc<ProfileIndex>>,
}

impl Profiles {
    fn start() -> Profiles {
        Profiles { startup: Mutex::new(Some(std::thread::spawn(ProfileIndex::scan_installed))), index: OnceLock::new() }
    }

    fn get(&self) -> Arc<ProfileIndex> {
        self.index
            .get_or_init(|| {
                let handle = self.startup.lock().expect("shared state must remain unpoisoned after each operation").take();
                Arc::new(handle.and_then(|h| h.join().ok()).unwrap_or_else(ProfileIndex::scan_installed))
            })
            .clone()
    }
}

type Out = Arc<Mutex<std::io::Stdout>>;

/// A dialog's `$then` edit (dialog_state::then_request) after its first edit `first`: taken on in the same undo step, the reply is the
/// two together (their parts and pages); refused, the first edit is undone too and the refusal is the reply (the dialog stays open).
fn dialog_then_edit(session: &Mutex<Option<Arc<Session>>>, then: &Value, first: Value) -> Value {
    let current = || session.lock().expect("shared state must remain unpoisoned after each operation").clone();
    let keep = |next: Option<Session>| if let Some(n) = next { *session.lock().expect("shared state must remain unpoisoned after each operation") = Some(Arc::new(n)); };
    let Some(s) = current() else { return json!({"ok": false, "error": "no document open"}) };
    let refused = match dialog_state::then_request(&s, then).map_err(|e| json!({"ok": false, "error": e})).and_then(|req| edit::run(&req, &s)) {
        Ok((next, reply)) if reply["ok"] == true => {
            keep(next);
            let mut both = reply;
            for key in ["parts", "pages", "created"] {
                let mut all: Vec<Value> = first[key].as_array().cloned().unwrap_or_default();
                for v in both[key].as_array().into_iter().flatten() { if !all.contains(v) { all.push(v.clone()); } }
                both[key] = Value::Array(all);
            }
            return both;
        }
        Ok((next, reply)) => { keep(next); reply }
        Err(e) => e,
    };
    if let Some(s) = current() && let Ok((next, _)) = edit::run(&json!({"cmd": "undo"}), &s) { keep(next); }
    refused
}

fn send(out: &Out, mut header: Value, body: &[u8]) {
    engine_messages::annotate(&mut header);
    header["bodyBytes"] = json!(body.len());
    // the reply to an attached MCP connection goes back to it (attach.rs)
    if attach::deliver(&header, body) { return; }
    let text = serde_json::to_vec(&header).expect("header");
    let mut o = out.lock().expect("shared state must remain unpoisoned after each operation");
    let _ = o.write_all(&(text.len() as u32).to_le_bytes());
    let _ = o.write_all(&text);
    let _ = o.write_all(body);
    let _ = o.flush();
}

/// Sends a header already written as JSON object text (`{"type":…}`), with `id`, `ok` and
/// no body.
fn send_text(out: &Out, id: &Value, object: &str) {
    if attach::deliver_text(id, object) { return; }
    let rest = object.strip_prefix('{').unwrap_or(object);
    let sep = if rest.trim_start().starts_with('}') { "" } else { "," };
    let head = format!(r#"{{"id":{id},"ok":true,"bodyBytes":0{sep}"#);
    let mut o = out.lock().expect("shared state must remain unpoisoned after each operation");
    let _ = o.write_all(&((head.len() + rest.len()) as u32).to_le_bytes());
    let _ = o.write_all(head.as_bytes());
    let _ = o.write_all(rest.as_bytes());
    let _ = o.flush();
}

fn reader_exe() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FOLIO_INDD_READER") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    let reader = if cfg!(windows) { "folio-indd-reader.exe" } else { "folio-indd-reader" };
    let mut candidates = vec![exe.with_file_name(reader)];
    // development layout: next/engine/target/<profile>/folio-engine.exe
    if let Some(root) = exe.ancestors().nth(5) {
        candidates.push(root.join("native/indd-reader/target/release").join(reader));
    }
    candidates.into_iter().find(|p| p.exists())
}

/// Reader output is cached per source file: re-opening an unchanged INDD skips conversion.
/// Key = path, size and modification time of the source plus the reader's own size and
/// time (a new reader invalidates). Only the newest `KEEP` entries are kept.
const KEEP: usize = 5;
const COMPLETE: &str = "folio-cache-complete";

fn app_cache(name: &str) -> PathBuf {
    if let Some(root) = std::env::var_os("FOLIO_CACHE_DIR").map(PathBuf::from) {
        return root.join(name);
    }
    // a relative or empty variable would put the cache (document content) in the working folder
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).filter(|p| p.is_absolute())
            .unwrap_or_else(std::env::temp_dir).join("FolioStudioNext").join(name)
    }
    #[cfg(not(windows))]
    {
        let home = || std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute());
        let base = if cfg!(target_os = "macos") {
            home().map(|p| p.join("Library/Caches"))
        } else {
            std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
                .filter(|p| p.is_absolute()).or_else(|| home().map(|p| p.join(".cache")))
        };
        base.unwrap_or_else(std::env::temp_dir).join("FolioStudioNext").join(name)
    }
}

fn cache_root() -> PathBuf {
    app_cache("reader-cache")
}

/// Sampled colour conversions per profile set (`folio_color`); `FOLIO_COLOR_CACHE`
/// overrides the folder.
fn color_cache_root() -> PathBuf {
    std::env::var_os("FOLIO_COLOR_CACHE").map(PathBuf::from).unwrap_or_else(|| app_cache("color-cache"))
}

fn profile_json(u: &folio_color::ProfileUse) -> Value {
    use folio_color::ProfileStatus as S;
    let status = match &u.status {
        S::Used => "used".to_string(),
        S::Display => "display".to_string(),
        S::NotNamed => "notNamed".to_string(),
        S::Missing => "missing".to_string(),
        S::Unusable(why) => format!("unusable: {why}"),
    };
    json!({"name": u.name, "file": u.path.as_ref().map(|p| p.display().to_string()), "status": status})
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

fn stamp(path: &Path) -> String {
    let m = std::fs::metadata(path).ok();
    let len = m.as_ref().map_or(0, |m| m.len());
    let modified = m
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("{len}:{modified}")
}

fn cache_key(source: &Path, reader: &Path) -> String {
    let canonical = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    format!("{:016x}", fnv1a(&format!("{}|{}|{}|{}", canonical.display(), stamp(source), reader.display(), stamp(reader))))
}

/// Keeps the newest complete entries; removes older ones and leftovers of interrupted runs.
/// Only touches folders inside our own cache root.
fn evict(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut complete = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        match std::fs::metadata(path.join(COMPLETE)).and_then(|m| m.modified()) {
            Ok(t) => complete.push((t, path)),
            Err(_) => {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
    complete.sort_by_key(|c| std::cmp::Reverse(c.0));
    for (_, path) in complete.into_iter().skip(KEEP) {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// Font folders InDesign would also use: "Document fonts" beside the document and its parent.
fn document_font_dirs(path: &Path) -> Vec<PathBuf> {
    path.ancestors()
        .skip(1)
        .take(2)
        .map(|d| d.join("Document fonts"))
        .filter(|d| d.is_dir())
        .collect()
}

fn open(id: &Value, path: &str, editable: bool, out: &Out, font_pool: &Fonts, profiles: &Arc<Profiles>) -> Result<Session, String> {
    let started = Instant::now();
    let progress = |stage: &str, detail: Value| {
        send(out, json!({"id": id, "event": "progress", "stage": stage, "detail": detail, "ms": started.elapsed().as_millis() as u64}), &[]);
    };
    load_session(path, editable, font_pool, profiles, &progress)
}

fn load_session(path: &str, editable: bool, font_pool: &Fonts, profiles: &Arc<Profiles>, progress: &dyn Fn(&str, Value)) -> Result<Session, String> {
    let started = Instant::now();
    let mut timings = serde_json::Map::new();
    let source_path = folio_links::uri::resolve_native(Path::new(path)).unwrap_or_else(|| PathBuf::from(path));
    let source = source_path.as_path();
    let path = source.to_str().unwrap_or(path);
    let bytes = std::fs::metadata(source).map_err(|e| format!("cannot read {path}: {e}"))?.len();
    let lower = path.to_lowercase();

    // A .folio: references resolved in its project, lock taken, then read like IDML.
    let mut folio = None;
    let mut edit_state: Option<edit::Shared> = None;
    let mut doc = if lower.ends_with(".folio") {
        progress("project", json!({"bytes": bytes}));
        let t = Instant::now();
        let f = project::open_folio(source)?;
        // a .folio is edited in place; read-only while another user holds the lock
        let (doc, state) = edit::start(&f.opened.file, source, f.opened.reader.clone(), Some(&f.opened.manifest))?;
        edit_state = Some(state);
        timings.insert("resolveMs".into(), json!(f.opened.summary.resolve_ms.round() as u64));
        timings.insert("parseMs".into(), json!((t.elapsed().as_secs_f64() * 1000.0 - f.opened.summary.resolve_ms).max(0.0).round() as u64));
        folio = Some(f);
        doc
    } else {
    // 1. INDD → IDML + previews with the existing reader (never writes its input).
    let (idml, result) = if lower.ends_with(".idml") {
        // FOLIO_IDML_READER_RESULT=<result.json>: previews from an INDD reader run of the same
        // document (tests that compare InDesign's own IDML with its PDF), as folio-pdf's
        // --reader-result
        (source.to_path_buf(), std::env::var_os("FOLIO_IDML_READER_RESULT").map(PathBuf::from))
    } else {
        progress("reader", json!({"bytes": bytes}));
        let exe = reader_exe().ok_or(if cfg!(windows) { "INDD reader (folio-indd-reader.exe) not found" } else { "INDD reader (folio-indd-reader) not found" })?;
        let root = cache_root();
        let dir = root.join(cache_key(source, &exe));
        let t = Instant::now();
        if dir.join(COMPLETE).is_file() && dir.join("document.idml").is_file() {
            // touch: most recently used entries survive eviction
            let _ = std::fs::write(dir.join(COMPLETE), b"ok");
            timings.insert("readerCached".into(), json!(true));
        } else {
            let _ = std::fs::remove_dir_all(&dir);
            let tmp = root.join(format!("tmp-{}-{}", std::process::id(), t.elapsed().as_nanos() ^ started.elapsed().as_nanos()));
            std::fs::create_dir_all(&tmp).map_err(|e| format!("cache folder: {e}"))?;
            let status = std::process::Command::new(&exe)
                .arg(source)
                .arg(&tmp)
                .arg("document")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output()
                .map_err(|e| format!("reader failed to start: {e}"))?;
            if !status.status.success() {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(format!("reader failed: {}", String::from_utf8_lossy(&status.stderr).trim()));
            }
            std::fs::write(tmp.join(COMPLETE), b"ok").map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &dir).map_err(|e| format!("cache folder: {e}"))?;
            evict(&root);
        }
        timings.insert("readerMs".into(), json!(t.elapsed().as_millis() as u64));
        (dir.join("document.idml"), Some(dir.join("result.json")))
    };

    // 2. IDML → model
    progress("parse", json!({}));
    let t = Instant::now();
    // an IDML opened for editing (tests): the parts in memory, nowhere to save them
    let doc = if editable && lower.ends_with(".idml") {
        let data = match result.as_deref() {
            Some(r) => folio_idml::ReaderData::from_result(r).map_err(|e| format!("IDML: {e:?}"))?,
            None => folio_idml::ReaderData::default(),
        };
        let (doc, state) = edit::start(&idml, source, data, None)?;
        edit_state = Some(state);
        doc
    } else {
        folio_idml::read_package(&idml, result.as_deref()).map_err(|e| format!("IDML: {e:?}"))?
    };
    timings.insert("parseMs".into(), json!(t.elapsed().as_millis() as u64));
    doc
    };

    pdfplace::attach(&mut doc, folio.as_ref(), source.parent());
    psdlayers::attach(&mut doc, folio.as_ref(), source.parent());
    textvars::attach(&mut doc, source, folio.as_ref());
    // Colour: the document's profiles from the installed index, tables from the cache (or
    // built), on their own thread while fonts and layout run.
    let color_job = {
        let profiles = profiles.clone();
        let (cmyk, rgb) = (doc.print.cmyk_profile.clone(), doc.print.rgb_profile.clone());
        let dir = source.parent().map(Path::to_path_buf);
        std::thread::Builder::new().name("folio-color".into()).spawn(move || {
            let index = profiles.get();
            let (dc, files) = color_setup::resolve(cmyk.as_deref(), rgb.as_deref(), &index, dir.as_deref());
            (dc, index.entries().len(), index.scan_ms(), files)
        })
    };

    // 3. fonts: the system scan already ran at startup; add the document's own folders
    progress("fonts", json!({}));
    let t = Instant::now();
    let mut fonts = font_pool.take();
    fonts.add_dirs(&document_font_dirs(source));
    if let Some(f) = &folio {
        fonts.add_dirs(&project::project_font_dirs(f));
    }
    timings.insert("fontsMs".into(), json!(t.elapsed().as_millis() as u64));

    // 4. text layout
    progress("layout", json!({}));
    let t = Instant::now();
    let mut layout = folio_text::layout_document(&doc, &fonts);
    layout.page_frames = folio_text::layout_master_page_numbers(&doc, &fonts, &layout)
        .into_iter()
        .map(|p| folio_model::layout::PageFrameText { page: p.page, frame: p.frame })
        .collect();
    timings.insert("layoutMs".into(), json!(t.elapsed().as_millis() as u64));

    // 5. colour conversions (normally ready by now)
    let t = Instant::now();
    let joined = color_job.ok().and_then(|h| h.join().ok());
    timings.insert("colorWaitMs".into(), json!(t.elapsed().as_millis() as u64));
    let mut warnings = doc.warnings.clone();
    let (mut cmyk_profile, mut rgb_profile) = (None, None);
    let (colors, color) = match joined {
        Some((dc, indexed, index_ms, (cmyk_file, rgb_file))) => {
            cmyk_profile = cmyk_file;
            rgb_profile = rgb_file;
            warnings.extend(dc.warnings.iter().cloned());
            let r = &dc.report;
            timings.insert("colorMs".into(), json!(r.total_ms.round() as u64));
            let color = json!({
                "cmyk": profile_json(&dc.cmyk),
                "rgb": profile_json(&dc.rgb),
                "cached": r.cached,
                "buildMs": r.build_ms.map(|v| (v * 10.0).round() / 10.0),
                "ms": (r.total_ms * 10.0).round() / 10.0,
                "cacheError": r.cache_error,
                "profilesIndexed": indexed,
                "indexMs": (index_ms * 10.0).round() / 10.0,
            });
            (dc.colors, color)
        }
        None => {
            warnings.push("colour: profile setup failed; colours shown without colour management".into());
            (folio_color::Colors::unmanaged(), Value::Null)
        }
    };
    timings.insert("totalMs".into(), json!(started.elapsed().as_millis() as u64));

    let summary = json!({
        "path": path,
        "bytes": bytes,
        "pages": pages::summary(&doc),
        "pageSettings": edit_state.as_ref().map(|e|pages::settings(e.lock().expect("shared state must remain unpoisoned after each operation").editor.parts())),
        "stories": doc.stories.len(),
        "warnings": warnings,
        "color": color,
        "missingFonts": layout.missing_fonts,
        "overset": layout.overset.len(),
        // Stories set with InDesign's saved lines (INDD) and those composed instead, with why.
        "layout": {"lines": layout.stats.lines, "glyphs": layout.stats.glyphs,
                   "savedLines": {"stories": layout.stats.saved_stories, "fallbacks": layout.stats.saved_fallbacks,
                                  "fallbackStories": layout.saved_fallbacks.iter().take(50)
                                      .map(|(id, why)| json!({"story": id, "reason": why})).collect::<Vec<_>>()}},
        "timings": timings,
        "bleed": {"top": doc.print.bleed[0], "bottom": doc.print.bleed[1], "inside": doc.print.bleed[2], "outside": doc.print.bleed[3]},
        "slug": {"top": doc.print.slug[0], "bottom": doc.print.slug[1], "inside": doc.print.slug[2], "outside": doc.print.slug[3]},
        "folio": folio.as_ref().map(project::FolioSession::summary),
        "editable": edit_state.is_some() && folio.as_ref().is_none_or(|f| !f.read_only()),
    });
    let session=Session {
        view: Arc::new(Mutex::new(view_state::initial())),
        dialog: Arc::new(Mutex::new(Value::Null)),
        doc,
        layout,
        colors,
        fonts: Arc::new(fonts),
        glyph_cache: Arc::new(GlyphCache::new()),
        images: Arc::new(ImageCache::new()),
        summary,
        source_path: source.to_path_buf(),
        source_dir: source.parent().map(Path::to_path_buf),
        source_name: source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        cmyk_profile,
        rgb_profile,
        print: Mutex::new(None),
        lazy: print::Lazy::default(),
        inks: OnceLock::new(),
        ink_images: folio_render::sep::InkImages::new(),
        planes: sep::PlaneCache::default(),
        folio: folio.map(Arc::new),
        inspect: OnceLock::new(),
        edit: edit_state,
    };
    place::configure_previews(&session);
    Ok(session)
}

/// Writes a PDF/X of the export settings in `req` (preset, bleed, marks, spreads, pages) to `path`
/// (atomically), one export at a time. `started(total pages)` runs when writing begins.
/// Returns the report, the page count and the time taken.
fn export_pdf(s: &Session, req: &Value, path: &Path, started: &dyn Fn(usize)) -> Result<(folio_pdf::Report, usize, f64), String> {
    let (exported, total, ms) = render_pdf(s, req, started)?;
    print::write_atomically(path, &exported.pdf)?;
    Ok((exported.report, total, ms))
}

fn render_pdf(s: &Session, req: &Value, started: &dyn Fn(usize)) -> Result<(folio_pdf::Exported, usize, f64), String> {
    let _one_at_a_time = print::export_lock().lock().unwrap_or_else(|e| e.into_inner());
    let t = Instant::now();
    let links = s.links();
    let roots = s.print.lock().expect("shared state must remain unpoisoned after each operation").as_ref().map(|p| p.roots.clone()).unwrap_or_default();
    let base = folio_pdf::ExportOptions {
        output_profile: s.cmyk_profile.clone(),
        rgb_profile: s.rgb_profile.clone(),
        search_roots: roots,
        document_dir: s.source_dir.clone(),
        document_name: s.source_name.clone(),
        document_id: pdfcomments::document_id(s),
        colors: Some(s.colors.clone()),
        ..Default::default()
    };
    let mut opts = print::export_options(req, base)?;
    if let Some(job) = req.get("printingJob") {
        let job = serde_json::from_value(job.clone()).map_err(|e| format!("invalid print job: {e}"))?;
        folio_pdf::printing::configure(&s.doc, &job, &mut opts)?;
    }
    // Print Booklet: the sheets of a booklet (selection and options as the bookletPlan query takes them)
    if let Some(b) = req.get("booklet") {
        opts.booklet = Some(printing::booklet_sheets(&s.doc, b)?);
    }
    // the pages that are output: those of the spreads that are not hidden (and in the range, if any); a booklet: its sheets
    let total = match &opts.booklet {
        Some(sheets) => sheets.len(),
        None => s.doc.spreads.iter().filter(|sp| !sp.hidden).flat_map(|sp| &sp.pages).filter(|pg| opts.pages.as_ref().is_none_or(|v| v.contains(&pg.index))).count(),
    };
    started(total);
    // an Output Date in the text shows the moment of this output
    let now = textvars::uses_output_date(&s.doc).then(|| textvars::output_now(s));
    let (doc, layout) = now.as_ref().map_or((&s.doc, &s.layout), |(d, l)| (d, l));
    let exported = folio_pdf::export(doc, layout, &s.fonts, Some((*links).clone()), &opts).map_err(|e| e.to_string())?;
    Ok((exported, total, t.elapsed().as_secs_f64() * 1000.0))
}

fn print_command(req: &Value, s: &Session, id: &Value, out: &Out) -> Result<Value, String> {
    match req["cmd"].as_str().unwrap_or("") {
        "links" => {
            let roots: Vec<PathBuf> = req["roots"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect()).unwrap_or_default();
            let state = print::build_links(&s.doc, s.source_dir.as_deref(), &roots);
            *s.print.lock().expect("shared state must remain unpoisoned after each operation") = Some(state);
            // effective resolution comes from the checks' own reading of each placement
            let images = print::image_facts(&s.preflight(false));
            let mut report = {
                let guard = s.print.lock().expect("shared state must remain unpoisoned after each operation");
                let state = guard.as_ref().expect("links just built");
                print::links_view(state, &s.doc, s.locations(), &images)
            };
            // content links beside the placed files (rows of their own: the counts and badges are of the files)
            // Editing rebuilds the print state while holding the editor: release print before reading it.
            let content = s.edit.as_ref().map(|e| xlink::content_links(s, &edit::lock_for_read(e))).unwrap_or_default();
            report["contentLinks"] = json!(print::content_link_rows(&content, &s.doc, s.locations()));
            Ok(json!({"type": "links", "report": report}))
        }
        "preflight" => {
            let report = s.preflight(true);
            let mut view = print::preflight_view(&report, s.locations());
            // the database of a data merge source that is not set up on this computer (the snapshot still merges and prints)
            dbmerge::preflight_note(&mut view, s);
            Ok(json!({"type": "preflight", "report": view}))
        }
        "export" => {
            let path = PathBuf::from(req["path"].as_str().unwrap_or(""));
            if path.as_os_str().is_empty() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                return Err("내보낼 PDF 경로가 없습니다".into());
            }
            let (report, total, ms) = export_pdf(s, req, &path, &|total| {
                send(out, json!({"id": id, "event": "progress", "job": "export", "stage": "write", "done": 0, "total": total}), &[]);
            })?;
            Ok(json!({"type": "exported", "result": print::export_view(&report, &path, total, ms)}))
        }
        "inks" => Ok(json!({"type": "inks", "inks": sep::inks_view(s.doc_inks())})),
        "inkAt" => {
            let page = req["page"].as_u64().unwrap_or(0) as usize;
            let (x, y) = (req["x"].as_f64().unwrap_or(0.0), req["y"].as_f64().unwrap_or(0.0));
            let inks = s.doc_inks();
            // the sharpest render on screen when it shows the point, else a small one around it
            let (values, scale, cached) = match s.planes.at_point(page, x, y) {
                Some((planes, px, py, scale)) => (planes.at(px, py), scale, true),
                None => {
                    const SCALE: f64 = 4.0;
                    let region = Rect { left: x - 1.0, top: y - 1.0, right: x + 1.0, bottom: y + 1.0 };
                    let (planes, area, _) = s.ink_planes(page, SCALE, Some(region), &s.colors)?;
                    let (px, py) = (((x - area.left) * SCALE).floor().max(0.0) as u32, ((y - area.top) * SCALE).floor().max(0.0) as u32);
                    (planes.at(px.min(planes.width - 1), py.min(planes.height - 1)), SCALE, false)
                }
            };
            let values = values.ok_or("point outside the page")?;
            Ok(sep::ink_at_view(inks, &values, page, x, y, scale, cached))
        }
        other => Err(format!("unknown cmd {other}")),
    }
}

enum RequestRoute {
    Handle(fn(&requests::Context<'_>, requests::Request)),
    Quit,
}

macro_rules! request_table {
    ($($name:literal => $route:expr,)*) => {
        &[$(($name, $route)),*]
    };
}

fn main() {
    // The command catalog reads these wire names from the request registry.
    const REQUESTS: &[(&str, RequestRoute)] = request_table! {
        "selfCheck" => RequestRoute::Handle(requests::self_check),
        "open" => RequestRoute::Handle(requests::open_document),
        "render" => RequestRoute::Handle(requests::render),
        "info" => RequestRoute::Handle(requests::info),
        "links" => RequestRoute::Handle(requests::print),
        "preflight" => RequestRoute::Handle(requests::print),
        "export" => RequestRoute::Handle(requests::print),
        "inks" => RequestRoute::Handle(requests::print),
        "inkAt" => RequestRoute::Handle(requests::print),
        "textHit" => RequestRoute::Handle(requests::text),
        "textCaret" => RequestRoute::Handle(requests::text),
        "textSelection" => RequestRoute::Handle(requests::text),
        "textStory" => RequestRoute::Handle(requests::text),
        "textCursor" => RequestRoute::Handle(requests::text),
        "textFormat" => RequestRoute::Handle(requests::text),
        "textCopy" => RequestRoute::Handle(requests::text),
        "textNavigate" => RequestRoute::Handle(requests::text),
        "styleInfo" => RequestRoute::Handle(requests::text),
        "styleShortcut" => RequestRoute::Handle(requests::text),
        "textFind" => RequestRoute::Handle(requests::text),
        "textLayout" => RequestRoute::Handle(requests::text),
        "textMarks" => RequestRoute::Handle(requests::text),
        "tableHit" => RequestRoute::Handle(requests::text),
        "tableCells" => RequestRoute::Handle(requests::text),
        "tableInfo" => RequestRoute::Handle(requests::text),
        "tableLines" => RequestRoute::Handle(requests::text),
        "documentInfo" => RequestRoute::Handle(requests::inspect),
        "pageItems" => RequestRoute::Handle(requests::inspect),
        "package" => RequestRoute::Handle(requests::package),
        "packagePlan" => RequestRoute::Handle(requests::package),
        "projectCreate" => RequestRoute::Handle(requests::project),
        "projectInfo" => RequestRoute::Handle(requests::project),
        "newDocumentFonts" => RequestRoute::Handle(requests::new_document_fonts),
        "documentNew" => RequestRoute::Handle(requests::new_document),
        "importPlan" => RequestRoute::Handle(requests::import),
        "importRun" => RequestRoute::Handle(requests::import),
        "importCancel" => RequestRoute::Handle(requests::import_cancel),
        "edit" => RequestRoute::Handle(requests::edit),
        "undo" => RequestRoute::Handle(requests::edit),
        "redo" => RequestRoute::Handle(requests::edit),
        "historyDelete" => RequestRoute::Handle(requests::edit),
        "historyClear" => RequestRoute::Handle(requests::edit),
        "editState" => RequestRoute::Handle(requests::edit),
        "labels" => RequestRoute::Handle(requests::edit),
        "selectionSize" => RequestRoute::Handle(requests::edit),
        "effects" => RequestRoute::Handle(requests::edit),
        "copy" => RequestRoute::Handle(requests::edit),
        "debugDigest" => RequestRoute::Handle(requests::edit),
        "conveyor" => RequestRoute::Handle(requests::conveyor),
        "save" => RequestRoute::Handle(requests::save),
        "saveCopy" => RequestRoute::Handle(requests::save),
        "dataMergeCreate" => RequestRoute::Handle(requests::save),
        "dataMergeExportPdf" => RequestRoute::Handle(requests::save),
        "historyNewDocument" => RequestRoute::Handle(requests::save),
        "exportIdml" => RequestRoute::Handle(requests::save),
        "lockOverride" => RequestRoute::Handle(requests::save),
        "projectStatus" => RequestRoute::Handle(requests::save),
        "close" => RequestRoute::Handle(requests::close),
        "packageCancel" => RequestRoute::Handle(requests::package_cancel),
        "quit" => RequestRoute::Quit,
    };

    let out: Out = Arc::new(Mutex::new(std::io::stdout()));
    let session: Arc<Mutex<Option<Arc<Session>>>> = Arc::new(Mutex::new(None));
    let font_pool = Arc::new(Fonts {
        startup: Mutex::new(Some(std::thread::spawn(|| FontStore::discover(&[])))),
        spare: Mutex::new(None),
    });
    let profiles = Arc::new(Profiles::start());
    send(&out, json!({"event": "ready", "version": env!("CARGO_PKG_VERSION")}), &[]);

    // render workers
    let (tx, rx) = mpsc::channel::<requests::RenderJob>();
    let rx = Arc::new(Mutex::new(rx));
    let workers = std::thread::available_parallelism().map(|n| n.get().clamp(1, 8)).unwrap_or(4);
    for _ in 0..workers {
        let rx = rx.clone();
        let out = out.clone();
        let profiles = profiles.clone();
        std::thread::spawn(move || loop {
            let job = rx.lock().expect("shared state must remain unpoisoned after each operation").recv();
            let Ok((id, s, page, scale, region, view, display)) = job else { break };
            if let Some(view) = view {
                // an ink view (separations, overprint, ink limit); View ▸ Proof Colors changes the colours of the first two (see render_view)
                let proof = proof_setup::colors_for_ink_views(&s, &profiles.get());
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.render_view(page, scale, region, &view, proof.as_deref()))) {
                    Ok(Ok((mut header, rgba))) => {
                        header["id"] = id;
                        send(&out, header, &rgba);
                    }
                    Ok(Err(e)) => send(&out, wire::error_reply(&id, e), &[]),
                    Err(_) => send(&out, wire::error_reply(&id, "engine panicked while drawing inks"), &[]),
                }
                continue;
            }
            // View ▸ Proof Colors: pages are drawn through the proof setup while it is on (proof_setup.rs)
            let (proof, proof_error) = proof_setup::colors_for(&s, &profiles.get());
            let scene = Scene { doc: &s.doc, colors: proof.as_deref().unwrap_or(&s.colors), layout: &s.layout, glyphs: &*s.fonts, glyph_cache: &s.glyph_cache, images: &s.images };
            match render_page_display(&scene, page, scale, region, display) {
                Some(r) => send(
                    &out,
                    json!({"id": id, "ok": true, "type": "page", "page": page, "width": r.width, "height": r.height, "region": region.map(|r| [r.left, r.top, r.right, r.bottom]),
                           "proofed": proof.is_some(), "proofError": proof_error,
                           "stats": {"ms": r.stats.elapsed_ms, "items": r.stats.items, "glyphs": r.stats.glyphs,
                                     "images": r.stats.images, "missingImages": r.stats.missing_images}}),
                    &r.rgba,
                ),
                None => send(&out, wire::error_reply(&id, format!("no page {page}")), &[]),
            }
        });
    }

    // One request line for the screen (stdin) and an attached MCP connection (attach.rs): both act on this session.
    let (requests, incoming) = mpsc::channel::<attach::Incoming>();
    {
        let requests = requests.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(text) = line else { break };
                if requests.send(attach::Incoming::Line { text, mcp: false }).is_err() { break; }
            }
            let _ = requests.send(attach::Incoming::Quit);
        });
    }
    attach::sync(&requests);
    let context = requests::Context { out: &out, session: &session, font_pool: &font_pool, profiles: &profiles, tx: &tx };
    for request in incoming.iter() {
        let attach::Incoming::Line { text: line, mcp } = request else { break };
        if line.trim().is_empty() {
            continue;
        }
        let mut req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                send(&out, json!({"ok": false, "error": format!("bad request: {e}")}), &[]);
                continue;
            }
        };
        let id = req["id"].clone();
        // an attached client's paths: relative to the project open in the app, checked, then this machine's (attach_paths.rs)
        if mcp {
            let current = session.lock().expect("shared state must remain unpoisoned after each operation").clone();
            match attach_paths::resolve(&mut req, current.as_deref()) {
                Ok(Some(resolved)) => attach::reply_relative_to(&id, resolved),
                Ok(None) => {}
                Err(refused) => {
                    let mut reply = json!({"id": id, "ok": false, "code": "refused", "error": refused.error});
                    // what the client can do about it (a setting a person turns on, and where)
                    for (key, value) in refused.detail.as_object().into_iter().flatten() { reply[key] = value.clone(); }
                    send(&out, reply, &[]);
                    continue;
                }
            }
        }
        let mut dialog_values = None;
        let mut dialog_then = None;
        // the New Document dialog lists the installed fonts (the font store is here)
        if req["cmd"] == "openDialog" && req["dialog"] == "newDocument" { dialog_state::set_fonts(project::new_document_fonts(&font_pool)); }
        let response = registered_queries::dispatch(&req, session.lock().expect("shared state must remain unpoisoned after each operation").as_deref(), &profiles);
        if let Some(mut response) = response {
            // an app dialog's request that needs what this loop holds: New Document the font store (on its own thread), New Book and
            // New Library the colour profiles; then the dialog is finished
            if let Some(request) = response.as_object_mut().and_then(|o| o.remove("_appRequest")) {
                if request["cmd"] != "documentNew" {
                    let reply = registered_queries::dispatch(&request, session.lock().expect("shared state must remain unpoisoned after each operation").as_deref(), &profiles)
                        .unwrap_or_else(|| json!({"ok": false, "error": format!("unknown cmd {}", request["cmd"])}));
                    let (mut header, event) = dialog_state::app_finished(&request, &reply);
                    if let Some(event) = event { send(&out, event, &[]); }
                    header["id"] = id;
                    send(&out, header, &[]);
                    continue;
                }
                let out2 = out.clone();
                let pool = font_pool.clone();
                std::thread::spawn(move || {
                    let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| project::new_document(&request, &out2, &pool)))
                        .unwrap_or_else(|_| json!({"ok": false, "code": "io", "error": "engine panicked while creating the document"}));
                    let (mut header, event) = dialog_state::app_finished(&request, &reply);
                    if let Some(event) = event { send(&out2, event, &[]); }
                    header["id"] = id;
                    send(&out2, header, &[]);
                });
                continue;
            }
            // a document dialog's request that takes long (File ▸ Export PDF writes its PDF, with progress events): run as the request
            // itself is, on its own thread, then the dialog goes on (dialog_state::document_finished)
            if let Some(job) = response.as_object_mut().and_then(|o| o.remove("_documentRequest")) {
                let Some(mut s) = session.lock().expect("shared state must remain unpoisoned after each operation").clone() else {
                    send(&out, wire::error_reply(&id, "no document open"), &[]);
                    continue;
                };
                if let Some(n) = edit::settle(&s) {
                    s = Arc::new(n);
                    *session.lock().expect("shared state must remain unpoisoned after each operation") = Some(s.clone());
                }
                // (File ▸ Package's running stage is shown before the thread starts)
                if let Some(event) = response.as_object_mut().and_then(|o| o.remove("_dialogEvent")) { send(&out, event, &[]); }
                let out2 = out.clone();
                std::thread::spawn(move || {
                    let request = job["requests"][0].clone();
                    let header = if request["cmd"] == "package" {
                        // (each progress event of the package also goes into the dialog's values, which the screen is told of)
                        let progress = |p: &Value| if let Some(event) = dialog_state::document_progress(&s, p) { send(&out2, event, &[]); };
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| package::package_with(&request, &s, Some(&out2), &progress)))
                            .unwrap_or_else(|_| json!({"ok": false, "code": "io", "error": "engine panicked while packaging"}))
                    } else {
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| print_command(&request, &s, &id, &out2))) {
                            Ok(Ok(mut v)) => { v["ok"] = json!(true); v }
                            Ok(Err(e)) => json!({"ok": false, "error": e}),
                            Err(_) => json!({"ok": false, "error": "engine panicked"}),
                        }
                    };
                    if request["cmd"] == "export" && header["ok"] == true { attach_paths::person_exported(header["result"]["path"].as_str().unwrap_or("")); }
                    let (mut answer, event) = dialog_state::document_finished(&s, &job, &header);
                    if let Some(event) = event { send(&out2, event, &[]); }
                    answer["id"] = id;
                    send(&out2, answer, &[]);
                });
                continue;
            }
            // an attached client's Place of a library item: the edit the screen sends with the item (its parts and the library's
            // folder never travel over MCP) runs here in the app session, as the screen's would (mcp-c6)
            if mcp && req["cmd"] == "library" && req["action"] == "place" && response["ok"] == true && response["command"].is_object() {
                let command = response["command"].take();
                response["_editRequest"] = json!({"cmd": "edit", "label": "placeLibraryAsset", "commands": [command], "selectionChanged": true});
            }
            // a dock action (dock_action.rs): the screen applies it and answers later; this request waits for that, not this loop
            if let Some(mut event) = response.as_object_mut().and_then(|o| o.remove("_screenRequest")) {
                event["request"] = id.clone();
                dock_action::wait(&id);
                send(&out, event, &[]);
                let out2 = out.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(dock_action::SCREEN_WAIT);
                    if dock_action::give_up(&id) { send(&out2, dock_action::no_screen(&id), &[]); }
                });
                continue;
            }
            // the screen's answer to a dock action: the reply to the request that waits for it (the screen's own reply follows)
            if let Some(reply) = response.as_object_mut().and_then(|o| o.remove("_screenReply")) { send(&out, reply, &[]); }
            if let Some(edit) = response.as_object_mut().and_then(|o| o.remove("_editRequest")) {
                dialog_values = response.as_object_mut().and_then(|o| o.remove("_dialogValues"));
                dialog_then = response.as_object_mut().and_then(|o| o.remove("_dialogThen"));
                req = edit;
            } else {
                for key in ["_viewEvent", "_settingsEvent", "_dialogEvent"] {
                    if let Some(event) = response.as_object_mut().and_then(|o| o.remove(key)) { send(&out, event, &[]); }
                }
                response["id"] = id;
                // a book or library an attached client made appears in the screen's project tree
                let made = mcp && matches!(req["cmd"].as_str(), Some("book" | "library")) && response["ok"] == true
                    && attached::file_actions(req["cmd"].as_str().unwrap_or("")).1.contains(&req["action"].as_str().unwrap_or(""));
                if made {
                    // (a book exported to PDF: the PDF; else the book or library file it changed)
                    let written = if req["action"] == "exportPdf" { req["output"].as_str() } else { response["path"].as_str() };
                    let file = std::path::Path::new(req["root"].as_str().unwrap_or("")).join(written.unwrap_or(""));
                    send(&out, json!({"event": "external-file", "path": Value::Null, "detail": {"kind": req["cmd"], "file": file, "source": "mcp"}}), &[]);
                }
                send(&out, response, &[]);
                // Preferences ▸ General ▸ Allow LLM connection (MCP) opens or closes the endpoint at once
                if req["cmd"] == "userSettings" { attach::sync(&requests); }
                continue;
            }
        }
        if source_queries::dispatch(&req, &out) { continue; }
        if let Some(reply) = pdf_presets::handle(&req) {
            send(&out, reply, &[]);
            continue;
        }
        let cmd = req["cmd"].as_str().unwrap_or("");
        let Some((_, route)) = REQUESTS.iter().find(|(name, _)| *name == cmd) else {
            send(&out, wire::error_reply(&id, format!("unknown cmd {cmd}")), &[]);
            continue;
        };
        match route {
            RequestRoute::Handle(handle) => {
                handle(&context, requests::Request { id, req, mcp, dialog_values, dialog_then });
            }
            RequestRoute::Quit => break,
        }
    }
    attach::shutdown();
}
