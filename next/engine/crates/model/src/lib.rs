//! Folio Studio document model.
//!
//! Units are points. Geometry follows IDML: every page item has an `ItemTransform`
//! (`a b c d tx ty`, mapping its inner coordinates to its parent's) and path points in its
//! inner coordinates. A spread's items are relative to the spread; a group's children are
//! relative to the group. Pages carry their own transform into spread coordinates.
//!
//! The model keeps colour as ink values (CMYK / spot / RGB / Lab) and resolves only for
//! display; text keeps resolved paragraph and character attributes per run.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod geom;
pub mod parents;
pub mod interop;
pub mod grep;
pub mod nested;
pub mod marker;
pub mod captions;
pub mod preflight;
pub mod stroke_styles;
pub mod fitting;
pub mod textpath;
pub mod opentype;
pub mod layout;
pub mod qr;
pub mod toc;
pub mod display;
pub mod printing;
pub mod hyperlinks;
pub mod conditions;
pub mod indexes;
pub mod traps;
pub mod clipping;
pub mod graphic_layers;
pub mod image_color;
pub mod library;
pub mod book;
pub mod paragraph_columns;
pub mod optical_margin;
pub mod polygon;
pub mod mathml;
pub mod strokebounds;
pub mod text_gradient;
pub mod pdfcomments;
pub use display::LocalDisplay;
pub use geom::path::paths_bounds;
pub use text_gradient::{GradientArea, PlacedGradient, TextGradient};

/// IDML `Self` id, e.g. `"u13955"`.
pub type Id = String;

// ---------------------------------------------------------------------------------------
// Geometry

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Axis-aligned rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

/// IDML affine transform `a b c d tx ty`: x' = a·x + c·y + tx, y' = b·x + d·y + ty.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

/// One IDML `PathPointType`: anchor plus the two direction handles.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathPoint {
    pub anchor: Point,
    pub left: Point,
    pub right: Point,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubPath {
    pub points: Vec<PathPoint>,
    pub open: bool,
}

// ---------------------------------------------------------------------------------------
// Colour

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpace {
    Cmyk,
    Rgb,
    Lab,
    Gray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorModel {
    Process,
    Spot,
    Registration,
    /// Swatch "None": paints nothing.
    None,
    /// Swatch "Paper": the paper colour (white on screen).
    Paper,
    Mixed,
}

/// A colour swatch as the document stores it (ink values), with a display approximation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub id: Id,
    pub name: String,
    pub model: ColorModel,
    pub space: ColorSpace,
    /// Ink or channel values as in IDML: CMYK 0–100, RGB 0–255, Lab L 0–100 / a b.
    pub values: Vec<f64>,
    /// For a tint swatch: the base swatch and the tint percent.
    pub base: Option<Id>,
    pub tint: Option<f64>,
    /// sRGB approximation for screen display (0–255).
    pub display_rgb: [u8; 3],
}

/// How a fill or stroke is painted.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Paint {
    #[default]
    None,
    /// A swatch (colour or tint) with an optional extra tint (percent, 100 = full).
    Swatch { id: Id, tint: f64 },
    /// A gradient swatch (`Document::gradients[id]`); display falls back to `fallback_rgb`
    /// until gradients render.
    Gradient { id: Id, fallback_rgb: [u8; 3] },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
}

/// One IDML `GradientStop`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Colour swatch id (`Document::swatches`).
    pub color: Id,
    /// Stop position along the ramp, percent 0–100.
    pub location: f64,
    /// Midpoint between the previous stop and this one, percent 0–100 (50 = linear).
    pub midpoint: f64,
}

/// A gradient swatch (IDML Resources/Graphic.xml `Gradient`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub id: Id,
    pub name: String,
    pub kind: GradientKind,
    pub stops: Vec<GradientStop>,
}

/// Where a gradient sits on an item (IDML `GradientFillStart/Length/Angle/HiliteLength/
/// HiliteAngle`, or the `GradientStroke…` set), in the item's inner coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GradientGeometry {
    pub start: Point,
    pub length: f64,
    /// Degrees, counter-clockwise as InDesign shows it (y is down in inner coordinates).
    pub angle: f64,
    pub hilite_length: f64,
    pub hilite_angle: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeAlignment {
    #[default]
    Center,
    Inside,
    Outside,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub weight: f64,
    pub alignment: StrokeAlignment,
    /// IDML `EndJoin` / `EndCap` / `MiterLimit` (InDesign defaults: miter, butt, 4).
    #[serde(default)]
    pub join: LineJoin,
    #[serde(default)]
    pub cap: LineCap,
    #[serde(default)]
    pub miter_limit: Option<f64>,
    /// IDML `StrokeType`: a stroke style's `Self` ("StrokeStyle/$ID/Dashed", a document's
    /// "DashedStrokeStyle/House dash"); `None` = solid.
    #[serde(default)]
    pub style: Option<String>,
    /// Arrowheads and their scale (`LeftLineEnd`, `RightLineEnd`, `…ArrowHeadScale`).
    #[serde(default)]
    pub ends: geom::LineEnds,
    /// Gap colour of a dashed, dotted or striped stroke (`GapColor` with `GapTint`).
    #[serde(default)]
    pub gap: Paint,
    /// The stroke as InDesign paints it when it is more than one solid line along the
    /// item's paths (stroke style, gap colour, arrowheads), lowered when the document is
    /// read (`geom::stroke_art`). Consumers paint these parts instead of the plain stroke.
    #[serde(default)]
    pub art: Option<geom::StrokeArt>,
    /// Corner options as set (top left, top right, bottom left, bottom right; InDesign keeps
    /// them with the stroke: "Stroke and Corner Options"). `paths` already have them applied.
    #[serde(default)]
    pub corners: [geom::CornerOption; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    /// IDML `ProjectingEndCap` (square).
    Projecting,
}

// ---------------------------------------------------------------------------------------
// Document structure

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub numbering_lists: Vec<numbering::NumberingList>,
    /// Story creation order from the IDML document's StoryList.
    #[serde(default)]
    pub story_order: Vec<Id>,
    #[serde(default)]
    pub conditions: Vec<conditions::Condition>,
    /// Whether condition indicators show on screen / print (`ConditionalTextPreference`).
    #[serde(default)]
    pub condition_indicators: conditions::Indicators,
    #[serde(default)]
    pub indexes: Vec<indexes::Index>,
    #[serde(default)]
    pub traps: traps::Settings,
    #[serde(default)]
    pub hyperlinks: hyperlinks::Hyperlinks,
    pub stroke_styles: Vec<stroke_styles::Definition>,
    #[serde(default)]
    pub preflight_profiles: preflight::Profiles,
    #[serde(default)]
    pub preflight_settings: preflight::Settings,
    #[serde(default)]
    pub toc_styles: Vec<toc::TocStyle>,
    #[serde(default)]
    pub toc_stories: BTreeMap<Id, Id>,
    pub source: SourceInfo,
    pub spreads: Vec<Spread>,
    pub master_spreads: Vec<Spread>,
    pub stories: BTreeMap<Id, Story>,
    pub swatches: BTreeMap<Id, Swatch>,
    pub layers: Vec<Layer>,
    /// Font family + style pairs the text uses (IDML `AppliedFont` + `FontStyle`).
    pub fonts_used: Vec<(String, String)>,
    /// Fonts the document declares (IDML Resources/Fonts.xml), for matching font files.
    pub font_table: Vec<FontInfo>,
    /// Non-fatal problems found while reading (unsupported objects, missing references).
    pub warnings: Vec<String>,
    /// Gradient swatches referenced by `Paint::Gradient`.
    #[serde(default)]
    pub gradients: BTreeMap<Id, Gradient>,
    /// Document-wide text settings (IDML Preferences.xml).
    #[serde(default)]
    pub text_preferences: TextPreferences,
    /// Print setup: bleed, colour profiles, inks (preflight and PDF export).
    #[serde(default)]
    pub print: PrintSetup,
    /// The pasteboard around each spread (`PasteboardPreference`).
    #[serde(default)]
    pub pasteboard: Pasteboard,
    /// Footnote numbering, spacing and separator rule (IDML Preferences.xml).
    #[serde(default)]
    pub footnotes: FootnoteOptions,
    /// How endnote numbers print (designmap `EndnoteOption`): numbering style, restart, prefix and suffix.
    #[serde(default)]
    pub endnotes: EndnoteOptions,
    /// The Swatches panel order: swatch and gradient ids (`swatches`, `gradients`) as the
    /// designmap's colour groups list them (the root group, then the other groups); without
    /// colour groups, the visible swatches of Graphic.xml in their order with None,
    /// Registration, Paper and Black first.
    #[serde(default)]
    pub swatch_order: Vec<Id>,
    /// Paragraph, character and object styles in the order and groups of Styles.xml.
    #[serde(default)]
    pub styles: StyleCatalog,
    /// Measurement units, ruler origin and guide settings (IDML `ViewPreference`,
    /// `GuidePreference`).
    #[serde(default)]
    pub view: ViewPreferences,
    /// The text variables the document defines (designmap `TextVariable`), by `Self`.
    #[serde(default)]
    pub text_variables: BTreeMap<Id, VariableDef>,
    /// The document's chapter number (`ChapterNumberPreference`), what a Chapter Number variable shows.
    #[serde(default)]
    pub chapter_number: String,
    #[serde(default = "book::chapter_source")]
    pub chapter_number_source: String,
    /// What the date and file name text variables read from the document.
    #[serde(default)]
    pub facts: DocumentFacts,
}

/// What the Creation Date, Modification Date, Output Date and File Name text variables read (oracle textvars): the dates are ISO 8601
/// with their offset, as the package's `META-INF/metadata.xml` has them (`xmp:CreateDate`, `xmp:ModifyDate`); the output date is the
/// last output (empty: none yet, the variable shows the creation date). The file's name (with its extension) and its folder relative to
/// the project root are set by whoever opened the document (never an absolute path).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentFacts {
    pub created: String,
    pub modified: String,
    pub output: String,
    pub file_name: String,
    pub folder: String,
}

/// A text variable the document defines (IDML designmap `TextVariable` and its `…VariablePreference`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VariableDef {
    pub id: Id,
    pub name: String,
    /// `VariableType`: "LastPageNumberType", "ChapterNumberType", "MatchParagraphStyleType", "MatchCharacterStyleType",
    /// "CustomTextType", "FileNameType", "CreationDateType", …
    pub kind: String,
    pub text_before: String,
    pub text_after: String,
    /// Page number variables: "SectionScope" or "DocumentScope".
    pub scope: String,
    /// Match style variables: the style's `Self`, the search ("FirstOnPage", "LastOnPage"), the case change
    /// (IDML's "None", "Uppercase", …) and whether the end punctuation is dropped.
    pub style: String,
    pub search: String,
    pub change_case: String,
    pub delete_end_punctuation: bool,
    /// Every setting of the preference element as the file has it (`Format`, `IncludePath`, …); a custom text's `Contents`.
    pub settings: std::collections::BTreeMap<String, String>,
}

