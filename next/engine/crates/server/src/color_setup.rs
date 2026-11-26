use folio_color::{DocumentColors, ProfileIndex};
use std::path::{Path, PathBuf};

pub fn resolve(cmyk: Option<&str>, rgb: Option<&str>, index: &ProfileIndex, dir: Option<&Path>)
    -> (DocumentColors, (Option<PathBuf>, Option<PathBuf>)) {
    let dc = folio_color::document_colors(cmyk, rgb, index, dir, Some(&crate::color_cache_root()));
    let file = |name: Option<&str>, used: &Option<PathBuf>| {
        used.clone().or_else(|| name.and_then(|n| index.find_for_document(n, dir)))
    };
    let files = (file(cmyk, &dc.cmyk.path), file(rgb, &dc.rgb.path));
    (dc, files)
}

pub fn refresh(cur: &crate::Session, next: &mut crate::Session) {
    let p = &next.doc.print;
    if p.cmyk_profile == cur.doc.print.cmyk_profile && p.rgb_profile == cur.doc.print.rgb_profile { return; }
    let index = ProfileIndex::scan_installed();
    let (dc, (cmyk, rgb)) = resolve(p.cmyk_profile.as_deref(), p.rgb_profile.as_deref(), &index, next.source_dir.as_deref());
    next.summary["color"] = serde_json::json!({"cmyk":crate::profile_json(&dc.cmyk),"rgb":crate::profile_json(&dc.rgb)});
    next.colors = dc.colors;
    next.cmyk_profile = cmyk;
    next.rgb_profile = rgb;
    // Converted image caches include the color transform key; configure_previews uses this session's profiles.
}
