//! Attach original vector sources and retain named failures in the import report.
use folio_model::{Document, Graphic, ItemKind, PageItem};
use std::path::PathBuf;

pub fn attach(doc: &mut Document, resolve: &dyn Fn(&Graphic) -> Option<PathBuf>) {
    fn walk(items: &mut [PageItem], resolve: &dyn Fn(&Graphic) -> Option<PathBuf>, warnings: &mut Vec<String>, missing: &mut bool) {
        for item in items {
            match &mut item.kind {
                ItemKind::Group { children } => walk(children, resolve, warnings, missing),
                ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => {
                    if let Some(g) = content.as_mut().filter(|g|g.element=="SVG") {
                        if let Some(path) = resolve(g) {
                            match folio_render::svg::read(&path) {
                                Ok(_) => {g.preview=Some(path.to_string_lossy().into_owned());g.preview_from_source=true;},
                                Err(e) => {warnings.push(format!("SVG {}: {e}",g.id));}
                            }
                        } else { *missing=true; }
                    } else if let Some(g) = content.as_mut().filter(|g|g.element=="WMF") {
                        // a Windows metafile (EMF, WMF) whose file is at hand: drawn from its records (build-a). A preview an imported
                        // INDD kept stays unless the records draw everything (no text, bitmap or clipping left out)
                        let kept = g.preview.as_deref().map(std::path::Path::new).is_some_and(|p| p.is_file() && !std::fs::read(p).is_ok_and(|b| folio_render::metafile::is_metafile(&b)));
                        let drawn = |p: &std::path::Path| std::fs::read(p).ok().and_then(|b| folio_render::metafile::convert(&b).ok())
                            .is_some_and(|c| !kept || c.skipped.is_empty()) && folio_render::svg::read(p).is_ok();
                        if let Some(path) = resolve(g).filter(|p| drawn(p)) {
                            g.preview=Some(path.to_string_lossy().into_owned());g.preview_from_source=true;
                        }
                    }
                },
                _ => {},
            }
            walk(&mut item.clipped,resolve,warnings,missing);
        }
    }
    let mut missing=false;
    for spread in doc.spreads.iter_mut().chain(&mut doc.master_spreads) {walk(&mut spread.items,resolve,&mut doc.warnings,&mut missing);}
    if !missing {doc.warnings.retain(|w|!w.starts_with("SVG without preview file"));}
}

pub fn report(links: &folio_links::LinkReport) -> Vec<serde_json::Value> {
    links.links.iter().filter_map(|entry| {
        let folio_links::LinkStatus::Found {path,..}=entry.status.as_ref()? else {return None};
        if entry.info.as_ref()?.as_ref().ok()?.format != folio_links::ImageFormat::Svg {return None;}
        folio_render::svg::read(path).err().map(|error|serde_json::json!({"file":path.file_name().map(|p|p.to_string_lossy()),"error":error}))
    }).collect()
}
