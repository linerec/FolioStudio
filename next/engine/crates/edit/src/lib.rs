//! Editing a Folio document by targeted edits of its IDML parts.
//!
//! The IDML parts are the document (principle 2): an edit changes the attributes and elements
//! InDesign itself would change for the same action, and leaves every other byte alone, so
//! nothing Folio does not model can be lost. The model (`folio-idml`) is read again from the
//! edited parts. What InDesign changes for each action is measured, not guessed: the oracle
//! (`next/oracle/`) runs the same steps in InDesign 2025 and keeps its IDML, and
//! `tests/oracle.rs` checks that these commands change the same things to the same values.
//!
//! Every change is kept as byte splices, so undo and redo are exact and cheap; one call of
//! [`Editor::apply`] is one undo step, all or nothing.

/// The 0-based records a Data Merge range (`2-3`, `1, 4`; empty: all) selects out of `count`, or InDesign's refusal.
pub fn data_merge_selection(range: &str, count: usize) -> Result<Vec<usize>, EditError> { hyperlinks::merge::select(range, count) }
pub mod geom;
pub mod commands;
mod preview;
mod history;
mod part_cache;
mod ops;
mod toc;
mod hyperlinks;
mod indexes;
/// A document's printed index entries for a book-wide index made in another document of its book (the server reads them).
pub use indexes::book_entries as book_index_entries;
pub use indexes::sort_options::{read_options as index_sort_options, SortGroup as IndexSortGroup};
mod text_style_usage;
pub mod text;
pub mod xml;
pub mod book_numbers;

pub use ops::{PathfinderOp, PointKind, AlignBounds, AlignMode, Anchor, Arrange, Axis, BlendLevel, CellChange, Command, ContentKind, DiagonalLines, Corner, EdgeStroke, Effect, RowKind, Shape, Fit, FrameOptions, GuideAxis, ItemKind, StrokeChange};
pub use ops::GraphicSource;
pub use ops::place::inline::prune_unreadable as prune_unreadable_pictures;
pub use ops::{FileStyle, describe_style_file};
pub use ops::color_commands;
pub use ops::mixedink_groups;
pub use ops::{DefaultFillStroke, GroupInfo, SwatchUse, color_groups, swatch_references, unnamed_color_id, unused_swatches};
pub use ops::Adjust;
pub use ops::clipping::{ClipPath, ClipSettings};
pub use ops::contentlink::ContentLink;
pub use ops::contentlink::foreign::{Foreign, ForeignMark, ForeignSource, external_document_of, marks as link_marks};
pub use ops::{LayoutAdjustSettings, PAGE_SIZE_PRESETS, auto_margins, page_size_of_preset, page_size_preset_of};
pub use ops::TableStyleKind;
pub use ops::ObjectStyleDefinition;
pub use ops::{HandleSide, PathPt};
pub use ops::handledrag::{MIN_SIZE as HANDLE_MIN_SIZE, handle_box, handle_rect, turn_angle};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Why an edit was refused; nothing was changed.
#[derive(Clone, Debug, PartialEq)]
pub struct EditError {
    /// `notFound`, `locked`, `unsupported`, `invalid`, `xml`.
    pub code: &'static str,
    pub message: String,
}

impl EditError {
    pub fn new(code: &'static str, message: impl Into<String>) -> EditError {
        EditError { code, message: message.into() }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// One reversible change of the parts.
#[derive(Clone, Debug)]
enum Splice {
    /// `removed` at `at` became `inserted`.
    Text { part: String, at: usize, removed: String, inserted: String },
    /// A whole part appeared (`before` None) or went away (`after` None).
    Part { part: String, before: Option<String>, after: Option<String> },
}

#[derive(Clone, Debug)]
struct Txn {
    label: String,
    splices: Vec<Splice>,
    /// Stories whose saved InDesign lines no longer hold (frame size changed).
    recompose: Vec<String>,
    text_before: text::InsertionState,
    text_after: text::InsertionState,
}

/// Which step `Editor::rollback` takes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rollback {
    Apply,
    Undo,
    Redo,
}

/// What an apply, undo or redo changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Document revision after the change (every apply, undo and redo counts one).
    pub rev: u64,
    pub label: String,
    /// Parts written or removed.
    pub parts: Vec<String>,
    /// Stories to compose afresh instead of with InDesign's saved lines.
    pub recompose: Vec<String>,
    /// Objects the change made (a group, duplicates), in order; empty for undo and redo.
    pub created: Vec<String>,
}

