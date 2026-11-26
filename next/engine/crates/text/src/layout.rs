//! Laying out each story's text into its chain of text frames.
//!
//! A story's first frame has `previous == None`; the chain follows `next`. Text frames
//! are found on spreads, master spreads, inside groups and pasted-into frames, and
//! anchored in other stories. Each paragraph is shaped once, composed with
//! `folio_compose` (Adobe Paragraph Composer rules) for the measure of the column it
//! lands in, and placed line by line: the first baseline of a column follows the frame's
//! first-baseline option, later baselines add the line's leading. A line whose baseline
//! would pass the column bottom moves to the next column, then the next frame; text left
//! over makes the story overset.
//!
//! Stories are independent, so `layout_document` lays them out on several threads, and
//! `layout_story` re-lays one story alone (after an edit) with the same result.
//!
//! A paragraph is composed as a whole with the width each of its lines gets (its column,
//! a later column or frame, the room beside a text wrap); when a line lands somewhere else
//! than planned, the paragraph is composed again with the widths learned. A line holds at
//! least the text up to its first break opportunity: a narrower slot beside a wrap is
//! skipped, and a word wider than the column is not set (the story is overset there).
//!
//! Text wrap: items with wrap on the same spread (all of them, or only those above the
//! frame under "text wrap only affects text beneath"); bounding box (from the item's visible
//! bounds, stroke included), contour as its bounding box, jump object, next column. With
//! "both sides" a row is the part left of the wrap and then the part right of it, each a
//! line of its own.
//!
//! A line that meets a wrap moves below it in whole points from where it was tried. Inline
//! anchored objects are placed at their character (`LaidLine::objects`).
//!
//! InDesign shows the composition stored in a document until a story is edited; where that
//! was made with other fonts (missing fonts, another font version) it differs from what
//! this layout (and InDesign composing the story again) gives (13 stories of #347:
//! artifacts/next-20261018/fit/fresh-composition-347.json). A story read from an INDD with
//! its saved lines (`Story::saved`) is therefore set with InDesign's line starts, frames and
//! columns (`saved`); justification and glyph positions within each line stay this
//! layout's own, and a story whose saved lines do not fit is composed as above.
//!
//! Not yet modelled: wrap of master items on document pages and of anchored objects,
//! vertical justification "Justify" with a paragraph spacing limit, drop caps of more than one line,
//! frame-specific/rotated baseline grids, balanced/span/split columns, paragraph rules and shading,
//! custom anchored-object positions, optical margin alignment.

mod band;
mod flow;
mod frames;
mod line;
mod line_breaks;
mod line_content;
mod paragraph;
mod placement;
mod rows;
mod story;
mod character_lines;
mod indicators;

pub(crate) use frames::{columns_of, stroke_inset, stroke_outset, vertical_columns};
use frames::{item_bounds, visible_bounds};
use line::{decorate, first_baseline, hyphen_glyph, inline_item, shift_line, visual_positions};
use paragraph::{leading_of, settings_of};
pub use story::{
    PageFrameText, layout_document, layout_document_with_threads, layout_master_page_numbers,
    layout_story, text_variable_values, variable_results,
};

mod anchors;
mod keep;
mod dropcap;
mod linestyles;
mod kenten;
mod saved;
mod baseline_grid;
mod strokes;
mod structures;
/// Placement of spanning and split paragraph columns.
pub mod paragraph_columns;
mod listnumbers;
mod listplan;
mod footnotes;
mod tables;
mod tab_leaders;
mod variables;
pub mod filevars;
mod region;
mod wrap;
pub use region::Shape;
use wrap::{first_room, own_shape, rooms_ahead, wrap_slot};
pub mod toc;
pub mod listconvert;

