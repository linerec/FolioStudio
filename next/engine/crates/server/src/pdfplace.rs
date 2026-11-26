//! Source previews live in the project's disposable cache. Import validation happens before
//! the edit; loading an older document reports an unsupported preview without losing data.
use std::path::{Path, PathBuf};

pub fn placed_options(doc: &folio_model::Document, id: &str) -> Option<folio_links::pdfplace::Options> {
    let mut result = None;
    folio_idml::for_each_item(doc, &mut |item| {
        use folio_model::ItemKind::*;
        if let Rectangle { content: Some(g) } | Oval { content: Some(g) } | Polygon { content: Some(g) } = &item.kind
            && (item.id == id || g.id == id) { result = g.pdf.as_ref().and_then(|p| p.try_into().ok()); }
    });
    result
}

pub fn facts(doc: &folio_model::Document) -> std::collections::HashMap<String, serde_json::Value> {
    let mut result = std::collections::HashMap::new();
    folio_idml::for_each_item(doc, &mut |item| {
        use folio_model::ItemKind::*;
        if let Rectangle { content: Some(g) } | Oval { content: Some(g) } | Polygon { content: Some(g) } = &item.kind
            && let Some(p) = &g.pdf {
            result.insert(g.id.clone(), serde_json::json!({"page": p.page, "crop": p.crop,
                "transparentBackground":p.transparent_background, "width":g.bounds.right-g.bounds.left, "height":g.bounds.bottom-g.bounds.top}));
        }
    });
    result
}

pub fn cache(folio: Option<&crate::project::FolioSession>) -> PathBuf {
    folio.map(|f| f.opened.project.root.join(".folio/cache/pdfplace"))
        .unwrap_or_else(|| crate::app_cache("pdfplace"))
}

pub fn prepare(file: &Path, source: &folio_edit::GraphicSource, folio: Option<&crate::project::FolioSession>) -> Result<(), String> {
    if matches!(source.format.as_str(), "SVG" | "EMF" | "WMF") { folio_render::svg::read(file)?; }
    if let Some(pdf) = &source.pdf {
        folio_links::pdfplace::previews::cached(file, pdf, &cache(folio))?;
    }
    Ok(())
}

pub fn attach(doc: &mut folio_model::Document, folio: Option<&crate::project::FolioSession>, source_dir: Option<&Path>) {
    let resolve_uri = |uri: &str| {
        let parsed = folio_links::parse_uri(uri)?;
        if let Some(path) = parsed.local_path().filter(|p| p.is_file()) { return Some(path); }
        let name = parsed.file_name()?;
        let dir = source_dir?;
        [dir.join(name), dir.join("Links").join(name)].into_iter().find(|p| p.is_file())
    };
    let resolve = |g: &folio_model::Graphic| resolve_uri(g.link_uri.as_deref()?);
    crate::svgplace::attach(doc, &resolve);
    crate::rasterplace::attach(doc, resolve_uri);
    // placed EPS: the preview the file carries (build-a)
    let eps_cache = folio.map(|f| f.opened.project.root.join(".folio/cache/epsplace")).unwrap_or_else(|| crate::app_cache("epsplace"));
    crate::epsplace::attach(doc, &eps_cache, &resolve);
    let result = folio_links::pdfplace::previews::attach(doc, &cache(folio), &resolve);
    if let Err(e) = result { doc.warnings.push(format!("PDF preview: {e}")); }
}
