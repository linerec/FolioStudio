//! Print production commands: link status, preflight and PDF/X export (PDF/X-4, PDF/X-1a,
//! PDF/X-3).
//!
//! The crates own the facts (`folio_links` which file each graphic uses, `folio_preflight`
//! the checks and item locations, `folio_pdf` the file); this module only shapes them for
//! the window: camelCase, page-relative bounds, one status word per link.

use folio_links::{FileIndex, LinkReport, LinkSearch};
use folio_preflight::{Location, Severity};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Link state of the open document; replaced when the search folders change.
pub struct PrintState {
    pub roots: Vec<PathBuf>,
    pub links: Arc<LinkReport>,
    pub ms: f64,
}

/// Facts computed once per document on first use.
#[derive(Default)]
pub struct Lazy {
    pub locations: OnceLock<HashMap<String, Location>>,
}

fn cache_dir() -> PathBuf {
    crate::app_cache("link-cache")
}

/// Probe results (image headers) survive restarts; loaded once per process.
fn load_probes() {
    static LOADED: OnceLock<()> = OnceLock::new();
    LOADED.get_or_init(|| {
        folio_links::probe::load_probe_cache(&cache_dir().join("probes.json"));
    });
}

fn save_probes() {
    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = folio_links::probe::save_probe_cache(&dir.join("probes.json"));
    }
}

/// Resolves every link of `doc`, searching `roots` (kept per root set in the cache folder).
pub fn build_links(doc: &folio_model::Document, document_dir: Option<&Path>, roots: &[PathBuf]) -> PrintState {
    let started = Instant::now();
    load_probes();
    let roots: Vec<PathBuf> = roots.iter().filter(|r| r.is_dir()).cloned().collect();
    let index = if roots.is_empty() {
        FileIndex::empty()
    } else {
        let key = roots.iter().map(|r| r.display().to_string().to_lowercase()).collect::<Vec<_>>().join("|");
        let dir = cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        FileIndex::scan_cached(&roots, &dir.join(format!("index-{:016x}.json", crate::fnv1a(&key))))
    };
    let search = LinkSearch { document_dir: document_dir.map(Path::to_path_buf), roots: index.roots().to_vec() };
    let links = LinkReport::build(doc, &search, &index);
    save_probes();
    PrintState { roots, links: Arc::new(links), ms: started.elapsed().as_secs_f64() * 1000.0 }
}

fn bounds(loc: Option<&Location>) -> Value {
    loc.map(|l| json!(l.bounds.map(|v| (v * 100.0).round() / 100.0))).unwrap_or(Value::Null)
}

/// The links panel: one row per placed link, with page, bounds and effective resolution.
pub fn links_view(
    state: &PrintState,
    doc: &folio_model::Document,
    locations: &HashMap<String, Location>,
    images: &HashMap<String, (String, [f64; 2])>,
) -> Value {
    let page_names: HashMap<usize, &str> = doc.spreads.iter().flat_map(|s| s.pages.iter()).map(|p| (p.index, p.name.as_str())).collect();
    let frames = folio_links::owner_frames(doc);
    let pdfs = crate::pdfplace::facts(doc);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let rows: Vec<Value> = state
        .links
        .links
        .iter()
        .filter(|e| !matches!(e.element.as_str(), "Movie" | "Sound"))
        .map(|e| {
            let status = e.state_word();
            *counts.entry(status).or_default() += 1;
            // the graphic's frame carries the page and the visible bounds
            let ppi = images.get(&e.owner).map(|(_, ppi)| ppi[0].min(ppi[1]));
            let frame = images.get(&e.owner).map(|(f, _)| f).or_else(|| frames.get(&e.owner));
            let loc = frame.and_then(|f| locations.get(f));
            let name = folio_links::parse_uri(&e.recorded.uri)
                .and_then(|l| l.file_name().map(str::to_owned))
                .unwrap_or_else(|| e.owner.clone());
            let info = e.info.as_ref().and_then(|r| r.as_ref().ok());
            json!({
                "owner": e.owner,
                "name": name,
                "page": loc.map(|l| l.page),
                "pageName": loc.and_then(|l| page_names.get(&l.page)),
                "status": status,
                "how": e.how_word(),
                "path": e.path().map(|p| p.display().to_string()),
                "recorded": folio_links::uri::percent_decode(&e.recorded.uri),
                "ppi": ppi,
                "space": info.map(|i| format!("{:?}", i.color_space)),
                "format": info.map(|i| if pdfs.contains_key(&e.owner) { "PDF".into() } else { format!("{:?}", i.format) }),
                "pdf": pdfs.get(&e.owner),
                "pdfPages": info.and_then(|i| i.pages),
                "bounds": bounds(loc),
            })
        })
        .collect();
    let c = |k: &str| counts.get(k).copied().unwrap_or(0);
    json!({
        "links": rows,
        "summary": {"links": rows.len(), "found": c("found"), "missing": c("missing"), "modified": c("modified"),
                    "embedded": c("embedded"), "ambiguous": c("ambiguous")},
        "roots": state.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        "ms": (state.ms * 10.0).round() / 10.0,
    })
}

