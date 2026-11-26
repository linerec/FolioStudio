use std::path::Path;
use folio_idml::read_package;
use folio_text::{FontStore, layout_document, layout::toc::sources};

#[test]
fn composed_heading_locations_use_section_page_names() {
    let path=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/toc.idml");
    let doc=read_package(&path,None).unwrap();
    let layout=layout_document(&doc,&FontStore::discover(&[]));
    let all=sources(&doc,&layout);
    let headings:Vec<_>=all.iter().filter(|s|doc.stories[&s.story].paragraphs[s.paragraph].style=="H1").collect();
    assert_eq!(headings.iter().filter(|s|!s.hidden).map(|s|s.page.as_str()).collect::<Vec<_>>(),["ii","10","11"],"{all:?}");
    assert_eq!(headings.iter().filter(|s|s.hidden).count(),1);
}
