//! Cached source previews; cache identity includes source bytes, page, crop and alpha mode.
use super::*;
use folio_model::{Document, Graphic, ItemKind, PageItem};
use std::path::PathBuf;

pub fn cached(path: &Path, source: &Source, cache: &Path) -> Result<PathBuf, String> {
    // A full content hash, including the import settings; file times are not identities.
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    hash.update(&bytes);
    hash.update(serde_json::to_vec(&(&source.options, source.bounds, &source.layers)).map_err(|e| e.to_string())?);
    let digest = hash.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
    let output = cache.join(format!("pdf-v2-{digest}.png"));
    if output.is_file() { return Ok(output); }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let temporary = output.with_extension(format!("{}-tmp", std::process::id()));
    let info = crate::probe::probe(path).map_err(|e| e.to_string())?;
    let result = preview(path, &info, source, &temporary).and_then(|_| {
        if output.is_file() { let _ = std::fs::remove_file(&temporary); return Ok(()); }
        std::fs::rename(&temporary, &output).map_err(|e| e.to_string())
    });
    if result.is_err() { let _ = std::fs::remove_file(&temporary); }
    result.map(|_| output)
}

pub fn attach(doc: &mut Document, cache: &Path, resolve: &dyn Fn(&Graphic) -> Option<PathBuf>) -> Result<(), String> {
    fn walk(items: &mut [PageItem], cache: &Path, resolve: &dyn Fn(&Graphic) -> Option<PathBuf>, missing: &mut bool) -> Result<(), String> {
        for item in items {
            match &mut item.kind {
                ItemKind::Group { children } => walk(children, cache, resolve, missing)?,
                ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => {
                    if let Some(g) = content.as_mut().filter(|g| g.pdf.is_some()) {
                        // Keep imported INDD previews; newly placed PDFs can initially point at the PDF itself.
                        let readable_preview = g.preview.as_ref().is_some_and(|p| Path::new(p).is_file() &&
                            crate::probe::probe(Path::new(p)).is_ok_and(|p| !p.vector));
                        if !readable_preview || g.layer_options.is_some() {
                            if let Some(path) = resolve(g) {
                                let p = g.pdf.as_ref().expect("the content filter requires PDF options");
                                let options = Options::try_from(p)?;
                                let b = g.bounds;
                                let source = Source { options, bounds: [b.left,b.top,b.right,b.bottom], metadata: String::new(), layers:g.layer_options.clone() };
                                let preview = cached(&path, &source, cache)?;
                                g.preview = Some(preview.to_string_lossy().into_owned());
                                g.preview_from_source = true;
                            } else { *missing = true; }
                        }
                    }
                }
                _ => {}
            }
            walk(&mut item.clipped, cache, resolve, missing)?;
        }
        Ok(())
    }
    let mut missing = false;
    for spread in doc.spreads.iter_mut().chain(&mut doc.master_spreads) {
        walk(&mut spread.items, cache, resolve, &mut missing)?;
    }
    if !missing { doc.warnings.retain(|w| !w.starts_with("PDF without preview file")); }
    Ok(())
}
