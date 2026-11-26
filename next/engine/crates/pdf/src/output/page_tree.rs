//! Writes page streams, page dictionaries, and the root of the page tree.

use super::{RefTables, Refs, write_resources};
use crate::content::ResUse;
use crate::pages::PageOut;
use crate::registry::{FormDef, Registry};
use pdf_writer::{Filter, Name, Pdf, Ref, TextStr};

pub(super) struct PageTree<'a> {
    pub(super) root: Ref,
    pub(super) pages: &'a [PageOut],
    pub(super) content: &'a [Vec<u8>],
    pub(super) image_alpha: &'a [bool],
    pub(super) document_id: Option<&'a str>,
    pub(super) flattening: bool,
}

pub(super) fn write_page_tree(
    pdf: &mut Pdf,
    refs: &mut Refs,
    reg: &Registry<'_>,
    refs_tables: &RefTables<'_>,
    tree: PageTree<'_>,
) -> usize {
    let PageTree { root: pages_ref, pages: outs, content: page_data, image_alpha, document_id, flattening } = tree;
    let page_refs: Vec<(Ref, Ref)> = outs.iter().map(|_| (refs.next(), refs.next())).collect();
    let needs_group = |used: &ResUse, direct: bool| -> bool {
        direct
            || used.images.iter().any(|i| image_alpha.get(*i).copied().unwrap_or(false))
            || used.forms.iter().any(|f| match &reg.forms[*f] {
                FormDef::Group { .. } | FormDef::Mask { .. } => true,
                FormDef::Flat(_) => false,
                FormDef::Imported(i) => reg.graphics.imported_transparency(*i),
            })
    };
    let mut transparent_pages = 0;
    for (i, out) in outs.iter().enumerate() {
        let (page_ref, content_ref) = page_refs[i];
        pdf.stream(content_ref, &page_data[i]).filter(Filter::FlateDecode);
        // PDF 1.3 has no transparency groups
        let group = !flattening && needs_group(&out.used, out.transparency);
        if group {
            transparent_pages += 1;
        }
        let mut p = pdf.page(page_ref);
        p.parent(pages_ref);
        let [w, h] = out.media;
        p.media_box(pdf_writer::Rect::new(0.0, 0.0, w as f32, h as f32));
        p.crop_box(pdf_writer::Rect::new(0.0, 0.0, w as f32, h as f32));
        let [b0, b1, b2, b3] = out.bleed;
        p.bleed_box(pdf_writer::Rect::new(b0 as f32, b1 as f32, b2 as f32, b3 as f32));
        let [t0, t1, t2, t3] = out.trim;
        p.trim_box(pdf_writer::Rect::new(t0 as f32, t1 as f32, t2 as f32, t3 as f32));
        p.contents(content_ref);
        // PDF Comments: the page names its document (InDesign's PieceInfo; the importer compares OriginalDocumentID)
        if let Some(id) = document_id {
            let mut piece = p.insert(Name(b"PieceInfo")).dict();
            let mut app = piece.insert(Name(b"InDesign")).dict();
            app.pair(Name(b"DocumentID"), TextStr(id));
            app.pair(Name(b"OriginalDocumentID"), TextStr(id));
        }
        if group {
            let mut g = p.group();
            g.transparency();
            g.color_space().device_cmyk();
        }
        write_resources(p.resources(), &out.used, refs_tables);
    }
    pdf.pages(pages_ref).kids(page_refs.iter().map(|(p, _)| *p)).count(page_refs.len() as i32);
    transparent_pages
}
