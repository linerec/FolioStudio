//! The picture a linked raster image is drawn from on screen when the document carries no preview of it. An IDML that
//! InDesign exported holds no picture previews (only an INDD read through the reader brings them), so a placed PNG, JPEG,
//! TIFF, GIF, BMP or PSD of an imported IDML had nothing to draw: its own file, found in the project, is the picture, as
//! for a picture placed in Folio (`edit.rs`: "use the source raster itself"). A file the drawing code cannot read stays
//! undrawn as before. The print output already reads the original file and is not changed by this.
use folio_model::{Document, ItemKind, PageItem};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A raster format the screen drawing decodes, by the file's first bytes (not its name).
pub fn is_raster(path: &Path) -> bool {
    let mut head = [0u8; 8];
    let Ok(n) = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) else { return false };
    let h = &head[..n];
    h.starts_with(b"\x89PNG") || h.starts_with(&[0xFF, 0xD8]) || h.starts_with(b"II*\0") || h.starts_with(b"MM\0*")
        || h.starts_with(b"8BPS") || h.starts_with(b"GIF8") || h.starts_with(b"BM")
}

pub fn attach(doc: &mut Document, resolve_uri: impl Fn(&str) -> Option<PathBuf>) {
    fn walk(items: &mut [PageItem], preview: &mut impl FnMut(&str) -> Option<String>, missing: &mut bool) {
        for item in items {
            match &mut item.kind {
                ItemKind::Group { children } => walk(children, preview, missing),
                ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => {
                    if let Some(g) = content.as_mut().filter(|g| g.element == "Image" && g.preview.is_none()) {
                        g.preview = g.link_uri.as_deref().and_then(&mut *preview);
                        *missing |= g.preview.is_none();
                    }
                }
                _ => {}
            }
            walk(&mut item.clipped, preview, missing);
        }
    }
    // Borrow the file's current result for this document pass, and look it up again on the next pass.
    let mut previews: HashMap<String, Option<String>> = HashMap::new();
    let mut preview = |uri: &str| {
        if let Some(preview) = previews.get(uri) {
            return preview.clone();
        }
        let preview = resolve_uri(uri).filter(|path| is_raster(path)).map(|path| path.to_string_lossy().into_owned());
        previews.insert(uri.to_owned(), preview.clone());
        preview
    };
    let mut missing = false;
    for spread in doc.spreads.iter_mut().chain(&mut doc.master_spreads) {
        walk(&mut spread.items, &mut preview, &mut missing);
    }
    if !missing {
        doc.warnings.retain(|w| !w.starts_with("Image without preview file"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_formats_are_known_by_their_first_bytes() {
        let dir = std::env::temp_dir().join(format!("folio-rasterplace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes, raster) in [("a.psd", &b"8BPS\0\x01"[..], true), ("b.png", &b"\x89PNG\r\n"[..], true), ("c.jpg", &[0xFF, 0xD8, 0xFF][..], true),
            ("d.tif", &b"II*\0"[..], true), ("e.pdf", &b"%PDF-1.4"[..], false), ("f.psd", &b"<svg"[..], false)] {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            assert_eq!(is_raster(&p), raster, "{name}");
        }
        assert!(!is_raster(&dir.join("missing.png")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
