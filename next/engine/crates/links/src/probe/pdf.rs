//! Page boxes and placed artwork metadata for PDF and PDF-compatible Illustrator files.
//! Content operators and resource dictionaries describe colours, fonts and transparency.

mod filters;
mod reader;
mod resources;
mod syntax;

use std::collections::HashSet;

use super::{ImageFormat, ImageInfo, PdfPage, ProbeError, Result};
use reader::Pdf;
use syntax::{Dict, Obj, get};

const MAX_PAGES: usize = 100_000;

struct PageTree {
    pages: Vec<PdfPage>,
    contents: Vec<Obj>,
    transparency: bool,
}

pub(crate) fn probe(file: &mut std::fs::File, len: u64) -> Result<ImageInfo> {
    // SAFETY: the map is read-only and dropped before returning; the file may change
    // underneath only if another program truncates it, in which case reads fault — the
    // same risk every reader of a shared file has. Files are opened read-only.
    let map = unsafe { memmap2::Mmap::map(&*file) }.map_err(|e| ProbeError::Io(e.to_string()))?;
    let b: &[u8] = &map;
    if len == 0 || b.is_empty() {
        return Err(ProbeError::Malformed("empty PDF".into()));
    }
    let (mut pdf, catalog) = Pdf::open(b)?;
    let intent = output_intent(&mut pdf, &catalog);
    let tree = read_pages(&mut pdf, &catalog)?;
    let (colors, fonts) = resources::inspect(&mut pdf, &tree.contents, tree.transparency, intent);
    let first = &tree.pages[0];
    let shown = first.crop.unwrap_or(first.media);
    let mut info = ImageInfo::raster(
        ImageFormat::Pdf,
        (shown[2] - shown[0]).abs().ceil() as u32,
        (shown[3] - shown[1]).abs().ceil() as u32,
        resources::dominant(&colors.spaces, &colors.image_spaces),
        0,
        0,
    );
    info.vector = true;
    info.fonts = fonts;
    info.pages = Some(tree.pages.len() as u32);
    info.page_boxes = tree.pages;
    info.vector_colors = Some(colors);
    Ok(info)
}

fn output_intent(pdf: &mut Pdf<'_>, catalog: &Dict) -> Option<String> {
    let mut output_intent = None;
    if let Some(oi) = get(catalog, "OutputIntents").cloned().and_then(|o| pdf.resolve(&o))
        && let Obj::Array(list) = oi
    {
        for i in list {
            if let Some(d) = pdf.resolve(&i).and_then(|o| o.dict().cloned()) {
                let id = get(&d, "OutputConditionIdentifier").or_else(|| get(&d, "OutputCondition"));
                if let Some(Obj::Str(s)) = id {
                    output_intent = Some(String::from_utf8_lossy(s).into_owned());
                }
            }
        }
    }
    output_intent
}

fn read_pages(pdf: &mut Pdf<'_>, catalog: &Dict) -> Result<PageTree> {
    let mut transparency = false;
    // Page tree with inherited boxes.
    let mut pages = Vec::new();
    let mut contents: Vec<Obj> = Vec::new();
    let mut stack = Vec::new();
    if let Some(root) = get(catalog, "Pages") {
        stack.push((root.clone(), None, None, 0, 0));
    }
    let mut visited = HashSet::new();
    while let Some((node, media, crop, rot, depth)) = stack.pop() {
        if pages.len() >= MAX_PAGES || depth > 64 {
            break;
        }
        if let Obj::Ref(r) = node
            && !visited.insert(r)
        {
            continue;
        }
        let Some(d) = pdf.resolve(&node).and_then(|o| o.dict().cloned()) else { continue };
        let media = rect(get(&d, "MediaBox").cloned().and_then(|o| pdf.resolve(&o))).or(media);
        let crop = rect(get(&d, "CropBox").cloned().and_then(|o| pdf.resolve(&o))).or(crop);
        let rot = get(&d, "Rotate").and_then(Obj::num).map(|r| r as i32).unwrap_or(rot);
        if let Some(Obj::Array(kids)) = get(&d, "Kids").cloned().and_then(|k| pdf.resolve(&k)) {
            for k in kids.into_iter().rev() {
                stack.push((k, media, crop, rot, depth + 1));
            }
            continue;
        }
        let b = |k: &str, pdf: &mut Pdf<'_>| rect(get(&d, k).cloned().and_then(|o| pdf.resolve(&o)));
        pages.push(PdfPage {
            media: media.unwrap_or([0.0, 0.0, 612.0, 792.0]),
            crop,
            bleed: b("BleedBox", pdf),
            trim: b("TrimBox", pdf),
            art: b("ArtBox", pdf),
            rotate: rot.rem_euclid(360),
        });
        if let Some(c) = get(&d, "Contents") {
            contents.push(c.clone());
        }
        if get(&d, "Group").is_some() {
            transparency = true;
        }
    }
    if pages.is_empty() {
        return Err(ProbeError::Malformed("PDF without pages".into()));
    }
    Ok(PageTree { pages, contents, transparency })
}

fn rect(o: Option<Obj>) -> Option<[f64; 4]> {
    match o? {
        Obj::Array(a) if a.len() == 4 => {
            let v: Vec<f64> = a.iter().filter_map(Obj::num).collect();
            (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
        }
        _ => None,
    }
}