/// A text variable placed in text: which one, and the result InDesign stored with it (what it showed when the file was saved).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NamedVariable {
    /// The variable's `Self`.
    pub id: Id,
    /// The result InDesign stored with the instance.
    pub result: String,
    /// The instance's own `Self`.
    pub instance: Id,
}

/// Styles as the styles panels list them (IDML Resources/Styles.xml).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleCatalog {
    pub paragraph: Vec<StyleEntry>,
    pub character: Vec<StyleEntry>,
    pub object: Vec<StyleEntry>,
    pub cell: Vec<StyleEntry>,
    pub table: Vec<StyleEntry>,
    /// The `Self` of every cell style group and table style group (empty ones too), in file order.
    pub cell_groups: Vec<Id>,
    pub table_groups: Vec<Id>,
    /// The `Self` of every paragraph style group and character style group (empty ones too), in file order.
    pub paragraph_groups: Vec<Id>,
    pub character_groups: Vec<Id>,
    /// The `Self` of every object style group (empty ones too), in file order.
    pub object_groups: Vec<Id>,
}

/// One style of [`StyleCatalog`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleEntry {
    /// `Self`, e.g. "ParagraphStyle/Group%3aBody".
    pub id: Id,
    /// `Name` as stored, e.g. "$ID/NormalParagraphStyle", "Group:Body".
    pub name: String,
    /// `Name`s of the style groups holding the style, outermost first (the root group not
    /// included), as stored.
    pub group: Vec<String>,
    /// Cell and table styles: the style it is based on (its `Self`, empty when none) and the settings it gives (its attributes as the
    /// file has them, without `Self`, `Name` and the shortcuts: the style dialogs show and edit them).
    pub based_on: String,
    pub settings: std::collections::BTreeMap<String, String>,
}

/// Document view settings (IDML `ViewPreference` and `GuidePreference`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPreferences {
    /// `HorizontalMeasurementUnits` / `VerticalMeasurementUnits` as stored ("Points",
    /// "Millimeters", "Picas", "Inches", "InchesDecimal", "Ciceros", "Q", "Ha", …).
    pub horizontal_units: String,
    pub vertical_units: String,
    /// `RulerOrigin`: "SpreadOrigin", "PageOrigin" or "SpineOrigin".
    pub ruler_origin: String,
    /// `Document/@ZeroPoint`: the point of the ruler (in the ruler origin's own coordinates) that the rulers, the Control panel
    /// and the Info panel call 0. Only what is shown shifts: items, guides and the IDML coordinates stay as they are
    /// (measured in InDesign 2025: `next/oracle/rules/docinput-c3.ko.md`).
    pub zero_point: [f64; 2],
    /// `GuidesLocked`, `GuidesShown`, `GuidesInBack`, `GuidesSnapto`.
    pub guides_locked: bool,
    pub guides_shown: bool,
    pub guides_in_back: bool,
    pub guides_snap: bool,
    /// `DocumentPreference/@ColumnGuideLocked`.
    pub column_guides_locked: bool,
    /// `GridPreference`: the baseline grid and the document grid (View ▸ Grids & Guides).
    pub grid: GridPreferences,
    /// `StrokeMeasurementUnits`: "Points" or "Millimeters".
    pub stroke_units: String,
    /// Keyboard increments: `ViewPreference/@CursorKeyIncrement` and `TextPreference/@LeadingKeyIncrement`,
    /// `BaselineShiftKeyIncrement` (points), `KerningKeyIncrement` (thousandths of an em).
    pub cursor_key_increment: f64,
    pub leading_key_increment: f64,
    pub baseline_shift_key_increment: f64,
    pub kerning_key_increment: f64,
    /// `ViewPreference/@GuideSnaptoZone`: how close (screen pixels) an object comes before it snaps.
    pub snap_zone: f64,
    /// `GuidePreference`: `RulerGuidesViewThreshold` (zoom percent) and `RulerGuidesColor`.
    pub ruler_guides_threshold: f64,
    pub ruler_guides_color: UiColor,
    /// `PasteboardPreference`: `PasteboardMargins` (horizontal, vertical), Match Preview Background to Theme Color, and
    /// the colours of the preview background and of the bleed and slug guides.
    pub pasteboard_margins: [f64; 2],
    pub match_preview_background: bool,
    pub preview_background_color: UiColor,
    pub bleed_guide_color: UiColor,
    pub slug_guide_color: UiColor,
}

/// The baseline grid and the document grid (`GridPreference`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GridPreferences {
    pub baseline_shown: bool,
    pub baseline_start: f64,
    pub baseline_division: f64,
    /// `BaselineViewThreshold`: the zoom (percent) below which the baseline grid is not shown.
    pub baseline_threshold: f64,
    /// `BaselineGridRelativeOption`: "TopOfPageOfBaselineGridRelativeOption" or "TopOfMarginOfBaselineGridRelativeOption".
    pub baseline_relative_to: String,
    pub baseline_color: UiColor,
    pub document_shown: bool,
    pub document_snap: bool,
    /// `HorizontalGridlineDivision` / `VerticalGridlineDivision` and the subdivisions.
    pub horizontal_division: f64,
    pub vertical_division: f64,
    pub horizontal_subdivision: f64,
    pub vertical_subdivision: f64,
    pub grid_color: UiColor,
    pub grids_in_back: bool,
}

impl Default for GridPreferences {
    fn default() -> Self {
        Self {
            baseline_shown: false,
            baseline_start: 36.0,
            baseline_division: 12.0,
            baseline_threshold: 75.0,
            baseline_relative_to: "TopOfPageOfBaselineGridRelativeOption".into(),
            baseline_color: UiColor::Named("LightBlue".into()),
            document_shown: false,
            document_snap: false,
            horizontal_division: 72.0,
            vertical_division: 72.0,
            horizontal_subdivision: 8.0,
            vertical_subdivision: 8.0,
            grid_color: UiColor::Named("LightGray".into()),
            grids_in_back: true,
        }
    }
}

impl Default for ViewPreferences {
    fn default() -> Self {
        Self {
            horizontal_units: "Points".into(),
            vertical_units: "Points".into(),
            ruler_origin: "SpreadOrigin".into(),
            zero_point: [0.0, 0.0],
            guides_locked: false,
            guides_shown: true,
            guides_in_back: false,
            guides_snap: true,
            column_guides_locked: true,
            grid: GridPreferences::default(),
            stroke_units: "Points".into(),
            cursor_key_increment: 1.0,
            leading_key_increment: 2.0,
            baseline_shift_key_increment: 2.0,
            kerning_key_increment: 20.0,
            snap_zone: 4.0,
            ruler_guides_threshold: 5.0,
            ruler_guides_color: UiColor::Named("Cyan".into()),
            pasteboard_margins: [-1.0, 72.0],
            match_preview_background: false,
            preview_background_color: UiColor::Named("LightGray".into()),
            bleed_guide_color: UiColor::Named("Fiesta".into()),
            slug_guide_color: UiColor::Named("GridBlue".into()),
        }
    }
}

/// A colour of InDesign's interface (layer and guide colours, IDML `UIColors`): a named
/// colour of the enumeration ("LightBlue", "Cyan", …) or an RGB triple (0–255), as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UiColor {
    Named(String),
    Rgb([f64; 3]),
}

impl Default for UiColor {
    fn default() -> Self {
        UiColor::Named("LightBlue".into())
    }
}

impl UiColor {
    /// The enumeration name, `None` for a custom colour.
    pub fn name(&self) -> Option<&str> {
        match self {
            UiColor::Named(n) => Some(n),
            UiColor::Rgb(_) => None,
        }
    }

    /// Screen colour (sRGB). Named colours are Folio's own approximations; an unknown name
    /// shows light blue.
    pub fn rgb(&self) -> [u8; 3] {
        match self {
            UiColor::Rgb(v) => v.map(|c| c.round().clamp(0.0, 255.0) as u8),
            UiColor::Named(n) => ui_color_rgb(n).unwrap_or([79, 153, 255]),
        }
    }
}

/// InDesign's interface colours (`UIColors`) as IDML spells them, in the order its colour menus list them
/// (measured: artifacts/prefs-20261023/grid-probe2.json).
pub const UI_COLORS: [&str; 37] = [
    "LightBlue", "Red", "Green", "Blue", "Yellow", "Magenta", "Cyan", "Gray", "Black", "Orange", "DarkGreen", "Teal", "Tan", "Brown",
    "Violet", "Gold", "DarkBlue", "Pink", "Lavender", "BrickRed", "OliveGreen", "Peach", "Burgundy", "GrassGreen", "Ochre", "Purple",
    "LightGray", "Charcoal", "GridBlue", "GridOrange", "Fiesta", "LightOlive", "Lipstick", "CuteTeal", "Sulphur", "GridGreen", "White",
];

/// Folio's screen approximations of the named interface colours.
pub fn ui_color_rgb(name: &str) -> Option<[u8; 3]> {
    Some(match name {
        "LightBlue" => [79, 153, 255],
        "Red" => [255, 0, 0],
        "Green" => [0, 190, 0],
        "Blue" => [0, 0, 255],
        "Yellow" => [255, 230, 0],
        "Magenta" => [255, 0, 255],
        "Cyan" => [0, 210, 255],
        "Gray" => [128, 128, 128],
        "Black" => [0, 0, 0],
        "Orange" => [255, 128, 0],
        "DarkGreen" => [0, 100, 0],
        "Teal" => [0, 128, 128],
        "Tan" => [205, 170, 125],
        "Brown" => [150, 80, 30],
        "Violet" => [150, 70, 230],
        "Gold" => [255, 190, 0],
        "DarkBlue" => [0, 0, 140],
        "Pink" => [255, 160, 200],
        "Lavender" => [180, 150, 255],
        "BrickRed" => [160, 30, 0],
        "OliveGreen" => [100, 120, 30],
        "Peach" => [255, 190, 150],
        "Burgundy" => [130, 0, 40],
        "GrassGreen" => [120, 200, 0],
        "Ochre" => [200, 140, 30],
        "Purple" => [110, 0, 160],
        "LightGray" => [190, 190, 190],
        "Charcoal" => [70, 70, 70],
        "GridBlue" => [120, 170, 230],
        "GridOrange" => [255, 170, 90],
        "GridGreen" => [140, 210, 120],
        "Fiesta" => [240, 80, 70],
        "LightOlive" => [160, 175, 90],
        "Lipstick" => [200, 30, 80],
        "CuteTeal" => [80, 200, 190],
        "Sulphur" => [235, 225, 60],
        "White" => [255, 255, 255],
        _ => return None,
    })
}

/// A ruler guide (IDML `Guide`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Guide {
    pub id: Id,
    /// `Orientation="Horizontal"`.
    pub horizontal: bool,
    /// `Location` as stored: measured from the ruler origin (`ViewPreferences::ruler_origin`).
    pub location: f64,
    /// Where the guide is in spread coordinates: y of a horizontal guide, x of a vertical one.
    pub position: f64,
    /// Index into `Spread::pages` of the page the guide belongs to; `None` = the spread.
    pub page: Option<usize>,
    /// `FitToPage`: the guide spans its page only (else the whole spread and pasteboard).
    pub fit_to_page: bool,
    pub locked: bool,
    pub layer: Option<Id>,
    /// `GuideColor`.
    pub color: UiColor,
    /// `ViewThreshold`: zoom (percent) below which the guide is not shown.
    pub view_threshold: Option<f64>,
    /// `GuideType`: "Ruler" or "Liquid".
    pub guide_type: Option<String>,
}