/// The content links (Content Placer ▸ Create Link) as rows of the Links panel, beside the placed files: the source's name, the
/// page of the placed frame, the status in the words of the other rows (`found` up to date, `modified` out of date, `missing` the
/// source is gone) and `content` with what the commands `updateContentLink` / `unlinkContentLink` need.
pub fn content_link_rows(links: &[folio_edit::ContentLink], doc: &folio_model::Document, locations: &HashMap<String, Location>) -> Vec<Value> {
    let page_names: HashMap<usize, &str> = doc.spreads.iter().flat_map(|s| s.pages.iter()).map(|p| (p.index, p.name.as_str())).collect();
    links
        .iter()
        .map(|c| {
            let loc = c.frame.as_deref().and_then(|f| locations.get(f));
            // a link to another document: its source is not read, so no warning (`content.status` says `external`)
            let status = match c.status { "normal" | "external" => "found", "outOfDate" => "modified", _ => "missing" };
            json!({
                "owner": c.target,
                "name": c.name,
                "page": loc.map(|l| l.page),
                "pageName": loc.and_then(|l| page_names.get(&l.page)),
                "status": status,
                "how": Value::Null,
                "path": Value::Null,
                "recorded": c.uri,
                "ppi": Value::Null,
                "space": Value::Null,
                "format": match (c.external, c.kind) { (false, "story") => "Internal Linked Story", (false, _) => "Internal Linked Object", (true, "story") => "External Linked Story", _ => "External Linked Object" },
                "bounds": bounds(loc),
                "content": {
                    "link": c.link, "kind": c.kind, "source": c.source, "status": c.status, "edited": c.edited,
                    "mapStyles": c.map_styles, "generic": c.generic, "external": c.external,
                },
            })
        })
        .collect()
}

/// The preflight panel: findings with page-relative bounds.
pub fn preflight_view(report: &folio_preflight::Report, locations: &HashMap<String, Location>) -> Value {
    let findings: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            let loc = f.item.as_deref().and_then(|i| locations.get(i)).filter(|l| Some(l.page) == f.page);
            json!({
                "rule": f.rule,
                "severity": match f.severity { Severity::Error => "error", Severity::Warning => "warning", Severity::Info => "info" },
                "page": f.page,
                "pageName": f.page_name,
                "item": f.item,
                "message": f.message,
                "messageId": f.message_id,
                "messageArgs": f.message_args,
                "bounds": bounds(loc),
            })
        })
        .collect();
    let s = &report.summary;
    json!({
        "findings": findings,
        "summary": {"errors": s.errors, "warnings": s.warnings, "infos": s.infos, "byRule": s.by_rule,
                    "elapsedMs": (s.elapsed_ms * 100.0).round() / 100.0},
        "limitations": report.limitations,
    })
}

/// Graphic id → (frame id, effective ppi), from a preflight run.
pub fn image_facts(report: &folio_preflight::Report) -> HashMap<String, (String, [f64; 2])> {
    report.images.iter().map(|(g, f)| (g.clone(), (f.frame.clone(), f.ppi))).collect()
}

/// Writes `bytes` to `path` through a temporary file beside it, so a failed or interrupted
/// export never leaves a half-written PDF under the chosen name.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().ok_or("no file name")?.to_string_lossy().to_string();
    let tmp = path.with_file_name(format!(".{name}.{}.partial", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e} (다른 프로그램이 파일을 열고 있지 않은지 확인하세요)", path.display())
    })
}

