//! Lists the ICC profiles the engine finds on this machine, as the document lookup sees them.
//! Usage: cargo run -p folio-color --release --example profiles [-- <description>...]
fn main() {
    let t = std::time::Instant::now();
    let index = folio_color::ProfileIndex::scan_installed();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    for root in index.roots() {
        println!("root {} {}", root.display(), if root.is_dir() { "" } else { "(missing)" });
    }
    for e in index.entries() {
        println!("{} {} {:>8} {} | {}", e.space, e.class, e.bytes, e.path.display(), e.names.join(" | "));
    }
    println!("{} profiles indexed in {ms:.1} ms (scan {:.1} ms)", index.entries().len(), index.scan_ms());
    for name in std::env::args().skip(1) {
        println!("find {name:?} -> {:?}", index.find(&name));
    }
}