impl Default for Guide {
    fn default() -> Self {
        Self {
            id: String::new(),
            horizontal: true,
            location: 0.0,
            position: 0.0,
            page: None,
            fit_to_page: true,
            locked: false,
            layer: None,
            color: UiColor::Named("Cyan".into()),
            view_threshold: None,
            guide_type: None,
        }
    }
}

/// A page's column grid (IDML `MarginPreference`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageColumns {
    /// `ColumnCount`.
    pub count: u32,
    /// `ColumnGutter`.
    pub gutter: f64,
    /// `ColumnsPositions`: left and right edge of each column, measured from the left margin.
    pub positions: Option<Vec<f64>>,
    /// `ColumnDirection`: "Horizontal" or "Vertical".
    pub direction: String,
}

impl Default for PageColumns {
    fn default() -> Self {
        Self { count: 1, gutter: 12.0, positions: None, direction: "Horizontal".into() }
    }
}

// ---------------------------------------------------------------------------------------
// Print facts (preflight, PDF export). Read as IDML stores them; checking them is the
// preflight's job and resolving linked files is `folio-links`'.

/// How far the pasteboard reaches beside and above/below a spread's pages (points); a
/// negative `horizontal` is InDesign's default, one page width.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pasteboard {
    pub horizontal: f64,
    pub vertical: f64,
}

impl Default for Pasteboard {
    fn default() -> Self {
        Pasteboard { horizontal: -1.0, vertical: 72.0 }
    }
}

impl Pasteboard {
    /// A spread's pasteboard in spread coordinates, around its pages' bounds (`pages`) and its
    /// pages' width (`page_width`).
    pub fn around(&self, pages: Rect, page_width: f64) -> Rect {
        let h = if self.horizontal < 0.0 { page_width } else { self.horizontal };
        Rect { left: pages.left - h, top: pages.top - self.vertical, right: pages.right + h, bottom: pages.bottom + self.vertical }
    }
}

/// Document print setup (IDML `DocumentPreference`, designmap colour settings, `Ink`s).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintSetup {
    /// Document bleed top, bottom, inside (left on single pages), outside (right), points.
    pub bleed: [f64; 4],
    /// Slug in the same order.
    pub slug: [f64; 4],
    /// `FacingPages` (false when not stored; see `facing_pages_stored`).
    pub facing_pages: bool,
    /// The document stores `FacingPages`.
    pub facing_pages_stored: bool,
    /// `PageBinding="RightToLeft"`: the pages of a facing-pages document run right to left (a booklet is folded the other way).
    pub right_to_left: bool,
    /// designmap `CMYKProfile` / `RGBProfile`; `None` when absent, empty or `$ID/None`.
    pub cmyk_profile: Option<String>,
    pub rgb_profile: Option<String>,
    /// designmap `SolidColorIntent`: the rendering intent for document artwork and text.
    pub solid_color_intent: Option<String>,
    /// `OverprintBlack`: the [Black] swatch at 100 % overprints on output.
    pub overprint_black: bool,
    /// Inks of the Ink Manager (process inks first, as IDML lists them).
    pub inks: Vec<Ink>,
    /// Mixed ink swatches (Graphic.xml `MixedInk`): swatch id → its inks (names without
    /// `$ID/`, e.g. "Process Cyan", "PANTONE 280 C") and percentages (0–100), in IDML order.
    pub mixed_inks: BTreeMap<Id, Vec<(String, f64)>>,
    /// MixedInkGroup id to its MixedInk member ids, in the graphic resource order.
    pub mixed_ink_groups: BTreeMap<Id, Vec<Id>>,
}

/// One IDML `Ink` (Ink Manager entry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ink {
    /// Ink name without `$ID/`, e.g. "Process Cyan", "PANTONE 280 C".
    pub name: String,
    /// One of the four process inks.
    pub process: bool,
    /// `ConvertToProcess`: the spot ink separates as CMYK.
    pub convert_to_process: bool,
    /// `AliasInkName` when the ink is aliased to another ink.
    pub alias: Option<String>,
    /// `PrintInk`.
    pub print: bool,
    /// `InkType`: Normal, Transparent, Opaque, OpaqueIgnore.
    pub ink_type: String,
    pub neutral_density: Option<f64>,
}

/// What IDML records about a placed file (`Link`): its location and identity when it was
/// last updated in InDesign.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinkInfo {
    /// `Link` `Self` id.
    pub id: Id,
    /// `LinkResourceURI` as stored (percent-encoded `file:` URI).
    pub uri: String,
    /// `LinkResourceFormat` without `$ID/`.
    pub format: String,
    /// `StoredState`: Normal (linked), Embedded, …
    pub stored_state: String,
    /// File size when last updated (`LinkResourceSize`).
    pub size: Option<u64>,
    /// File modification time when last updated, Unix seconds UTC (`LinkImportStamp`).
    pub modified_unix: Option<f64>,
    /// `LinkImportModificationTime` as stored (local time of the machine that updated it).
    pub modification_time: Option<String>,
    /// `LinkResourceModified` / `LinkObjectModified` as saved.
    pub resource_modified: bool,
    pub object_modified: bool,
}

impl LinkInfo {
    pub fn embedded(&self) -> bool {
        self.stored_state == "Embedded"
    }
}

/// Attributes panel and effect facts of a page item.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemPrint {
    /// `Nonprinting`.
    pub nonprinting: bool,
    /// `OverprintFill` / `OverprintStroke`.
    pub overprint_fill: bool,
    pub overprint_stroke: bool,
    /// `OverprintGap`: the gap colour of a styled stroke overprints.
    pub overprint_gap: bool,
    /// Effects and partial transparency on the item that the model does not carry, e.g.
    /// "DropShadowSetting (object)", "fill opacity", "blend mode Multiply (stroke)".
    pub effects: Vec<String>,
    /// Link of a placed movie or sound (graphics keep theirs in `Graphic::link`).
    pub media_link: Option<LinkInfo>,
    /// Poster image of a movie or sound (what prints in its place), in the item's inner
    /// coordinates like a frame's graphic.
    pub poster: Option<Box<Graphic>>,
    /// Group transparency (`BlendingSetting` `IsolateBlending` / `KnockoutGroup`).
    pub isolate_blending: bool,
    pub knockout_group: bool,
    /// Object-level effects with their settings (IDML defaults filled in).
    pub fx: ObjectEffects,
    /// Effects of the item's parts (the Effects panel's Stroke, Fill and Text or Graphic
    /// levels), each drawn with its part.
    pub level_fx: LevelEffects,
    /// Partial transparency (`FillTransparencySetting`, `StrokeTransparencySetting`,
    /// `ContentTransparencySetting` blending): opacity 0–1 and blend mode of the fill, the
    /// stroke and the content (placed graphic, text); `None` = opaque, normal.
    pub fill_blend: Option<(f64, BlendMode)>,
    /// The item is the vector content of a placed graphic (a QR code InDesign generated,
    /// `qr`): the document's "Overprint [Black]" preference does not apply to its paint, as
    /// it does not to placed EPS or PDF content.
    pub placed_art: bool,
    pub stroke_blend: Option<(f64, BlendMode)>,
    pub content_blend: Option<(f64, BlendMode)>,
}

/// Object effects of an item's `TransparencySetting` (object level), as InDesign exports
/// them live: shadows and glows behind the object, feathers fading it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectEffects {
    pub drop_shadow: Option<ShadowEffect>,
    pub outer_glow: Option<ShadowEffect>,
    pub feather: Option<FeatherEffect>,
    pub directional_feather: Option<DirectionalFeather>,
    pub gradient_feather: Option<GradientFeather>,
    /// Inner effects (InDesign exports them live too), painted inside the object's shape;
    /// an inner shadow keeps its choke in `spread` and its distance and angle as offsets.
    pub inner_shadow: Option<ShadowEffect>,
    pub inner_glow: Option<InnerGlow>,
    pub bevel: Option<BevelEmboss>,
    pub satin: Option<Satin>,
}

/// The effects of an item's stroke, fill and content (a text frame's text, a placed graphic:
/// the graphic's own `TransparencySetting`, which wins over the frame's content level).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LevelEffects {
    pub stroke: ObjectEffects,
    pub fill: ObjectEffects,
    pub content: ObjectEffects,
}

impl ObjectEffects {
    /// Whether any effect is on.
    pub fn any(&self) -> bool {
        self.drop_shadow.is_some() || self.outer_glow.is_some() || self.feather.is_some() || self.directional_feather.is_some()
            || self.gradient_feather.is_some() || self.inner_shadow.is_some() || self.inner_glow.is_some() || self.bevel.is_some() || self.satin.is_some()
    }
}

/// `InnerGlowSetting`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InnerGlow {
    pub blend: BlendMode,
    /// 0–1.
    pub opacity: f64,
    pub size: f64,
    /// 0–1 (`Choke` percent).
    pub choke: f64,
    pub noise: f64,
    pub color: Paint,
    /// `Source="CenterSourced"`: the glow fills from the centre instead of the edges.
    pub center: bool,
    /// The Precise technique (Softer blurs; Precise ramps along the distance).
    pub precise: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BevelStyle {
    #[default]
    Inner,
    Outer,
    Emboss,
    Pillow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BevelTechnique {
    #[default]
    Smooth,
    ChiselHard,
    ChiselSoft,
}

/// `BevelAndEmbossSetting`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BevelEmboss {
    pub style: BevelStyle,
    pub technique: BevelTechnique,
    /// Bevel size and soften (points), depth 0–10 (`Depth` percent / 100).
    pub size: f64,
    pub soften: f64,
    pub depth: f64,
    /// Light from the top (`Direction="Up"`) or the bottom.
    pub up: bool,
    /// Light angle and altitude (degrees).
    pub angle: f64,
    pub altitude: f64,
    pub highlight: Paint,
    pub highlight_blend: BlendMode,
    pub highlight_opacity: f64,
    pub shadow: Paint,
    pub shadow_blend: BlendMode,
    pub shadow_opacity: f64,
}

/// `SatinSetting`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Satin {
    pub blend: BlendMode,
    pub opacity: f64,
    pub angle: f64,
    pub distance: f64,
    pub size: f64,
    pub invert: bool,
    pub color: Paint,
}

/// `DropShadowSetting` / `OuterGlowSetting` (a glow has no offset).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShadowEffect {
    pub blend: BlendMode,
    /// 0–1.
    pub opacity: f64,
    /// Offset in the item's parent coordinates (points).
    pub x_offset: f64,
    pub y_offset: f64,
    /// Blur size (points).
    pub size: f64,
    /// 0–1 (IDML `Spread` percent).
    pub spread: f64,
    pub noise: f64,
    pub color: Paint,
    /// The object knocks the shadow out where it is transparent.
    pub knocked_out: bool,
    /// A glow's Precise technique (Softer blurs; Precise ramps along the distance).
    pub precise: bool,
}

