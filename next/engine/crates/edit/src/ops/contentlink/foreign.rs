//! Content links to an object of another document of the same project (conveyor-c4).
//!
//! InDesign 2025 (artifacts/conveyor-c3-20261023 probes b3/b4, conveyor-c4 probe b5): placing an object of a saved document into
//! another one with Create Link gives the copy a `Link` with `LinkResourceURI="idexternalSharedArticle:<path of the document>?<n>"` (a
//! story: `idexternalcontent:`), format `External Linked Object` / `External Linked Story`; the source document gets
//! `LinkResourceId="<n>"` on the object (and its story). The state follows the source *file*: with the source closed the link
//! is out of date when the object in the file changed, Update Link takes it from the file, a moved file is `LINK_MISSING`.
//!
//! Folio (principle 1: no address of this computer in a project): the document is a reference of the placing document's manifest
//! (`folio:path-ref/<id>`: root-relative path, the document's id; the path of this computer only appears in an IDML export). The server
//! (`xlink.rs`) opens the source `.folio` briefly, writes the link numbers into it ([`mark_sources`], the command `markLinkSources`
//! run on that document's own editor, written atomically and never under someone who has it open) and hands the numbers and states to
//! the editor of the placing document ([`ForeignSource`]); a link's status reads the source document the same way ([`Foreign`]).
use super::*;
use std::sync::Arc;

/// One object of the source document (and, for a text frame, its story) as the link to it needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct ForeignMark {
    /// The object's `Self` in the source document.
    pub id: String,
    /// Its `LinkResourceId` there (0: it has none).
    pub resource: u64,
    /// The hash of its state (what the placed copy was made from, see `state`).
    pub stamp: u32,
    /// A text frame's story: its `LinkResourceId` and state hash.
    pub story: Option<(u64, u32)>,
    /// A group: the `LinkResourceId` of each member in document order (the copy's members name them).
    pub members: Vec<u64>,
}

/// What the editor of the placing document needs to link what it pastes to objects of another document.
#[derive(Clone, Debug, PartialEq)]
pub struct ForeignSource {
    /// The reference to the source document as the `Link` writes it (`folio:path-ref/r3`).
    pub document: String,
    pub marks: Vec<ForeignMark>,
}

/// The document a link names, as the reader of a link's status finds it.
#[derive(Clone, Debug)]
pub enum Foreign {
    /// Not a document of this project (an address from an IDML import): the state is not known.
    Unknown,
    /// A document of the project that is not there any more.
    Missing,
    /// The document (its file name for the Links panel, its parts).
    Doc { name: String, parts: Arc<BTreeMap<String, String>> },
}

/// The element `id` of a document's parts.
fn element_in(parts: &BTreeMap<String, String>, id: &str) -> Option<String> {
    let pat = format!(" Self=\"{id}\"");
    for (_, t) in parts.iter().filter(|(n, t)| n.ends_with(".xml") && t.contains(&pat)) {
        let Ok(ix) = Index::scan(t) else { continue };
        if let Some(i) = ix.by_self(t, id) {
            return Some(t[ix.nodes[i].start..ix.nodes[i].end].to_owned());
        }
    }
    None
}

/// `(LinkResourceId or 0, state hash)` of an element.
fn resource_and_state(xml: &str) -> (u64, u32) {
    let resource = Index::scan(xml).ok().and_then(|ix| ix.attr(xml, 0, "LinkResourceId").and_then(|v| v.parse().ok())).unwrap_or(0);
    (resource, hash32(&state(xml)))
}

/// The link numbers and state hashes of the objects `ids` (and the stories of their text frames) in `parts`.
pub fn marks(parts: &BTreeMap<String, String>, ids: &[String]) -> Result<Vec<ForeignMark>, EditError> {
    let mut out = Vec::new();
    for id in ids {
        let xml = element_in(parts, id).ok_or_else(|| EditError::new("notFound", format!("{id} is not in the document")))?;
        let (resource, stamp) = resource_and_state(&xml);
        let story = Index::scan(&xml).ok().filter(|ix| ix.local(&xml, 0) == "TextFrame")
            .and_then(|ix| ix.attr(&xml, 0, "ParentStory").filter(|s| *s != "n").map(str::to_owned))
            .and_then(|s| element_in(parts, &s))
            .map(|sx| resource_and_state(&sx));
        let members = match Index::scan(&xml) {
            Ok(ix) => (1..ix.nodes.len()).filter(|&k| ITEMS.contains(&ix.local(&xml, k)) && ix.attr(&xml, k, "Self").is_some())
                .map(|k| ix.attr(&xml, k, "LinkResourceId").and_then(|v| v.parse().ok()).unwrap_or(0)).collect(),
            Err(_) => Vec::new(),
        };
        out.push(ForeignMark { id: id.clone(), resource, stamp, story, members });
    }
    Ok(out)
}

