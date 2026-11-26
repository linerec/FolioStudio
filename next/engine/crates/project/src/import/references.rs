//! Builds the manifest references and the edits that link IDML elements to them.

use super::files::Placed;
use super::planning::graphics_of;
use super::{Plan, label_value};
use crate::format::{self, GraphicRefs, RefKind, Reference};
use folio_model::Document;
use std::collections::HashMap;

pub(super) struct References {
    pub(super) references: Vec<Reference>,
    pub(super) doc_id: String,
    pub(super) gtable: Vec<GraphicRefs>,
    pub(super) edits_labels: HashMap<String, Vec<(String, String)>>,
    pub(super) edits_links: HashMap<String, String>,
    pub(super) uri_refs: HashMap<String, String>,
}

pub(super) fn assemble(doc: &Document, plan: &Plan, placed: &[Placed]) -> References {
    let mut references: Vec<Reference> = Vec::new();
    let mut ref_of_item: Vec<Option<usize>> = vec![None; plan.items.len()];
    for (i, it) in plan.items.iter().enumerate() {
        let pl = &placed[i];
        let r = Reference {
            id: format!("r{}", references.len() + 1),
            kind: it.kind,
            path: pl.path.clone(),
            sha256: pl.sha.clone(),
            bytes: pl.bytes,
            hint: it.hint.clone(),
            original_name: it.original_name.clone(),
            status: it.status.map(str::to_string),
            font: it.font.clone(),
            extra: Default::default(),
        };
        ref_of_item[i] = Some(references.len());
        references.push(r);
    }
    let doc_id = crate::util::uuid_v4();
    let mut gtable: Vec<GraphicRefs> = Vec::new();
    let mut edits_labels: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut edits_links: HashMap<String, String> = HashMap::new();
    let rref = |i: Option<usize>| i.and_then(|i| ref_of_item[i]).map(|r| &references[r]);
    let frames = folio_links::owner_frames(doc);
    for g in &plan.graphics {
        let folio_id = format!("g{}", crate::util::short_id());
        let (link, preview, emb) = (rref(g.link), rref(g.preview), rref(g.embedded));
        if let (Some(ls), Some(l)) = (&g.link_self, link) {
            edits_links.insert(ls.clone(), format!("{}{}", format::REF_SCHEME, l.id));
        } else if let (Some(ls), Some(e)) = (&g.link_self, emb) {
            // an embedded link: its URI is the extracted file's reference
            edits_links.insert(ls.clone(), format!("{}{}", format::REF_SCHEME, e.id));
        }
        // on the frame (InDesign keeps frame labels through IDML; graphic labels it drops)
        let carrier = frames.get(&g.self_id).cloned().unwrap_or_else(|| g.self_id.clone());
        edits_labels.insert(carrier.clone(), vec![(format::LABEL_REF.into(), label_value(&folio_id, &[("link", link), ("embedded", emb), ("preview", preview)]))]);
        gtable.push(GraphicRefs {
            self_id: g.self_id.clone(),
            folio_id,
            frame: Some(carrier),
            link_self: g.link_self.clone(),
            link: link.map(|r| r.id.clone()).or_else(|| g.link_self.as_ref().and(emb).map(|r| r.id.clone())),
            preview: preview.map(|r| r.id.clone()),
            embedded: emb.map(|r| r.id.clone()),
        });
    }
    let uri_refs: HashMap<String, String> = plan.uris.iter().filter_map(|(u, &i)| rref(Some(i)).map(|r| (u.clone(), r.id.clone()))).collect();
    // embedded links whose file was not extracted (buffered INDD, IDML: the file is in
    // Contents) keep pointing at their original location, as a hint reference
    for (_, link) in graphics_of(doc) {
        if let Some(l) = link
            && l.embedded()
            && !edits_links.contains_key(&l.id)
        {
            let id = format!("r{}", references.len() + 1);
            references.push(Reference { id: id.clone(), kind: RefKind::Embedded, path: None, sha256: None, bytes: None, hint: Some(l.uri.clone()), original_name: None, status: Some("embedded".into()), font: None, extra: Default::default() });
            edits_links.insert(l.id.clone(), format!("{}{}", format::REF_SCHEME, id));
            if let Some(gr) = gtable.iter_mut().find(|g| g.link_self.as_deref() == Some(l.id.as_str())) {
                gr.link = Some(id);
            }
        }
    }
    References { references, doc_id, gtable, edits_labels, edits_links, uri_refs }
}