/// `FeatherSetting` (basic feather): edges fade over `width` points.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatherEffect {
    pub width: f64,
    /// 0–1 (`ChokeAmount` percent).
    pub choke: f64,
    pub corners: FeatherCorners,
    /// 0–100 (`Noise`).
    pub noise: f64,
}

/// `FeatherSetting` `CornerType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatherCorners {
    Sharp,
    Rounded,
    #[default]
    Diffusion,
}

/// `DirectionalFeatherSetting` `FollowShapeMode`: which edges each line ramps from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FollowShape {
    /// The object's bounds.
    FirstEdge,
    /// The first and last covered pixel of each line.
    #[default]
    LeadingEdge,
    /// Every run of covered pixels.
    AllEdges,
}

/// `DirectionalFeatherSetting`: per-side feather widths (points).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DirectionalFeather {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub choke: f64,
    pub angle: f64,
    pub follow: FollowShape,
    /// 0–100 (`Noise`).
    pub noise: f64,
}

/// `GradientFeatherSetting`: opacity ramp over the object.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientFeather {
    pub kind: GradientKind,
    /// Degrees, counter-clockwise.
    pub angle: f64,
    /// Ramp length and start in the item's inner coordinates; `None` = across the bounds.
    pub length: Option<f64>,
    pub start: Option<Point>,
    /// (location 0–100, opacity 0–100) stops; empty = 100 % → 0 %.
    pub stops: Vec<(f64, f64)>,
}

/// Document-wide text settings from IDML `TextPreference` and `GridPreference`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextPreferences {
    /// Synthesised small caps size, percent of point size.
    pub small_cap: f64,
    /// Percent of point size.
    pub superscript_size: f64,
    /// Baseline raise, percent of point size.
    pub superscript_position: f64,
    pub subscript_size: f64,
    /// Baseline drop, percent of point size.
    pub subscript_position: f64,
    /// Leading applies per paragraph (largest on the line otherwise).
    pub use_paragraph_leading: bool,
    /// Text wrap affects only text stacked below the wrapping item.
    pub z_order_text_wrap: bool,
    /// Baseline grid start and increment (points).
    pub baseline_start: f64,
    pub baseline_division: f64,
    /// IDML `BaselineGridRelativeOption`, e.g. "TopOfPageOfBaselineGridRelativeOption".
    pub baseline_relative_to: String,
}

impl Default for TextPreferences {
    fn default() -> Self {
        Self {
            small_cap: 70.0,
            superscript_size: 58.3,
            superscript_position: 33.3,
            subscript_size: 58.3,
            subscript_position: 33.3,
            use_paragraph_leading: false,
            z_order_text_wrap: false,
            baseline_start: 36.0,
            baseline_division: 12.0,
            baseline_relative_to: "TopOfPageOfBaselineGridRelativeOption".into(),
        }
    }
}

/// One `Font` of IDML Resources/Fonts.xml.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FontInfo {
    /// `FontFamily` name, as `AppliedFont` refers to it.
    pub family: String,
    /// `FontStyleName`, as `FontStyle` refers to it.
    pub style: String,
    pub postscript_name: String,
    pub full_name: String,
    /// e.g. OpenTypeCFF, TrueType, OpenTypeTT.
    pub font_type: String,
    #[serde(default)]
    pub version: String,
    /// IDML `Status` when the document was saved (Installed, NotAvailable, …).
    pub status: String,
    /// IDML `WritingScript`: the font's writing system as a Mac OS script code (0 Roman,
    /// 1 Japanese, 2 Traditional Chinese, 3 Korean, 4 Arabic, 5 Hebrew, 9 Devanagari,
    /// 21 Thai, 25 Simplified Chinese, …); `None` when the document does not record it.
    /// InDesign picks the substitute for a missing font by it.
    #[serde(default)]
    pub writing_script: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    pub path: String,
    pub bytes: u64,
    pub reader: String,
    pub dom_version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Layer {
    pub id: Id,
    pub name: String,
    pub visible: bool,
    pub printable: bool,
    /// Higher draws later (on top).
    pub order: usize,
    /// `Locked`: the layer's items cannot be selected or changed.
    #[serde(default)]
    pub locked: bool,
    /// `LayerColor`: the colour of selections and frame edges on the layer.
    #[serde(default)]
    pub color: UiColor,
    /// `ShowGuides` / `LockGuides`: the layer's ruler guides are shown / locked.
    #[serde(default = "yes")]
    pub show_guides: bool,
    #[serde(default)]
    pub lock_guides: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Spread {
    pub id: Id,
    /// Master spreads only: e.g. "A-Master".
    pub name: Option<String>,
    pub transform: Affine,
    pub pages: Vec<Page>,
    /// Items in z-order, first = bottom (within each layer).
    pub items: Vec<PageItem>,
    /// Ruler guides of the spread and its pages.
    #[serde(default)]
    pub guides: Vec<Guide>,
    /// Layout ▸ Pages ▸ Hide Spread (`SpreadHidden`): a hidden spread's pages are left out of every PDF
    /// export, even of a page range that names them (oracle: basic-hide-spread).
    #[serde(default)]
    pub hidden: bool,
    /// Pages panel ▸ Parent Pages ▸ Show Parent Items off (`ShowMasterItems="false"`): the spread's pages show none of their
    /// parent's items (oracle: pages-settings).
    #[serde(default)]
    pub hide_master_items: bool,
    /// Master spreads only: `NamePrefix` ("A") and `BaseName` ("Master").
    #[serde(default)]
    pub name_prefix: Option<String>,
    #[serde(default)]
    pub base_name: Option<String>,
}

/// The first `Page::index` of a master spread's pages: they number on from here, over all the master spreads, so every page
/// of the document has an index of its own (the editor opens a master like a document: Pages panel, double click).
pub const MASTER_PAGE_BASE: usize = 1_000_000;

impl Spread {
    /// The page of `master` that sits under `page` of this spread. One master page: that page. Two: a left-hand page takes the
    /// first, a right-hand page the last. Three or more: the page's place in its spread, up to the master's last page.
    pub fn master_page<'a>(&self, master: &'a Spread, page: &Page, right_hand: bool) -> Option<&'a Page> {
        match master.pages.len() {
            0 | 1 => master.pages.first(),
            2 => if right_hand { master.pages.last() } else { master.pages.first() },
            n => master.pages.get(self.pages.iter().position(|p| p.id == page.id).unwrap_or(0).min(n - 1)),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Page {
    pub id: Id,
    pub name: String,
    /// Page rectangle in page inner coordinates (IDML GeometricBounds).
    pub bounds: Rect,
    /// Page inner → spread coordinates.
    pub transform: Affine,
    /// Applied master spread id.
    pub master: Option<Id>,
    pub master_transform: Affine,
    /// Position in the whole document, 0-based.
    pub index: usize,
    /// The section marker of the page's section (designmap `Section` `Marker`): what a
    /// section marker character shows on it.
    #[serde(default)]
    pub section_marker: String,
    /// The name of the last page of the page's section (what a Last Page Number variable of section scope shows).
    #[serde(default)]
    pub section_last: String,
    /// Document page index where this section begins; page names can repeat across sections.
    #[serde(default)]
    pub section_start: usize,
    /// Master page items not shown on this page (IDML `OverrideList`: overridden by a page
    /// copy, or removed); ids of the master items, at any nesting depth.
    #[serde(default)]
    pub master_overrides: Vec<Id>,
    /// Margins top, left, bottom, right (IDML `MarginPreference`), points.
    #[serde(default)]
    pub margins: [f64; 4],
    /// Columns of the page's margin guides (IDML `MarginPreference`).
    #[serde(default)]
    pub columns: PageColumns,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageItem {
    pub id: Id,
    pub kind: ItemKind,
    /// Item inner → parent (spread or group) coordinates.
    pub transform: Affine,
    pub paths: Vec<SubPath>,
    pub fill: Paint,
    pub stroke: Stroke,
    /// 0–1 (IDML transparency opacity / 100).
    pub opacity: f64,
    pub visible: bool,
    pub layer: Option<Id>,
    /// Text wrap applied around this item (other frames avoid it).
    pub wraps: bool,
    /// Text wrap settings when `wraps` is set.
    #[serde(default)]
    pub text_wrap: Option<TextWrap>,
    /// Items pasted inside this frame (InDesign Paste Into), in this item's inner
    /// coordinates and clipped to its `paths`; drawn after the fill and graphic, before the
    /// stroke. Groups keep their members in `ItemKind::Group` instead.
    #[serde(default)]
    pub clipped: Vec<PageItem>,
    /// Gradient placement when `fill` / `stroke.paint` is a `Paint::Gradient`.
    #[serde(default)]
    pub fill_gradient: Option<GradientGeometry>,
    #[serde(default)]
    pub stroke_gradient: Option<GradientGeometry>,
    /// How the item (as a whole, with `opacity`) combines with what is below it.
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// Nonprinting, overprint, effects (preflight and PDF export).
    #[serde(default)]
    pub print: ItemPrint,
    /// How an item anchored in text is placed (`AnchoredObjectSetting`); `None` for items
    /// on a spread. An anchored item's own geometry has its bottom left at the anchor.
    #[serde(default)]
    pub anchor: Option<AnchorSettings>,
    /// `Name` when the user named the item (InDesign's own `$ID/…` names left out).
    #[serde(default)]
    pub name: Option<String>,
    /// `Locked`: the item cannot be selected or changed.
    #[serde(default)]
    pub locked: bool,
    /// `AppliedObjectStyle` (the style's `Self`), as the item has it.
    #[serde(default)]
    pub object_style: Option<Id>,
    /// `AllowOverrides="false"`: an item of a parent page that the pages may not override.
    #[serde(default)]
    pub overrides_off: bool,
    /// `LocalDisplaySetting`: Object ▸ Display Performance (see [`LocalDisplay`]).
    #[serde(default)]
    pub local_display: LocalDisplay,
    #[serde(default)]
    pub fitting: fitting::FrameFitting,
    /// Supported text attached to the path; the item's shape and paint remain independent.
    #[serde(default)]
    pub text_path: Option<textpath::HorizontalTextPath>,
    /// Imported MathML content inside this frame. Appearance is not composed yet.
    #[serde(default)]
    pub math: Option<mathml::MathObject>,
}

/// Where an anchored object goes (IDML `AnchoredPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorPosition {
    /// In the line, as a character.
    #[default]
    Inline,
    /// On its own above the line that holds it, between the space above and the space
    /// after (`AnchorSpaceAbove`, `AnchorYoffset`).
    AboveLine,
    /// Out of the text flow, by its reference points ("custom" in InDesign's dialog).
    Custom,
}

/// What a custom anchored object is positioned against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorReference {
    /// The anchor marker's position (horizontal only).
    AnchorLocation,
    ColumnEdge,
    #[default]
    TextFrame,
    PageMargins,
    PageEdge,
    /// Vertical only: a height of the anchor's line.
    LineBaseline,
    Capheight,
    LineXheight,
    TopOfLeading,
    EmboxTop,
    EmboxMiddle,
    EmboxBottom,
}

/// Left/top, centre or right/bottom; `TextAlign` follows the paragraph's alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorAlign {
    #[default]
    Start,
    Center,
    End,
    TextAlign,
}