/// The source side of a link (command `markLinkSources`, run on the source document's own editor): every object gets a
/// `LinkResourceId`, and the story of a text frame too, unless it has one (a second link to the same object shares it).
pub(crate) fn mark_sources(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let story = {
            let (sp, six, si) = w.find(id)?;
            let st = w.text(&sp)?;
            (six.local(st, si) == "TextFrame").then(|| six.attr(st, si, "ParentStory").filter(|s| *s != "n").map(str::to_owned)).flatten()
        };
        mark_source(w, id)?;
        mark_members(w, id)?;
        if let Some(story) = story {
            mark_source(w, &story)?;
        }
    }
    Ok(())
}

/// Create Link for objects pasted from another document: each placed object (and the story of a text frame) gets an external
/// `Link` with the numbers and states [`ForeignSource`] holds. The placing document's own objects get no `LinkResourceId`.
pub(super) fn link_foreign(w: &mut Work<'_>, ids: &[String], placed: &[String], map_styles: bool, date: &str) -> Result<(), EditError> {
    let source = w.ed.foreign_source.clone().ok_or_else(|| {
        EditError::new("unsupported", "a content link to an object of another document needs that document to be one of this project (the link numbers come from it)")
    })?;
    // check everything before changing anything
    for (src, dst) in ids.iter().zip(placed) {
        let m = source.marks.iter().find(|m| &m.id == src).ok_or_else(|| EditError::new("invalid", format!("{src}: no link number from the source document")))?;
        if m.resource == 0 {
            return Err(EditError::new("invalid", format!("{src}: the source document gave it no link number")));
        }
        w.find(dst)?;
    }
    let mut made = Vec::new();
    for (src, dst) in ids.iter().zip(placed) {
        let m = source.marks.iter().find(|m| &m.id == src).cloned().expect("checked above");
        strip_resource_id(w, dst)?;
        strip_member_ids(w, dst)?;
        made.push((add_link_to(w, dst, false, Some(&source.document), m.resource, m.stamp, map_styles, date)?, dst.clone()));
        name_sources(w, dst, &m.members)?;
        if let Some((resource, stamp)) = m.story {
            let (dp, dix, di) = w.find(dst)?;
            let dt = w.text(&dp)?;
            let placed_story = dix.attr(dt, di, "ParentStory").filter(|s| *s != "n").map(str::to_owned);
            if let (Some(placed_story), true) = (placed_story, resource != 0) {
                strip_resource_id(w, &placed_story)?;
                made.push((add_link_to(w, &placed_story, true, Some(&source.document), resource, stamp, map_styles, date)?, placed_story));
            }
        }
    }
    record_stamps(w, &made)
}

/// The document an external link names: `idexternalSharedArticle:<document>?<n>` → `<document>`.
pub fn external_document_of(uri: &str) -> Option<&str> {
    external_document(uri)
}

pub(super) fn external_document(uri: &str) -> Option<&str> {
    let rest = [EXTERNAL_OBJECT_PREFIX, EXTERNAL_STORY_PREFIX].into_iter().find_map(|p| uri.strip_prefix(p))?;
    rest.rsplit_once('?').map(|(d, _)| d)
}

