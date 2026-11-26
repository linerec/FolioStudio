//! `cargo run -p folio-math --example dump -- <formulas.tsv> <out dir> [size]`: lays out each line
//! `name<TAB>mathml` and writes `<name>.svg` (sized in points) and a line of dimensions.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("formulas");
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out).unwrap();
    let size: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    for line in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let (name, src) = line.split_once('\t').expect("name<TAB>mathml");
        match folio_math::layout(src, &folio_math::Options { size, display: false, color: [0.0; 3] }) {
            Ok(eq) => {
                std::fs::write(out.join(format!("{name}.svg")), &eq.preview_svg).unwrap();
                println!("{name}\t{:.3}\t{:.3}\tdepth {:.3}\t{}\t{:?}", eq.width, eq.height, eq.depth, eq.font, eq.notes);
            }
            Err(e) => println!("{name}\tERROR {e}"),
        }
    }
}
