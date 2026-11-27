//! Read-only inspection for the editor's canvas and panels.
//!
//! `{"cmd":"documentInfo"}`: units, ruler origin, facing pages, layers, swatches, masters,
//! pages and styles. `{"cmd":"pageItems","page":N}`: one document page's size, margins,
//! columns, guides, items and master items with their geometry in page coordinates (points
//! from the page's top-left, y down: the convention of preflight `bounds` and render
//! regions).
//!
//! Draw order follows `folio_render::render_page`: the applied master's items, then the
//! spread's, each list stably sorted by layer; a group's members and a frame's pasted items
//! the same way inside their parent. `z` numbers that order over the whole spread: the
//! master items of each of the spread's pages (in page order), then the spread's items,
//! parents before their members.
//!
//! Nothing here changes the document or lays text out again; facts every request needs are
//! gathered once per document ([`Index`]). Replies are written as JSON text straight from
//! typed records (a page of detailed vector art lists thousands of items).

mod document;
mod geometry;
mod items;
mod page;
mod swatches;

#[cfg(test)]
mod tests;

use crate::wire;
use document::document_info;
use folio_color::Colors;
use folio_model::layout::DocumentLayout;
use folio_model::{Document, PageItem};
pub(crate) use items::{paint_value, text_frame_dialog_facts};
use page::page_items;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Facts shared by every request of one document.
pub struct Index {
    /// The `documentInfo` reply body, as JSON text.
    info: String,
    /// Stories with text that did not fit their frames.
    overset: HashSet<String>,
    /// Stories without a character (Object ▸ Content ▸ Graphic / Unassigned of their frame).
    empty: HashSet<String>,
    /// Vertical stories (`Story::vertical`, Type ▸ Type Orientation).
    vertical: HashSet<String>,
    /// Page index → (spread index, position in the spread): the document's spreads, then the master spreads
    /// (`MASTER_PAGE_BASE` and up) counted on from them.
    pages: HashMap<usize, (usize, usize)>,
    layers: HashMap<String, LayerFacts>,
    facing: bool,
    anchors: std::collections::BTreeMap<String, Vec<PageItem>>,
    anchored_ids: HashSet<String>,
}

#[derive(Clone, Copy)]
struct LayerFacts {
    order: usize,
    visible: bool,
    locked: bool,
    lock_guides: bool,
}

/// The reply header of an inspect command as JSON object text, without `id`, `ok` and
/// `bodyBytes`; `ms` is the time taken, writing the text included.
pub fn command(req: &Value, doc: &Document, layout: &DocumentLayout, colors: &Colors, index: &std::sync::OnceLock<Index>) -> Result<String, String> {
    let started = Instant::now();
    let index = index.get_or_init(|| Index::new(doc, layout, colors));
    let mut reply = match req["cmd"].as_str().unwrap_or("") {
        "documentInfo" => format!(r#"{{"type":"documentInfo","info":{}"#, index.info),
        "pageItems" => {
            let page = wire::required_index(req, "page", "pageItems needs a page number")?;
            let items = page_items(doc, colors, index, page)?;
            let mut text = br#"{"type":"pageItems","items":"#.to_vec();
            serde_json::to_writer(&mut text, &items).map_err(|e| e.to_string())?;
            String::from_utf8(text).map_err(|e| e.to_string())?
        }
        other => return Err(format!("unknown cmd {other}")),
    };
    reply.push_str(&format!(r#","ms":{}}}"#, round(started.elapsed().as_secs_f64() * 1000.0, 3)));
    Ok(reply)
}

impl Index {
    pub fn new(doc: &Document, layout: &DocumentLayout, colors: &Colors) -> Index {
        let facing = if doc.print.facing_pages_stored { doc.print.facing_pages } else { doc.spreads.iter().any(|s| s.pages.len() >= 2) };
        let mut pages = HashMap::new();
        // the document's pages first: a master page whose number is taken (only in a document built by hand) stays unlisted
        for (si, s) in doc.spreads.iter().chain(&doc.master_spreads).enumerate() {
            for (pi, p) in s.pages.iter().enumerate() {
                pages.entry(p.index).or_insert((si, pi));
            }
        }
        let layers = doc
            .layers
            .iter()
            .map(|l| (l.id.clone(), LayerFacts { order: l.order, visible: l.visible, locked: l.locked, lock_guides: l.lock_guides }))
            .collect();
        let info = document_info(doc, colors, facing).to_string();
        let empty = doc.stories.iter().filter(|(_, s)| s.paragraphs.len() <= 1 && s.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty() && r.inline.is_empty()))).map(|(id, _)| id.clone()).collect();
        let anchors = folio_model::layout::anchored_items(doc, layout);
        let anchored_ids = anchors.values().flatten().map(|i| i.id.clone()).collect();
        let vertical = doc.stories.iter().filter(|(_, s)| s.vertical).map(|(id, _)| id.clone()).collect();
        Index { info, overset: layout.overset.iter().cloned().collect(), empty, vertical, pages, layers, facing, anchors, anchored_ids }
    }
}

fn round(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    // + 0.0 turns -0 into 0
    (v * k).round() / k + 0.0
}

fn r3(v: f64) -> f64 {
    round(v, 3)
}