/// Export settings sent by the window, applied over the PDF/X-4 defaults. `preset`: "x4"
/// (default), "x1a" or "x3".
pub fn export_options(req: &Value, base: folio_pdf::ExportOptions) -> Result<folio_pdf::ExportOptions, String> {
    if let Some(value) = req.get("settings") {
        let p: folio_pdf::PdfPreset = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        let mut opts = p.apply(base)?;
        opts.pages = req.get("pages").filter(|v| !v.is_null()).map(|v| serde_json::from_value(v.clone())).transpose().map_err(|e| e.to_string())?;
        return Ok(opts);
    }
    let b = |v: &Value| v.as_bool().unwrap_or(false);
    let m = &req["marks"];
    // the marks' offset from the page and line weight, when the request gives them (the print dialog does)
    let mut marks_at = (base.mark_offset, base.mark_weight);
    if let Some(v) = req["markOffset"].as_f64().filter(|v| v.is_finite() && (0.0..=72.0).contains(v)) { marks_at.0 = v; }
    if let Some(v) = req["markWeight"].as_f64().filter(|v| v.is_finite() && *v > 0.0 && *v <= 8.0) { marks_at.1 = v; }
    // the bleed boxes the request names (top, bottom, inside, outside), instead of the document's bleed
    let custom_bleed = req["customBleed"].as_array().filter(|a| a.len() == 4).and_then(|a| {
        let v: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|n| n.is_finite() && (0.0..=432.0).contains(n)).collect();
        <[f64; 4]>::try_from(v).ok()
    });
    Ok(folio_pdf::ExportOptions {
        mark_offset: marks_at.0,
        mark_weight: marks_at.1,
        custom_bleed,
        preset: req["preset"].as_str().and_then(folio_pdf::Preset::parse).unwrap_or_default(),
        bleed: b(&req["bleed"]),
        spreads: b(&req["spreads"]),
        marks: folio_pdf::Marks { crop: b(&m["crop"]), bleed: b(&m["bleed"]), registration: b(&m["registration"]), page_info: b(&m["pageInfo"]) },
        pages: req["pages"].as_array().map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as usize).collect()),
        // Create Acrobat Layers and Export Layers (a book's export asks for them this way)
        acrobat_layers: b(&req["acrobatLayers"]),
        export_layers: req["exportLayers"].as_str().and_then(folio_pdf::ExportLayers::parse).unwrap_or_default(),
        ..base
    })
}

pub fn export_view(report: &folio_pdf::Report, path: &Path, pages: usize, ms: f64) -> Value {
    json!({
        "path": path.display().to_string(),
        "bytes": report.bytes,
        "pages": pages,
        "ms": ms.round(),
        "warnings": report.warnings.iter().map(|w| json!({"kind": w.kind, "message": w.message, "count": w.count, "pages": w.pages})).collect::<Vec<_>>(),
        "fontsEmbedded": report.fonts_embedded.len(),
        "fontsSubstituted": report.fonts_missing.iter().map(|f| json!({"family": f.family, "style": f.style, "embedded": f.embedded})).collect::<Vec<_>>(),
        "inks": report.inks.iter().map(|i| json!({"name": i.name, "kind": i.kind, "output": i.output})).collect::<Vec<_>>(),
        "outputIntent": if folio_pdf::Preset::parse(&report.preset).is_some_and(|p| p.is_pdfx()) { report.output_intent.description.as_str() } else { "" },
        "preset": report.preset,
        "standard": report.standard,
        "flatten": report.flatten.as_ref().map(|f| json!({
            "pages": f.pages, "regions": f.regions, "objects": f.objects, "images": f.images,
            "lineArtImages": f.line_art_images, "lineArtPpi": f.line_art_ppi, "smoothPpi": f.smooth_ppi,
            "previewGraphics": f.preview_graphics, "bytes": f.bytes, "kinds": f.kinds, "perPage": f.per_page,
        })),
        "converted": report.converted.as_ref().map(|c| json!({"rgbImages": c.rgb_images, "placedPdfs": c.placed_pdfs, "destination": c.destination})),
    })
}

/// Serialises one long job per kind (exports must not interleave).
pub fn export_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