/// The brackets of a text path after a Selection tool drag and before it (the model counts a plain horizontal line's brackets along x, so a screen preview moves
/// its own values by the difference).
#[derive(Clone, Copy, Debug)]
pub struct BracketDrag { pub start: f64, pub end: f64, pub was_start: f64, pub was_end: f64, /** the centre bracket went over the path: the text flips and the two values swap */ pub flip: bool }

/// The parts being edited and their history.
pub struct Editor {
    /// The document's file name (`issue.folio`), for what shows it (a cross-reference's File Name block); the host sets it.
    file_name: String,
    parts: BTreeMap<String, String>,
    /// Revision of every part written or removed since opening: the editor's own tick (`part_tick`) of its last change. A part not listed is
    /// as opened. What reads the parts (the host's model of the document) keeps the revisions it read and so knows which parts to read again.
    part_revs: BTreeMap<String, u64>,
    part_tick: u64,
    indexes: std::cell::RefCell<HashMap<String, part_cache::Cached>>,
    undo: Vec<Txn>,
    redo: Vec<Txn>,
    rev: u64,
    /// The document's identity for "unsaved changes": counts apply, undo and redo like `rev`, not preview steps (preview.rs).
    version: u64,
    /// The open preview session (preview.rs).
    preview: Option<preview::Preview>,
    touched: BTreeSet<String>,
    /// Stories that must not use saved lines any more (every applied change counts; undo
    /// does not bring the saved lines back, composing afresh is always correct).
    recompose: BTreeSet<String>,
    where_is: HashMap<String, String>,
    /// Next free `u…` id: after every id the package had or this session gave out, so a
    /// deleted object's id is never given again (a `.folio` manifest may still name it).
    next_uid: Option<u64>,
    text_state: text::InsertionState,
    /// Current composition's first character per frame (None: empty/overset frame).
    /// Supplied by the caller before an edit; XML alone cannot locate a thread boundary.
    frame_starts: BTreeMap<String, Option<usize>>,
    /// Current composition's character range [start, end) each frame shows (None: no text in it), supplied with `frame_starts`:
    /// scaling a text frame scales the text it shows, and only that.
    frame_spans: BTreeMap<String, Option<(usize, usize)>>,
    anchor_placements: BTreeMap<String, ops::anchored::Placement>,
    /// Lines and places an object can be anchored at (`set_anchor_targets`, only for an edit that anchors an existing object).
    anchor_targets: ops::anchored::Targets,
    /// The ids of the cells a merge covered, by (table, column, row): InDesign keeps them and
    /// unmerging gives them back.
    merged_cells: BTreeMap<(String, usize, usize), String>,
    /// Edit ▸ Copy: the document then and the objects copied.
    clipboard: Option<ops::clipboard::Clip>,
    /// Content links to another document: the numbers and states of the source objects of what the next paste pastes (set by the host
    /// before a `linkPlaced` / `placeAndLink` of objects copied in another document; not undo state).
    foreign_source: Option<ops::contentlink::foreign::ForeignSource>,
    /// The documents that content links of this document name, by the reference its `Link`s write (host-read, for an update; not undo state).
    foreign_docs: BTreeMap<String, std::sync::Arc<BTreeMap<String, String>>>,
    /// This editor's number among the documents open in the process: a paste of objects
    /// copied in another document brings their swatches and styles along.
    doc: u64,
    /// Object ▸ Transform Again: the moves, rotations, scales and flips done since the selection
    /// last changed, the last one being what plain Transform Again repeats. Not part of the
    /// undo history (InDesign keeps them across Undo).
    again: Vec<ops::Step>,
    /// The selection changed since the last of them: the next one starts a new sequence.
    again_stale: bool,
    /// Resolved link URIs supplied by the host; never written into project parts.
    link_uris: BTreeMap<String, String>,
    font_records: Vec<BTreeMap<String, String>>,
    /// Glyph tables of the faces the next glyph command uses (supplied by the host; not undo state).
    glyph_fonts: Vec<std::sync::Arc<text::GlyphFont>>,
    /// Style shortcuts: the order styles were given their shortcut in this session, by style id (InDesign: of two styles with the same shortcut the one
    /// assigned last wins; the order is not in the IDML, a reloaded document falls back to the document order). Not undo state.
    shortcut_order: BTreeMap<String, u64>,
}

