//! Compare installed-font outlines with an independently captured layout.
use folio_model::layout::GlyphSource;
fn main() {
    let path = std::env::args().nth(1).expect("IDML path");
    let doc = folio_idml::read_package(std::path::Path::new(&path), None).expect("IDML");
    let fonts = folio_text::FontStore::discover(&[]);
    let layout = folio_text::layout_document(&doc, &fonts);
    let mut outlines = std::collections::BTreeMap::new();
    for frame in layout.frames.values() {
        for g in frame.lines.iter().flat_map(|l| &l.glyphs) {
            outlines.entry(format!("{}:{}", g.font, g.glyph)).or_insert_with(|| fonts.outline(g.font, g.glyph).map(|x| (*x).clone()));
        }
    }
    println!("{}", serde_json::to_string(&serde_json::json!({"layout":layout,"outlines":outlines})).unwrap());
}
