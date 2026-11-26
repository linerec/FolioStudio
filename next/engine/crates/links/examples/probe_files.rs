//! cargo run --example probe_files -- <file or folder>...  — header facts as one line each.
fn main() {
    for a in std::env::args().skip(1) {
        let p = std::path::PathBuf::from(&a);
        let files: Vec<_> = if p.is_dir() {
            let mut v: Vec<_> = std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
            v.sort();
            v
        } else {
            vec![p]
        };
        for f in files {
            let t = std::time::Instant::now();
            match folio_links::probe(&f) {
                Ok(i) => println!(
                    "{:<28} {:?} {}x{} {:?} {}bit ch{} alpha={} ppi={:?} icc={} pages={:?} boxes={} colors={:?} [{:.2} ms]",
                    f.file_name().unwrap().to_string_lossy(), i.format, i.width, i.height, i.color_space, i.bits, i.channels, i.alpha,
                    i.ppi.map(|(x, y)| (x.round(), y.round())), i.icc.as_ref().map_or(0, |v| v.len()), i.pages, i.page_boxes.len(),
                    i.vector_colors.as_ref().map(|c| (&c.spaces, &c.spots, &c.image_spaces, c.content_scanned)), t.elapsed().as_secs_f64() * 1e3
                ),
                Err(e) => println!("{:<28} ERR {e}", f.file_name().unwrap().to_string_lossy()),
            }
        }
    }
}