/// IDML `AnchoredObjectSetting`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnchorSettings {
    pub position: AnchorPosition,
    /// The point of the object placed at the reference (`AnchorPoint`), as fractions of its
    /// width and height from its top left (0, 0.5, 1).
    pub point: (f64, f64),
    pub horizontal_reference: AnchorReference,
    pub horizontal_alignment: AnchorAlign,
    pub vertical_reference: AnchorReference,
    pub vertical_alignment: AnchorAlign,
    /// `AnchorXoffset`: away from the reference edge (left of a left-aligned reference,
    /// right of a right-aligned one, right of a centred one).
    pub x_offset: f64,
    /// `AnchorYoffset`: down from a custom reference, up for an inline object, the space
    /// after an above-line object.
    pub y_offset: f64,
    /// `AnchorSpaceAbove` of an above-line object.
    pub space_above: f64,
    /// `PinPosition`: a custom object stays between the column's top and bottom.
    pub pin: bool,
    /// `SpineRelative`: on a right-hand page left and right are swapped.
    pub spine_relative: bool,
}

impl Default for AnchorSettings {
    fn default() -> Self {
        Self {
            position: AnchorPosition::Inline,
            point: (1.0, 1.0),
            horizontal_reference: AnchorReference::TextFrame,
            horizontal_alignment: AnchorAlign::Start,
            vertical_reference: AnchorReference::LineBaseline,
            vertical_alignment: AnchorAlign::Start,
            x_offset: 0.0,
            y_offset: 0.0,
            space_above: 0.0,
            pin: true,
            spine_relative: false,
        }
    }
}

/// IDML `BlendMode` of an item's object transparency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    ColorDodge,
    ColorBurn,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl PageItem {
    /// Composition settings for either a text frame or supported text on this item's path.
    pub fn text_frame(&self) -> Option<&TextFrame> {
        match &self.kind {
            ItemKind::TextFrame(frame) => Some(frame),
            _ => self.text_path.as_ref().map(|path| &path.frame),
        }
    }

    /// Visits this item and every item nested in it (group members and `clipped` items,
    /// recursively), parents first. Items anchored in text are reached through stories.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a PageItem)) {
        f(self);
        if let ItemKind::Group { children } = &self.kind {
            for c in children {
                c.walk(f);
            }
        }
        for c in &self.clipped {
            c.walk(f);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextWrapMode {
    #[default]
    BoundingBox,
    Contour,
    JumpObject,
    NextColumn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextWrapSide {
    #[default]
    BothSides,
    LeftSide,
    RightSide,
    SideTowardsSpine,
    SideAwayFromSpine,
    LargestArea,
}

/// IDML `TextWrapPreference` (resolved with the object style).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextWrap {
    pub mode: TextWrapMode,
    /// Offsets top, left, bottom, right (points).
    pub offsets: [f64; 4],
    pub side: TextWrapSide,
    pub inverse: bool,
    /// Wrap applies only on master pages (IDML `ApplyToMasterPageOnly`).
    pub master_only: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    Rectangle { content: Option<Graphic> },
    Oval { content: Option<Graphic> },
    Polygon { content: Option<Graphic> },
    GraphicLine,
    TextFrame(TextFrame),
    Group { children: Vec<PageItem> },
    /// Anything not modelled yet; kept so its place is known.
    Other { element: String },
}

/// A placed image (or EPS/PDF shown through its preview) inside a frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Graphic {
    pub id: Id,
    /// IDML element: Image, EPS, PDF, ImportedPage, …
    pub element: String,
    /// Graphic inner → frame inner coordinates.
    pub transform: Affine,
    /// Graphic bounds in its inner coordinates.
    pub bounds: Rect,
    /// Original link (e.g. file:/Users/…/photo.jpg) when linked.
    pub link_uri: Option<String>,
    /// Preview image file written by the INDD reader, when available.
    pub preview: Option<String>,
    /// Runtime-only: a preview rendered from the original has real alpha, unlike INDD proxies.
    #[serde(skip)]
    pub preview_from_source: bool,
    /// Actual pixel size of the preview or linked image, when known.
    pub actual_ppi: Option<(f64, f64)>,
    /// The graphic's IDML `Link` (linked or embedded file), when it has one.
    #[serde(default)]
    pub link: Option<LinkInfo>,
    /// Image colour space InDesign recorded (`Space` without `$ID/#Links_`), e.g. "RGB",
    /// "CMYK", "Grayscale", "LAB", "Black and White".
    #[serde(default)]
    pub image_space: Option<String>,
    /// Effective resolution InDesign recorded when saving (`EffectivePpi`).
    #[serde(default)]
    pub effective_ppi: Option<(f64, f64)>,
    /// `ImageTypeName` without `$ID/`, e.g. "Photoshop", "JPEG", "EPS".
    #[serde(default)]
    pub image_type: Option<String>,
    /// Placed PDF / AI page settings (`PDFAttribute`), for PDF and ImportedPage graphics.
    #[serde(default)]
    pub pdf: Option<PlacedPdf>,
    /// The image's clipping path (Photoshop path, alpha channel or detected edges, as
    /// InDesign computed it), in the graphic's inner coordinates.
    #[serde(default)]
    pub clip: Option<GraphicClip>,
    pub clipping_options: Option<clipping::Options>,
    #[serde(default)]
    pub layer_options: Option<graphic_layers::Options>,
    pub image_color: Option<image_color::ImageColor>,
    /// The file data of an embedded graphic when the IDML carries it (`Properties/Contents`,
    /// base64-decoded); not serialised.
    #[serde(skip)]
    pub embedded_data: Option<std::sync::Arc<Vec<u8>>>,
    /// The graphic's own `LocalDisplaySetting` (Object ▸ Display Performance sets the frame's and the graphic's).
    #[serde(default)]
    pub local_display: LocalDisplay,
}

/// IDML `ClippingPathSettings` with its `ClippingPathGeometry`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicClip {
    /// `ClippingType`: PhotoshopPath, AlphaChannel, DetectEdges, UserModifiedPath.
    pub kind: String,
    pub paths: Vec<SubPath>,
    /// `InvertPath`: the image shows outside the detected or chosen path. InDesign writes
    /// the effective geometry: an inverted path comes with the image bounds as an outer
    /// subpath of the other winding (content-drops specimen drops-gfx: detected edges inset
    /// 3 pt, inverted), so `paths` are drawn as they are, nonzero, either way.
    pub invert: bool,
}

/// IDML `PDFAttribute` of a placed PDF. `Graphic::bounds` is the area InDesign shows (the box
/// the crop option picks), measured from the page's CropBox bottom-left corner with y up
/// (x = pdf x − crop.x0, y = pdf y − crop.y0); InDesign draws that box right side up with its
/// top-left corner at the bounds' top-left (artifacts/pdf-crop-20261021).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlacedPdf {
    /// 1-based page number.
    pub page: u32,
    /// `PDFCrop`, e.g. "CropContentVisibleLayers", "CropMedia", "CropTrim".
    pub crop: String,
    pub transparent_background: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirstBaseline {
    #[default]
    Ascent,
    CapHeight,
    XHeight,
    Leading,
    EmBoxHeight,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalJustification {
    #[default]
    Top,
    Center,
    Bottom,
    Justify,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextFrame {
    pub story: Id,
    pub previous: Option<Id>,
    pub next: Option<Id>,
    pub columns: u32,
    pub gutter: f64,
    /// Fixed column width when `fixed_column_width` is set.
    pub column_width: Option<f64>,
    /// Inset spacing top, left, bottom, right.
    pub insets: [f64; 4],
    pub first_baseline: FirstBaseline,
    pub min_first_baseline_offset: f64,
    pub vertical_justification: VerticalJustification,
    pub ignore_wrap: bool,
    /// Text Frame Options ▸ Auto-Size: the frame takes the size of its text (`None`: off).
    pub auto_size: Option<AutoSize>,
}

/// Text Frame Options ▸ Auto-Size (`TextFramePreference` AutoSizing*).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoSize {
    pub kind: AutoSizeKind,
    /// The reference point that keeps its place: (0, 0.5 or 1 across, 0, 0.5 or 1 down) of the
    /// frame (`AutoSizingReferencePoint`, CenterPoint by default).
    pub point: (f64, f64),
    /// `MinimumHeightForAutoSizing` / `MinimumWidthForAutoSizing` when used.
    pub min_height: Option<f64>,
    pub min_width: Option<f64>,
    /// `UseNoLineBreaksForAutoSizing`: the width follows the longest line, unbroken.
    pub no_line_breaks: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoSizeKind {
    HeightOnly,
    WidthOnly,
    HeightAndWidth,
    HeightAndWidthProportionally,
}

// ---------------------------------------------------------------------------------------
// Text

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Story {
    pub id: Id,
    #[serde(default)]
    pub optical_margin: optical_margin::OpticalMargin,
    #[serde(default)]
    pub notes: Vec<EditorialNote>,
    #[serde(default)]
    pub changes: Vec<TrackedChange>,
    #[serde(default)]
    pub track_changes: bool,
    /// Vertical text (`StoryPreference/@StoryOrientation="Vertical"`): lines run top to bottom and stack from
    /// right to left; full-width characters stand upright, others turn 90° clockwise (`folio_text::vertical`).
    #[serde(default)]
    pub vertical: bool,
    pub paragraphs: Vec<Paragraph>,
    /// The lines InDesign last composed for the story (INDD documents only). Layout sets
    /// the story with exactly these line starts and frames; an edit to the story must drop
    /// them (InDesign shows its saved composition only until the story is edited).
    #[serde(default)]
    pub saved: SavedComposition,
}

/// An editorial note occupies one invisible character in its containing story.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditorialNote {
    pub at: usize,
    pub text: String,
    pub collapsed: bool,
    pub user_name: String,
    pub creation_date: String,
    pub modification_date: String,
    pub name: Option<String>,
}

/// A change uses visible story offsets; deleted text has an empty range.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackedChange {
    pub start: usize,
    pub end: usize,
    pub change_type: String,
    pub user_name: String,
    pub date: String,
    pub text: String,
}

/// A story's saved composition (`folio-saved-lines` sidecar of the INDD reader).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SavedComposition {
    /// None saved (IDML documents, or the reader gave none for the story).
    #[default]
    None,
    /// The lines, in text order.
    Lines(Vec<SavedLine>),
    /// Saved lines that do not fit this story's text, and why.
    Unusable(String),
}

/// One saved line.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedLine {
    /// Story index of the line's first character, counted as layout counts characters:
    /// every character of the story's runs and one per paragraph return.
    pub start: usize,
    /// Text frame (`Self`) of the line; `None` when InDesign left it overset.
    pub frame: Option<Id>,
    /// Column of the frame.
    pub column: u16,
    /// Baseline below the top of the frame's text area (insets and the frame's stroke
    /// inside), vertical justification included; `None` when not known.
    pub baseline: Option<f64>,
    /// The line's segments: one, or more where a text wrap or a drop cap splits it.
    pub segments: Vec<SavedSegment>,
}

