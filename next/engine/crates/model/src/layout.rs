//! Composed text: the output of `folio-text`, the input of `folio-render`.
//!
//! Positions are in the text frame's inner coordinates (the same space as the frame's
//! `paths`), y down, baselines in points. Table cells and footnotes are laid out into the
//! frame that shows them: their text as more lines, their fills and strokes as
//! [`TextShape`]s.

use crate::{Id, Paint};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Anchored items in their containing frame's coordinates, using the existing composition.
pub fn anchored_items(doc: &crate::Document, layout: &DocumentLayout) -> BTreeMap<Id, Vec<crate::PageItem>> {
    let inline: BTreeMap<_, _> = doc.stories.values().flat_map(|s| &s.paragraphs).flat_map(|p| &p.runs).flat_map(|r| &r.inline).map(|i| (i.id.as_str(), i)).collect();
    layout.frames.iter().map(|(id, frame)| {
        let items = frame.lines.iter().flat_map(|l| &l.objects).filter_map(|o| {
            let mut item = (*inline.get(o.item.as_str())?).clone();
            item.transform = crate::Affine::translate(o.x, o.y).then_inner(&item.transform);
            Some(item)
        }).collect();
        (id.clone(), items)
    }).collect()
}

/// Index of a loaded font face in the font store.
pub type FontId = u32;

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Runtime-only source mapping, rebuilt with the line (never an IDML property).
/// Scalar character indexes, as InDesign counts insertionPoints (oracle text-special-*).
/// x0/x1 are the logical leading/trailing edges: reversed for RTL clusters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextCluster {
    pub start: usize,
    pub end: usize,
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
    /// Text on a path: composition lays the line out straight, and this maps a point of that straight line (the
    /// cluster's box above) to where the cluster really is on the path, in the frame's coordinates. `None` for
    /// ordinary text.
    pub path_map: Option<crate::Affine>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacedGlyph {
    pub font: FontId,
    pub glyph: u16,
    /// Pen position of the glyph origin (left of the advance, on the baseline).
    pub x: f64,
    pub y: f64,
    pub size: f64,
    /// Horizontal scale (1.0 = 100%) and skew in degrees, applied around the origin.
    pub hscale: f64,
    pub vscale: f64,
    pub skew: f64,
    /// Additional linear transform in the frame's y-down coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_transform: Option<crate::Affine>,
    pub fill: Paint,
    #[serde(default)]
    pub stroke: Paint,
    #[serde(default)]
    pub stroke_weight: f64,
    #[serde(default)]
    pub overprint_fill: bool,
    #[serde(default)]
    pub overprint_stroke: bool,
    /// Where the gradient of `fill` / `stroke` sits, when that paint is a gradient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_gradient: Option<crate::PlacedGradient>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_gradient: Option<crate::PlacedGradient>,
}