/// What Edit ▸ Copy took in some document, to paste in it or in another (`Editor::clipboard`).
#[derive(Clone, Debug)]
pub struct Clipboard(ops::clipboard::Clip);

static NEXT_DOC: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Clipboard {
    /// The objects copied (their `Self` in the document they were copied in), in the order copied.
    pub fn ids(&self) -> Vec<String> {
        self.0.ids.clone()
    }

    /// The link numbers and states the copied objects (and the stories of their text frames) had in the document they were copied in, in
    /// the order copied: what a content link made from them compares the source with.
    pub fn marks(&self) -> Vec<ForeignMark> {
        ops::contentlink::foreign::marks(self.0.source.parts(), &self.0.ids).unwrap_or_default()
    }

    /// The files the copied graphics link to (`LinkResourceURI`), in order, each once.
    pub fn link_uris(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, _, uri) in ops::clipboard::links(&self.0) {
            if !out.contains(&uri) {
                out.push(uri);
            }
        }
        out
    }

    /// One clipboard per copied object (each pastes alone), sharing the document state they were copied from.
    pub fn split(&self) -> Vec<Clipboard> {
        ops::clipboard::split(&self.0).into_iter().map(Clipboard).collect()
    }

    /// The copied objects top to bottom: (their `Self` where they were copied, IDML element name, `Name`).
    pub fn summary(&self) -> Vec<(String, String, String)> {
        ops::clipboard::summary(&self.0)
    }

    /// The same objects with their graphics linking elsewhere: `map` gives a link's new URI
    /// (links it does not name stay).
    pub fn with_link_uris(&self, map: &BTreeMap<String, String>) -> Clipboard {
        Clipboard(ops::clipboard::with_links(&self.0, map))
    }
}

/// Changes made by one command, applied as they are made.
pub(crate) struct Work<'e> {
    ed: &'e mut Editor,
    splices: Vec<Splice>,
    recompose: Vec<String>,
    pub(crate) created: Vec<String>,
}

impl Editor {
    /// The document's file name as a cross-reference's File Name block shows it (InDesign: `chapter1.indd`, with its extension).
    pub fn set_file_name(&mut self, name: &str) { self.file_name = name.to_owned(); }
    pub fn file_name(&self) -> &str { &self.file_name }

    /// `parts`: every part of the IDML package by its path (`designmap.xml`,
    /// `Spreads/Spread_u1.xml`, …), text as stored.
    pub fn new(parts: BTreeMap<String, String>) -> Editor {
        Editor { file_name: String::new(), indexes: Default::default(), parts, part_revs: BTreeMap::new(), part_tick: 0, undo: Vec::new(), redo: Vec::new(), rev: 0, version: 0, preview: None, touched: BTreeSet::new(), recompose: BTreeSet::new(), where_is: HashMap::new(), next_uid: None, text_state: Default::default(), frame_starts: BTreeMap::new(), frame_spans: BTreeMap::new(), anchor_placements:BTreeMap::new(), anchor_targets: Default::default(), merged_cells: BTreeMap::new(), clipboard: None, foreign_source: None, foreign_docs: BTreeMap::new(),
            doc: NEXT_DOC.fetch_add(1, std::sync::atomic::Ordering::Relaxed), again: Vec::new(), again_stale: false, link_uris: BTreeMap::new(), font_records: Vec::new(), glyph_fonts: Vec::new(), shortcut_order: BTreeMap::new() }
    }

    /// The selection changed: the next move, rotation, scale or flip starts a new Transform Sequence
    /// Again sequence (what was done is kept until then, to be repeated on the new selection).
    pub fn selection_changed(&mut self) {
        self.again_stale = true;
    }

    /// Refresh immediately before applying commands that copy a threaded text frame.
    pub fn set_frame_starts(&mut self, starts: BTreeMap<String, Option<usize>>) {
        self.frame_starts = starts;
    }

    /// Refresh immediately before applying commands that scale a text frame: the story characters each frame shows.
    pub fn set_frame_spans(&mut self, spans: BTreeMap<String, Option<(usize, usize)>>) {
        self.frame_spans = spans;
    }