/// What a link to the document `name`'s object `n` is: `normal` / `outOfDate` by the stamp it was made with, `missing` when the
/// object is not in the document any more; with its name (`<file>#<name>`) and the source's id.
pub(super) fn status_in(parts: &BTreeMap<String, String>, doc_name: &str, story: bool, n: u64, stamp: Option<u32>) -> (&'static str, Option<(String, String, Option<&'static str>)>) {
    let Some((part, ix, k)) = find_source_in(parts, story, n) else { return ("missing", None) };
    let t = &parts[&part];
    let xml = &t[ix.nodes[k].start..ix.nodes[k].end];
    let status = if stamp == Some(hash32(&state(xml))) { "normal" } else { "outOfDate" };
    let source = ix.attr(t, k, "Self").unwrap_or_default().to_owned();
    let (inner, _) = if story {
        frame_of_story(parts, &source).map_or((String::new(), None), |(_, frame)| element_name(parts, &frame))
    } else {
        element_name(parts, xml)
    };
    (status, Some((source, format!("{doc_name}#{inner}"), None)))
}

/// The parts of the document an external link names, as the host gave them (`Editor::set_foreign_documents`).
pub(super) fn document_of(w: &Work<'_>, f: &Found) -> Result<Arc<BTreeMap<String, String>>, EditError> {
    let t = w.text(&f.part)?;
    let uri = unescape(f.ix.attr(t, f.link, "LinkResourceURI").unwrap_or_default());
    let doc = external_document(&uri).ok_or_else(|| EditError::new("invalid", "not a link to another document"))?;
    w.ed.foreign_docs.get(doc).cloned().ok_or_else(|| {
        EditError::new("notFound", "the source document of the link is not available (it is not a document of this project, or its file is gone)")
    })
}

/// A link to an object of another document that InDesign made and an import kept (its `LinkImportStamp` is a number of InDesign's own,
/// the address of the other computer a hint) is joined to the project document the server found for it (`document` is the reference
/// the `Link` writes, its parts are given by the host, `set_foreign_documents`): the links that name it name the reference of that document
/// (`to`) and get the stamp Folio compares by, so their state is read from that document like any link of the project (conveyor-c5).
///
/// InDesign wrote `LinkResourceModified` with the file: `false` says the placed object was up to date with its source then, so the
/// source in the project document (an import of that very file) is the baseline; `true` says the source had changed, so the stamp is
/// one no source has and the link shows out of date until Update Link takes the source. A link whose source object is not in the
/// document keeps what it has (its state becomes `missing`). Nothing else of the link or the objects changes.
pub(crate) fn reconnect(w: &mut Work<'_>, document: &str, _target: &Option<String>, to: &Option<String>) -> Result<(), EditError> {
    let to = to.as_deref().ok_or_else(|| EditError::new("invalid", "the reference of the project document (`to`) is needed"))?;
    let parts = w.ed.foreign_docs.get(document).cloned().ok_or_else(|| {
        EditError::new("notFound", "the document to join the links to is not available (it is not a document of this project, or its file is gone)")
    })?;
    let names: Vec<String> = w.ed.parts.keys().filter(|n| n.starts_with("Spreads/") || n.starts_with("Stories/")).cloned().collect();
    let mut joined = 0;
    for name in names {
        let t = w.text(&name)?.to_owned();
        if !t.contains("idexternal") {
            continue;
        }
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for k in (0..ix.nodes.len()).filter(|&k| is_content_link(&ix, &t, k)) {
            let uri = unescape(ix.attr(&t, k, "LinkResourceURI").unwrap_or_default());
            let Some((story, n, true)) = content_uri(&uri) else { continue };
            if external_document(&uri) != Some(document) {
                continue;
            }
            let Some((part, six, si)) = find_source_in(&parts, story, n) else { continue };
            let st = &parts[&part];
            let h = hash32(&state(&st[six.nodes[si].start..six.nodes[si].end]));
            let stamp = if ix.attr(&t, k, "LinkResourceModified") == Some("true") { !h } else { h };
            // the link names the reference of the project document from now on (the address of the .indd stays the hint of the old one)
            edits.push(attr_edit(&ix, &t, k, "LinkResourceURI", &uri.replacen(document, to, 1)));
            edits.push(attr_edit(&ix, &t, k, "LinkImportStamp", &stamp.to_string()));
            joined += 1;
        }
        if !edits.is_empty() {
            w.splice(&name, edits)?;
        }
    }
    if joined == 0 {
        return Err(EditError::new("notFound", "no link of the document names that document (or none of their sources is in it)"));
    }
    Ok(())
}