/// A segment of a saved line.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSegment {
    /// x and width in the frame's text area, paragraph indents not subtracted (points).
    pub x: f64,
    pub width: f64,
    /// Story index of the segment's first character (the line's for the first segment);
    /// `None` for a later segment that holds none.
    pub start: Option<usize>,
}

impl Story {
    /// Every run of the story and of the text nested in it (table cells, footnotes), in text
    /// order: what walks over anchored items, fonts or paints must visit.
    pub fn all_runs(&self) -> Vec<&TextRun> {
        let mut out = Vec::new();
        for p in &self.paragraphs {
            p.collect_runs(&mut out);
        }
        out
    }
}

impl Paragraph {
    /// This paragraph's runs and those of the text nested in them, in text order.
    pub fn collect_runs<'a>(&'a self, out: &mut Vec<&'a TextRun>) {
        for r in &self.runs {
            out.push(r);
            for t in &r.tables {
                for c in &t.cells {
                    for p in &c.paragraphs {
                        p.collect_runs(out);
                    }
                }
            }
            if let Some(n) = &r.note {
                for p in &n.paragraphs {
                    p.collect_runs(out);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    /// Applied paragraph style name.
    pub style: String,
    pub attrs: ParagraphAttrs,
    /// Runs in order; their text has no paragraph return.
    pub runs: Vec<TextRun>,
}

/// Where a paragraph starts (`ParagraphAttrs::start`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParagraphStart {
    #[default]
    Anywhere,
    NextColumn,
    NextFrame,
    NextPage,
    NextOddPage,
    NextEvenPage,
}

impl ParagraphStart {
    /// IDML's value (`StartParagraph`, `ParagraphBreakType`); unknown values start anywhere.
    pub fn from_idml(v: &str) -> ParagraphStart {
        match v {
            "NextColumn" => ParagraphStart::NextColumn,
            "NextFrame" => ParagraphStart::NextFrame,
            "NextPage" => ParagraphStart::NextPage,
            "NextOddPage" => ParagraphStart::NextOddPage,
            "NextEvenPage" => ParagraphStart::NextEvenPage,
            _ => ParagraphStart::Anywhere,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Justification {
    #[default]
    LeftAlign,
    CenterAlign,
    RightAlign,
    LeftJustified,
    CenterJustified,
    RightJustified,
    FullyJustified,
    ToBindingSide,
    AwayFromBindingSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpacingRange {
    pub min: f64,
    pub desired: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabAlignment {
    #[default]
    Left,
    Center,
    Right,
    Character,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TabStop {
    pub position: f64,
    pub alignment: TabAlignment,
    pub leader: String,
    pub align_char: String,
}

fn hyphen_last_default() -> bool { true }
fn hyphen_zone_default() -> f64 { 36.0 }
fn hyphen_weight_default() -> u32 { 5 }
/// `DropcapDetail`: bit 1 Align Left Edge, bit 2 Scale for Descenders; InDesign's default is Align Left Edge on.
fn drop_cap_detail_default() -> u32 { 1 }
fn single_word_default() -> Justification { Justification::FullyJustified }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParagraphAttrs {
    #[serde(default)]
    pub columns: paragraph_columns::ParagraphColumns,
    pub justification: Justification,
    #[serde(default = "single_word_default")]
    pub single_word_justification: Justification,
    /// IDML composer name, e.g. "HL Composer" (Adobe Paragraph Composer).
    pub composer: String,
    #[serde(default)]
    pub balance_ragged_lines: bool,
    #[serde(default)]
    pub ignore_edge_alignment: bool,
    /// `ParagraphDirection="RightToLeftDirection"`: the paragraph's base direction is right
    /// to left (bidi paragraph level 1; the first-line indent is on the right).
    #[serde(default)]
    pub right_to_left: bool,
    pub left_indent: f64,
    pub right_indent: f64,
    pub first_line_indent: f64,
    pub last_line_indent: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub word_spacing: SpacingRange,
    pub letter_spacing: SpacingRange,
    pub glyph_scaling: SpacingRange,
    pub hyphenation: bool,
    pub hyphenate_words_longer_than: u32,
    pub hyphenate_after_first: u32,
    pub hyphenate_before_last: u32,
    pub hyphenate_capitalized: bool,
    pub hyphen_ladder_limit: u32,
    #[serde(default = "hyphen_last_default")]
    pub hyphenate_last_word: bool,
    #[serde(default = "hyphen_zone_default")]
    pub hyphenation_zone: f64,
    /// `HyphenWeight` 0-10 (the slider Better Spacing - Fewer Hyphens); InDesign's default is 5.
    #[serde(default = "hyphen_weight_default")]
    pub hyphen_weight: u32,
    /// `HyphenateAcrossColumns`: a column's last line may end in a hyphen.
    #[serde(default = "hyphen_last_default")]
    pub hyphenate_across_columns: bool,
    pub drop_cap_lines: u32,
    pub drop_cap_characters: u32,
    /// The paragraph style's nested line styles (`AllLineStyles`); the runs carry what each item makes of them
    /// (`TextRun::line_attrs`). Applied by the layout, which knows the lines (rules/dropcap.ko.md).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_styles: Vec<nested::NestedLineStyle>,
    /// Where End Nested Style Characters sit in the paragraph: each is an offset into the paragraph's text (the runs' text
    /// joined), the character stands before the one at that offset. The character is invisible and takes no room, so it is
    /// not in the text; nested styles end items at it and a GREP expression can name it (rules/specialchars-c1.ko.md).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_nested_marks: Vec<usize>,
    /// Which U+FEFF characters of the paragraph's text are editorial note anchors (offsets into the text, as characters): the others
    /// are index markers. GREP's `~I` finds an index marker and passes over a note anchor (measured, rules/specialchars-c1.ko.md).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub note_anchor_marks: Vec<usize>,
    /// The break character (column, frame, page, odd or even page) that ends this paragraph; `Anywhere`: a plain paragraph return.
    /// Only GREP's `~M ~R ~P ~L ~E` need it (the paragraph after it starts where the break says: `start`).
    #[serde(default)]
    pub ends_with_break: ParagraphStart,
    /// `DropcapDetail` bit field: 1 = Align Left Edge, 2 = Scale for Descenders (measured: rules/dropcap.ko.md).
    #[serde(default = "drop_cap_detail_default")]
    pub drop_cap_detail: u32,
    pub keep_lines_together: bool,
    pub keep_with_next: u32,
    pub tabs: Vec<TabStop>,
    /// Align baselines to the document grid.
    pub align_to_baseline: bool,
    /// Restrict baseline grid alignment to the paragraph's first line.
    pub grid_align_first_line_only: bool,
    /// Auto tate-chu-yoko (IDML `AutoTcy`): in vertical text, runs of at most this many half-width digits stand as one
    /// horizontal group (0: off).
    #[serde(default)]
    pub auto_tcy: u32,
    /// `AutoTcyIncludeRoman`: auto tate-chu-yoko takes runs of half-width Roman letters too.
    #[serde(default)]
    pub auto_tcy_roman: bool,
    /// Space between consecutive paragraphs of the same style (IDML `SameParaStyleSpacing`);
    /// replaces space after/before between them. `None` = ignored (InDesign default).
    #[serde(default)]
    pub same_style_spacing: Option<f64>,
    /// Where the paragraph starts: IDML `StartParagraph`, or the break character that ends
    /// the paragraph before it (`ParagraphBreakType` on the range of that paragraph return:
    /// Type ▸ Insert Break Character, oracle text-special).
    #[serde(default)]
    pub start: ParagraphStart,
    /// Keep options beyond `keep_lines_together`: all lines, or the first/last N lines.
    #[serde(default)]
    pub keep_all_lines: bool,
    #[serde(default = "two")]
    pub keep_first_lines: u32,
    #[serde(default = "two")]
    pub keep_last_lines: u32,
    /// Paragraph rules (IDML `RuleAbove…`, `RuleBelow…`), when on.
    #[serde(default)]
    pub rule_above: Option<ParagraphRule>,
    #[serde(default)]
    pub rule_below: Option<ParagraphRule>,
    /// Paragraph shading (`ParagraphShading…`) and border (`ParagraphBorder…`), when on.
    #[serde(default)]
    pub shading: Option<ParagraphShading>,
    #[serde(default)]
    pub border: Option<ParagraphBorder>,
    /// Bullets or numbering (`BulletsAndNumberingListType` other than NoList).
    #[serde(default)]
    pub list: Option<ListStyle>,
}

fn two() -> u32 {
    2
}

/// Whether a paragraph rule, shading or border spans the column or the text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleWidth {
    /// The column (frame column edges; paragraph indents do not apply).
    #[default]
    Column,
    /// The text of the line (rules) or of the paragraph's lines (shading, border).
    Text,
}

/// A paragraph rule above or below.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParagraphRule {
    pub weight: f64,
    /// The rule colour with its tint; `None` = "Text Color": the fill of the first (rule
    /// above) or last (rule below) character as it is (`text_tint` is kept for IDML and has no effect).
    pub paint: Option<Paint>,
    pub text_tint: Option<f64>,
    /// Rule above: from the first baseline up to the bottom of the rule; rule below: from the
    /// last baseline down to the top of the rule.
    pub offset: f64,
    /// Inward from the column edges (column width) or from the text's ends (text width).
    pub left_indent: f64,
    pub right_indent: f64,
    pub width: RuleWidth,
    pub overprint: bool,
    /// Stroke style name without `StrokeStyle/$ID/` ("Solid", "Dashed", …).
    pub stroke_type: String,
    /// What fills the gaps of a patterned rule (`RuleAboveGapColor` with `GapTint`); none shows the page.
    pub gap_paint: Paint,
    pub gap_overprint: bool,
    /// `KeepRuleAboveInFrame` (rule above only): a paragraph that starts a column sits low
    /// enough that its rule stays inside the frame.
    pub keep_in_frame: bool,
}

/// Where paragraph shading or a border starts above the first line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TopOrigin {
    /// The first line's ascent (the "Ascent" first-baseline height).
    #[default]
    Ascent,
    Baseline,
    /// The first baseline less its leading.
    Leading,
}

/// Where paragraph shading or a border ends below the last line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BottomOrigin {
    /// The last line's descent.
    #[default]
    Descent,
    Baseline,
}

/// Paragraph shading: a filled box behind a paragraph (one per column it occupies).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParagraphShading {
    /// Colour with its tint.
    pub paint: Paint,
    pub overprint: bool,
    /// Offsets top, left, bottom, right, outward from the origins / column or text edges.
    pub offsets: [f64; 4],
    pub top: TopOrigin,
    pub bottom: BottomOrigin,
    pub width: RuleWidth,
    pub clip_to_frame: bool,
    /// "Do not print or export".
    pub suppress_printing: bool,
}

/// A paragraph border: four strokes centred on the edges of the paragraph's box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParagraphBorder {
    pub paint: Paint,
    pub overprint: bool,
    /// Line weights top, left, bottom, right.
    pub weights: [f64; 4],
    /// Offsets top, left, bottom, right, outward.
    pub offsets: [f64; 4],
    pub top: TopOrigin,
    pub bottom: BottomOrigin,
    pub width: RuleWidth,
    pub stroke_type: String,
    /// What fills the gaps of a patterned border (`ParagraphBorderGapColor` with `GapTint`); none shows the page.
    pub gap_paint: Paint,
    pub gap_overprint: bool,
    /// Draw the closing edges where the paragraph continues in another column or frame.
    pub display_if_splits: bool,
    /// `MergeConsecutiveParaBorders`: consecutive paragraphs with the same border (and
    /// shading) share one box.
    pub merge: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListKind {
    #[default]
    Bullet,
    Numbered,
}

/// Bullets or numbering of a paragraph. The bullet or number is not part of the story's
/// text: layout sets it at the start of the paragraph's first line.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListStyle {
    pub kind: ListKind,
    /// Bullet character(s).
    pub bullet: String,
    /// Text after the bullet with InDesign's metacharacters resolved (`^t` → tab).
    pub bullet_text_after: String,
    /// `NumberingExpression` with metacharacters resolved except the numbers: `^#` is
    /// this level's number, `^1` … `^9` the numbers of those levels.
    pub expression: String,
    /// `NumberingFormat`, e.g. "1, 2, 3, 4...", "I, II, III, IV...", "a, b, c, d...".
    pub format: String,
    /// `NumberingStartAt` when the paragraph restarts numbering (`NumberingContinue`
    /// false); `None` continues from the previous number of its list and level.
    pub restart_at: Option<u32>,
    /// `NumberingStartAt` (the first number of a list that has not started yet).
    pub start_at: u32,
    pub level: u32,
    /// `AppliedNumberingList` name; empty for the default list (numbered per story).
    pub list: String,
    /// `NumberingApplyRestartPolicy`: a higher level restarts this level's numbers.
    pub restart_after_higher_level: bool,
    /// Bullet / number alignment within its space (`BulletsAlignment`,
    /// `NumberingAlignment`).
    pub alignment: TabAlignment,
    /// Attributes of the bullet or number: the paragraph's first character with the
    /// bullet / numbering character style and the bullet font applied.
    pub marker: CharacterAttrs,
}

impl Default for ParagraphAttrs {
    fn default() -> Self {
        Self {
            columns: paragraph_columns::ParagraphColumns::default(),
            justification: Justification::LeftAlign,
            single_word_justification: single_word_default(),
            composer: "HL Composer".into(),
            balance_ragged_lines: false,
            ignore_edge_alignment: false,
            right_to_left: false,
            left_indent: 0.0,
            right_indent: 0.0,
            first_line_indent: 0.0,
            last_line_indent: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            word_spacing: SpacingRange { min: 80.0, desired: 100.0, max: 133.0 },
            letter_spacing: SpacingRange { min: 0.0, desired: 0.0, max: 0.0 },
            glyph_scaling: SpacingRange { min: 100.0, desired: 100.0, max: 100.0 },
            hyphenation: true,
            hyphenate_words_longer_than: 5,
            hyphenate_after_first: 2,
            hyphenate_before_last: 2,
            hyphenate_capitalized: true,
            hyphen_ladder_limit: 3,
            hyphenate_last_word: true,
            hyphenation_zone: 36.0,
            hyphen_weight: 5,
            hyphenate_across_columns: true,
            drop_cap_lines: 0,
            drop_cap_characters: 0,
            line_styles: Vec::new(),
            end_nested_marks: Vec::new(),
            note_anchor_marks: Vec::new(),
            ends_with_break: ParagraphStart::Anywhere,
            drop_cap_detail: 1,
            keep_lines_together: false,
            keep_with_next: 0,
            tabs: Vec::new(),
            align_to_baseline: false,
            grid_align_first_line_only: false,
            auto_tcy: 0,
            auto_tcy_roman: false,
            same_style_spacing: None,
            start: ParagraphStart::Anywhere,
            keep_all_lines: false,
            keep_first_lines: 2,
            keep_last_lines: 2,
            rule_above: None,
            rule_below: None,
            shading: None,
            border: None,
            list: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Leading {
    /// Auto leading: `auto_leading` percent of the largest point size on the line.
    #[default]
    Auto,
    Fixed(f64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capitalization {
    #[default]
    Normal,
    SmallCaps,
    AllCaps,
    CapToSmallCap,
    LowerCase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    #[default]
    Normal,
    Superscript,
    Subscript,
    OtSuperscript,
    OtSubscript,
    OtNumerator,
    OtDenominator,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KerningMethod {
    #[default]
    Metrics,
    MetricsRomanOnly,
    Optical,
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterAttrs {
    pub font_family: String,
    pub font_style: String,
    pub point_size: f64,
    pub leading: Leading,
    /// Percent of point size used when `leading` is Auto (default 120).
    pub auto_leading: f64,
    /// 1/1000 em.
    pub tracking: f64,
    pub kerning_method: KerningMethod,
    pub horizontal_scale: f64,
    pub vertical_scale: f64,
    pub baseline_shift: f64,
    pub skew: f64,
    pub fill: Paint,
    pub stroke: Paint,
    pub stroke_weight: f64,
    pub capitalization: Capitalization,
    pub position: Position,
    pub ligatures: bool,
    #[serde(default)]
    pub opentype: opentype::OpenType,
    pub underline: bool,
    pub strike_through: bool,
    pub no_break: bool,
    pub language: String,
    pub character_style: String,
    /// Manual kerning (IDML `KerningValue`, 1/1000 em) between each character of the run
    /// and the character after it.
    #[serde(default)]
    pub kerning_value: f64,
    /// Presence of IDML KerningValue: explicit zero disables metric pair kerning too.
    #[serde(default)]
    pub kerning_manual: bool,
    /// Underline offset below the baseline and weight (points); `None` = font default.
    #[serde(default)]
    pub underline_offset: Option<f64>,
    #[serde(default)]
    pub underline_weight: Option<f64>,
    /// Underline paint; `None` = the text fill.
    #[serde(default)]
    pub underline_color: Option<Paint>,
    #[serde(default)]
    pub strike_through_offset: Option<f64>,
    #[serde(default)]
    pub strike_through_weight: Option<f64>,
    #[serde(default)]
    pub strike_through_color: Option<Paint>,
    #[serde(default)]
    pub underline_overprint: bool,
    #[serde(default)]
    pub underline_type: String,
    #[serde(default)]
    pub underline_gap_color: Paint,
    #[serde(default)]
    pub underline_gap_overprint: bool,
    #[serde(default)]
    pub strike_through_type: String,
    #[serde(default)]
    pub strike_through_gap_color: Paint,
    #[serde(default)]
    pub strike_through_gap_overprint: bool,

    #[serde(default)]
    pub strike_through_overprint: bool,
    /// `Self` of the visible conditions the text is in (`AppliedConditions`), for their indicators.
    #[serde(default)]
    pub conditions: Vec<String>,

    /// `OverprintFill` / `OverprintStroke` of the text.
    #[serde(default)]
    pub overprint_fill: bool,
    #[serde(default)]
    pub overprint_stroke: bool,
    /// Emphasis marks over (or under) each character (`KentenKind` other than None).
    #[serde(default)]
    pub kenten: Option<Kenten>,
    /// IDML `CustomGlyph` (`$ID/g155`): the glyph id of the face the run's U+FFFD characters show (a glyph
    /// without a Unicode value, inserted from the Glyphs panel or by Find/Change ▸ Glyph).
    #[serde(default)]
    pub custom_glyph: Option<u16>,
    /// Where the gradient of `fill` / `stroke` sits (IDML `GradientFillStart/Length/Angle`, `GradientStroke…`); only
    /// used when that paint is a gradient.
    #[serde(default)]
    pub fill_gradient: TextGradient,
    #[serde(default)]
    pub stroke_gradient: TextGradient,
    /// Vertical text options of the run (`Tatechuyoko`, `CharacterRotation`, …); all off in horizontal text.
    #[serde(default)]
    pub vertical: VerticalChars,
}

/// How a run sits in vertical text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VerticalChars {
    /// Tate-chu-yoko (`Tatechuyoko`): the run is set horizontally in one upright em of the vertical line.
    #[serde(default)]
    pub tatechuyoko: bool,
    /// `TatechuyokoXOffset` / `TatechuyokoYOffset` in points: the horizontal group moved right / up.
    #[serde(default)]
    pub tatechuyoko_x: f64,
    #[serde(default)]
    pub tatechuyoko_y: f64,
    /// `CharacterRotation` in degrees, counter-clockwise around the glyph's em box centre.
    #[serde(default)]
    pub rotation: f64,
    /// `RotateSingleByteCharacters`: half-width characters stand upright instead of turning.
    #[serde(default)]
    pub upright_single_byte: bool,
}

/// The mark of `Kenten` (IDML `KentenKind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KentenKind {
    #[default]
    SesameDot,
    WhiteSesameDot,
    Fisheye,
    Bullseye,
    BlackCircle,
    WhiteCircle,
    BlackTriangle,
    WhiteTriangle,
    SmallBlackCircle,
    SmallWhiteCircle,
    /// `KentenCustomCharacter`, set in the run's font.
    Custom,
}

/// Emphasis marks (kenten) of a run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kenten {
    pub kind: KentenKind,
    /// `KentenFontSize` in points; `None` = half the point size (InDesign's "Auto").
    pub size: Option<f64>,
    /// `KentenPlacement`: points further from the text.
    pub placement: f64,
    /// `KentenPosition="BelowLeft"`: under the text (horizontal text).
    pub below: bool,
    /// `KentenFillColor` with `KentenTint`; `None` = the text colour.
    pub paint: Option<Paint>,
    pub custom: String,
}

impl Default for CharacterAttrs {
    fn default() -> Self {
        Self {
            font_family: "Minion Pro".into(),
            font_style: "Regular".into(),
            point_size: 12.0,
            leading: Leading::Auto,
            auto_leading: 120.0,
            tracking: 0.0,
            kerning_method: KerningMethod::Metrics,
            horizontal_scale: 100.0,
            vertical_scale: 100.0,
            baseline_shift: 0.0,
            skew: 0.0,
            fill: Paint::None,
            stroke: Paint::None,
            stroke_weight: 1.0,
            capitalization: Capitalization::Normal,
            position: Position::Normal,
            ligatures: true,
            opentype: opentype::OpenType::default(),
            underline: false,
            strike_through: false,
            no_break: false,
            language: "English: USA".into(),
            character_style: String::new(),
            kerning_value: 0.0,
            kerning_manual: false,
            underline_offset: None,
            underline_weight: None,
            underline_color: None,
            strike_through_offset: None,
            strike_through_weight: None,
            strike_through_color: None,
            underline_overprint: false,
            underline_type: String::new(),
            underline_gap_color: Paint::None,
            underline_gap_overprint: false,
            strike_through_type: String::new(),
            strike_through_gap_color: Paint::None,
            strike_through_gap_overprint: false,

            strike_through_overprint: false,
            conditions: Vec::new(),

            overprint_fill: false,
            overprint_stroke: false,
            kenten: None,
            custom_glyph: None,
            fill_gradient: TextGradient::default(),
            stroke_gradient: TextGradient::default(),
            vertical: VerticalChars::default(),
        }
    }
}

/// Text with one set of attributes. Inside `text`:
/// - `\n` is a forced line break (IDML U+2028 / `<Br>` inside a paragraph is a paragraph end),
/// - `\t` a tab, U+FFFC an inline/anchored object (see `inline`),
/// - U+0007 "indent to here", U+0008 "right indent tab" (InDesign's own codes),
/// - other InDesign special characters are mapped to Unicode.
///
/// A paragraph's last run may have empty `text`: it carries the attributes of the paragraph
/// return (InDesign counts them for the last line's leading; an empty paragraph has only it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    pub attrs: CharacterAttrs,
    /// Items anchored in this run, one per U+FFFC in `text`, in order.
    pub inline: Vec<PageItem>,
    /// Set when the run is a value InDesign computes at layout time; `text` is then a
    /// placeholder ("#") to replace.
    #[serde(default)]
    pub variable: Option<TextVariable>,
    /// Tables anchored in this run, one per U+0016 in `text`, in order.
    #[serde(default)]
    pub tables: Vec<Table>,
    /// The footnote or endnote this run's reference marker stands for (`variable` is
    /// `FootnoteNumber` / `EndnoteNumber`, `text` "#").
    #[serde(default)]
    pub note: Option<Box<Note>>,
    /// The text variable this run is an instance of (`variable` is `Named`, `text` "#").
    #[serde(default)]
    pub named: Option<Box<NamedVariable>>,
    /// The run's attributes with each of the paragraph's nested line styles (`ParagraphAttrs::line_styles`, same order)
    /// applied to it; empty for a paragraph without. The layout swaps them in for the lines the items cover.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_attrs: Vec<CharacterAttrs>,
}

/// Text InDesign computes during layout (IDML `<?ACE 18?>` / `<?ACE 19?>` markers, footnote
/// and endnote references and their numbers in the note text, `<?ACE 4?>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextVariable {
    /// Number of the page the character is laid out on.
    PageNumber,
    /// Next / previous page number (`<?ACE 18?>` with `PageNumberType`): a jump line's
    /// "continued on / from page".
    NextPageNumber,
    PreviousPageNumber,
    /// Name/marker of the section the character is laid out in.
    SectionMarker,
    /// A footnote reference in the text (the run carries the footnote) or the footnote's
    /// number at the start of its text.
    FootnoteNumber,
    /// An endnote reference in the text or the endnote's number in the endnote story; the
    /// run's `note` has the number.
    EndnoteNumber,
    /// An instance of a document text variable (`TextRun::named`), worked out for the page it is laid out on.
    Named,
}

/// A footnote (its text) or an endnote reference (its number; the text is in the endnote
/// story).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    /// Endnotes: the `Endnote` id the reference and the endnote text share.
    pub id: Id,
    pub paragraphs: Vec<Paragraph>,
    /// Endnotes: the number, in endnote order. Footnotes are numbered at layout.
    pub number: Option<u32>,
    /// Endnotes: this is the number at the start of the endnote text (in the endnote story), not the reference in the text.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub body: bool,
}

/// How endnote numbers print (designmap `EndnoteOption`). The numbers themselves are `Note::number`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EndnoteOptions {
    /// `EndnoteNumberingStyle`: the footnote styles ("Arabic", "UpperRoman", "LowerLetters", "Symbols", "Asterisks", ...).
    pub numbering: String,
    /// `RestartEndnoteNumbering`: Continuous (the whole document) or StoryRestart (each story from the start number).
    pub restart: String,
    pub prefix: String,
    pub suffix: String,
    /// `ShowEndnotePrefixSuffix`: NoPrefixSuffix, PrefixSuffixReference (in the text), PrefixSuffixMarker (in the endnote), PrefixSuffixBoth.
    pub show_prefix_suffix: String,
}

impl Default for EndnoteOptions {
    fn default() -> Self {
        Self { numbering: "Arabic".into(), restart: "Continuous".into(), prefix: String::new(), suffix: String::new(), show_prefix_suffix: "NoPrefixSuffix".into() }
    }
}

impl EndnoteOptions {
    /// Whether the prefix and suffix print around the number in the text (`body` false) or in the endnote text (`body` true).
    pub fn wraps(&self, body: bool) -> bool {
        match self.show_prefix_suffix.as_str() {
            "PrefixSuffixBoth" => true,
            "PrefixSuffixReference" => !body,
            "PrefixSuffixMarker" => body,
            _ => false,
        }
    }
}

/// Footnote options of the document (IDML `FootnoteOption`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FootnoteOptions {
    pub start_at: u32,
    /// `FootnoteNumberingStyle`, e.g. "Arabic", "UpperRoman", "LowerLetters", "Asterisks".
    pub numbering: String,
    /// `RestartNumbering`: DontRestart, PageRestart, SpreadRestart, SectionRestart.
    pub restart: String,
    pub prefix: String,
    pub suffix: String,
    /// `ShowPrefixSuffix`: NoPrefixSuffix, PrefixSuffixReference, PrefixSuffixMarker,
    /// PrefixSuffixBoth.
    pub show_prefix_suffix: String,
    /// Space between footnotes and the least space between the column's text and the
    /// first footnote.
    pub space_between: f64,
    pub spacer: f64,
    pub first_baseline: FirstBaseline,
    pub min_first_baseline_offset: f64,
    /// The separator rule above the first footnote of a column, when on.
    pub rule: Option<FootnoteRule>,
    /// Separator above a footnote continued from a preceding column or frame.
    pub continuing_rule: Option<FootnoteRule>,
    /// `EnableStraddling`: in a frame of several columns the footnotes span all of them, at
    /// the frame's bottom, and every column's text ends above them.
    pub straddle: bool,
    pub end_of_story: bool,
    pub no_splitting: bool,
}

impl Default for FootnoteOptions {
    fn default() -> Self {
        Self {
            start_at: 1,
            numbering: "Arabic".into(),
            restart: "DontRestart".into(),
            prefix: String::new(),
            suffix: String::new(),
            show_prefix_suffix: "NoPrefixSuffix".into(),
            space_between: 0.0,
            spacer: 0.0,
            first_baseline: FirstBaseline::Leading,
            min_first_baseline_offset: 0.0,
            rule: Some(FootnoteRule::default()),
            continuing_rule: Some(FootnoteRule { length: 288.0, ..FootnoteRule::default() }),
            straddle: false,
            end_of_story: false,
            no_splitting: false,
        }
    }
}

/// The footnote separator rule: drawn centred `offset` above the footnote area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FootnoteRule {
    pub weight: f64,
    pub paint: Paint,
    pub offset: f64,
    pub left_indent: f64,
    pub length: f64,
    pub overprint: bool,
    pub stroke_type: String,
}

