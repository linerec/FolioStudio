//! Prints tiff-crate decoding details for given files.
fn main() {
    use tiff::decoder::Decoder;
    use tiff::tags::Tag;
    for f in std::env::args().skip(1) {
        let bytes = std::fs::read(&f).unwrap();
        let mut d = match Decoder::new(std::io::Cursor::new(&bytes)) { Ok(d) => d, Err(e) => { println!("{f}: new: {e}"); continue } };
        println!("{f}: colortype {:?} map {:?}", d.colortype(), d.get_tag_u16_vec(Tag::ColorMap).map(|m| m.len()));
        match d.read_image() { Ok(_) => println!("  read ok"), Err(e) => println!("  read: {e}") }
    }
}