use crate::fonts::{FontStore, Resolved};
use saved::{ForcedLine, StoryPlan};
pub use saved::SavedUse;
use structures::{Shown, TextState, justify_vertically};
use footnotes::footnote_starts;
use tables::slice_paragraph;
use folio_model::nested::NestedLineStyle;
pub use structures::format_number;
use crate::shape::{ClusterKind, FaceTable, ShapePrefs, ShapedParagraph, Shaper};
use folio_compose::hyphenate::{self, Dictionary};
use folio_compose::{Alignment, Composer, HyphenSettings, Item, Range as CRange, Settings};
use folio_model::layout::{
    Decoration, DecorationKind, DocumentLayout, FontId, FrameText, LaidLine, MissingGlyphs, PlacedGlyph, PlacedObject,
    TextShape,
};
use folio_model::{
    Affine, BottomOrigin, CharacterAttrs, Document, FirstBaseline, FootnoteOptions, Id, ItemKind, Justification, Leading,
    ListKind, PageItem, Paint, Paragraph, ParagraphAttrs, ParagraphRule, Rect, RuleWidth, Spread, Story, TabAlignment,
    TextFrame, TextRun, TextVariable, TextWrap, TextWrapMode, TextWrapSide, TopOrigin, VerticalJustification, paths_bounds,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Positions within this distance count as inside (InDesign's tolerance is ~1/2048 pt).
const EPS: f64 = 0.001;
/// InDesign's width tolerance (the composer's).
const TOL: f64 = folio_compose::cost::TOL;
/// InDesign's default tab stops when a paragraph has none left: every half inch.
const DEFAULT_TAB: f64 = 36.0;
/// Attempts to compose a paragraph with the line widths it meets before falling back to
/// recomposing only the remainder.
const MAX_REPLANS: usize = 64;

// ---------------------------------------------------------------------------------------
// Frames

/// A text frame and where it sits.
#[derive(Clone, Debug)]
pub struct FrameRef<'d> {
    pub item: &'d PageItem,
    pub frame: &'d TextFrame,
    /// Name of the page the frame is on (master spreads: the master prefix, e.g. "A"),
    /// for page-number variables. `None` for anchored frames.
    pub page_name: Option<String>,
    /// The section marker of the frame's page (`Page::section_marker`).
    pub section_marker: Option<String>,
    /// The frame is on a left-hand (verso) page: binding on its right.
    pub verso: bool,
    /// Frame inner → spread coordinates (identity for anchored frames).
    pub to_spread: Affine,
    /// Page inner → spread coordinates of the frame's page.
    pub page_transform: Option<Affine>,
    /// The frame's page in spread coordinates and its margins (top, left, bottom, right).
    pub page_rect: Option<Rect>,
    pub margins: [f64; 4],
    /// Text wrap of other items on the spread, in the frame's inner coordinates (a vertical frame: in its virtual
    /// horizontal frame, `crate::vertical`).
    pub wraps: Vec<Wrap>,
    /// The frame's own text area when it is not a rectangle (it applies even when the frame
    /// ignores text wrap); a vertical frame's in its virtual horizontal frame.
    pub shape: Option<Shape>,
    /// The frame holds a vertical story (not on a path): its wraps and shape are turned into the virtual frame.
    pub turned: bool,
    /// Index into `spreads` then `master_spreads` (as one list); `None` when anchored.
    spread: Option<usize>,
    /// The frame's page within its spread.
    page_in_spread: Option<usize>,
    z: (usize, usize),
}

/// An area text wraps around, in a frame's inner coordinates (wrap offsets applied).
#[derive(Clone, Debug, PartialEq)]
pub struct Wrap {
    pub rect: Rect,
    pub mode: TextWrapMode,
    pub side: TextWrapSide,
    pub inverse: bool,
    outline: Option<region::Outline>,
    scan: Option<region::Scan>,
}

#[derive(Clone, Debug)]
struct Obstacle<'d> {
    id: &'d str,
    spread: usize,
    z: (usize, usize),
    bounds: Rect,
    wrap: &'d TextWrap,
    outline: Option<region::Outline>,
}