    /// The character range each frame shows in `layout` (tables' and footnotes' own lines do not count), for `set_frame_spans`.
    pub fn spans_of<'a>(frames: impl Iterator<Item = &'a str>, layout: &folio_model::layout::DocumentLayout) -> BTreeMap<String, Option<(usize, usize)>> {
        frames.map(|id| {
            let lines = layout.frames.get(id).map(|f| f.lines.as_slice()).unwrap_or_default();
            let mut own = lines.iter().filter(|l| l.cell.is_none());
            let span = own.next().map(|first| own.fold((first.start, first.end), |(s, e), l| (s.min(l.start), e.max(l.end))));
            (id.to_owned(), span)
        }).collect()
    }

    /// Installed face metadata, supplied by the host for explicit family/style changes.
    pub fn set_font_records(&mut self, records: Vec<BTreeMap<String, String>>) {
        self.font_records = records;
    }

    pub fn parts(&self) -> &BTreeMap<String, String> {
        &self.parts
    }

    /// The revision of each part written or removed since opening (see `part_revs`); a part that is not listed has not changed.
    /// Every change of a part's text (apply, undo, redo, a preview, a rollback) gives it a new, larger revision.
    pub fn part_revs(&self) -> &BTreeMap<String, u64> {
        &self.part_revs
    }

    /// Notes that `part` was written, added or removed.
    fn bump(&mut self, part: &str) {
        self.part_tick += 1;
        let tick = self.part_tick;
        self.indexes.get_mut().retain(|name, cache| name != part || cache.rev == tick);
        match self.part_revs.get_mut(part) {
            Some(r) => *r = tick,
            None => {
                self.part_revs.insert(part.to_owned(), tick);
            }
        }
    }

    fn cached_index(&self, part: &str) -> Result<xml::Index, EditError> {
        let text = self.parts.get(part).ok_or_else(|| EditError::new("notFound", format!("part {part}")))?;
        let rev = self.part_revs.get(part).copied().unwrap_or(0);
        let mut indexes = self.indexes.borrow_mut();
        if let Some(c) = indexes.get(part).filter(|c| c.rev == rev) {
            return Ok(c.index.clone());
        }
        let index = xml::Index::scan(text).map_err(|e| EditError::new("xml", format!("{part}: {e}")))?;
        indexes.insert(part.into(), part_cache::Cached::new(rev, index.clone()));
        Ok(index)
    }

    fn record_splice(&mut self, part: &str, range: std::ops::Range<usize>, new: &str) {
        let indexes = self.indexes.get_mut();
        if let Some(c) = indexes.get_mut(part)
            && !c.splice(&self.parts[part], range, new, self.part_tick + 1)
        {
            indexes.remove(part);
        }
    }

    /// Complete existing spread children covering every change since `since`, if only attribute
    /// values inside those children changed. Element structure, Self ids and page settings are unchanged.
    pub fn item_fragments(&self, part: &str, since: u64) -> Option<Vec<&str>> {
        let indexes = self.indexes.borrow();
        let c = indexes.get(part)?;
        if c.rev != self.part_revs.get(part).copied().unwrap_or(0) { return None; }
        c.fragments(self.parts.get(part)?, since)
    }

    /// The text frames of the parents, for Object ▸ Primary Text Frame: `primary` are the frames of every thread that is a parent's primary
    /// text frame (`MasterSpread@PrimaryTextFrame`, "n" for none), `empty` the frames whose story holds no text (the only ones the menu
    /// acts on).
    pub fn parent_text_frames(&self) -> (Vec<String>, Vec<String>) {
        let (mut primary, mut empty) = (Vec::new(), Vec::new());
        for (_, t) in self.parts.iter().filter(|(name, _)| name.starts_with("MasterSpreads/")) {
            let Ok(ix) = xml::Index::scan(t) else { continue };
            // the element with the attributes (the root `idPkg:MasterSpread` has the same local name)
            let Some(m) = (0..ix.nodes.len()).find(|&k| ix.local(t, k) == "MasterSpread" && ix.attr(t, k, "Self").is_some()) else { continue };
            let frames: Vec<usize> = (0..ix.nodes.len()).filter(|&k| ix.local(t, k) == "TextFrame").collect();
            for &f in &frames {
                let (Some(id), Some(story)) = (ix.attr(t, f, "Self"), ix.attr(t, f, "ParentStory")) else { continue };
                let blank = self.parts.get(&format!("Stories/Story_{story}.xml")).is_some_and(|s| text::contents(s, story).is_ok_and(|c| c.is_empty()));
                if blank {
                    empty.push(id.to_owned());
                }
            }
            // the primary thread: from the first frame along the next frames
            let mut at = ix.attr(t, m, "PrimaryTextFrame").filter(|v| *v != "n").and_then(|id| ix.by_self(t, id));
            while let Some(k) = at {
                primary.push(ix.attr(t, k, "Self").unwrap_or_default().to_owned());
                at = ix.attr(t, k, "NextTextFrame").filter(|v| *v != "n").and_then(|id| ix.by_self(t, id));
                if at.is_some_and(|n| primary.iter().any(|p| Some(p.as_str()) == ix.attr(t, n, "Self"))) { break; }
            }
        }
        (primary, empty)
    }

    /// The embedded data (base64 as the document holds it) of the graphic `id` names (a frame or the graphic): none when it is not
    /// embedded.
    pub fn embedded_data(&self, id: &str) -> Option<String> {
        ops::embedded_data(self, id)
    }

    /// The alternating pattern settings of table `id` (its own values, else its table style's, else InDesign's defaults), as IDML
    /// names and writes them; none if there is no such table.
    pub fn table_options(&self, id: &str) -> Option<BTreeMap<String, String>> {
        ops::table_options(self.parts(), id)
    }

    /// The content links of the Content Placer's Create Link, with their status (normal, out of date, source missing).
    pub fn content_links(&self) -> Vec<ContentLink> {
        ops::contentlink::list(self.parts())
    }

    /// [`Editor::content_links`] with the state of the links to objects of other documents of the project: `others` reads the document a
    /// link names (the reference as its `Link` writes it, `folio:path-ref/r3`).
    pub fn content_links_with(&self, others: &dyn Fn(&str) -> Foreign) -> Vec<ContentLink> {
        ops::contentlink::list_with(self.parts(), others)
    }

    /// Content links to another document: the source objects' numbers and states, for the next paste of what was copied there.
    pub fn set_foreign_source(&mut self, source: Option<ForeignSource>) {
        self.foreign_source = source;
    }

    /// The documents that links to other documents name (by the reference their `Link`s write), for the next update of such a link.
    pub fn set_foreign_documents(&mut self, docs: BTreeMap<String, std::sync::Arc<BTreeMap<String, String>>>) {
        self.foreign_docs = docs;
    }

    /// The document a link to another document names (`folio:path-ref/r3`), for the host to read.
    pub fn link_documents(&self, links: &[String]) -> Vec<(String, String)> {
        let all = self.content_links();
        links.iter().filter_map(|id| {
            let c = all.iter().find(|c| &c.link == id && c.external)?;
            let doc = ops::contentlink::foreign::external_document_of(&c.uri)?;
            Some((id.clone(), doc.to_owned()))
        }).collect()
    }


    pub fn part(&self, name: &str) -> Option<&str> {
        self.parts.get(name).map(String::as_str)
    }

    /// Parts changed since opening (written or removed), for saving.
    pub fn touched(&self) -> &BTreeSet<String> {
        &self.touched
    }

    pub fn recompose(&self) -> &BTreeSet<String> {
        &self.recompose
    }

    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// The document's identity for "unsaved changes": like `rev`, but a preview (preview.rs) does not move it.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Moving the UI insertion point leaves the pending typing format behind.
    /// This is selection state, not a document change or an undo step.
    pub fn reset_insertion_style(&mut self, story: &str) {
        self.text_state.remove(story);
    }

    /// An object's path bounds `[x0, y0, x1, y1]` in spread coordinates (a group: its
    /// members'): where Rotate and Flip find their reference point.
    pub fn bounds(&mut self, id: &str) -> Option<[f64; 4]> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        w.bounds_of(id).ok()
    }

    /// The exact stroke outline for transform controls, in inner or spread coordinates.
    pub fn outline_bounds(&mut self, id: &str, inner: bool) -> Option<[f64; 4]> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        let (p, ix, i) = w.find(id).ok()?;
        ops::outline_bounds(&w, &ix, w.text(&p).ok()?, i, inner)
    }

    /// The path's own bounds (the stroke left out), in inner or spread coordinates.
    pub fn path_bounds(&mut self, id: &str, inner: bool) -> Option<[f64; 4]> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        let (p, ix, i) = w.find(id).ok()?;
        ops::path_bounds_of(&ix, w.text(&p).ok()?, i, inner)
    }

    /// The Effects dialog's view of an object: each level's opacity, blend mode and effects
    /// with all their settings, and the document's global light (no object: the light only).
    pub fn effects(&mut self, id: Option<&str>) -> Result<serde_json::Value, EditError> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        ops::effects_of(&mut w, id)
    }

    /// The Table menu's view of a table (rows, columns, cells and their options).
    pub fn table(&mut self, id: &str) -> Result<serde_json::Value, EditError> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        ops::table_of(&mut w, id)
    }

    /// Edit ▸ Copy (and the first half of Cut): the objects as they are now, for Paste.
    pub fn copy(&mut self, ids: &[String]) -> Result<(), EditError> {
        ops::clipboard::copy(self, ids)
    }

    /// The Content Collector tool: the objects as they are now (the copy Edit ▸ Copy makes), given back for the conveyor to hold
    /// without touching the clipboard.
    pub fn collect(&self, ids: &[String]) -> Result<Clipboard, EditError> {
        ops::clipboard::take(self, ids).map(Clipboard)
    }

    /// Whether objects have been copied.
    pub fn can_paste(&self) -> bool {
        self.clipboard.is_some()
    }

    /// What was copied last in this editor, to give to another document's editor.
    pub fn clipboard(&self) -> Option<Clipboard> {
        self.clipboard.clone().map(Clipboard)
    }

    /// What Paste pastes: objects copied here or in another document.
    pub fn set_clipboard(&mut self, clip: Option<Clipboard>) {
        self.clipboard = clip.map(|c| c.0);
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|t| t.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|t| t.label.as_str())
    }

    /// Window ▸ History: the steps that can be undone, oldest first, and those that can be
    /// redone, the next one first.
    pub fn history(&self) -> (Vec<&str>, Vec<&str>) {
        (self.undo.iter().map(|t| t.label.as_str()).collect(), self.redo.iter().rev().map(|t| t.label.as_str()).collect())
    }

    /// Runs the commands as one undo step. On any error nothing stays changed.
    pub fn apply(&mut self, label: &str, commands: &[Command]) -> Result<Outcome, EditError> {
        if let Some(r) = preview::dispatch(self, label, commands) {
            return r;
        }
        // an unconfirmed preview is taken back before any other edit
        let settled = self.preview_settle();
        let text_before = self.text_state.clone();
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        for c in commands {
            if let Err(e) = ops::run(&mut w, c) {
                let splices = std::mem::take(&mut w.splices);
                for s in splices.iter().rev() {
                    self.unsplice(s);
                }
                self.where_is.clear();
                self.text_state = text_before;
                return Err(e);
            }
        }
        let (splices, recompose, created) = (w.splices, w.recompose, w.created);
        if splices.is_empty() {
            return Ok(Outcome { rev: self.rev, label: label.into(), parts: settled, ..Outcome::default() });
        }
        for c in commands {
            if let Some(step) = ops::Step::of(c) {
                if self.again_stale {
                    self.again.clear();
                    self.again_stale = false;
                }
                self.again.push(step);
            }
        }
        let txn = Txn { label: label.into(), splices, recompose, text_before, text_after: self.text_state.clone() };
        self.redo.clear();
        let mut out = self.finish(&txn);
        out.created = created;
        out.parts = out.parts.into_iter().chain(settled).collect::<BTreeSet<_>>().into_iter().collect();
        self.undo.push(txn);
        Ok(out)
    }

    /// Runs the commands as part of the last undo step: what an edit sets off (Auto-Size)
    /// goes back with it. On any error nothing of them stays.
    pub fn amend(&mut self, commands: &[Command]) -> Result<Outcome, EditError> {
        let label = self.undo.last().map(|t| t.label.clone()).unwrap_or_default();
        let before = self.undo.len();
        let out = self.apply(&label, commands)?;
        if before > 0 && self.undo.len() == before + 1 {
            let txn = self.undo.pop().expect("the step just applied");
            let last = self.undo.last_mut().expect("the step amended");
            last.splices.extend(txn.splices);
            last.recompose.extend(txn.recompose);
            last.text_after = txn.text_after;
        }
        Ok(out)
    }

    pub fn undo(&mut self) -> Option<Outcome> {
        let settled = self.preview_settle();
        let txn = self.undo.pop()?;
        for s in txn.splices.iter().rev() {
            self.unsplice(s);
        }
        self.where_is.clear();
        self.text_state = txn.text_before.clone();
        let mut out = self.finish(&txn);
        out.parts = out.parts.into_iter().chain(settled).collect::<BTreeSet<_>>().into_iter().collect();
        self.redo.push(txn);
        Some(out)
    }

    pub fn redo(&mut self) -> Option<Outcome> {
        let settled = self.preview_settle();
        let txn = self.redo.pop()?;
        for s in &txn.splices {
            self.resplice(s);
        }
        self.where_is.clear();
        self.text_state = txn.text_after.clone();
        let mut out = self.finish(&txn);
        out.parts = out.parts.into_iter().chain(settled).collect::<BTreeSet<_>>().into_iter().collect();
        self.undo.push(txn);
        Some(out)
    }

    /// Takes back the last apply, undo or redo as if it never happened (no history entry,
    /// revision restored). For a caller that could not use the result, e.g. a failed rebuild.
    pub fn rollback(&mut self, was: Outcome, kind: Rollback) {
        let txn = match kind {
            Rollback::Apply | Rollback::Redo => self.undo.pop(),
            Rollback::Undo => self.redo.pop(),
        };
        let Some(txn) = txn else { return };
        match kind {
            Rollback::Apply | Rollback::Redo => {
                for s in txn.splices.iter().rev() {
                    self.unsplice(s);
                }
                self.text_state = txn.text_before.clone();
            }
            Rollback::Undo => {
                for s in &txn.splices {
                    self.resplice(s);
                }
                self.text_state = txn.text_after.clone();
            }
        }
        self.where_is.clear();
        self.rev = was.rev - 1;
        self.version -= 1;
        match kind {
            Rollback::Apply => {}
            Rollback::Redo => self.redo.push(txn),
            Rollback::Undo => self.undo.push(txn),
        }
    }

    fn finish(&mut self, txn: &Txn) -> Outcome {
        self.frame_starts.clear();
        self.frame_spans.clear();
        self.anchor_placements.clear();
        self.anchor_targets = Default::default();
        self.rev += 1;
        self.version += 1;
        let parts: BTreeSet<String> = txn.splices.iter().map(|s| match s {
            Splice::Text { part, .. } | Splice::Part { part, .. } => part.clone(),
        }).collect();
        self.touched.extend(parts.iter().cloned());
        self.recompose.extend(txn.recompose.iter().cloned());
        Outcome { rev: self.rev, label: txn.label.clone(), parts: parts.into_iter().collect(), recompose: txn.recompose.clone(), created: Vec::new() }
    }

    fn unsplice(&mut self, s: &Splice) {
        match s {
            Splice::Text { part, at, removed, inserted } => {
                self.record_splice(part, *at..at + inserted.len(), removed);
                let t = self.parts.get_mut(part).expect("spliced part exists");
                t.replace_range(*at..at + inserted.len(), removed);
            }
            Splice::Part { part, before, .. } => match before {
                Some(b) => {
                    self.parts.insert(part.clone(), b.clone());
                }
                None => {
                    self.parts.remove(part);
                }
            },
        }
        match s {
            Splice::Text { part, .. } | Splice::Part { part, .. } => self.bump(part),
        }
    }

    fn resplice(&mut self, s: &Splice) {
        match s {
            Splice::Text { part, at, removed, inserted } => {
                self.record_splice(part, *at..at + removed.len(), inserted);
                let t = self.parts.get_mut(part).expect("spliced part exists");
                t.replace_range(*at..at + removed.len(), inserted);
            }
            Splice::Part { part, after, .. } => match after {
                Some(a) => {
                    self.parts.insert(part.clone(), a.clone());
                }
                None => {
                    self.parts.remove(part);
                }
            },
        }
        match s {
            Splice::Text { part, .. } | Splice::Part { part, .. } => self.bump(part),
        }
    }
}