impl PlacedGlyph {
    pub fn transform(&self, rotation: f64) -> crate::Affine {
        use crate::Affine;
        let (sn,cs) = rotation.to_radians().sin_cos();
        Affine::translate(self.x,self.y)
            .then_inner(&Affine::new(cs,sn,-sn,cs,0.0,0.0))
            .then_inner(&self.path_transform.unwrap_or(Affine::IDENTITY))
            .then_inner(&Affine::new(1.0,0.0,-self.skew.to_radians().tan(),1.0,0.0,0.0))
            .then_inner(&Affine::scale(self.size*self.hscale,-self.size*self.vscale))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LaidLine {
    #[serde(skip)]
    pub text_clusters: Vec<TextCluster>,
    pub baseline: f64,
    /// Line box left and right (after indents) for selection and debugging.
    pub left: f64,
    pub right: f64,
    pub glyphs: Vec<PlacedGlyph>,
    /// The line's letter spacing (the paragraph's desired value plus what justification added) as a fraction of the font's
    /// space width, relative to the line's own glyph scale: InDesign's PDF writes it as the `Tc` of the line's text,
    /// rounded to whole thousandths of the em. 0 for text without letter spacing.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub letter_spacing: f64,
    /// Story character range [start, end) this line shows.
    pub start: usize,
    pub end: usize,
    /// Underlines and strike-throughs on this line.
    #[serde(default)]
    pub decorations: Vec<Decoration>,
    /// Inline anchored objects on this line.
    #[serde(default)]
    pub objects: Vec<PlacedObject>,
    /// Filled areas laid out with this line: table cell fills and strokes, paragraph rules,
    /// shading and borders, footnote separator rules. Renderers paint the shapes of all the
    /// frame's lines, in line order, before any of its glyphs.
    #[serde(default)]
    pub shapes: Vec<TextShape>,
    /// Degrees every glyph of the line is turned clockwise on the page around its origin
    /// (text of a rotated table cell: 90, 180, 270); 0 for upright text. Glyph positions
    /// are already where the turned line puts them.
    #[serde(default)]
    pub rotation: f64,
    /// The piece of a table this line sets: its grid, for picking and selecting cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableGrid>,
    /// A nested text line (table cell or footnote), with its own character indexes.
    /// The outer indexes stay at the story's table or footnote reference character.
    #[serde(skip)]
    pub cell: Option<CellText>,
}

/// Nested text geometry, counting from 0. The legacy field/type name also covers footnotes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellText {
    /// A cell Self or a transient footnote text address; never written into IDML here.
    pub id: String,
    pub start: usize,
    pub end: usize,
    pub clusters: Vec<TextCluster>,
}

/// The grid of a table piece in frame inner coordinates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableGrid {
    /// The table's `Self`.
    pub table: String,
    /// The column grid lines, left to right (one more than the columns).
    pub columns: Vec<f64>,
    /// The rows set in this piece, in order: row index, top and bottom grid lines.
    pub rows: Vec<(usize, f64, f64)>,
    /// What the cells of each of those rows need at least (insets plus text depth); a row cannot be dragged smaller than this.
    #[serde(default)]
    pub content_min: Vec<f64>,
}

/// An area or stroke drawn with a frame's text, in frame inner coordinates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextShape {
    pub paths: Vec<crate::SubPath>,
    #[serde(default)]
    pub shape: crate::geom::PartShape,
    pub paint: Paint,
    /// The fill, stroke or rule overprints (InDesign's own setting; the document's
    /// "overprint [Black]" applies on output as for other fills).
    #[serde(default)]
    pub overprint: bool,
}

impl TextShape {
    /// An axis-aligned rectangle; `None` when it is empty.
    pub fn rect(left: f64, top: f64, right: f64, bottom: f64, paint: Paint, overprint: bool) -> Option<TextShape> {
        if !(right > left && bottom > top) || paint == Paint::None {
            return None;
        }
        let p = |x: f64, y: f64| {
            let a = crate::Point::new(x, y);
            crate::PathPoint { anchor: a, left: a, right: a }
        };
        let points = vec![p(left, top), p(right, top), p(right, bottom), p(left, bottom)];
        Some(TextShape { paths: vec![crate::SubPath { points, open: false }], paint, overprint, ..Default::default() })
    }

    /// Conservative bounds including stroke width and joins.
    pub fn bounds(&self) -> Option<crate::Rect> {
        let b = crate::paths_bounds(&self.paths)?;
        let m = match self.shape {
            crate::geom::PartShape::Line { width, miter_limit, .. } => width / 2.0 * miter_limit.max(1.5),
            crate::geom::PartShape::Fill => 0.0,
        };
        Some(crate::Rect { left: b.left - m, top: b.top - m, right: b.right + m, bottom: b.bottom + m })
    }
}