/// Every text frame of a document and the frame chain of every story.
#[derive(Debug, Default)]
pub struct DocumentFrames<'d> {
    /// An item laid out in place of the document's item with its id (Auto-Size trials).
    replace: Option<&'d PageItem>,
    frames: HashMap<&'d str, FrameRef<'d>>,
    chains: BTreeMap<&'d str, Vec<&'d str>>,
    obstacles: Vec<Obstacle<'d>>,
    layer_order: HashMap<&'d str, usize>,
    counter: usize,
    /// The document has facing pages: only then does a wrap have a spine side (oracle text-wrap/wrap-side-spine).
    facing: bool,
    /// Stories set vertically (`Story::vertical`).
    vertical: HashSet<&'d str>,
}

// ---------------------------------------------------------------------------------------
// Result of one story

/// Layout of one story: text per frame of its chain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoryLayout {
    pub story: Id,
    pub frames: Vec<FrameText>,
    pub overset: bool,
    /// Family + style pairs shown with a substitute.
    pub missing_fonts: Vec<(String, String)>,
    /// Characters set as .notdef boxes, per font.
    pub missing_glyphs: Vec<MissingGlyphs>,
    pub paragraphs: usize,
    pub lines: usize,
    pub glyphs: usize,
    /// Whether the story was set with InDesign's saved lines.
    pub saved: SavedUse,
    /// The composed row heights of its tables (nested ones too), by table: what InDesign
    /// stores as an auto-grow row's `SingleRowHeight`.
    pub table_rows: BTreeMap<String, Vec<f64>>,
}

// ---------------------------------------------------------------------------------------
// Story layout

struct Ctx<'c, 'a> {
    doc: &'c Document,
    list_plan: listplan::Plan,
    /// First footnote number of each story with footnotes.
    note_starts: &'c HashMap<String, u32>,
    frames: &'c DocumentFrames<'c>,
    fonts: &'c FontStore,
    table: &'c FaceTable<'a>,
    /// Faces for characters substitute faces lack.
    fallbacks: &'c HashMap<char, FontId>,
    prefs: ShapePrefs,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Column {
    pub(crate) left: f64,
    pub(crate) right: f64,
    pub(crate) top: f64,
    pub(crate) bottom: f64,
    pub(crate) previous: Option<f64>,
}

impl Column {
    fn width(&self) -> f64 {
        self.right - self.left
    }
}

struct FlowFrame<'c> {
    facing: bool,
    grid: Option<baseline_grid::Grid>,
    id: &'c str,
    frame: &'c TextFrame,
    verso: bool,
    cols: Vec<Column>,
    /// The frame's own columns: more columns than this are the parcels of paragraph columns (`spancols`).
    base_columns: usize,
    start: usize,
    wraps: &'c [Wrap],
    /// The frame's own text area when it is not a rectangle.
    shape: Option<&'c Shape>,
    /// Lines with their column.
    lines: Vec<FlowLine>,
    /// Footnotes placed at the bottom of the columns, in reference order.
    notes: Vec<NoteBlock>,
    /// Depth kept for footnotes spanning the frame's columns (learned by an earlier pass).
    reserve: f64,
    /// The frame's bounds and its page with margins (top, left, bottom, right), in the
    /// frame's inner coordinates: what custom-positioned anchored objects align to.
    area: Rect,
    page: Option<(Rect, [f64; 4])>,
    /// A vertical story's frame: the quarter turn from its virtual (horizontal) frame into the frame.
    vmap: Option<Affine>,
}

impl FlowFrame<'_> {
    /// Footnotes span every column of this frame.
    fn spans_notes(&self, opts: &FootnoteOptions) -> bool {
        opts.straddle && self.cols.len() > 1
    }
}

/// A laid line and what later passes need about it.
#[derive(Clone, Debug)]
struct FlowLine {
    col: usize,
    line: LaidLine,
    /// Paragraph of the laid text (index into the paragraphs laid); `None` for table cell
    /// and footnote lines.
    para: Option<usize>,
    m: LineMetrics,
    grid: Option<baseline_grid::Grid>,
    /// Left and right of the line's visible text.
    text: (f64, f64),
}

