//! cargo run --example item_pages -- <document.idml | reader folder> OUT.json — page of every item,
//! page rectangles of every spread, and graphic bounds (for comparisons with InDesign).
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let input = std::path::PathBuf::from(&a[0]);
    let (idml, result) = if input.is_dir() { (input.join("document.idml"), Some(input.join("result.json"))) } else { (input.clone(), None) };
    let doc = folio_idml::read_package(&idml, result.as_deref()).expect("read");
    let layout = folio_model::layout::DocumentLayout::default();
    let rows: Vec<serde_json::Value> = folio_preflight::item_pages(&doc, &layout)
        .into_iter()
        .map(|(id, page, prints, b)| serde_json::json!({"id": id, "page": page, "prints": prints, "bounds": b}))
        .collect();
    let spreads: Vec<serde_json::Value> = doc
        .spreads
        .iter()
        .map(|s| {
            serde_json::json!({"id": s.id, "pages": s.pages.iter().map(|p| {
                let r = p.transform.apply_rect(&p.bounds);
                serde_json::json!({"name": p.name, "rect": [r.left, r.top, r.right, r.bottom]})
            }).collect::<Vec<_>>()})
        })
        .collect();
    std::fs::write(&a[1], serde_json::to_vec(&serde_json::json!({"items": rows, "spreads": spreads})).unwrap()).unwrap();
}
