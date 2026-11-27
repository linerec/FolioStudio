//! folio-pdf: an InDesign document (IDML, or the INDD reader's output folder) to PDF/X-4,
//! PDF/X-1a:2001 or PDF/X-3:2002.
//!
//! ```text
//! folio-pdf <input.idml | reader-output-folder> <out.pdf> [--preset x4|x1a|x3] [--bleed]
//!     [--marks crop,bleed,reg,info] [--spreads] [--acrobat-layers] [--export-layers visiblePrintable|visible|all] [--search-root <dir>]...
//!     [--output-profile <icc>] [--rgb-profile <icc>] [--document-dir <dir>]
//!     [--pages 1-3,7] [--report <json>] [--index-cache <file>] [--threads <n>] [--name <text>]
//!     [--font-dir <dir>]... [--reader-result <result.json>] [--no-downsample]
//! ```
//! `--reader-result` attaches the INDD reader's previews to an IDML exported by InDesign;
//! `--font-dir` adds font folders (searched first).
//! The JSON report (warnings, missing fonts and links, inks, timings) goes next to the PDF
//! as `<out>.report.json` unless `--report` says otherwise.

use folio_pdf::{ExportOptions, Marks};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage: folio-pdf <input.idml | reader-output-folder> <out.pdf> [--preset x4|x1a|x3] [--bleed] [--marks crop,bleed,reg,info] \
         [--spreads] [--acrobat-layers] [--export-layers visiblePrintable|visible|all] [--search-root <dir>]... [--output-profile <icc>] [--rgb-profile <icc>] [--document-dir <dir>] \
         [--pages 1-3,7] [--report <json>] [--index-cache <file>] [--threads <n>] [--name <text>]          [--font-dir <dir>]... [--reader-result <result.json>] [--no-downsample]"
    );
    std::process::exit(2)
}

fn parse_pages(s: &str) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (usize, usize) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            out.extend((a.max(1)..=b).map(|p| p - 1));
        } else {
            out.push(part.parse::<usize>().ok()?.max(1) - 1);
        }
    }
    Some(out)
}

#[cfg(windows)]
fn peak_memory_bytes() -> Option<u64> {
    #[repr(C)]
    struct Pmc {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Pmc, cb: u32) -> i32;
    }
    let mut c = Pmc {
        cb: std::mem::size_of::<Pmc>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: a correctly sized, writable counters struct for the current process.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    (ok != 0).then_some(c.peak_working_set_size as u64)
}

#[cfg(not(windows))]
fn peak_memory_bytes() -> Option<u64> {
    None
}