/// A footnote laid out at a column's width: lines with baselines from the note's top, and
/// its depth (to the last baseline).
#[derive(Clone, Debug)]
struct NoteBlock {
    frame: usize,
    col: usize,
    continued: bool,
    continues: bool,
    overset: bool,
    lines: Vec<FlowLine>,
    height: f64,
}

/// Where the next line goes.
struct Flow<'c> {
    frames: Vec<FlowFrame<'c>>,
    fi: usize,
    ci: usize,
    /// Baseline of the last line in the current column.
    last: Option<f64>,
    /// Top of the current text area below a text wrap that text jumped (else the column top).
    region_top: Option<f64>,
    /// Baseline of the line that met the wrap jumped: the line below moves down from it in
    /// whole points (see `place_paragraph`).
    jumped_from: Option<f64>,
    /// Space after of the previous paragraph, still to add before the next line.
    pending_after: f64,
    /// A drop cap taller than its paragraph: the lines of the next paragraphs still indented
    /// beside it, and the indent (InDesign flows the following text around it).
    drop_carry: Option<(usize, f64)>,
    /// The part right of a both-sides wrap on the row just set: the paragraph's next line
    /// goes there when it fits.
    pending_part: Option<RightPart>,
    /// Space after the table just set, added before the next line of text (not before
    /// another table: InDesign 2025, scratch experiment exp2 "I").
    table_after: f64,
    /// Footnote layout options (main stories only).
    notes: Option<&'c folio_model::FootnoteOptions>,
    /// Columns (frame, column) holding lines set on InDesign's saved baselines, which have
    /// its vertical justification already (`saved`).
    saved_columns: HashSet<(usize, usize)>,
    keep: keep::State,
    /// A vertical story: its frames are composed turned (`crate::vertical`).
    vertical: bool,
}

/// The part of a row right of a both-sides wrap, after a line was set left of the wrap.
#[derive(Clone, Copy, Debug)]
struct RightPart {
    baseline: f64,
    left: f64,
    right: f64,
    /// The line left of the wrap (frame, index in its lines) and how far it moves right once
    /// the right part is filled: centred text then keeps to the wrap (InDesign 2025,
    /// result-both10.json); with the right part empty it stays centred (#347 u4722e).
    line: (usize, usize),
    shift: f64,
}

/// Where a line at some height may go, given the text wraps.
enum Slot {
    /// Left and right of the line, the part right of a both-sides wrap that the next line of
    /// the row may fill, and the room the frame's own shape gives (the column without one):
    /// a line narrower than that is a fragment beside a wrap.
    Fits(f64, f64, Option<(f64, f64)>, (f64, f64)),
    /// Nothing fits beside a wrap: continue below this y.
    Below(f64),
    /// Jump-object keeps advancing on the paragraph's leading (wrap-jump).
    JumpBelow(f64),
    SkipRow,
    /// The frame's own shape has no room for the line here: InDesign tries the band 1 pt
    /// lower (`GetTilesOfRect`).
    Step,
    NextColumn,
}

/// A flow position to return to.
struct Checkpoint {
    keep_lines: usize,
    fi: usize,
    ci: usize,
    last: Option<f64>,
    region_top: Option<f64>,
    jumped_from: Option<f64>,
    pending_after: f64,
    drop_carry: Option<(usize, f64)>,
    table_after: f64,
    pending_part: Option<RightPart>,
    lines: Vec<usize>,
    notes: Vec<usize>,
    /// Each frame's columns (paragraph columns add parcels to them) and where its flow starts.
    cols: Vec<(usize, usize)>,
}

