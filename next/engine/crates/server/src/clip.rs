//! The engine's objects clipboard: Edit ▸ Copy in one document, Paste in it or in another
//! (each document gets a new editor when it opens, so the clipboard lives here).
//!
//! Pasting in another document brings the copied graphics' files into its project as placing
//! them would (principle 1: the project's own file when it holds the same content, else a
//! copy by content hash under `assets/<document>/`; root-relative references), and the
//! swatches and styles the objects use come along in the editor (folio-edit `xdoc`, as
//! InDesign 2025 brings them).
use crate::{Session, edit::EditState, place::Prepared};
use folio_project::{format::{REF_SCHEME, RefKind}, paths};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) struct Held {
    pub(crate) clip: folio_edit::Clipboard,
    /// The document copied from.
    from: PathBuf,
    /// Each copied graphic link's file on this computer (None: not found then).
    files: BTreeMap<String, Option<PathBuf>>,
}

impl Held {
    /// The document the objects were copied in.
    pub(crate) fn from(&self) -> &std::path::Path {
        &self.from
    }

    /// `clip`, taken in `es` (of `s`), with where its graphics' files are.
    pub(crate) fn of(s: &Session, es: &EditState, clip: folio_edit::Clipboard) -> Held {
        let files = clip.link_uris().into_iter().map(|uri| {
            let file = source_file(s, es, &uri);
            (uri, file)
        }).collect();
        Held { clip, from: es.path.clone(), files }
    }
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);

#[cfg(test)]
impl Held {
    /// A clipboard that was copied in no document (the conveyor's tests).
    pub(crate) fn test(clip: folio_edit::Clipboard) -> Held {
        Held { clip, from: PathBuf::new(), files: BTreeMap::new() }
    }
}

/// After Edit ▸ Copy in `s`: what was copied, with where its graphics' files are.
pub fn hold(s: &Session, es: &EditState) {
    let Some(clip) = es.editor.clipboard() else { return };
    *HELD.lock().unwrap_or_else(|p| p.into_inner()) = Some(Held::of(s, es, clip));
}

/// The file a copied graphic links to: a `.folio`'s reference resolved in its project (a
/// graphic placed since opening: the file it was placed from); otherwise where the document's
/// links were found (its folder, its Links folder…), or the URI's own path.
pub(crate) fn source_file(s: &Session, es: &EditState, uri: &str) -> Option<PathBuf> {
    if let Some(rid) = uri.strip_prefix(REF_SCHEME) {
        let f = s.folio.as_ref()?;
        let g = es.graphics.as_ref()?;
        if let Some(o) = g.placed_sources.get(rid) {
            return folio_links::parse_uri(&o.uri)?.local_path().filter(|p| p.is_file());
        }
        let r = g.references.iter().find(|r| r.id == rid && r.kind == RefKind::Link)?;
        return paths::resolve_on_disk(&f.opened.project.root, r.path.as_ref()?);
    }
    let report = s.links();
    if let Some(found) = report.links.iter().filter(|l| !l.embedded && l.recorded.uri == uri).find_map(|l| l.status.as_ref()?.path()) {
        return Some(found.to_path_buf());
    }
    folio_links::parse_uri(uri)?.local_path().filter(|p| p.is_file())
}

/// What a Paste in the document of `cur` pastes: the engine's clipboard, its graphics linking
/// to files of this project when it was copied in another document. None: nothing copied.
pub fn for_paste(cur: &Session, es: &EditState, prepared: &mut Prepared) -> Result<Option<folio_edit::Clipboard>, String> {
    let held = HELD.lock().unwrap_or_else(|p| p.into_inner());
    let Some(h) = held.as_ref() else { return Ok(None) };
    let clip = arrive(h, cur, es, prepared)?;
    if prepared.wants_link {
        crate::xlink::prepare(h, cur, es, prepared)?;
    }
    Ok(Some(clip))
}

/// `h` as it is pasted in the document of `cur`: as copied when it is that document, else with its graphics' files
/// brought into the project.
pub(crate) fn arrive(h: &Held, cur: &Session, es: &EditState, prepared: &mut Prepared) -> Result<folio_edit::Clipboard, String> {
    if h.from == es.path {
        return Ok(h.clip.clone());
    }
    let mut map = BTreeMap::new();
    for (uri, file) in &h.files {
        let Some(file) = file else {
            return Err(format!("붙일 그림의 원본 파일을 찾지 못했습니다 ({uri}). 원본 문서에서 링크를 먼저 연결하세요"));
        };
        let new = match &cur.folio {
            Some(f) => {
                let ext = file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "bin".into());
                let (reference, _) = crate::place::import_link(file, &ext, f, es, prepared)?;
                format!("{REF_SCHEME}{}", reference.id)
            }
            None => paths::file_uri(file),
        };
        map.insert(uri.clone(), new);
    }
    Ok(h.clip.with_link_uris(&map))
}