impl Default for FootnoteRule {
    fn default() -> Self {
        Self {
            weight: 1.0,
            paint: Paint::None,
            offset: 0.0,
            left_indent: 0.0,
            length: 72.0,
            overprint: false,
            stroke_type: "Solid".into(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Tables

/// A stroke on a table cell edge or diagonal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellStroke {
    pub weight: f64,
    /// Colour with its tint.
    pub paint: Paint,
    /// Stroke style without `StrokeStyle/$ID/` ("Solid", "Dashed", …).
    pub stroke_type: String,
    pub overprint: bool,
    /// `…StrokePriority`: on an edge two cells share, the higher priority wins.
    pub priority: i32,
    /// What fills the gaps of a dashed, dotted or striped stroke (`…StrokeGapColor` with its tint; [Paper] unless set, and it paints:
    /// a white gap shows over a cell's fill). Left `None` for a solid stroke.
    pub gap: Paint,
    pub gap_overprint: bool,
    /// The cell or its cell style sets this edge (not InDesign's default stroke): on an edge two cells share with equal priority the
    /// one that is set shows (oracle tablestyles cs-edge-top).
    pub explicit: bool,
}

impl Default for CellStroke {
    fn default() -> Self {
        Self { weight: 1.0, paint: Paint::None, stroke_type: "Solid".into(), overprint: false, priority: 0, gap: Paint::None, gap_overprint: false, explicit: false }
    }
}

/// One table cell (merged cells: the top-left one, with its spans).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCell {
    pub id: Id,
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    pub paragraphs: Vec<Paragraph>,
    /// Text insets top, left, bottom, right, from the cell's grid lines.
    pub insets: [f64; 4],
    /// Fill with its tint (alternating table fills already resolved).
    pub fill: Paint,
    pub fill_overprint: bool,
    /// Edge strokes top, left, bottom, right (table borders resolved on outer edges).
    pub edges: [CellStroke; 4],
    pub vertical_justification: VerticalJustification,
    pub first_baseline: FirstBaseline,
    pub min_first_baseline_offset: f64,
    /// Text rotation in degrees (0, 90, 180, 270).
    pub rotation: f64,
    /// Cell writing direction perpendicular to its parent story (IDML WritingDirection=false).
    pub perpendicular: bool,
    pub clip: bool,
    /// `TopLeftDiagonalLine` (top left to bottom right) / `TopRightDiagonalLine`.
    pub diagonal_down: bool,
    pub diagonal_up: bool,
    pub diagonal: CellStroke,
    pub diagonal_in_front: bool,
}

/// One table row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableRow {
    /// Height InDesign stored (`SingleRowHeight`).
    pub height: f64,
    pub min_height: f64,
    pub max_height: f64,
    /// "At least" rows grow with their content; "exactly" rows keep `height`.
    pub auto_grow: bool,
    pub keep_with_next: bool,
    /// `StartRow`: Anywhere, NextColumn, NextFrame, NextPage, …
    pub start: String,
}

impl Default for TableRow {
    fn default() -> Self {
        Self { height: 3.0, min_height: 3.0, max_height: 600.0, auto_grow: true, keep_with_next: false, start: "Anywhere".into() }
    }
}

/// A table anchored in a story (IDML `Table`), with its table and cell styles resolved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Table {
    pub id: Id,
    pub style: String,
    pub rows: Vec<TableRow>,
    /// Column widths.
    pub columns: Vec<f64>,
    pub header_rows: usize,
    pub footer_rows: usize,
    /// Every cell, merged cells once (row-major order).
    pub cells: Vec<TableCell>,
    /// Space before the table (not at the top of a column) and after it (before text).
    pub space_before: f64,
    pub space_after: f64,
    /// Table border weights top, left, bottom, right (place the table in the column).
    pub border_weights: [f64; 4],
    /// The first header / last footer is not repeated where the table starts / ends.
    pub skip_first_header: bool,
    pub skip_last_footer: bool,
}

impl Default for PageItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: ItemKind::Other { element: String::new() },
            transform: Affine::IDENTITY,
            paths: Vec::new(),
            fill: Paint::None,
            stroke: Stroke::default(),
            opacity: 1.0,
            visible: true,
            layer: None,
            wraps: false,
            text_wrap: None,
            clipped: Vec::new(),
            fill_gradient: None,
            stroke_gradient: None,
            blend_mode: BlendMode::Normal,
            print: ItemPrint::default(),
            anchor: None,
            name: None,
            locked: false,
            object_style: None,
            overrides_off: false,
            local_display: LocalDisplay::UseView,
            fitting: fitting::FrameFitting::default(),
            text_path: None,
            math: None,
        }
    }
}

pub mod numbering;
pub mod dbmerge;
pub mod datamerge;
