//! Legacy edit command arguments and dispatch.

use super::*;

fn yes() -> bool { true }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    /// Insert another text-frame chain after this frame, or detach its following chain.
    ThreadText { id: String, #[serde(default)] next: Option<String> },
    /// Make a frame and insert it into the thread as a single reversible edit.
    CreateThreadFrame { id: String, page: String, rect: [f64; 4], #[serde(default)] layer: Option<String> },
    /// Links panel ▸ Relink: the graphic of a frame (or the graphic itself) takes another file as a replacement does (displayed
    /// size and place kept), the link keeping its identity (oracle: basic-links).
    RelinkGraphic { source: GraphicSource, id: String },
    /// Links panel ▸ Embed Link: the graphic of a frame (or the graphic) holds the file's bytes (`data`: base64 in 76 column lines,
    /// as InDesign writes `Contents`) and its link reads Embedded (oracle: basic-links embed-link).
    EmbedLink { id: String, data: String },
    /// Links panel ▸ Unembed Link: the graphic keeps everything but its `Contents`; its link reads Normal and points at `uri` (the
    /// file the contents were written to). Measured: oracle basic-links unembed-link (InDesign changes nothing else).
    UnembedLink { id: String, uri: String },
    /// Place a probed graphic into a frame, or on a page at a point / drawn rectangle.
    PlaceGraphic { source: GraphicSource, #[serde(default)] id: Option<String>, #[serde(default)] page: Option<String>, #[serde(default)] point: Option<[f64; 2]>, #[serde(default)] rect: Option<[f64; 4]>, #[serde(default)] layer: Option<String> },
    InsertPages { count: usize, #[serde(default)] before: Option<String>, #[serde(default)] master: Option<String> },
    DeletePages { pages: Vec<String> },
    MovePages { pages: Vec<String>, #[serde(default)] before: Option<String> },
    SetPageShuffle { allowed: bool, #[serde(default)] spread: Option<String> },
    /// View ▸ Grids & Guides and Edit ▸ Preferences ▸ Grids: a document setting. On or off (`guidesLocked`, `guidesSnap`,
    /// `guidesInBack`, `columnGuidesLocked`, `baselineGridShown`, `documentGridShown`, `documentGridSnap`, `gridsInBack`), a
    /// number in points (`baselineStart`, `baselineDivision`, `baselineThreshold` in percent, `horizontalDivision`,
    /// `verticalDivision`, `horizontalSubdivision`, `verticalSubdivision`), `baselineRelativeTo` (`page` or `margin`), or
    /// a colour (`baselineColor`, `gridColor`: an interface colour name or three numbers). Edit ▸ Preferences ▸ Units &
    /// Increments and Guides & Pasteboard add `horizontalUnits`, `verticalUnits`, `strokeUnits`, `rulerOrigin`,
    /// `cursorKeyIncrement`, `leadingKeyIncrement`, `baselineShiftKeyIncrement`, `kerningKeyIncrement`, `snapZone`,
    /// `rulerGuidesThreshold`, `matchPreviewBackground` and the colours `rulerGuidesColor`, `previewBackgroundColor`,
    /// `bleedGuideColor`, `slugGuideColor`.
    SetViewPref { key: String, value: serde_json::Value },
    /// Layout ▸ Pages ▸ Hide Spread / Show Spread.
    SetSpreadHidden { spreads: Vec<String>, hidden: bool },
    DuplicateSpread { spread: String },
    ApplyMaster { pages: Vec<String>, #[serde(default)] master: Option<String> },
    SetSection { page: String, #[serde(default)] start: Option<u32>, style: String, #[serde(default)] prefix: String, #[serde(default)] include_prefix: bool, #[serde(default)] marker: String },
    DocumentSetup { width: f64, height: f64 },
    /// Creates a page item from page-local path bounds; a line uses the two endpoints.
    CreateItem { page: String, kind: ItemKind, rect: [f64; 4], #[serde(default)] layer: Option<String>, #[serde(default = "create::six")] sides: u32 },
    /// The Pen tool: a path through `points` (page coordinates, handles optional) — a Polygon when closed, a GraphicLine
    /// when open — on the page, as a script's `entirePath` makes it (oracle: basic-paths).
    CreatePath { page: String, points: Vec<PathPt>, closed: bool, #[serde(default)] layer: Option<String> },
    /// Direct Selection: moves the anchors `indices` of the item's `path` with their handles by `by` (the item's own space).
    MovePathPoints { id: String, #[serde(default)] path: usize, indices: Vec<usize>, by: [f64; 2] },
    /// Direct Selection: moves one handle of a point to `to`, or by `by` from where it is (the item's own space); a smooth
    /// point turns its other handle to stay in line unless `independent` (Alt).
    MovePathHandle { id: String, #[serde(default)] path: usize, index: usize, side: HandleSide, #[serde(default)] to: Option<[f64; 2]>, #[serde(default)] by: Option<[f64; 2]>, #[serde(default)] independent: bool },
    /// Add Anchor Point: a point `t` (0 to 1) of the way along the segment that starts at point `segment`, the curve unchanged.
    AddPathPoint { id: String, #[serde(default)] path: usize, segment: usize, t: f64 },
    /// Delete Anchor Point: the point goes, its neighbours keep their handles.
    DeletePathPoint { id: String, #[serde(default)] path: usize, index: usize },
    /// Convert Direction Point: a smooth or corner point to a plain corner point (both handles into the anchor).
    PlainPathPoint { id: String, #[serde(default)] path: usize, index: usize },
    /// Convert Direction Point dragged out of a point: both handles to these places (the item's own space).
    SetPathHandles { id: String, #[serde(default)] path: usize, index: usize, left: [f64; 2], right: [f64; 2] },
    /// Native InDesign character indices (Unicode scalars, returns and inline objects).
    /// `story` is a Story Self, or a TextFrame Self resolving to its ParentStory.
    /// CR is Enter, LF/U+2028 is Shift+Enter; tabs are literal tab characters.
    InsertText { story: String, at: usize, text: String },
    /// Type ▸ Insert Special Character / White Space / Break Character: InDesign's special
    /// character `special` (its scripting name: `EM_DASH`, `AUTO_PAGE_NUMBER`, `COLUMN_BREAK`…)
    /// at `at`, written as InDesign writes it (oracle text-special).
    InsertSpecial { story: String, at: usize, special: String },
    /// Type ▸ Text Variables ▸ Insert Variable: an instance of the document's text variable `variable` (its `Self`) at `at`, in a
    /// range of its own (oracle variables var-insert-*). Its `ResultText` is the custom text's text; other kinds start empty and take
    /// theirs from `SetVariableResults`.
    InsertVariable { story: String, at: usize, variable: String },
    /// What the instances of text variables show now (`(the instance's Self, its text)`), stored as `ResultText` the way InDesign does
    /// when it composes: the page-dependent values come from the layout (`folio_text::variable_results`).
    SetVariableResults { results: Vec<(String, String)> },
    /// Type ▸ Text Variables ▸ Define ▸ New: a text variable `name` of `kind` (`VariableType`: `CustomTextType`, `MatchParagraphStyleType`,
    /// `MatchCharacterStyleType`, `ChapterNumberType`, `LastPageNumberType`, date and file name types) with `options` over the defaults
    /// (the preference's attributes; a custom text's `Contents`), placed where its name sorts (oracle variables var-def-*).
    DefineTextVariable { name: String, kind: String, #[serde(default)] options: std::collections::BTreeMap<String, String> },
    /// Type ▸ Text Variables ▸ Convert Variable to Text: the instances of text variables in [start, end) of the story become the text they
    /// show (oracle variables var-convert-*).
    ConvertVariableToText { story: String, start: usize, end: usize },
    /// Define ▸ Delete: the variable goes, its instances become the text they show (a parent's placeholder just goes).
    DeleteTextVariable { id: String },
    /// Define ▸ Edit: new `options` and/or a new name for the variable `id` (its `Self`); instances take the name, a custom text's its
    /// contents (var-edit-*).
    EditTextVariable { id: String, #[serde(default)] rename: Option<String>, #[serde(default)] options: std::collections::BTreeMap<String, String> },
    /// Type ▸ Change Case of [start, end): `mode` UPPERCASE, LOWERCASE, TITLECASE or
    /// SENTENCECASE (oracle text-special case-*).
    ChangeCase { story: String, start: usize, end: usize, mode: String },
    /// Deletes [start, end), including paragraph returns and inline objects.
    DeleteText { story: String, start: usize, end: usize },
    ParagraphStyle { story: String, start: usize, end: usize, style: String, #[serde(default)] clear: bool },
    CharacterStyle { story: String, start: usize, end: usize, style: String, #[serde(default)] clear: bool },
    TextFormat { story: String, start: usize, end: usize, props: std::collections::BTreeMap<String, serde_json::Value> },
    PasteText { story: String, at: usize, fragment: crate::text::TextFragment },
    StyleCreate { kind: String, name: String, #[serde(default)] story: Option<String>, #[serde(default)] start: usize, #[serde(default)] end: usize, #[serde(default)] props: std::collections::BTreeMap<String, serde_json::Value>, #[serde(default = "yes")] apply: bool, #[serde(default)] group: Option<String> },
    StyleEdit { kind: String, style: String, props: std::collections::BTreeMap<String, serde_json::Value> },
    StyleRedefine { kind: String, style: String, story: String, start: usize, end: usize },
    StyleDelete { kind: String, style: String, replacement: String },
    TextWrap { id: String, #[serde(default)] mode: Option<String>, #[serde(default)] offsets: Option<[f64;4]>, #[serde(default)] inverse: Option<bool>, #[serde(default)] master_only: Option<bool>, #[serde(default)] side: Option<String> },
    FindReplace { query: crate::text::FindQuery, #[serde(default)] text: Option<String>, #[serde(default)] props: std::collections::BTreeMap<String, serde_json::Value> },
    /// Moves objects by a distance in spread coordinates (points, y down).
    Move { ids: Vec<String>, by: [f64; 2] },
    /// Reparents objects to a destination spread, then offsets in spread coordinates.
    MoveToSpread { ids: Vec<String>, spread: String, #[serde(default)] by: [f64; 2] },
    /// Align/distribute using the stroke outline in spread coordinates (oracle: align-*,
    /// distribute-*). A key object stays fixed; `gap` is the requested distance in points.
    Align { ids: Vec<String>, how: AlignMode, #[serde(default)] bounds: AlignBounds, #[serde(default)] key: Option<String> },
    Distribute { ids: Vec<String>, how: AlignMode, #[serde(default)] bounds: AlignBounds, #[serde(default)] key: Option<String>, #[serde(default)] gap: Option<f64> },
    /// Sets an object's width and height as the control panel shows them: in its own
    /// (rotated) frame, stroke included, the `anchor` reference point staying put.
    /// With `include_stroke` off (Transform panel menu ▸ Dimensions Include Stroke Weight) the size is the path's.
    Resize { id: String, size: [f64; 2], #[serde(default)] anchor: Anchor, #[serde(default = "yes")] include_stroke: bool },
    /// Sets the path's bounds in the item's own space to `rect` (`[x0, y0, x1, y1]`): a
    /// selection handle dragged. The same change as `Resize` to the box it leads to.
    ResizeBox { id: String, rect: [f64; 4] },
    /// Resizes selected paths together in spread axes (true: control W/H, false: handles).
    ResizeMany { ids: Vec<String>, size: [f64; 2], #[serde(default)] anchor: Anchor, #[serde(default = "yes")] include_stroke: bool },
    Delete { ids: Vec<String> },
    /// Object lock and visibility (Object ▸ Lock, Hide).
    SetItem { id: String, #[serde(default)] locked: Option<bool>, #[serde(default)] visible: Option<bool> },
    SetLayer { id: String, #[serde(default)] visible: Option<bool>, #[serde(default)] locked: Option<bool>, #[serde(default)] printable: Option<bool>,
        #[serde(default)] name: Option<String>, #[serde(default)] color: Option<String>, #[serde(default)] show_guides: Option<bool>, #[serde(default)] lock_guides: Option<bool> },
    /// A colour swatch (`Color/<name>`): process or spot, CMYK/RGB/Lab values as the Color panel gives them.
    NewSwatch { name: String, #[serde(default)] spot: bool, space: String, value: Vec<f64> },
    /// A colour swatch's values, space, type (spot/process) or name.
    SetSwatch { id: String, #[serde(default)] value: Option<Vec<f64>>, #[serde(default)] space: Option<String>, #[serde(default)] spot: Option<bool>, #[serde(default)] rename: Option<String> },
    /// Deletes a swatch, putting `replace` everywhere it was used.
    DeleteSwatch { id: String, replace: String },
    NewTint { base: String, tint: f64 },
    DuplicateSwatch { id: String },
    /// Ink Manager: one ink's type (Normal, Transparent, Opaque, OpaqueIgnore), spot-to-process and Ink Alias
    /// (`alias`: the ink to print on, "" for none).
    SetInk { name: String, #[serde(default)] ink_type: Option<String>, #[serde(default)] convert_to_process: Option<bool>, #[serde(default)] alias: Option<String> },
    /// Ink Manager ▸ All Spots to Process.
    AllSpotsToProcess { on: bool },
    /// A ruler guide dragged out of a ruler: `at` points from the top (horizontal) or left
    /// (vertical) edge of `page`; `spread` for a spread guide (Ctrl-drag).
    NewGuide { page: String, orientation: GuideAxis, at: f64, #[serde(default)] spread: bool, #[serde(default)] layer: Option<String> },
    /// Layout ▸ Create Guides on every page of `page`'s spread: `rows` × `columns` with their
    /// gutters, fitted to the margins or the page, after removing the spread's ruler guides
    /// when `remove_existing` (oracle basic-layout guides-create-*).
    CreateGuides { page: String, rows: u32, columns: u32, #[serde(default)] row_gutter: f64, #[serde(default)] column_gutter: f64, #[serde(default)] fit_margins: bool, #[serde(default)] remove_existing: bool },
    /// Layout ▸ Margins and Columns on pages: IDML's Top, Bottom, Left and Right (inside and
    /// outside on facing pages), column count and gutter (oracle basic-layout margins-*).
    SetMargins { pages: Vec<String>, #[serde(default)] top: Option<f64>, #[serde(default)] bottom: Option<f64>, #[serde(default)] left: Option<f64>, #[serde(default)] right: Option<f64>, #[serde(default)] columns: Option<u32>, #[serde(default)] gutter: Option<f64> },
    /// Drags a guide across: `at` as for NewGuide, measured on `page` of the guide's spread.
    MoveGuide { id: String, page: String, at: f64 },
    DeleteGuide { id: String },
    /// Object ▸ Corner Options: all four corners (top left, top right, bottom left, bottom
    /// right) of each object.
    SetCorners { ids: Vec<String>, corners: [Corner; 4] },
    /// Opacity (percent) and/or blend mode (IDML name: "Multiply") of a level of each object.
    SetBlending { ids: Vec<String>, level: BlendLevel, #[serde(default)] opacity: Option<f64>, #[serde(default)] mode: Option<String>,
        /// Effects panel ▸ Isolate Blending, Knockout Group (the object level).
        #[serde(default)] isolate: Option<bool>, #[serde(default)] knockout: Option<bool> },
    /// Object ▸ Text Frame Options (columns, insets, vertical justification, first baseline,
    /// auto-size).
    TextFrameOptions { id: String, options: FrameOptions },
    /// Auto-Size's result for a frame: its path's width and height, the reference point
    /// (fractions across and down) staying put (what composing its story gives).
    FitFrame { id: String, size: [f64; 2], point: [f64; 2] },
    /// The Object Styles panel: a click (`clear` false) or an Alt-click (`clear` true).
    ApplyObjectStyle { ids: Vec<String>, style: String, #[serde(default)] clear: bool },
    /// Clear Overrides: the object's own values give way to its style.
    ClearObjectStyleOverrides { ids: Vec<String> },
    /// Pages panel ▸ New Parent (pages: 1 or 2; prefix and name, or the next free ones).
    NewParent { #[serde(default)] pages: Option<u32>, #[serde(default)] prefix: Option<String>, #[serde(default)] name: Option<String>, #[serde(default)] based_on: Option<String> },
    DuplicateParent { id: String },
    /// Ctrl+Shift+click on a parent item, Pages ▸ Override All Parent Page Items: copies of the page's parent items (`ids`; none:
    /// every one that shows on the page) onto `page`.
    OverrideParentItems { page: String, #[serde(default)] ids: Vec<String> },
    /// Parent Pages ▸ Remove All Local Overrides: the page's copies (`ids`; none: all) go, the parent items show again.
    RemoveLocalOverrides { page: String, #[serde(default)] ids: Vec<String> },
    /// Pages panel ▸ Page Attributes ▸ Color Label: `color` is "UseMasterColor", "Nothing" or an interface colour (IDML name) for the
    /// pages and parents (`masters`: a parent's own colour is on its spread and each of its pages).
    SetPageColor { #[serde(default)] pages: Vec<String>, #[serde(default)] masters: Vec<String>, color: String },
    /// Pages panel ▸ Parent Pages ▸ Show Parent Items, for the spreads.
    SetSpreadShowMaster { spreads: Vec<String>, show: bool },
    /// Pages panel ▸ Page Attributes ▸ Spread Flattening: `level` Default, None or Custom; the custom values (raster/vector
    /// balance 0–100, line art and text resolution 1–9600, gradient and mesh resolution 1–1200 ppi, clip complex regions, convert
    /// all strokes / text to outlines) are kept with the spread whatever the level.
    SetSpreadFlattener { spreads: Vec<String>, #[serde(default)] level: Option<String>, #[serde(default)] balance: Option<f64>, #[serde(default)] line_art: Option<f64>,
        #[serde(default)] gradient: Option<f64>, #[serde(default)] clip: Option<bool>, #[serde(default)] strokes: Option<bool>, #[serde(default)] text: Option<bool> },
    /// Parent Pages ▸ Detach All Objects from Parent: the page's copies (`ids`; none: all) stand alone, their parent items stay off the page.
    DetachFromParent { page: String, #[serde(default)] ids: Vec<String> },
    /// Allow Parent Item Overrides on Selection (items of a parent page).
    AllowParentOverrides { ids: Vec<String>, allow: bool },
    /// The control panel's rotation and shear angles as values, about the reference point of
    /// the object's own bounds.
    /// `content`: the placed graphic of the frame `id` (the control panel with the content selected).
    /// `totals`: Transformations are Totals (Transform panel menu; rules/transform-totals.ko.md). On, the angle is the object's total
    /// on the page (its group's turn counted in), off the one relative to its group. A top-level object has no group: the same. Omitted: on.
    SetAngles { id: String, #[serde(default)] rotation: Option<f64>, #[serde(default)] shear: Option<f64>, #[serde(default)] anchor: Anchor, #[serde(default)] content: bool, #[serde(default = "yes")] totals: bool },
    /// The control panel's scale percentages (factors), applied to the content.
    Scale { id: String, sx: f64, sy: f64, #[serde(default)] anchor: Anchor, #[serde(default)] adjust: Option<scale::Adjust> },
    /// The Stroke panel: cap, join, miter limit, alignment, type, arrowheads, gap.
    SetStroke { ids: Vec<String>, change: StrokeChange },
    /// The Effects dialog: one effect of a level of each object, switched on or off and/or with
    /// settings changed (IDML attribute names: `{"Size": 12, "EffectColor": "Color/Black"}`; a
    /// drop shadow also takes `Distance` and `Angle` for its offsets).
    SetEffect { ids: Vec<String>, level: BlendLevel, effect: Effect, #[serde(default)] on: Option<bool>, #[serde(default)] values: std::collections::BTreeMap<String, serde_json::Value> },
    /// Object ▸ Effects ▸ Global Light: the document's light angle and altitude (degrees).
    SetGlobalLight { #[serde(default)] angle: Option<f64>, #[serde(default)] altitude: Option<f64> },
    DeleteParent { id: String },
    /// Parent Page Options: prefix, name, based on (a parent's Self, or "n" for [None]).
    ParentOptions { id: String, #[serde(default)] prefix: Option<String>, #[serde(default)] name: Option<String>, #[serde(default)] based_on: Option<String> },
    /// A new layer above `above` (default: the active layer); it becomes active.
    NewLayer { #[serde(default)] name: Option<String>, #[serde(default)] color: Option<String>, #[serde(default)] above: Option<String> },
    /// A layer directly above or below another one.
    MoveLayer { id: String, #[serde(default)] above: Option<String>, #[serde(default)] below: Option<String> },
    DeleteLayer { id: String },
    /// Layers panel ▸ Duplicate Layer: "<name> copy" above it, with copies of everything on it.
    DuplicateLayer { id: String },
    /// Layers panel ▸ Merge Layers: the objects of `ids` go to `into`, those layers go.
    MergeLayers { into: String, ids: Vec<String> },
    /// Layers panel ▸ Delete Unused Layers.
    DeleteUnusedLayers,
    /// Layers panel ▸ Unlock All (every object lock in the document); `page`: only the spread
    /// of that page (Object ▸ Unlock All on Spread).
    UnlockAll { #[serde(default)] page: Option<String> },
    /// Object ▸ Effects ▸ Clear Effects: the object level's effects (opacity and blending stay).
    ClearEffects { ids: Vec<String> },
    /// Object ▸ Effects ▸ Clear All Transparency: every level's opacity, blending and effects.
    ClearAllTransparency { ids: Vec<String> },
    /// Object ▸ Transform ▸ Clear Transformations: no rotation, shear, scale or flip; the path's
    /// middle stays.
    ClearTransformations { ids: Vec<String> },
    /// Transform panel menu ▸ Redefine Scaling as 100 %: a scale in the transform goes into the path.
    RedefineScaling { ids: Vec<String> },
    /// Object ▸ Content ▸ Graphic / Text / Unassigned of empty frames (oracle basic-object-menu2/3).
    SetContentType { ids: Vec<String>, kind: ContentKind },
    ActiveLayer { id: String },
    /// Objects to the top of a layer.
    ToLayer { ids: Vec<String>, layer: String },
    /// Object ▸ Arrange.
    Arrange { id: String, how: Arrange },
    /// Applies a matrix in spread coordinates (`[a, b, c, d, tx, ty]`) to the objects' own
    /// transforms: rotating and flipping about a reference point. InDesign changes only
    /// `ItemTransform` for these (oracle: rotate-*, flip-*); scaling is not this command.
    Transform { ids: Vec<String>, matrix: [f64; 6] },
    /// Type ▸ Bulleted & Numbered Lists ▸ Apply/Remove Bullets (`numbers` false) and Apply/Remove Numbers on the
    /// paragraphs of the range.
    ToggleList { story: String, start: usize, end: usize, numbers: bool },
    /// Type ▸ Bulleted & Numbered Lists ▸ Restart/Continue Numbering.
    ToggleNumberingRestart { story: String, start: usize, end: usize },
    /// Object ▸ Transform ▸ Transform Again, Transform Sequence Again and the Individually
    /// variants: repeats what the editor remembers (the last move, rotation, scale or flip; with
    /// `sequence` all of those since the selection last changed) on `ids`, each step about the
    /// reference point of the objects together, or of each object with `individually` (oracle:
    /// basic-transform-again).
    TransformAgain { ids: Vec<String>, #[serde(default)] sequence: bool, #[serde(default)] individually: bool },
    /// Rotates by `by` degrees (counter-clockwise, as InDesign) about the reference point of
    /// the objects' bounds together (stroke included): the control panel's rotation.
    Rotate { ids: Vec<String>, by: f64, #[serde(default = "center")] anchor: Anchor },
    /// The Rotate tool: rotates by `by` degrees (counter-clockwise) about `point`, a point of the spread (IDML spread coordinates),
    /// the same matrix as the control panel's rotation about a point of the bounds (not remembered for Transform Again).
    RotateAbout { ids: Vec<String>, by: f64, point: [f64; 2] },
    /// Shears by `by` degrees (positive leans to the right, as the control panel's shear angle) along
    /// the horizontal axis, about the reference point of the objects together: Object ▸ Transform ▸
    /// Shear… (oracle: basic-transform-shear).
    Shear { ids: Vec<String>, by: f64, #[serde(default = "center")] anchor: Anchor, #[serde(default)] adjust: Option<scale::Adjust> },
    /// Mirrors on the page about the reference point: the control panel's flip buttons.
    Flip { ids: Vec<String>, axis: Axis, #[serde(default = "center")] anchor: Anchor },
    /// Object ▸ Paths ▸ Make Compound Path: the rearmost of `ids` takes the others' paths (oracle basic-compound).
    MakeCompoundPath { ids: Vec<String> },
    /// Object ▸ Paths ▸ Release Compound Path: each path after the first becomes an object of its own.
    ReleaseCompoundPath { id: String },
    /// Object ▸ Group (Ctrl+G): siblings into a new group in the frontmost one's place.
    Group { ids: Vec<String> },
    /// Object ▸ Ungroup (`remember_layers`: the Layers panel's Ungroup Remembers Layers, default on - the
    /// members go back to the layers they had when grouped; off: all on the group's layer).
    Ungroup { id: String, #[serde(default)] remember_layers: Option<bool> },
    /// Edit ▸ Duplicate and Alt+drag: copies right above each original, moved by `by`.
    Duplicate { ids: Vec<String>, #[serde(default)] by: [f64; 2] },
    /// Fill and stroke: swatch (`Color/…`, `Swatch/None`), tint, stroke weight.
    SetPaint {
        ids: Vec<String>,
        #[serde(default)] fill: Option<String>,
        #[serde(default)] fill_tint: Option<f64>,
        #[serde(default)] stroke: Option<String>,
        #[serde(default)] stroke_tint: Option<f64>,
        #[serde(default)] stroke_weight: Option<f64>,
    },
    /// Moves the graphic inside a frame (direct selection, content grabber).
    MoveContent { id: String, by: [f64; 2] },
    /// Object ▸ Fitting.
    Fit { id: String, how: Fit },
    /// A script label (InDesign's insertLabel): `key` → `value` on the object.
    SetLabel { id: String, key: String, value: String },
    /// Object ▸ Primary Text Frame (a text frame of a parent): the frame becomes the parent's primary text frame (`primary`),
    /// or the parent has none (`MasterSpread@PrimaryTextFrame="n"`).
    SetPrimaryTextFrame { id: String, primary: bool },
    /// Table ▸ Insert Table at a story position (its end without one).
    InsertTable { story: String, #[serde(default)] at: Option<usize>, rows: u32, columns: u32, #[serde(default)] header: u32, #[serde(default)] footer: u32 },
    /// Table ▸ Convert Text to Table: paragraphs to rows, tabs to columns (to the story's end without `end`).
    ConvertToTable { story: String, #[serde(default)] start: usize, #[serde(default)] end: Option<usize> },
    /// Table ▸ Insert ▸ Row(s) beside row `at` (after it unless `before`).
    InsertRows { table: String, at: usize, #[serde(default = "one")] count: u32, #[serde(default)] before: bool },
    /// Table ▸ Insert ▸ Column(s) beside column `at`.
    InsertColumns { table: String, at: usize, #[serde(default = "one")] count: u32, #[serde(default)] before: bool },
    /// Table ▸ Paste Before / Paste After with rows copied (rows `from..=to` of `source`, this table without one) beside row `at`.
    PasteRows { table: String, #[serde(default)] source: Option<String>, from: usize, to: usize, at: usize, #[serde(default)] before: bool },
    /// Table ▸ Paste Before / Paste After with columns copied beside column `at`.
    PasteColumns { table: String, #[serde(default)] source: Option<String>, from: usize, to: usize, at: usize, #[serde(default)] before: bool },
    /// Table ▸ Delete ▸ Row.
    DeleteRows { table: String, at: usize, #[serde(default = "one")] count: u32 },
    /// Table ▸ Delete ▸ Column.
    DeleteColumns { table: String, at: usize, #[serde(default = "one")] count: u32 },
    /// Table ▸ Merge Cells: the rectangle from `from` to `to` (`[column, row]`, as cell names).
    MergeCells { table: String, from: [usize; 2], to: [usize; 2] },
    /// Table ▸ Unmerge Cells.
    UnmergeCell { table: String, cell: [usize; 2] },
    /// Table ▸ Split Cell Horizontally (a new row) or Vertically (a new column).
    SplitCell { table: String, cell: [usize; 2], #[serde(default)] vertical: bool },
    /// Row height (an auto-grow row's minimum) and whether rows grow with their text.
    SetRows { table: String, rows: Vec<usize>, #[serde(default)] height: Option<f64>, #[serde(default)] auto_grow: Option<bool> },
    /// Column width.
    SetColumns { table: String, columns: Vec<usize>, width: f64 },
    /// Cell Options.
    SetCells { table: String, cells: Vec<[usize; 2]>, change: CellChange },
    /// Table ▸ Convert Cell to Graphic Cell / Text Cell (`graphic`).
    SetCellType { table: String, cells: Vec<[usize; 2]>, graphic: bool },
    /// Table Options ▸ header rows.
    SetHeaderRows { table: String, header: usize },
    /// Table Options ▸ Row Strokes / Column Strokes / Fills: the alternating pattern attributes of the table (IDML names such as
    /// `StartRowFillCount`, values as IDML writes them; a value equal to the table style's goes away). Oracle: tables-alternating.
    SetTableOptions { table: String, options: std::collections::BTreeMap<String, String> },
    /// Cell Styles panel: the cell style `style` (its `Self`: `CellStyle/Name`, `CellStyle/$ID/[None]`) on the cells ([column, row]) of a
    /// table, in that order (oracle tablestyles cs-apply-*).
    ApplyCellStyle { table: String, cells: Vec<[usize; 2]>, style: String },
    /// Clear Cell Style Overrides.
    ClearCellOverrides { table: String, cells: Vec<[usize; 2]> },
    /// Table Styles panel: the table style `style` (`TableStyle/Name`, `TableStyle/$ID/[No table style]`) on the table (ts-apply-*).
    ApplyTableStyle { table: String, style: String },
    /// Clear Table Style Overrides.
    ClearTableOverrides { table: String },
    /// Cell Styles / Table Styles ▸ New Style: the settings are IDML's attributes of the style (`FillColor`, `StartRowFillCount`, …;
    /// `BasedOn`: the style it is based on; a region's cell style also makes the region its own) (oracle tablestyles cs-create*, ts-create).
    CreateTableStyle { kind: TableStyleKind, name: String, #[serde(default)] props: std::collections::BTreeMap<String, String>, #[serde(default)] group: Option<String> },
    /// Style Options: new settings (an empty value takes a setting back) and a new name; references follow (cs-edit*, ts-edit).
    EditTableStyle { kind: TableStyleKind, style: String, #[serde(default)] rename: Option<String>, #[serde(default)] props: std::collections::BTreeMap<String, String> },
    /// Delete Style: what has it gets `replacement` as if it were applied (cs-delete*, ts-delete).
    DeleteTableStyle { kind: TableStyleKind, style: String, replacement: String },
    /// Duplicate Style: `<Name> copy` (or `name`) with the settings and the base of `style`, next to it (cs-duplicate, ts-duplicate).
    DuplicateTableStyle { kind: TableStyleKind, style: String, #[serde(default)] name: Option<String> },
    /// New Style Group, at the top level or in `parent` (a group's `Self`, `CellStyleGroup/$ID/Grp`) (cs-group-new, cs-group-nested).
    CreateTableStyleGroup { kind: TableStyleKind, name: String, #[serde(default)] parent: Option<String> },
    /// Rename a style group; the styles and groups in it and every reference follow (cs-group-rename).
    RenameTableStyleGroup { kind: TableStyleKind, group: String, name: String },
    /// Delete an empty style group.
    DeleteTableStyleGroup { kind: TableStyleKind, group: String },
    /// Move a style into a group (no `group`: to the top level) (cs-group-move, ts-group-move).
    MoveTableStyle { kind: TableStyleKind, style: String, #[serde(default)] group: Option<String> },
    /// Table ▸ Delete ▸ Table.
    DeleteTable { table: String },
    /// Edit ▸ Paste (`at`: the objects' middle, page coordinates), Paste in Place (no `at`),
    /// Paste Into (`into` a frame); on `page`'s spread and `layer` (default: the active one;
    /// `remember_layers`, the Layers panel's Paste Remembers Layers: each object on the layer it was
    /// copied from).
    Paste { #[serde(default)] page: Option<String>, #[serde(default)] at: Option<[f64; 2]>, #[serde(default)] layer: Option<String>, #[serde(default)] into: Option<String>, #[serde(default)] remember_layers: bool },
    /// Object ▸ Convert Shape, Object ▸ Paths ▸ Open / Close Path.
    ConvertShape { ids: Vec<String>, to: Shape },
    /// Object ▸ Paths ▸ Reverse Path.
    ReversePath { ids: Vec<String> },
    /// Table Options ▸ footer rows.
    SetFooterRows { table: String, footer: usize },
    /// Table ▸ Convert Rows ▸ To Header / Body / Footer.
    ConvertRows { table: String, at: usize, #[serde(default = "one_usize")] count: usize, to: RowKind },
    /// Table ▸ Distribute Rows Evenly over rows [first, last].
    DistributeRows { table: String, first: usize, last: usize },
    /// Table ▸ Distribute Columns Evenly over columns [first, last].
    DistributeColumns { table: String, first: usize, last: usize },
    /// Table ▸ Convert Table to Text.
    ConvertTableToText { table: String },
    /// Composed heights of a table's auto-grow rows (from composing the story).
    SetRowHeights { table: String, heights: Vec<f64> },
    /// The control panel's scale percentages with several objects selected (factors), applied to
    /// the content, about the reference point of the selection (oracle: ctl2-scale-many-*).
    ScaleMany { ids: Vec<String>, sx: f64, sy: f64, #[serde(default)] anchor: Anchor, #[serde(default)] adjust: Option<scale::Adjust> },
    /// Place ▸ a text file into a text frame or an empty shape (`id`: its text is replaced), a new frame at `rect` on `page`, or the
    /// frame a click at `point` on `page` makes (`ops/place_text.rs`). `source` is what
    /// `folio_textimport::read` made of the file (the server reads it from `file`); nothing links back to the file (oracle placetext-c1).
    PlaceText { source: folio_textimport::TextSource, #[serde(default)] id: Option<String>, #[serde(default)] page: Option<String>, #[serde(default)] rect: Option<[f64; 4]>, #[serde(default)] layer: Option<String>, #[serde(default)] point: Option<[f64; 2]> },
    #[serde(untagged)]
    ObjectStyleDefinition(ObjectStyleDefinition),
    #[serde(untagged)]
    Registered(crate::commands::RegisteredCommand),
    #[serde(untagged)]
    Color(color_commands::ColorCommand),
}

fn one() -> u32 { 1 }
fn one_usize() -> usize { 1 }

fn center() -> Anchor {
    Anchor::Center
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Horizontal,
    Vertical,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    Proportionally,
    Fill,
    ContentToFrame,
    FrameToContent,
    Center,
}

/// What an empty frame is for (Object ▸ Content).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    Graphic,
    Text,
    Unassigned,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// Position in the box, 0–1 left to right and top to bottom.
    pub fn at(self) -> (f64, f64) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Arrange {
    Front,
    Forward,
    Backward,
    Back,
}

pub(crate) fn run(w: &mut Work<'_>, c: &Command) -> Result<(), EditError> {
    match c {
        Command::ObjectStyleDefinition(definition) => objectstyledefs::run(w, definition),
        Command::Registered(command) => command.run(w),
        Command::PlaceText { source, id, page, rect, layer, point } => place_text::run(w, source, id.as_deref(), page.as_deref(), *point, *rect, layer.as_deref()),
        Command::Color(c) => c.run(w),
        Command::ThreadText { id, next } => threading::run(w, id, next.as_deref()),
        Command::CreateThreadFrame { id, page, rect, layer } => {
            create::run(w, page, ItemKind::TextFrame, *rect, layer.as_deref(), 6, None)?;
            let next=w.created.last().unwrap().clone();
            threading::run(w,id,Some(&next))
        },
        Command::PlaceGraphic { source, id, page, point, rect, layer } => place::run(w, source, id.as_deref(), page.as_deref(), *point, *rect, layer.as_deref(), false),
        Command::RelinkGraphic { source, id } => place::relink(w, source, id),
        Command::EmbedLink { id, data } => place::embed(w, id, data),
        Command::UnembedLink { id, uri } => place::unembed(w, id, uri),
        Command::InsertPages { .. } | Command::DeletePages { .. } | Command::MovePages { .. }
        | Command::SetPageShuffle { .. } | Command::SetSpreadHidden { .. } | Command::DuplicateSpread { .. } | Command::ApplyMaster { .. }
        | Command::SetSection { .. } | Command::DocumentSetup { .. } => pages::run(w, c),
        Command::CreateItem { page, kind, rect, layer, sides } => create::run(w, page, *kind, *rect, layer.as_deref(), *sides, None),
        Command::CreatePath { page, points, closed, layer } => {
            let pts: Vec<PathPoint> = points.iter().map(PathPt::point).collect();
            let need = if *closed { 3 } else { 2 };
            if pts.len() < need || pts.iter().any(|p| [p.anchor, p.left, p.right].iter().any(|q| !q.0.is_finite() || !q.1.is_finite())) {
                return Err(EditError::new("invalid", format!("a path needs at least {need} points")));
            }
            let b = geom::path_bounds(&[(pts.clone(), *closed)]).ok_or_else(|| EditError::new("invalid", "empty path"))?;
            // the box only has to be a valid one: the points decide the shape (a path with no width, a horizontal line, is fine)
            let rect = [b[0], b[1], b[2].max(b[0] + 1e-3), b[3].max(b[1] + 1e-3)];
            create::run(w, page, if *closed { ItemKind::Polygon } else { ItemKind::Line }, rect, layer.as_deref(), 6, Some(&pts))
        }
        Command::MovePathPoints { id, path, indices, by } => pathedit::move_points(w, id, *path, indices, *by),
        Command::AddPathPoint { id, path, segment, t } => pathedit::add_point(w, id, *path, *segment, *t),
        Command::DeletePathPoint { id, path, index } => pathedit::delete_point(w, id, *path, *index),
        Command::PlainPathPoint { id, path, index } => pathedit::plain_point(w, id, *path, *index),
        Command::SetPathHandles { id, path, index, left, right } => pathedit::set_handles(w, id, *path, *index, *left, *right),
        Command::MovePathHandle { id, path, index, side, to, by, independent } => pathedit::move_handle(w, id, *path, *index, *side, *to, *by, *independent),
        Command::InsertText { story, at, text } => crate::text::insert(w, story, *at, text),
        Command::InsertSpecial { story, at, special } => crate::text::insert_special(w, story, *at, special),
        Command::InsertVariable { story, at, variable } => crate::text::insert_variable(w, story, *at, variable),
        Command::SetVariableResults { results } => set_variable_results(w, results),
        Command::DefineTextVariable { name, kind, options } => textvariables::define(w, name, kind, options),
        Command::ConvertVariableToText { story, start, end } => crate::text::convert_variables(w, story, *start, *end),
        Command::DeleteTextVariable { id } => textvariables::delete(w, id),
        Command::EditTextVariable { id, rename, options } => textvariables::edit(w, id, rename.as_deref(), options),
        Command::ChangeCase { story, start, end, mode } => crate::text::change_case(w, story, *start, *end, mode),
        Command::DeleteText { story, start, end } => crate::text::delete(w, story, *start, *end),
        Command::ParagraphStyle { story, start, end, style, clear } => crate::text::format::apply_style(w, story, *start, *end, style, true, *clear),
        Command::CharacterStyle { story, start, end, style, clear } => crate::text::format::apply_style(w, story, *start, *end, style, false, *clear),
        Command::TextFormat { story, start, end, props } => crate::text::format::format(w, story, *start, *end, props),
        Command::ToggleList { story, start, end, numbers } => crate::text::format::toggle_list(w, story, *start, *end, *numbers),
        Command::ToggleNumberingRestart { story, start, end } => crate::text::format::toggle_numbering_restart(w, story, *start, *end),
        Command::PasteText { story, at, fragment } => crate::text::format::paste(w, story, *at, fragment),
        Command::StyleCreate { .. } | Command::StyleEdit { .. } | Command::StyleRedefine { .. } | Command::StyleDelete { .. } => crate::text::format::styles::run(w,c),
        Command::FindReplace {query,text,props} => crate::text::format::find::replace(w,query,text.as_deref(),props),
        Command::TextWrap { id,mode,offsets,inverse,master_only,side } => wrap::run(w,id,mode.as_deref(),*offsets,*inverse,*master_only,side.as_deref()),
        Command::Move { ids, by } => {
            let ids = expand(w, ids);
            move_items(w, &ids, (by[0], by[1]))
        }
        Command::MoveToSpread { ids, spread, by } => { let ids = expand(w, ids); relocate::run(w, &ids, spread, *by) }
        Command::Align { ids, how, bounds, key } => alignment::run(w, ids, *how, *bounds, key.as_deref(), None, false),
        Command::Distribute { ids, how, bounds, key, gap } => alignment::run(w, ids, *how, *bounds, key.as_deref(), *gap, true),
        Command::Resize { id, size, anchor, include_stroke } => resize(w, id, Target::Size((size[0], size[1]), *anchor, *include_stroke)),
        Command::ResizeBox { id, rect } => resize(w, id, Target::Box(*rect)),
        Command::ResizeMany { ids, size, anchor, include_stroke } => resize_group::many(w, ids, *size, *anchor, *include_stroke),
        Command::Delete { ids } => {
            for id in ids {
                delete(w, id)?;
            }
            Ok(())
        }
        Command::SetItem { id, locked, visible } => {
            let mut set = Vec::new();
            if let Some(l) = locked {
                set.push(("Locked", bool_text(*l)));
            }
            if let Some(v) = visible {
                set.push(("Visible", bool_text(*v)));
            }
            set_attrs(w, id, &set)
        }
        Command::NewSwatch { name, spot, space, value } => swatches::new_swatch(w, name, *spot, space, value),
        Command::SetSwatch { id, value, space, spot, rename } => swatches::set_swatch(w, id, value.as_deref(), space.as_deref(), *spot, rename.as_deref()),
        Command::DeleteSwatch { id, replace } => swatches::delete_swatch(w, id, replace),
        Command::NewTint { base, tint } => swatches::new_tint(w, base, *tint),
        Command::DuplicateSwatch { id } => swatches::duplicate_swatch(w, id),
        Command::SetInk { name, ink_type, convert_to_process, alias } => inks::set_ink(w, name, ink_type.as_deref(), *convert_to_process, alias.as_deref()),
        Command::AllSpotsToProcess { on } => inks::all_spots(w, *on),
        Command::CreateGuides { page, rows, columns, row_gutter, column_gutter, fit_margins, remove_existing } =>
            guides::create_guides(w, page, *rows, *columns, *row_gutter, *column_gutter, *fit_margins, *remove_existing),
        Command::SetMargins { pages, top, bottom, left, right, columns, gutter } => guides::set_margins(w, pages, [*top, *bottom, *left, *right], *columns, *gutter),
        Command::NewGuide { page, orientation, at, spread, layer } => guides::new_guide(w, guides::At { page, horizontal: *orientation == GuideAxis::Horizontal, at: *at }, *spread, layer.as_deref()),
        Command::MoveGuide { id, page, at } => guides::move_guide(w, id, page, *at),
        Command::DeleteGuide { id } => guides::delete_guide(w, id),
        Command::SetCorners { ids, corners } => corners::set_corners(w, ids, corners),
        Command::SetBlending { ids, level, opacity, mode, isolate, knockout } =>
            blending::set_blending(w, ids, *level, *opacity, mode.as_deref(), blending::Group { isolate: *isolate, knockout: *knockout }),
        Command::TextFrameOptions { id, options } => textframe::set_text_frame(w, id, options),
        Command::FitFrame { id, size, point } => textframe::fit_frame(w, id, *size, *point),
        Command::ApplyObjectStyle { ids, style, clear } => objectstyle::apply(w, ids, style, *clear),
        Command::ClearObjectStyleOverrides { ids } => objectstyle::clear_overrides(w, ids),
        Command::NewParent { pages, prefix, name, based_on } => parents::new_parent(w, *pages, prefix.as_deref(), name.as_deref(), based_on.as_deref()),
        Command::DuplicateParent { id } => parents::duplicate_parent(w, id),
        Command::OverrideParentItems { page, ids } => overrides::override_items(w, page, ids),
        Command::RemoveLocalOverrides { page, ids } => overrides::remove_overrides(w, page, ids),
        Command::DetachFromParent { page, ids } => overrides::detach(w, page, ids),
        Command::AllowParentOverrides { ids, allow } => overrides::set_allow(w, ids, *allow),
        Command::SetPageColor { pages, masters, color } => pagesettings::set_page_color(w, pages, masters, color),
        Command::SetSpreadShowMaster { spreads, show } => pagesettings::set_show_master(w, spreads, *show),
        Command::SetSpreadFlattener { spreads, level, balance, line_art, gradient, clip, strokes, text } => {
            pagesettings::set_flattener(w, spreads, level.as_deref(), pagesettings::Custom { balance: *balance, line_art: *line_art, gradient: *gradient, clip: *clip, strokes: *strokes, text: *text })
        }
        Command::SetStroke { ids, change } => stroke::set_stroke(w, ids, change),
        Command::SetEffect { ids, level, effect, on, values } => effects::set_effect(w, ids, *level, *effect, *on, values),
        Command::SetGlobalLight { angle, altitude } => effects::set_global_light(w, *angle, *altitude),
        Command::Scale { id, sx, sy, anchor, adjust } => {
            for id in expand(w, std::slice::from_ref(id)) {
                scale::scale(w, &id, *sx, *sy, *anchor, *adjust)?;
            }
            Ok(())
        }
        Command::SetAngles { id, rotation, shear, anchor, content, totals } => angles::set_angles(w, id, *rotation, *shear, *anchor, *content, *totals),
        Command::ScaleMany { ids, sx, sy, anchor, adjust } => scale::scale_many(w, &expand(w, ids), *sx, *sy, *anchor, *adjust),
        Command::DeleteParent { id } => parents::delete_parent(w, id),
        Command::ParentOptions { id, prefix, name, based_on } => parents::parent_options(w, id, prefix.as_deref(), name.as_deref(),
            based_on.as_deref().map(|b| (b != "n").then_some(b))),
        Command::NewLayer { name, color, above } => layers::new_layer(w, name.as_deref(), color.as_deref(), above.as_deref()),
        Command::MoveLayer { id, above, below } => layers::move_layer(w, id, above.as_deref(), below.as_deref()),
        Command::DeleteLayer { id } => layers::delete_layer(w, id),
        Command::DuplicateLayer { id } => layers::duplicate_layer(w, id),
        Command::MergeLayers { into, ids } => layers::merge_layers(w, into, ids),
        Command::DeleteUnusedLayers => layers::delete_unused(w),
        Command::UnlockAll { page } => layers::unlock_all(w, page.as_deref()),
        Command::ClearEffects { ids } => clear::clear_effects(w, ids),
        Command::ClearAllTransparency { ids } => clear::clear_all_transparency(w, ids),
        Command::ClearTransformations { ids } => clear::clear_transformations(w, ids),
        Command::RedefineScaling { ids } => {
            for id in expand(w, ids) {
                scale::redefine(w, &id)?;
            }
            Ok(())
        }
        Command::SetContentType { ids, kind } => clear::set_content_type(w, ids, *kind),
        Command::ActiveLayer { id } => layers::set_active(w, id),
        Command::ToLayer { ids, layer } => layers::to_layer(w, ids, layer),
        Command::SetLayer { id, visible, locked, printable, name, color, show_guides, lock_guides } => {
            layers::set_options(w, id, name.as_deref(), color.as_deref(), *show_guides, *lock_guides)?;
            let (part, ix, i) = w.find(id)?;
            if ix.local(w.text(&part)?, i) != "Layer" {
                return Err(EditError::new("invalid", format!("{id} is not a layer")));
            }
            let mut set = Vec::new();
            for (name, v) in [("Visible", visible), ("Locked", locked), ("Printable", printable)] {
                if let Some(v) = v {
                    set.push((name, bool_text(*v)));
                }
            }
            set_attrs(w, id, &set)
        }
        Command::Arrange { id, how } => arrange(w, id, *how),
        Command::Transform { ids, matrix } => transform_items(w, ids, &Mat(*matrix)),
        Command::Rotate { ids, by, anchor } => {
            let ids = expand(w, ids);
            turn(w, &ids, *by, *anchor)
        }
        Command::RotateAbout { ids, by, point } => {
            let ids = expand(w, ids);
            turn_about(w, &ids, *by, *point)
        }
        Command::SetViewPref { key, value } => set_view_pref(w, key, value),
        Command::Shear { ids, by, anchor, adjust } => {
            let ids = expand(w, ids);
            tooltransform::shear_items(w, &ids, *by, *anchor, *adjust)
        }
        Command::Flip { ids, axis, anchor } => {
            let ids = expand(w, ids);
            mirror(w, &ids, *axis, *anchor)
        }
        Command::TransformAgain { ids, sequence, individually } => {
            let ids = expand(w, ids);
            again(w, &ids, *sequence, *individually)
        }
        Command::MakeCompoundPath { ids } => compound::make(w, ids),
        Command::ReleaseCompoundPath { id } => compound::release(w, id),
        Command::Group { ids } => group(w, ids),
        Command::Ungroup { id, remember_layers } => ungroup(w, id, remember_layers.unwrap_or(true)),
        Command::Duplicate { ids, by } => {
            let src = clipboard::Source::of(w.ed);
            for id in ids {
                duplicate(w, &src, id, (by[0], by[1]))?;
            }
            Ok(())
        }
        Command::SetPaint { ids, fill, fill_tint, stroke, stroke_tint, stroke_weight } => {
            for id in expand(w, ids) {
                set_paint(w, &id, fill.as_deref(), *fill_tint, stroke.as_deref(), *stroke_tint, *stroke_weight)?;
            }
            Ok(())
        }
        Command::MoveContent { id, by } => move_content(w, id, (by[0], by[1])),
        Command::Fit { id, how } => fit(w, id, *how),
        Command::SetLabel { id, key, value } => set_label(w, id, key, value),
        Command::SetPrimaryTextFrame { id, primary } => parents::set_primary_text_frame(w, id, *primary),
        Command::InsertTable { story, at, rows, columns, header, footer } => tables::insert_table(w, story, *at, *rows, *columns, *header, *footer),
        Command::ConvertToTable { story, start, end } => tables::convert_to_table(w, story, *start, *end),
        Command::InsertRows { table, at, count, before } => tables::insert_rows(w, table, *at, *count, *before),
        Command::InsertColumns { table, at, count, before } => tables::insert_columns(w, table, *at, *count, *before),
        Command::PasteRows { table, source, from, to, at, before } => tables::paste_rows(w, table, source.as_deref(), *from, *to, *at, *before),
        Command::PasteColumns { table, source, from, to, at, before } => tables::paste_columns(w, table, source.as_deref(), *from, *to, *at, *before),
        Command::DeleteRows { table, at, count } => tables::delete_rows(w, table, *at, *count),
        Command::DeleteColumns { table, at, count } => tables::delete_columns(w, table, *at, *count),
        Command::MergeCells { table, from, to } => tables::merge_cells(w, table, *from, *to),
        Command::UnmergeCell { table, cell } => tables::unmerge_cell(w, table, *cell),
        Command::SplitCell { table, cell, vertical } => tables::split_cell(w, table, *cell, *vertical),
        Command::SetRows { table, rows, height, auto_grow } => tables::set_rows(w, table, rows, *height, *auto_grow),
        Command::SetColumns { table, columns, width } => tables::set_columns(w, table, columns, *width),
        Command::SetCells { table, cells, change } => tables::set_cells(w, table, cells, change),
        Command::SetCellType { table, cells, graphic } => tables::celltype::set_cell_type(w, table, cells, *graphic),
        Command::SetHeaderRows { table, header } => tables::set_header_rows(w, table, *header),
        Command::SetTableOptions { table, options } => tableopts::set_options(w, table, options),
        Command::ApplyCellStyle { table, cells, style } => tables::styles::apply_cell_style(w, table, cells, style),
        Command::ClearCellOverrides { table, cells } => tables::styles::clear_cell_overrides(w, table, cells),
        Command::ApplyTableStyle { table, style } => tables::styles::apply_table_style(w, table, style),
        Command::ClearTableOverrides { table } => tables::styles::clear_table_overrides(w, table),
        Command::CreateTableStyle { kind, name, props, group } => tablestyledefs::create(w, *kind, name, props, group.as_deref()),
        Command::DuplicateTableStyle { kind, style, name } => tablestylegroups::duplicate(w, *kind, style, name.as_deref()),
        Command::CreateTableStyleGroup { kind, name, parent } => tablestylegroups::create_group(w, *kind, name, parent.as_deref()),
        Command::RenameTableStyleGroup { kind, group, name } => tablestylegroups::rename_group(w, *kind, group, name),
        Command::DeleteTableStyleGroup { kind, group } => tablestylegroups::delete_group(w, *kind, group),
        Command::MoveTableStyle { kind, style, group } => tablestylegroups::move_style(w, *kind, style, group.as_deref()),
        Command::EditTableStyle { kind, style, rename, props } => tablestyledefs::edit(w, *kind, style, rename.as_deref(), props),
        Command::DeleteTableStyle { kind, style, replacement } => tablestyledefs::delete(w, *kind, style, replacement),
        Command::DeleteTable { table } => tables::delete_table(w, table),
        Command::ConvertShape { ids, to } => convert::convert_shape(w, ids, *to),
        Command::Paste { page, at, layer, into, remember_layers } => clipboard::paste(w, page.as_deref(), *at, layer.as_deref(), into.as_deref(), *remember_layers),
        Command::ReversePath { ids } => convert::reverse_path(w, ids),
        Command::SetFooterRows { table, footer } => tables::set_footer_rows(w, table, *footer),
        Command::ConvertRows { table, at, count, to } => tables::convert_rows(w, table, *at, *count, *to),
        Command::ConvertTableToText { table } => tables::to_text(w, table),
        Command::DistributeRows { table, first, last } => tables::distribute_rows(w, table, *first, *last),
        Command::DistributeColumns { table, first, last } => tables::distribute_columns(w, table, *first, *last),
        Command::SetRowHeights { table, heights } => tables::set_row_heights(w, table, heights),
    }
}

/// The `ResultText` of instances of text variables (`(Self, text)`), for the ones that differ.
fn set_variable_results(w: &mut Work<'_>, results: &[(String, String)]) -> Result<(), EditError> {
    for (instance, text) in results {
        let (part, ix, i) = w.find(instance)?;
        let t = w.text(&part)?;
        if ix.local(t, i) != "TextVariableInstance" {
            return Err(EditError::new("invalid", format!("{instance} is not a text variable instance")));
        }
        if ix.value(t, i, "ResultText").as_deref() == Some(text.as_str()) {
            continue;
        }
        let edit = attr_edit(&ix, t, i, "ResultText", text);
        w.splice(&part, vec![edit])?;
    }
    Ok(())
}
