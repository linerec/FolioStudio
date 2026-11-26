//! folio-preflight <input.idml | reader-output-folder> [--search-root DIR]... [--json OUT]
//!     [--document-dir DIR]   folder of the original INDD (relink search, Document fonts)
//!     [--cache-dir DIR]      keep the file index, header probes and image samples there
//!     [--no-image-ink]       do not sample CMYK images for ink coverage
//!     [--repeat N]           run the checks N times and report the median (warm timing)
//!     [--max-ink PCT] [--bleed-mm MM] [--safety-mm MM] [--small-text-pt PT] [--ppi-warn N]
//!     [--ppi-error N] [--include-pasteboard] [--quiet]
//!
//! Reads the document like the engine server does (IDML → model, system fonts + the
//! document's "Document fonts", text layout), resolves links, runs the checks and prints a
//! summary; `--json` writes the full report with timings.

use folio_links::{FileIndex, LinkReport, LinkSearch};
use folio_preflight::{Options, Severity, font_facts, preflight};
use folio_text::FontStore;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 10000.0).round() / 10.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input = None;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut json_out = None;
    let mut doc_dir = None;
    let mut cache_dir: Option<PathBuf> = None;
    let mut repeat = 1usize;
    let mut quiet = false;
    let mut o = Options { image_ink: true, ..Options::default() };
    let num = |v: Option<String>| v.and_then(|s| s.parse::<f64>().ok());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--search-root" => roots.extend(args.next().map(PathBuf::from)),
            "--json" => json_out = args.next().map(PathBuf::from),
            "--document-dir" => doc_dir = args.next().map(PathBuf::from),
            "--cache-dir" => cache_dir = args.next().map(PathBuf::from),
            "--no-image-ink" => o.image_ink = false,
            "--repeat" => repeat = args.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1),
            "--max-ink" => o.max_ink = num(args.next()).unwrap_or(o.max_ink),
            "--bleed-mm" => o.bleed_pt = num(args.next()).map(|v| v * 72.0 / 25.4),
            "--safety-mm" => o.safety_margin_pt = num(args.next()).map(|v| v * 72.0 / 25.4).unwrap_or(o.safety_margin_pt),
            "--small-text-pt" => o.small_text_pt = num(args.next()).unwrap_or(o.small_text_pt),
            "--ppi-warn" => o.image_ppi_warn = num(args.next()).unwrap_or(o.image_ppi_warn),
            "--ppi-error" => o.image_ppi_error = num(args.next()).unwrap_or(o.image_ppi_error),
            "--include-pasteboard" => o.include_pasteboard = true,
            "--quiet" => quiet = true,
            "-h" | "--help" => {
                eprintln!("usage: folio-preflight <input.idml | reader-output-folder> [--search-root DIR]... [--json OUT] [--document-dir DIR] [--cache-dir DIR] [--no-image-ink] [--repeat N]");
                return;
            }
            _ => input = Some(PathBuf::from(a)),
        }
    }
    let Some(input) = input else {
        eprintln!("usage: folio-preflight <input.idml | reader-output-folder> [--search-root DIR]... [--json OUT]");
        std::process::exit(2);
    };
    let started = Instant::now();
    let (idml, result) = if input.is_dir() {
        let r = input.join("result.json");
        (input.join("document.idml"), r.is_file().then_some(r))
    } else {
        (input.clone(), None)
    };
    let document_dir = doc_dir.or_else(|| if input.is_dir() { None } else { idml.parent().map(Path::to_path_buf) });

    let t = Instant::now();
    let doc = match folio_idml::read_package(&idml, result.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {}: {e}", idml.display());
            std::process::exit(1);
        }
    };
    let parse_ms = ms(t);

    // Fonts as the engine server finds them: system folders, then "Document fonts" next to
    // the document and in its parent folder.
    let t = Instant::now();
    let mut fonts = FontStore::discover(&[]);
    let font_dirs: Vec<PathBuf> = document_dir
        .iter()
        .flat_map(|d| d.ancestors().take(2).map(|a| a.join("Document fonts")).collect::<Vec<_>>())
        .filter(|d| d.is_dir())
        .collect();
    fonts.add_dirs(&font_dirs);
    let fonts_ms = ms(t);

    let t = Instant::now();
    let mut layout = folio_text::layout_document(&doc, &fonts);
    layout.page_frames = folio_text::layout_master_page_numbers(&doc, &fonts, &layout)
        .into_iter()
        .map(|p| folio_model::layout::PageFrameText { page: p.page, frame: p.frame })
        .collect();
    let layout_ms = ms(t);
    o.fonts = Some(font_facts(&fonts, &doc, &layout));

    // Links: index of the search roots, InDesign's relink order, header probes.
    let t = Instant::now();
    let (index_cache, probe_cache, ink_cache) = match &cache_dir {
        Some(d) => (Some(d.join("file-index.json")), Some(d.join("probes.json")), Some(d.join("image-ink.json"))),
        None => (None, None, None),
    };
    let index = match &index_cache {
        Some(c) => FileIndex::scan_cached(&roots, c),
        None => FileIndex::scan(&roots),
    };
    let index_ms = ms(t);
    let loaded_probes = probe_cache.as_deref().map(folio_links::probe::load_probe_cache).unwrap_or(0);
    let loaded_ink = ink_cache.as_deref().map(folio_preflight::image_ink::load_cache).unwrap_or(0);
    let t = Instant::now();
    let search = LinkSearch { document_dir: document_dir.clone(), roots: index.roots().to_vec() };
    let links = LinkReport::build(&doc, &search, &index);
    let links_ms = ms(t);

    // Checks (cold: first run samples images; warm: everything cached).
    let mut runs = Vec::new();
    let mut report = None;
    for _ in 0..repeat {
        let t = Instant::now();
        let r = preflight(&doc, &layout, &links, &o);
        runs.push(ms(t));
        report = Some(r);
    }
    let report = report.expect("one run");
    let first = runs[0];
    let mut sorted = runs.clone();
    sorted.sort_by(f64::total_cmp);
    let warm = if runs.len() > 1 {
        let mut w = runs[1..].to_vec();
        w.sort_by(f64::total_cmp);
        Some(w[w.len() / 2])
    } else {
        None
    };
    if let Some(p) = &probe_cache {
        let _ = folio_links::probe::save_probe_cache(p);
    }
    if let Some(p) = &ink_cache {
        let _ = folio_preflight::image_ink::save_cache(p);
    }

    let s = &report.summary;
    let ls = &links.summary;
    if !quiet {
        println!(
            "{}: {} errors, {} warnings, {} info",
            input.display(),
            s.errors,
            s.warnings,
            s.infos
        );
        for (rule, n) in &s.by_rule {
            let sev = report.findings.iter().find(|f| &f.rule == rule).map(|f| f.severity).unwrap_or(Severity::Info);
            println!("  {rule:<20} {n:>5}  ({sev:?})");
        }
        println!(
            "links {}: embedded {}, original {}, relative {}, Links folder {}, search folder+name {}, name only {}, ambiguous {}, missing {}, modified {}",
            ls.links, ls.embedded, ls.original, ls.document_relative, ls.links_folder, ls.search_with_folder, ls.search_name_only, ls.ambiguous, ls.missing, ls.modified
        );
        println!(
            "timings ms: parse {parse_ms}, fonts {fonts_ms}, layout {layout_ms}, index {index_ms} ({}; {} files), links resolve {:.1} + probe {:.1} (probe cache {loaded_probes}, ink cache {loaded_ink}), preflight first {first}{}",
            index.stats().source,
            index.stats().files,
            ls.resolve_ms,
            ls.probe_ms,
            warm.map(|w| format!(", warm median {w} ({} runs)", runs.len() - 1)).unwrap_or_default()
        );
        for l in &report.limitations {
            println!("  limitation: {l}");
        }
    }
    if let Some(out) = json_out {
        // ICC profiles stay out of the report (their size is kept)
        let mut links_json = serde_json::to_value(&links).expect("links");
        if let Some(list) = links_json.get_mut("links").and_then(serde_json::Value::as_array_mut) {
            for e in list {
                if let Some(info) = e.pointer_mut("/info/Ok").and_then(serde_json::Value::as_object_mut)
                    && let Some(icc) = info.remove("icc")
                {
                    let n = icc.as_str().map_or(0, |h| h.len() / 2);
                    info.insert("iccBytes".into(), json!(n));
                }
            }
        }
        let doc_json = json!({
            "input": input, "documentDir": document_dir, "searchRoots": roots,
            "timingsMs": {"parse": parse_ms, "fonts": fonts_ms, "layout": layout_ms, "index": index_ms, "links": links_ms,
                          "linksResolve": ls.resolve_ms, "linksProbe": ls.probe_ms, "preflightRuns": runs, "preflightFirst": first, "preflightWarmMedian": warm,
                          "total": ms(started)},
            "index": index.stats(),
            "caches": {"probesLoaded": loaded_probes, "imageInkLoaded": loaded_ink},
            "options": o,
            "links": links_json,
            "report": report,
        });
        if let Some(p) = out.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if let Err(e) = std::fs::write(&out, serde_json::to_vec_pretty(&doc_json).expect("json")) {
            eprintln!("cannot write {}: {e}", out.display());
            std::process::exit(1);
        }
    }
}
