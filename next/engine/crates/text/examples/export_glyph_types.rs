fn main() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/renderer/src/generated/glyphs.ts");
    std::fs::write(path,folio_text::glyphs::typescript()).unwrap();
}
