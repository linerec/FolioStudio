//! cargo run --release --example scan_index -- <root>... [--cache FILE] — scan timings (cold, then warm).
fn main() {
    let mut roots = Vec::new();
    let mut cache = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--cache" {
            cache = args.next().map(std::path::PathBuf::from);
        } else {
            roots.push(std::path::PathBuf::from(a));
        }
    }
    for round in 0..5 {
        let t = std::time::Instant::now();
        let i = match &cache {
            Some(c) => folio_links::FileIndex::scan_cached(&roots, c),
            None => folio_links::FileIndex::scan(&roots),
        };
        let s = i.stats();
        println!("round {round}: {:.1} ms (stats {:.1} ms) files {} dirs {} listed {} reused {} errors {} source {}", t.elapsed().as_secs_f64() * 1e3, s.elapsed_ms, s.files, s.dirs, s.listed_dirs, s.reused_dirs, s.errors, s.source);
    }
}
