//! Every link of a document resolved once, compared with what IDML recorded, and probed.

use crate::{FileIndex, How, ImageInfo, LinkSearch, LinkStatus, ProbeError, probe, resolve};
use folio_model::{Document, Id, ItemKind, LinkInfo, PageItem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

/// Seconds a file time may differ from the recorded one (FAT / network rounding).
const TIME_TOLERANCE: f64 = 2.0;

/// The found file differs from what InDesign recorded when the link was last updated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modified {
    pub recorded_size: Option<u64>,
    pub actual_size: u64,
    pub recorded_mtime_unix: Option<f64>,
    pub actual_mtime_unix: Option<f64>,
    pub size_differs: bool,
    pub time_differs: bool,
}

/// One placed file (graphic, or movie / sound).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkEntry {
    /// The graphic's id (`Graphic::id`), or the movie / sound item's id.
    pub owner: Id,
    /// IDML element of the owner: Image, PDF, EPS, ImportedPage, Movie, Sound, …
    pub element: String,
    /// What IDML recorded.
    pub recorded: LinkInfo,
    /// Stored in the document (InDesign "Embedded"); no file is needed.
    pub embedded: bool,
    /// Where the file is now; `None` for embedded links (not looked up).
    pub status: Option<LinkStatus>,
    /// Set when the found file's size or time differs from the recorded ones.
    pub modified: Option<Modified>,
    /// For ambiguous links: candidates whose size (and time, when recorded) equal the
    /// recorded ones — evidence for a manual choice; never chosen automatically.
    #[serde(default)]
    pub identity_matches: Vec<PathBuf>,
    /// Header facts of the found file.
    pub info: Option<Result<ImageInfo, ProbeError>>,
}

impl LinkEntry {
    pub fn path(&self) -> Option<&Path> {
        self.status.as_ref().and_then(LinkStatus::path)
    }

    /// One word for the link's state, as the links panel and the package report show it:
    /// embedded, found, modified (found, but size or time differ from the record),
    /// ambiguous or missing.
    pub fn state_word(&self) -> &'static str {
        if self.embedded {
            return "embedded";
        }
        match &self.status {
            Some(LinkStatus::Found { .. }) if self.modified.as_ref().is_some_and(|m| m.size_differs || m.time_differs) => "modified",
            Some(LinkStatus::Found { .. }) => "found",
            Some(LinkStatus::Ambiguous(_)) => "ambiguous",
            Some(LinkStatus::Missing) | None => "missing",
        }
    }

    /// How a found file was located: recorded (the original path), document (relative to
    /// the document folder), links (the document's Links folder), folder (search folder,
    /// with shared folder names) or name (search folder, by name only).
    pub fn how_word(&self) -> Option<&'static str> {
        match &self.status {
            Some(LinkStatus::Found { how, .. }) => Some(match how {
                How::Original => "recorded",
                How::DocumentRelative => "document",
                How::LinksFolder => "links",
                How::Search { score } if *score > 0 => "folder",
                How::Search { .. } => "name",
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkSummary {
    pub links: usize,
    pub unique_uris: usize,
    pub embedded: usize,
    pub original: usize,
    pub document_relative: usize,
    pub links_folder: usize,
    /// Index matches with at least one shared folder / by unique name only.
    pub search_with_folder: usize,
    pub search_name_only: usize,
    pub ambiguous: usize,
    pub missing: usize,
    pub modified: usize,
    pub probed: usize,
    pub probe_failed: usize,
    pub resolve_ms: f64,
    pub probe_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkReport {
    pub search: LinkSearch,
    pub links: Vec<LinkEntry>,
    pub summary: LinkSummary,
}

fn collect<'a>(item: &'a PageItem, out: &mut Vec<(&'a str, &'a str, &'a LinkInfo)>) {
    item.walk(&mut |i| {
        match &i.kind {
            ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => {
                if let Some(l) = &g.link {
                    out.push((&g.id, &g.element, l));
                }
            }
            ItemKind::Other { element } => {
                if let Some(l) = &i.print.media_link {
                    out.push((&i.id, element, l));
                }
                if let Some(g) = &i.print.poster
                    && let Some(l) = &g.link
                {
                    out.push((&g.id, &g.element, l));
                }
            }
            _ => {}
        }
    });
}

/// Every link of `doc` with its owner id and element, in document order (masters, spreads,
/// then items anchored in text).
pub fn document_links(doc: &Document) -> Vec<(&str, &str, &LinkInfo)> {
    let mut out = Vec::new();
    for s in doc.master_spreads.iter().chain(&doc.spreads) {
        for i in &s.items {
            collect(i, &mut out);
        }
    }
    for st in doc.stories.values() {
        for r in st.all_runs() {
            for i in &r.inline {
                collect(i, &mut out);
            }
        }
    }
    out
}

/// Link owner id → id of the page item that shows it (the frame holding a graphic, or the
/// media item itself), for every link [`document_links`] lists.
pub fn owner_frames(doc: &Document) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut visit = |item: &PageItem| {
        item.walk(&mut |i| match &i.kind {
            ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => {
                out.insert(g.id.clone(), i.id.clone());
            }
            ItemKind::Other { .. } => {
                out.insert(i.id.clone(), i.id.clone());
                if let Some(g) = &i.print.poster {
                    out.insert(g.id.clone(), i.id.clone());
                }
            }
            _ => {}
        });
    };
    for s in doc.master_spreads.iter().chain(&doc.spreads) {
        for i in &s.items {
            visit(i);
        }
    }
    for st in doc.stories.values() {
        for r in st.all_runs() {
            for i in &r.inline {
                visit(i);
            }
        }
    }
    out
}

type FileIdentity = (u64, Option<f64>);

fn file_identity(p: &Path) -> Option<FileIdentity> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.len(), m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64())))
}