/// What `place_paragraph` needs about the paragraph.
struct ParaCtx<'p> {
    para: &'p Paragraph,
    shaped: &'p ShapedParagraph,
    run_fonts: &'p [Resolved],
    /// Story index of each character of `shaped` and of the paragraph end.
    index: &'p [usize],
    /// Index of the paragraph among those laid (for later passes) and its neighbourhood.
    pi: usize,
    last_para: bool,
    same_style: bool,
    /// Footnote text of each run that is a footnote reference (numbered), or empty.
    notes: &'p [Option<folio_model::Note>],
    /// InDesign's saved lines of the paragraph (`saved`): its breaks and columns.
    forced: Option<&'p [ForcedLine]>,
    drop_cap: Option<dropcap::DropCap>,
    /// Character offsets where the paragraph's lines 2, 3, ... must start (nested line styles settled one line at a time).
    pins: &'p [usize],
}

/// What `lay_text` knows about a paragraph besides its text.
struct Para<'p> {
    pi: usize,
    last_para: bool,
    same_style: bool,
    notes: &'p [Option<folio_model::Note>],
    forced: Option<&'p [ForcedLine]>,
    drop_cap: Option<dropcap::DropCap>,
    pins: &'p [usize],
}

/// The room of a line placed in an attempt of a paragraph (`layout_paragraph`).
#[derive(Clone, Copy, Debug)]
struct Room {
    width: f64,
    /// Frame, column, baseline and leading of the line, when the frame's own shape gave it
    /// its room.
    at: Option<(usize, usize, f64, f64)>,
    /// The line was the first of its area in a shaped frame: the area's top and the height of
    /// its band (top to baseline), which goes down 1 pt at a time until a tile holds the
    /// line's first unit (`first_room`).
    first: Option<(f64, f64)>,
    /// The row's second slot, right of a both-sides wrap: its width.
    next: Option<f64>,
    /// The line was the first of a column or frame (not the paragraph's first line): the composer is told so that a
    /// hyphen before it can cost `hyphenateAcrossColumns` off (`Slots::column_start`).
    column_start: bool,
}

/// Which part of a row a composed piece of text is (`compose_slots`): a whole row, the part
/// left of a both-sides wrap, or the part right of it (the row's second slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Whole,
    Left,
    Right,
}

/// The room of a line not placed yet (`rooms_ahead`): its width, and for the first line of a
/// column, the column and the band it starts from (`first_room`).
#[derive(Clone, Copy, Debug)]
struct Ahead {
    width: f64,
    first: Option<(usize, usize, f64, f64)>,
}

/// Composer items of a paragraph for a measure, with dictionary hyphen points inserted.
struct Prepared {
    items: Vec<Item>,
    /// Cluster of each item; `None` for an inserted hyphen point.
    origin: Vec<Option<usize>>,
    /// Run of each item.
    run: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
struct LineMetrics {
    ascent: f64,
    cap: f64,
    x_height: f64,
    embox: f64,
    leading: f64,
    descent: f64,
    /// Height of the line's inline objects above the baseline (the "Ascent" first baseline
    /// counts them as characters).
    objects_above: f64,
    /// Room the line's above-line objects take before it (`anchors`).
    above_line: f64,
    /// Largest `size x head yMax` / `size x -head yMin` of the faces on the line (a highlight indicator's height).
    bbox_above: f64,
    bbox_below: f64,
    /// Depth of the em box below the baseline (vertical text: a line whose em box passes the column's far side moves on).
    embox_below: f64,
}

impl LineMetrics {
    fn add_font(&mut self, m: &crate::fonts::FaceMetrics, size: f64) {
        self.ascent = self.ascent.max(m.ascent * size);
        self.cap = self.cap.max(m.cap_height * size);
        self.x_height = self.x_height.max(m.x_height * size);
        self.embox = self.embox.max(m.embox_top * size);
        self.embox_below = self.embox_below.max((1.0 - m.embox_top) * size);
        self.descent = self.descent.max(-m.typo_descender * size);
        self.bbox_above = self.bbox_above.max(m.bbox_top * size);
        self.bbox_below = self.bbox_below.max(m.bbox_bottom * size);
    }
}
