//! Tries to decode every preview the INDD reader wrote (result.json) and reports failures.
//! Usage: cargo run -p folio-render --release --example decode_previews -- <reader output dir>
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("reader output dir"));
    let text = std::fs::read_to_string(dir.join("result.json")).expect("result.json");
    let files: Vec<String> = text
        .split("\"file\":\"")
        .skip(1)
        .filter_map(|s| s.split('"').next().map(str::to_string))
        .collect();
    let cache = folio_render::ImageCache::new();
    let mode = folio_render::ImageMode { blend: folio_color::BlendSpace::Rgb, proxy: false };
    let colors = folio_color::Colors::unmanaged();
    let (mut ok, mut failed) = (0, Vec::new());
    let started = std::time::Instant::now();
    for f in &files {
        let path = dir.join(f);
        match cache.get(&path.to_string_lossy(), mode, &colors) {
            Some(_) => ok += 1,
            None => {
                let head = std::fs::read(&path).map(|b| b.iter().take(4).map(|x| format!("{x:02x}")).collect::<String>()).unwrap_or_default();
                failed.push(format!("{f} ({head})"));
            }
        }
    }
    println!("decoded {ok}/{} in {:.0} ms", files.len(), started.elapsed().as_secs_f64() * 1000.0);
    for f in failed.iter().take(20) {
        println!("  failed {f}");
    }
    println!("failed total {}", failed.len());
}