/// An anchored object placed with a line: the translation that puts the item (found by its
/// id in `TextRun::inline`; its geometry is relative to its anchor) where it goes, in frame
/// inner coordinates. An inline object's is its anchor (pen position on the baseline).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlacedObject {
    pub item: Id,
    pub x: f64,
    pub y: f64,
    /// Placed against the frame or page horizontally / vertically (a custom position, an
    /// above-line object's column): moving the line does not move it on that axis.
    #[serde(default)]
    pub fixed_x: bool,
    #[serde(default)]
    pub fixed_y: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecorationKind {
    #[default]
    Underline,
    StrikeThrough,
    /// A condition's highlight indicator: an opaque rectangle painted behind the text of a line
    /// (`y` is its top, `thickness` its height, `shapes` the filled rectangle).
    Highlight,
}

/// A horizontal rule under or through part of a line, in frame inner coordinates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Decoration {
    pub kind: DecorationKind,
    pub x0: f64,
    pub x1: f64,
    /// Centre of the rule (y down).
    pub y: f64,
    pub thickness: f64,
    pub paint: Paint,
    #[serde(default)]
    pub stroke_type: String,
    #[serde(default)]
    pub gap: Paint,
    #[serde(default)]
    pub gap_overprint: bool,
    /// Shapes relative to (x0, y), shared by screen and print.
    #[serde(default)]
    pub shapes: Vec<TextShape>,
    #[serde(default)]
    pub overprint: bool,
    /// A condition indicator: stroked in this fixed RGB (0-255), not a swatch.
    #[serde(default)]
    pub rgb: Option<[u8; 3]>,
    /// Shown on screen only: an indicator that does not print (`ShowIndicators`).
    #[serde(default)]
    pub screen_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameText {
    pub frame: Id,
    pub lines: Vec<LaidLine>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentLayout {
    /// Text per frame id.
    pub frames: BTreeMap<Id, FrameText>,
    /// Stories with text that did not fit their frames.
    pub overset: Vec<Id>,
    /// Font family + style pairs that were not found (shown with a fallback).
    pub missing_fonts: Vec<(String, String)>,
    pub stats: LayoutStats,
    /// Master frames laid out again for one document page (auto page numbers); drawn
    /// instead of `frames[frame]` on that page.
    #[serde(default)]
    pub page_frames: Vec<PageFrameText>,
    /// Characters set as their font's .notdef glyph (an empty box), per story and font.
    #[serde(default)]
    pub missing_glyphs: Vec<MissingGlyphs>,
    /// Stories with saved lines (`Story::saved`) that layout composed itself instead, and
    /// why (the text differs, a frame or column is not in the story's thread, …).
    #[serde(default)]
    pub saved_fallbacks: Vec<(Id, String)>,
}

/// Characters of one story and font that print as the font's .notdef glyph (an empty box)
/// because the face has no glyph for them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MissingGlyphs {
    pub story: Id,
    /// The font the text asks for.
    pub family: String,
    pub style: String,
    /// The font is installed and lacks the glyphs (InDesign prints the same boxes). False:
    /// the font is missing and no installed face has these characters.
    pub installed: bool,
    /// The distinct characters, in story order (at most 64).
    pub chars: String,
    /// Characters set as boxes.
    pub count: usize,
    /// Story character ranges `[start, end)` holding them; nested text (table cells,
    /// footnotes) is at its anchor character.
    pub ranges: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PageFrameText {
    /// `Page::index` of the document page.
    pub page: usize,
    pub frame: FrameText,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutStats {
    pub stories: usize,
    pub paragraphs: usize,
    pub lines: usize,
    pub glyphs: usize,
    pub elapsed_ms: u64,
    /// Stories set with InDesign's saved lines, and stories that had saved lines but were
    /// composed by layout (`DocumentLayout::saved_fallbacks`).
    #[serde(default)]
    pub saved_stories: usize,
    #[serde(default)]
    pub saved_fallbacks: usize,
}

/// One outline command in font units scaled to 1 em = 1.0, y up (font convention).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum OutlineCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Glyph outlines for drawing; implemented by the font store.
pub trait GlyphSource: Sync {
    /// Outline of `glyph` in `font`, 1 em = 1.0, y up. `None` for empty glyphs.
    fn outline(&self, font: FontId, glyph: u16) -> Option<std::sync::Arc<Vec<OutlineCmd>>>;
}
