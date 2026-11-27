//! Map placed links to report rows and the files to collect in placement order.

use super::{FileCopy, LinkFile, LinkRow, PlacedFont};
use crate::names::NameTable;
use crate::{Inputs, LINKS_DIR, Options, file_name, fsops, page_names, protect};
use folio_links::LinkStatus;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) struct LinkPlan {
    pub(super) files: Vec<LinkFile>,
    pub(super) rows: Vec<LinkRow>,
    pub(super) nested: Vec<String>,
    pub(super) placed_fonts: Vec<PlacedFont>,
}

fn has_conjoining_jamo(s: &str) -> bool {
    s.chars().any(|c| ('\u{1100}'..='\u{11FF}').contains(&c))
}

pub(super) fn plan(inp: &Inputs, opts: &Options, compare_contents: bool) -> LinkPlan {
    let pages = page_names(inp.doc);
    let frames = folio_links::owner_frames(inp.doc);
    let mut files: Vec<LinkFile> = Vec::new();
    let mut by_path: HashMap<PathBuf, usize> = HashMap::new();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut table = NameTable::new();
    let mut rows = Vec::new();
    let mut nested: Vec<String> = Vec::new();
    let mut placed_fonts: Vec<PlacedFont> = Vec::new();
    for e in &inp.links.links {
        let name = folio_links::parse_uri(&e.recorded.uri).and_then(|l| l.file_name().map(str::to_owned)).unwrap_or_else(|| e.owner.clone());
        let fact = inp.preflight.images.get(&e.owner);
        let frame = fact.map(|f| f.frame.clone()).or_else(|| frames.get(&e.owner).cloned());
        let loc = frame.as_ref().and_then(|f| inp.locations.get(f));
        let info = e.info.as_ref().and_then(|r| r.as_ref().ok());
        let found = match &e.status {
            Some(LinkStatus::Found { path, .. }) => Some(path.as_path()),
            _ => None,
        };
        let mut row = LinkRow {
            owner: e.owner.clone(),
            element: e.element.clone(),
            name: name.clone(),
            status: e.state_word(),
            how: e.how_word(),
            source: found.map(Path::to_path_buf),
            packaged: None,
            file: None,
            page: loc.map(|l| l.page),
            page_name: loc.and_then(|l| pages.get(&l.page).cloned()),
            ppi: fact.map(|f| f.ppi[0].min(f.ppi[1])),
            space: info.map(|i| format!("{:?}", i.color_space)),
            format: info.map(|i| format!("{:?}", i.format)),
            recorded: folio_links::uri::percent_decode(&e.recorded.uri),
            candidates: match &e.status {
                Some(LinkStatus::Ambiguous(c)) => c.clone(),
                _ => Vec::new(),
            },
            nfd_name: found.is_some_and(|p| has_conjoining_jamo(&file_name(p))),
        };
        if let Some(i) = info {
            for f in i.fonts.iter().filter(|f| !f.embedded) {
                if !placed_fonts.iter().any(|p| p.link == name && p.font == f.name) {
                    placed_fonts.push(PlacedFont { link: name.clone(), font: f.name.clone(), kind: f.kind.clone() });
                }
            }
        }
        if let Some(path) = found {
            let ext = path.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "indd" | "indt" | "idml" | "ai") && !nested.contains(&name) {
                nested.push(name.clone());
            }
        }
        if let (true, Some(path)) = (opts.links, found) {
            let key = protect::normalize(path);
            let idx = match by_path.get(&key) {
                Some(&i) => i,
                None => {
                    let disk_name = file_name(path);
                    let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
                    let nk = folio_links::uri::name_key(&disk_name);
                    let same = if compare_contents {
                        by_name.get(&nk).and_then(|c| {
                            c.iter().copied().find(|&j| files[j].copy.bytes == bytes && fsops::same_content(&files[j].copy.source, path).unwrap_or(false))
                        })
                    } else {
                        None
                    };
                    let i = match same {
                        Some(j) => {
                            files[j].same_as.push(path.to_path_buf());
                            j
                        }
                        None => {
                            let packaged = table.unique(&disk_name);
                            files.push(LinkFile {
                                copy: FileCopy { source: path.to_path_buf(), dest: format!("{LINKS_DIR}/{packaged}"), bytes },
                                renamed: packaged != disk_name,
                                original_name: disk_name,
                                same_as: Vec::new(),
                                owners: Vec::new(),
                            });
                            by_name.entry(nk).or_default().push(files.len() - 1);
                            files.len() - 1
                        }
                    };
                    by_path.insert(key, i);
                    i
                }
            };
            files[idx].owners.push(e.owner.clone());
            row.file = Some(idx);
            row.packaged = Some(files[idx].copy.dest.clone());
        }
        rows.push(row);
    }
    LinkPlan { files, rows, nested, placed_fonts }
}
