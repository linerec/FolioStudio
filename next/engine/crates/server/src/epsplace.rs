//! Placed EPS files are shown and printed through the preview they carry (build-a, `folio_render::eps`), a Photoshop EPS through its
//! own picture at its full resolution (build-a2): the picture is written once into the project's disposable cache
//! (`.folio/cache/epsplace/eps-v2-<sha256>.<tif|jpg|png>`, named by the file's content) and becomes the graphic's preview. A preview
//! an imported INDD kept (a raster file) stays.
use folio_model::{Document, Graphic, ItemKind, PageItem};
use std::path::{Path, PathBuf};

/// Whether `path` is an EPS (PostScript text or the DOS binary header).
pub fn is_eps(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 4];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && (&head == b"%!PS" || head == [0xC5, 0xD0, 0xD3, 0xC6])
}

/// The cached picture of the EPS `file` (made when missing): a Photoshop EPS's own picture (`.tif`, `.jpg`; build-a2), else the preview
/// it carries (`.png`); `size` in points is the placeholder's when the file has neither.
pub fn cached(file: &Path, size: [f64; 2], cache: &Path) -> Result<PathBuf, String> {
    let (hash, _) = folio_project::hash::sha256_file(file).map_err(|e| format!("{}: {e}", file.display()))?;
    if let Some(out) = ["tif", "jpg", "png"].iter().map(|ext| cache.join(format!("eps-v2-{hash}.{ext}"))).find(|p| p.is_file()) {
        return Ok(out);
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let (ext, data, _) = folio_render::eps::picture_file(&bytes, size)?;
    let out = cache.join(format!("eps-v2-{hash}.{ext}"));
    let tmp = out.with_extension(format!("{}-tmp.{ext}", std::process::id()));
    let made = std::fs::write(&tmp, &data).map_err(|e| e.to_string()).and_then(|_| std::fs::rename(&tmp, &out).map_err(|e| e.to_string()));
    if made.is_err() {
        let _ = std::fs::remove_file(&tmp);
        if out.is_file() {
            return Ok(out);
        }
    }
    made.map(|_| out)
}

pub fn attach(doc: &mut Document, cache: &Path, resolve: &dyn Fn(&Graphic) -> Option<PathBuf>) {
    fn walk(items: &mut [PageItem], cache: &Path, resolve: &dyn Fn(&Graphic) -> Option<PathBuf>, warnings: &mut Vec<String>) {
        for item in items {
            match &mut item.kind {
                ItemKind::Group { children } => walk(children, cache, resolve, warnings),
                ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => {
                    if let Some(g) = content.as_mut().filter(|g| g.element == "EPS") {
                        // an INDD proxy (a raster file) stays; a project keeps the EPS itself as the graphic's file
                        let own = g.preview.as_deref().map(PathBuf::from).filter(|p| p.is_file());
                        if own.as_deref().is_some_and(|p| !is_eps(p)) {
                            continue;
                        }
                        let Some(file) = own.or_else(|| resolve(g)).filter(|p| is_eps(p)) else { continue };
                        let b = g.bounds;
                        match cached(&file, [b.right - b.left, b.bottom - b.top], cache) {
                            Ok(png) => {
                                g.preview = Some(png.to_string_lossy().into_owned());
                                g.preview_from_source = true;
                            }
                            Err(e) => warnings.push(format!("EPS preview: {e}")),
                        }
                    }
                }
                _ => {}
            }
            walk(&mut item.clipped, cache, resolve, warnings);
        }
    }
    let mut warnings = Vec::new();
    for spread in doc.spreads.iter_mut().chain(&mut doc.master_spreads) {
        walk(&mut spread.items, cache, resolve, &mut warnings);
    }
    doc.warnings.extend(warnings);
}