fn modified(recorded: &LinkInfo, (size, mtime): FileIdentity) -> Option<Modified> {
    let size_differs = recorded.size.is_some_and(|s| s != size);
    let time_differs = match (recorded.modified_unix, mtime) {
        (Some(r), Some(a)) => (r - a).abs() > TIME_TOLERANCE,
        _ => false,
    };
    (size_differs || time_differs).then_some(Modified {
        recorded_size: recorded.size,
        actual_size: size,
        recorded_mtime_unix: recorded.modified_unix,
        actual_mtime_unix: mtime,
        size_differs,
        time_differs,
    })
}

fn identity_matches(recorded: &LinkInfo, candidates: &[PathBuf], identity: &mut impl FnMut(&Path) -> Option<FileIdentity>) -> Vec<PathBuf> {
    if recorded.size.is_none() {
        return Vec::new();
    }
    candidates.iter().filter(|c| identity(c).is_some_and(|facts| modified(recorded, facts).is_none())).cloned().collect()
}

impl LinkReport {
    /// Resolves (once per URI), compares and probes every link of `doc`.
    pub fn build(doc: &Document, search: &LinkSearch, index: &FileIndex) -> LinkReport {
        let t = Instant::now();
        let found = document_links(doc);
        let mut by_uri: HashMap<&str, LinkStatus> = HashMap::new();
        for (_, _, l) in &found {
            if !l.embedded() && !by_uri.contains_key(l.uri.as_str()) {
                by_uri.insert(&l.uri, resolve(&l.uri, search, index));
            }
        }
        let mut by_path: HashMap<PathBuf, Option<FileIdentity>> = HashMap::new();
        let mut identity = |path: &Path| {
            if let Some(facts) = by_path.get(path) {
                return *facts;
            }
            let facts = file_identity(path);
            by_path.insert(path.to_path_buf(), facts);
            facts
        };
        let mut links: Vec<LinkEntry> = found
            .iter()
            .map(|(owner, element, l)| {
                let embedded = l.embedded();
                let status = (!embedded).then(|| by_uri.get(l.uri.as_str()).cloned().unwrap_or(LinkStatus::Missing));
                let modified = status.as_ref().and_then(LinkStatus::path).and_then(&mut identity).and_then(|facts| modified(l, facts));
                let matches = match &status {
                    Some(LinkStatus::Ambiguous(c)) => identity_matches(l, c, &mut identity),
                    _ => Vec::new(),
                };
                LinkEntry {
                    owner: (*owner).to_owned(),
                    element: (*element).to_owned(),
                    recorded: (*l).clone(),
                    embedded,
                    status,
                    modified,
                    identity_matches: matches,
                    info: None,
                }
            })
            .collect();
        let resolve_ms = t.elapsed().as_secs_f64() * 1000.0;

        // Probe each found file once, in parallel (results are cached per file identity).
        let t = Instant::now();
        let mut paths: Vec<PathBuf> = links.iter().filter_map(|e| e.path().map(Path::to_path_buf)).collect();
        paths.sort();
        paths.dedup();
        let threads = std::thread::available_parallelism().map(|n| n.get().clamp(1, 8)).unwrap_or(4);
        let chunk = paths.len().div_ceil(threads).max(1);
        let probed: HashMap<PathBuf, Result<ImageInfo, ProbeError>> = std::thread::scope(|s| {
            let hs: Vec<_> = paths.chunks(chunk).map(|c| s.spawn(move || c.iter().map(|p| (p.clone(), probe(p))).collect::<Vec<_>>())).collect();
            hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        for e in &mut links {
            if let Some(p) = e.path() {
                e.info = probed.get(p).cloned();
            }
        }
        let probe_ms = t.elapsed().as_secs_f64() * 1000.0;

        let mut s = LinkSummary { links: links.len(), unique_uris: by_uri.len(), resolve_ms, probe_ms, ..LinkSummary::default() };
        for e in &links {
            if e.embedded {
                s.embedded += 1;
            }
            match &e.status {
                Some(LinkStatus::Found { how, .. }) => match how {
                    How::Original => s.original += 1,
                    How::DocumentRelative => s.document_relative += 1,
                    How::LinksFolder => s.links_folder += 1,
                    How::Search { score: 0 } => s.search_name_only += 1,
                    How::Search { .. } => s.search_with_folder += 1,
                },
                Some(LinkStatus::Ambiguous(_)) => s.ambiguous += 1,
                Some(LinkStatus::Missing) => s.missing += 1,
                None => {}
            }
            if e.modified.is_some() {
                s.modified += 1;
            }
            match &e.info {
                Some(Ok(_)) => s.probed += 1,
                Some(Err(_)) => s.probe_failed += 1,
                None => {}
            }
        }
        LinkReport { search: search.clone(), links, summary: s }
    }

    /// The entry of a graphic (or movie / sound item).
    pub fn entry(&self, owner: &str) -> Option<&LinkEntry> {
        self.links.iter().find(|e| e.owner == owner)
    }

    /// The file a graphic uses, when found.
    pub fn path_for(&self, owner: &str) -> Option<&Path> {
        self.entry(owner).and_then(LinkEntry::path)
    }

    /// Entries by owner id.
    pub fn by_owner(&self) -> HashMap<&str, &LinkEntry> {
        self.links.iter().map(|e| (e.owner.as_str(), e)).collect()
    }
}