fn main() {
    let started = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let mut opts = ExportOptions {
        reuse_image_rows: std::env::var("FOLIO_ACCEL").as_deref() != Ok("off"),
        ..ExportOptions::default()
    };
    let mut report_path: Option<PathBuf> = None;
    let mut index_cache: Option<PathBuf> = None;
    let mut font_dirs_extra: Vec<PathBuf> = Vec::new();
    let mut reader_result_opt: Option<PathBuf> = None;
    let mut i = 0;
    let next = |i: &mut usize| -> String {
        *i += 1;
        args.get(*i).cloned().unwrap_or_else(|| usage())
    };
    while i < args.len() {
        match args[i].as_str() {
            "--preset" => {
                let p = next(&mut i);
                match folio_pdf::Preset::parse(&p) {
                    Some(pr) => opts.preset = pr,
                    None => {
                        eprintln!("unknown preset {p} (x4, x1a, x3)");
                        std::process::exit(2);
                    }
                }
            }
            "--bleed" => opts.bleed = true,
            "--spreads" => opts.spreads = true,
            "--acrobat-layers" => opts.acrobat_layers = true,
            "--export-layers" => opts.export_layers = folio_pdf::ExportLayers::parse(&next(&mut i)).unwrap_or_else(|| usage()),
            "--marks" => {
                let mut m = Marks::default();
                for k in next(&mut i).split(',') {
                    match k.trim() {
                        "crop" => m.crop = true,
                        "bleed" => m.bleed = true,
                        "reg" | "registration" => m.registration = true,
                        "info" => m.page_info = true,
                        "all" => m = Marks { crop: true, bleed: true, registration: true, page_info: true },
                        other => {
                            eprintln!("unknown mark {other}");
                            std::process::exit(2);
                        }
                    }
                }
                opts.marks = m;
            }
            "--search-root" => opts.search_roots.push(PathBuf::from(next(&mut i))),
            "--output-profile" => opts.output_profile = Some(PathBuf::from(next(&mut i))),
            "--rgb-profile" => opts.rgb_profile = Some(PathBuf::from(next(&mut i))),
            "--document-dir" => opts.document_dir = Some(PathBuf::from(next(&mut i))),
            "--pages" => opts.pages = Some(parse_pages(&next(&mut i)).unwrap_or_else(|| usage())),
            "--report" => report_path = Some(PathBuf::from(next(&mut i))),
            "--index-cache" => index_cache = Some(PathBuf::from(next(&mut i))),
            "--threads" => opts.threads = next(&mut i).parse().unwrap_or_else(|_| usage()),
            "--name" => opts.document_name = next(&mut i),
            "--font-dir" => font_dirs_extra.push(PathBuf::from(next(&mut i))),
            "--reader-result" => reader_result_opt = Some(PathBuf::from(next(&mut i))),
            "--no-downsample" => opts.downsample = None,
            "-h" | "--help" => usage(),
            a if a.starts_with("--") => {
                eprintln!("unknown option {a}");
                usage()
            }
            a => positional.push(a.to_owned()),
        }
        i += 1;
    }
    if positional.len() != 2 {
        usage();
    }
    let input = PathBuf::from(&positional[0]);
    let out = PathBuf::from(&positional[1]);
    let report_path = report_path.unwrap_or_else(|| PathBuf::from(format!("{}.report.json", out.display())));
    let mut stages = serde_json::Map::new();

    // 1. IDML (+ the reader's previews).
    let t = Instant::now();
    let (idml, reader_result) = if input.is_dir() {
        (input.join("document.idml"), Some(input.join("result.json")).filter(|p| p.is_file()))
    } else {
        (input.clone(), reader_result_opt.clone())
    };
    let doc = match folio_idml::read_package(&idml, reader_result.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {}: {e}", idml.display());
            std::process::exit(1);
        }
    };
    stages.insert("readMs".into(), json!(t.elapsed().as_secs_f64() * 1000.0));
    if opts.document_name.is_empty() {
        opts.document_name = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    }
    if opts.document_dir.is_none() && !input.is_dir() {
        opts.document_dir = input.parent().map(Path::to_path_buf);
    }

    // 2. Fonts and text layout.
    let t = Instant::now();
    let mut font_dirs = font_dirs_extra.clone();
    font_dirs.extend(opts.document_dir.as_deref().map(|d| folio_pdf::document_font_dirs(&d.join("x"))).unwrap_or_default());
    let fonts = folio_text::FontStore::discover(&font_dirs);
    stages.insert("fontScanMs".into(), json!(t.elapsed().as_secs_f64() * 1000.0));
    let t = Instant::now();
    let mut layout = folio_text::layout_document(&doc, &fonts);
    layout.page_frames = folio_text::layout_master_page_numbers(&doc, &fonts, &layout)
        .into_iter()
        .map(|p| folio_model::layout::PageFrameText { page: p.page, frame: p.frame })
        .collect();
    stages.insert("layoutMs".into(), json!(t.elapsed().as_secs_f64() * 1000.0));

    // 3. Linked files.
    let t = Instant::now();
    let roots = opts.search_roots.clone();
    let index = match &index_cache {
        Some(c) => folio_links::FileIndex::scan_cached(&roots, c),
        None => folio_links::FileIndex::scan(&roots),
    };
    let search = folio_links::LinkSearch { document_dir: opts.document_dir.clone(), roots: index.roots().to_vec() };
    let links = folio_links::LinkReport::build(&doc, &search, &index);
    stages.insert("linksMs".into(), json!(t.elapsed().as_secs_f64() * 1000.0));

    // 4. Profiles: given, or installed profiles found by the document's descriptions.
    let t = Instant::now();
    if opts.output_profile.is_none() || opts.rgb_profile.is_none() {
        let idx = folio_color::profiles::ProfileIndex::scan_installed();
        if opts.output_profile.is_none()
            && let Some(name) = doc.print.cmyk_profile.as_deref()
        {
            opts.output_profile = idx.find_for_document(name, opts.document_dir.as_deref());
        }
        if opts.rgb_profile.is_none() {
            let name = doc.print.rgb_profile.as_deref().unwrap_or("sRGB IEC61966-2.1");
            opts.rgb_profile = idx.find_for_document(name, opts.document_dir.as_deref());
        }
    }
    stages.insert("profilesMs".into(), json!(t.elapsed().as_secs_f64() * 1000.0));

    // 5. PDF.
    let t = Instant::now();
    let exported = match folio_pdf::export(&doc, &layout, &fonts, Some(links), &opts) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("export failed: {e}");
            std::process::exit(1);
        }
    };
    stages.insert("exportMs".into(), json!(t.elapsed().as_secs_f64() * 1000.0));
    if let Err(e) = std::fs::write(&out, &exported.pdf) {
        eprintln!("cannot write {}: {e}", out.display());
        std::process::exit(1);
    }
    stages.insert("totalMs".into(), json!(started.elapsed().as_secs_f64() * 1000.0));
    let mut report = exported.report;
    report.output = out.display().to_string();
    let mut value = serde_json::to_value(&report).unwrap_or_default();
    value["stages"] = serde_json::Value::Object(stages);
    value["peakWorkingSetBytes"] = json!(peak_memory_bytes());
    value["input"] = json!(input.display().to_string());
    value["layout"] = json!({"missingFonts": layout.missing_fonts.len(), "overset": layout.overset.len(), "glyphs": layout.stats.glyphs});
    value["documentWarnings"] = json!(doc.warnings.len());
    if let Err(e) = std::fs::write(&report_path, serde_json::to_vec_pretty(&value).unwrap_or_default()) {
        eprintln!("cannot write report {}: {e}", report_path.display());
    }
    println!(
        "{}: {} pages, {} bytes, {} warnings, {} missing fonts, links {:?}, {:.0} ms",
        out.display(),
        report.counts.pages,
        report.bytes,
        report.warnings.len(),
        report.fonts_missing.len(),
        report.links_summary,
        started.elapsed().as_secs_f64() * 1000.0
    );
}