impl Work<'_> {
    pub(crate) fn text(&self, part: &str) -> Result<&str, EditError> {
        self.ed.parts.get(part).map(String::as_str).ok_or_else(|| EditError::new("notFound", format!("part {part}")))
    }

    /// Replaces byte ranges of one part (non-overlapping, any order), recording each.
    pub(crate) fn splice(&mut self, part: &str, mut edits: Vec<(std::ops::Range<usize>, String)>) -> Result<(), EditError> {
        edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        for w in edits.windows(2) {
            if w[1].0.end > w[0].0.start {
                return Err(EditError::new("invalid", format!("overlapping edits in {part}")));
            }
        }
        self.text(part)?;
        let mut written = false;
        for (r, new) in edits {
            let t = &self.ed.parts[part];
            if t.get(r.clone()) == Some(new.as_str()) {
                continue;
            }
            let removed = t[r.clone()].to_owned();
            self.ed.record_splice(part, r.clone(), &new);
            self.ed.parts.get_mut(part).expect("spliced part exists").replace_range(r.clone(), &new);
            self.splices.push(Splice::Text { part: part.into(), at: r.start, removed, inserted: new });
            written = true;
        }
        if written {
            self.ed.bump(part);
        }
        Ok(())
    }

    pub(crate) fn remove_part(&mut self, part: &str) {
        if let Some(before) = self.ed.parts.remove(part) {
            self.splices.push(Splice::Part { part: part.into(), before: Some(before), after: None });
            self.ed.bump(part);
        }
    }

    pub(crate) fn add_part(&mut self, part: String, text: String) -> Result<(), EditError> {
        if self.ed.parts.contains_key(&part) {
            return Err(EditError::new("invalid", format!("part already exists: {part}")));
        }
        self.ed.parts.insert(part.clone(), text.clone());
        self.ed.bump(&part);
        self.splices.push(Splice::Part { part, before: None, after: Some(text) });
        Ok(())
    }

    pub(crate) fn recompose(&mut self, story: &str) {
        if !self.recompose.iter().any(|s| s == story) {
            self.recompose.push(story.into());
        }
    }

    /// Path bounds `[x0, y0, x1, y1]` of an object in spread coordinates (a group: its
    /// members'), as the item's own path through its transforms.
    pub(crate) fn bounds_of(&mut self, id: &str) -> Result<[f64; 4], EditError> {
        let (part, ix, i) = self.find(id)?;
        let t = self.text(&part)?;
        ops::spread_bounds(&ix, t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no bounds")))
    }

    /// The part holding the element whose `Self` is `id` (spreads, parent spreads, stories,
    /// designmap, resources).
    pub(crate) fn find(&mut self, id: &str) -> Result<(String, xml::Index, usize), EditError> {
        if let Some(p) = self.ed.where_is.get(id).cloned()
            && let Some(t) = self.ed.parts.get(&p)
            && let Ok(ix) = self.ed.cached_index(&p)
            && let Some(i) = ix.by_self(t, id)
        {
            return Ok((p, ix, i));
        }
        let pat = format!(" Self=\"{}\"", xml::escape(id));
        let order = |n: &String| if n.starts_with("Spreads/") { 0 } else if n.starts_with("MasterSpreads/") { 1 } else if n == "designmap.xml" { 2 } else { 3 };
        let mut names: Vec<&String> = self.ed.parts.keys().filter(|n| n.ends_with(".xml")).collect();
        names.sort_by_key(|n| order(n));
        for name in names {
            let t = &self.ed.parts[name];
            if !t.contains(&pat) {
                continue;
            }
            let ix = self.ed.cached_index(name)?;
            if let Some(i) = ix.by_self(t, id) {
                let name = name.clone();
                self.ed.where_is.insert(id.into(), name.clone());
                return Ok((name, ix, i));
            }
        }
        Err(EditError::new("notFound", format!("no object {id}")))
    }

    pub(crate) fn index(&self, part: &str) -> Result<xml::Index, EditError> {
        self.ed.cached_index(part)
    }

    /// A new object id InDesign's way (`u` + hex), after every id in the package.
    pub(crate) fn new_id(&mut self) -> String {
        let parts = &self.ed.parts;
        let n = *self.ed.next_uid.get_or_insert_with(|| {
            // every `u<hex>` word in attribute values and lists (`Self="u1a"`, `StoryList="u1 u2"`)
            let mut max = 0u64;
            for t in parts.values() {
                let b = t.as_bytes();
                for p in 1..b.len().saturating_sub(1) {
                    if b[p] != b'u' || !(b[p - 1] == b'"' || b[p - 1] == b' ') {
                        continue;
                    }
                    let mut j = p + 1;
                    while j < b.len() && b[j].is_ascii_hexdigit() {
                        j += 1;
                    }
                    if j > p + 1 && j < b.len() && (b[j] == b'"' || b[j] == b' ')
                        && let Ok(v) = u64::from_str_radix(&t[p + 1..j], 16)
                    {
                        max = max.max(v);
                    }
                }
            }
            max + 1
        });
        self.ed.next_uid = Some(n + 1);
        format!("u{n:x}")
    }

    pub(crate) fn parts_named(&self, prefix: &str) -> Vec<String> {
        self.ed.parts.keys().filter(|n| n.starts_with(prefix)).cloned().collect()
    }
}
