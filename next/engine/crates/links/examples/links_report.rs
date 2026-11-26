//! cargo run --example links_report -- <document.idml | reader-output-folder> [--search-root DIR]... [--document-dir DIR]
//!             [--index-cache FILE] [--json OUT]
//!
//! Resolves every link of the document and prints a summary (counts by how each file was
//! found); `--json` writes the full report.

use folio_links::{FileIndex, LinkReport, LinkSearch};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input = None;
    let mut roots = Vec::new();
    let mut doc_dir = None;
    let mut json = None;
    let mut cache = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--search-root" => roots.extend(args.next().map(PathBuf::from)),
            "--document-dir" => doc_dir = args.next().map(PathBuf::from),
            "--json" => json = args.next().map(PathBuf::from),
            "--index-cache" => cache = args.next().map(PathBuf::from),
            _ => input = Some(PathBuf::from(a)),
        }
    }
    let Some(input) = input else {
        eprintln!("usage: folio-links <document.idml | reader-output-folder> [--search-root DIR]... [--document-dir DIR] [--index-cache FILE] [--json OUT]");
        std::process::exit(2);
    };
    let (idml, result) = if input.is_dir() { (input.join("document.idml"), Some(input.join("result.json"))) } else { (input.clone(), None) };
    let t = Instant::now();
    let doc = match folio_idml::read_package(&idml, result.as_deref().filter(|r| r.is_file())) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {}: {e}", idml.display());
            std::process::exit(1);
        }
    };
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let index = match &cache {
        Some(c) => FileIndex::scan_cached(&roots, c),
        None => FileIndex::scan(&roots),
    };
    let index_ms = t.elapsed().as_secs_f64() * 1000.0;
    let document_dir = doc_dir.or_else(|| if result.is_none() { idml.parent().map(|p| p.to_path_buf()) } else { None });
    let search = LinkSearch { document_dir, roots: index.roots().to_vec() };
    let report = LinkReport::build(&doc, &search, &index);
    let s = &report.summary;
    println!(
        "parse {parse_ms:.0} ms, index {index_ms:.0} ms ({} files, {} dirs, {}), resolve {:.1} ms, probe {:.1} ms",
        index.stats().files,
        index.stats().dirs,
        index.stats().source,
        s.resolve_ms,
        s.probe_ms
    );
    println!(
        "links {} (unique {}): embedded {}, original {}, document-relative {}, Links folder {}, search folder+name {}, search name only {}, ambiguous {}, missing {}; modified {}; probed {} (failed {})",
        s.links, s.unique_uris, s.embedded, s.original, s.document_relative, s.links_folder, s.search_with_folder, s.search_name_only, s.ambiguous, s.missing, s.modified, s.probed, s.probe_failed
    );
    if let Some(out) = json {
        let text = serde_json::to_vec_pretty(&report).expect("report");
        if let Err(e) = std::fs::write(&out, text) {
            eprintln!("cannot write {}: {e}", out.display());
            std::process::exit(1);
        }
    }
}
