//! Object ▸ Text Frame Options (next/oracle/scenarios/basic-textframe.json): columns, gutter,
//! fixed column width, insets, vertical justification, ignore text wrap, first baseline.
//! `TextColumnCount` and `TextColumnFixedWidth` are always written; the other settings only
//! where they differ from what the object style gives (its `TextFramePreference`, or InDesign's
//! defaults). The column width follows the frame: (width − left and right insets − gutters) ÷
//! columns; with a fixed column width the frame's width follows the columns instead (its left
//! edge stays). Insets are written as four values, or as one on a shaped frame when all four are
//! equal (corners.rs). The frame's story is composed again. Auto-Size settings (basic-autosize)
//! are written the same way; the size it gives comes from composing the story (folio-text
//! autosize.rs) and is set with `fit_frame`, in the same undo step.
use super::*;

/// Settings InDesign writes under `TextFramePreference` and their defaults.
const DEFAULTS: [(&str, &str); 13] = [("TextColumnGutter", "12"), ("UseFixedColumnWidth", "false"), ("VerticalJustification", "TopAlign"),
    ("IgnoreWrap", "false"), ("FirstBaselineOffset", "AscentOffset"), ("MinimumFirstBaselineOffset", "0"),
    ("AutoSizingType", "Off"), ("AutoSizingReferencePoint", "CenterPoint"), ("UseMinimumHeightForAutoSizing", "false"),
    ("MinimumHeightForAutoSizing", "0"), ("UseMinimumWidthForAutoSizing", "false"), ("MinimumWidthForAutoSizing", "0"),
    ("UseNoLineBreaksForAutoSizing", "false")];
const AUTO_SIZES: [&str; 5] = ["Off", "HeightOnly", "WidthOnly", "HeightAndWidth", "HeightAndWidthProportionally"];
const POINTS: [&str; 9] = ["TopLeftPoint", "TopCenterPoint", "TopRightPoint", "LeftCenterPoint", "CenterPoint", "RightCenterPoint",
    "BottomLeftPoint", "BottomCenterPoint", "BottomRightPoint"];
const VERTICAL: [&str; 4] = ["TopAlign", "CenterAlign", "BottomAlign", "JustifyAlign"];
const BASELINE: [&str; 6] = ["AscentOffset", "CapHeight", "LeadingOffset", "EmboxHeight", "XHeight", "FixedHeight"];

/// A change to a frame's options; absent fields stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameOptions {
    #[serde(default)] pub columns: Option<u32>,
    #[serde(default)] pub gutter: Option<f64>,
    /// Fixed column width: the columns keep `column_width` and the frame follows.
    #[serde(default)] pub fixed: Option<bool>,
    #[serde(default)] pub column_width: Option<f64>,
    /// Top, left, bottom, right (points).
    #[serde(default)] pub insets: Option<[f64; 4]>,
    #[serde(default)] pub vertical: Option<String>,
    #[serde(default)] pub ignore_wrap: Option<bool>,
    #[serde(default)] pub first_baseline: Option<String>,
    #[serde(default)] pub min_first_baseline: Option<f64>,
    /// Auto-Size: `Off`, `HeightOnly`, `WidthOnly`, `HeightAndWidth`, `HeightAndWidthProportionally`.
    #[serde(default)] pub auto_size: Option<String>,
    /// The Auto-Size reference point (`TopLeftPoint` … `BottomRightPoint`, `CenterPoint`).
    #[serde(default)] pub auto_size_point: Option<String>,
    #[serde(default)] pub use_min_height: Option<bool>,
    #[serde(default)] pub min_height: Option<f64>,
    #[serde(default)] pub use_min_width: Option<bool>,
    #[serde(default)] pub min_width: Option<f64>,
    #[serde(default)] pub no_line_breaks: Option<bool>,
}

/// What a frame's `TextFramePreference` says now.
pub(super) struct Prefs {
    pub pref: usize,
    pub count: f64,
    pub gutter: f64,
    pub insets: [f64; 4],
    pub fixed: bool,
    pub fixed_width: f64,
    /// The stroke's reach into the frame on each side.
    pub stroke: f64,
}

impl Prefs {
    pub fn read(w: &Work<'_>, ix: &Index, t: &str, frame: usize) -> Option<Prefs> {
        let pref = *ix.named(t, frame, "TextFramePreference").first()?;
        let num = |k: &str, d: f64| ix.attr(t, pref, k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
        let insets = read_insets(ix, t, pref).unwrap_or([0.0; 4]);
        Some(Prefs {
            pref,
            count: num("TextColumnCount", 1.0).max(1.0),
            gutter: num("TextColumnGutter", 12.0),
            insets,
            fixed: ix.attr(t, pref, "UseFixedColumnWidth") == Some("true"),
            fixed_width: num("TextColumnFixedWidth", 0.0),
            stroke: stroke_inset(w, ix, t, frame),
        })
    }

    /// The width of one column in a frame `width` wide.
    pub fn column(&self, width: f64) -> f64 {
        (width - 2.0 * self.stroke - self.insets[1] - self.insets[3] - self.gutter * (self.count - 1.0)) / self.count
    }
}

pub(super) fn read_insets(ix: &Index, t: &str, pref: usize) -> Option<[f64; 4]> {
    let at = ix.path(t, pref, &["Properties", "InsetSpacing"])?;
    if ix.attr(t, at, "type") != Some("list") {
        return Some([element_text(ix, t, at).parse().unwrap_or(0.0); 4]);
    }
    let items: Vec<f64> = ix.named(t, at, "ListItem").iter()
        .filter_map(|&item| element_text(ix, t, item).parse().ok()).collect();
    Some(items.try_into().unwrap_or([0.0; 4]))
}

/// `TextColumnFixedWidth` after the frame becomes `width` wide (none with a fixed width).
pub(super) fn column_width(w: &Work<'_>, ix: &Index, t: &str, frame: usize, width: f64) -> Option<(std::ops::Range<usize>, String)> {
    let p = Prefs::read(w, ix, t, frame)?;
    ix.attr(t, p.pref, "TextColumnFixedWidth")?;
    let fixed = ix.attr(t, p.pref, "UseFixedColumnWidth").map_or_else(
        || inherited(w, ix, t, frame, "UseFixedColumnWidth") == "true",
        |fixed| fixed == "true",
    );
    if fixed {
        return None;
    }
    Some(attr_edit(ix, t, p.pref, "TextColumnFixedWidth", &geom::num(p.column(width))))
}

/// What the frame's object style gives for a `TextFramePreference` setting (or the default).
pub(super) fn inherited(w: &Work<'_>, ix: &Index, t: &str, frame: usize, name: &str) -> String {
    let enable = if name.contains("AutoSizing") { "EnableTextFrameAutoSizingOptions" } else if name.contains("Baseline") { "EnableTextFrameBaselineOptions" } else { "EnableTextFrameGeneralOptions" };
    super::objectstyle::from_style(w, ix, t, frame, &["TextFramePreference"], name, enable)
        .unwrap_or_else(|| DEFAULTS.iter().find(|(k, _)| *k == name).map_or("", |(_, v)| v).to_owned())
}

/// How far the frame's stroke reaches into it: insets count from its inner edge (none for a
/// stroke without colour).
pub(super) fn stroke_inset(w: &Work<'_>, ix: &Index, t: &str, frame: usize) -> f64 {
    let weight = stroke_attr(w, ix, t, frame, "StrokeWeight").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    if weight <= 0.0 || stroke_attr(w, ix, t, frame, "StrokeColor").is_none_or(|c| c == "Swatch/None") {
        return 0.0;
    }
    match stroke_attr(w, ix, t, frame, "StrokeAlignment").as_deref() {
        Some("InsideAlignment") => weight,
        Some("OutsideAlignment") => 0.0,
        _ => weight / 2.0,
    }
}

pub(super) fn set_text_frame(w: &mut Work<'_>, id: &str, o: &FrameOptions) -> Result<(), EditError> {
    let bad = |m: &str| Err(EditError::new("invalid", m.to_owned()));
    if o.columns.is_some_and(|c| !(1..=40).contains(&c)) { return bad("columns: 1 to 40"); }
    let length = |v: Option<f64>| v.is_some_and(|v| !v.is_finite() || v < 0.0);
    if length(o.gutter) || length(o.min_first_baseline) || o.insets.is_some_and(|i| i.iter().any(|v| !v.is_finite() || *v < 0.0)) {
        return bad("lengths must be points, 0 or more");
    }
    if o.column_width.is_some_and(|v| !v.is_finite() || v <= 0.0) { return bad("column width must be more than 0"); }
    if o.vertical.as_deref().is_some_and(|v| !VERTICAL.contains(&v)) { return bad("unknown vertical justification"); }
    if o.first_baseline.as_deref().is_some_and(|v| !BASELINE.contains(&v)) { return bad("unknown first baseline offset"); }
    if o.auto_size.as_deref().is_some_and(|v| !AUTO_SIZES.contains(&v)) { return bad("unknown auto-size"); }
    if o.auto_size_point.as_deref().is_some_and(|v| !POINTS.contains(&v)) { return bad("unknown auto-size reference point"); }
    if length(o.min_height) || length(o.min_width) { return bad("lengths must be points, 0 or more"); }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "TextFrame" {
        return Err(EditError::new("invalid", format!("{id} is not a text frame")));
    }
    let mut p = Prefs::read(w, &ix, &t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no TextFramePreference")))?;
    let path = own_path(&ix, &t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, q, c)| (q.clone(), *c)).collect();
    let [x0, y0, x1, y1] = geom::path_bounds(&shapes).ok_or_else(|| EditError::new("unsupported", format!("{id} has an empty path")))?;
    // the settings as they will be
    if let Some(c) = o.columns { p.count = c as f64; }
    if let Some(g) = o.gutter { p.gutter = g; }
    if let Some(f) = o.fixed { p.fixed = f; }
    if let Some(cw) = o.column_width { p.fixed_width = cw; }
    if let Some(ins) = o.insets { p.insets = ins; }
    let (width, column) = if p.fixed {
        let cw = if p.fixed_width > 0.0 { p.fixed_width } else { p.column(x1 - x0) };
        (cw * p.count + p.gutter * (p.count - 1.0) + p.insets[1] + p.insets[3] + 2.0 * p.stroke, cw)
    } else {
        (x1 - x0, p.column(x1 - x0))
    };
    if column <= 0.0 {
        return bad("the columns do not fit in the frame");
    }
    let mut edits = vec![
        attr_edit(&ix, &t, p.pref, "TextColumnCount", &geom::num(p.count)),
        attr_edit(&ix, &t, p.pref, "TextColumnFixedWidth", &geom::num(column)),
    ];
    let set = |name: &str, value: String, edits: &mut Vec<(std::ops::Range<usize>, String)>| {
        let base = inherited(w, &ix, &t, i, name);
        if value == base || same_number(&value, &base) {
            edits.extend(attr_removal(&ix, &t, p.pref, name));
        } else {
            edits.push(attr_edit(&ix, &t, p.pref, name, &value));
        }
    };
    if o.gutter.is_some() { set("TextColumnGutter", geom::num(p.gutter), &mut edits); }
    if o.fixed.is_some() { set("UseFixedColumnWidth", bool_text(p.fixed), &mut edits); }
    if let Some(v) = &o.vertical { set("VerticalJustification", v.clone(), &mut edits); }
    if let Some(v) = o.ignore_wrap { set("IgnoreWrap", bool_text(v), &mut edits); }
    if let Some(v) = &o.first_baseline { set("FirstBaselineOffset", v.clone(), &mut edits); }
    if let Some(v) = o.min_first_baseline { set("MinimumFirstBaselineOffset", geom::num(v), &mut edits); }
    if let Some(v) = &o.auto_size { set("AutoSizingType", v.clone(), &mut edits); }
    if let Some(v) = &o.auto_size_point { set("AutoSizingReferencePoint", v.clone(), &mut edits); }
    if let Some(v) = o.use_min_height { set("UseMinimumHeightForAutoSizing", bool_text(v), &mut edits); }
    if let Some(v) = o.min_height { set("MinimumHeightForAutoSizing", geom::num(v), &mut edits); }
    if let Some(v) = o.use_min_width { set("UseMinimumWidthForAutoSizing", bool_text(v), &mut edits); }
    if let Some(v) = o.min_width { set("MinimumWidthForAutoSizing", geom::num(v), &mut edits); }
    if let Some(v) = o.no_line_breaks { set("UseNoLineBreaksForAutoSizing", bool_text(v), &mut edits); }
    if o.insets.is_some() {
        edits.extend(super::corners::write_insets(&ix, &t, i, p.insets));
    }
    let story = ix.attr(&t, i, "ParentStory").map(str::to_owned);
    w.splice(&part, edits)?;
    if (width - (x1 - x0)).abs() > 1e-9 {
        resize(w, id, Target::Box([x0, y0, x0 + width, y1]))?;
    }
    if let Some(s) = story {
        w.recompose(&s);
    }
    Ok(())
}

/// Auto-Size's result (basic-autosize): the frame's path becomes `size` with its inner top-left
/// kept, the frame moves so the reference point `point` (fractions across and down) stays where
/// it was, and its column width follows.
pub(super) fn fit_frame(w: &mut Work<'_>, id: &str, size: [f64; 2], point: [f64; 2]) -> Result<(), EditError> {
    if !(size[0].is_finite() && size[1].is_finite() && size[0] > 0.0 && size[1] > 0.0) {
        return Err(EditError::new("invalid", "auto-size: a size more than 0"));
    }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "TextFrame" {
        return Err(EditError::new("invalid", format!("{id} is not a text frame")));
    }
    let path = own_path(&ix, &t, i).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, q, c)| (q.clone(), *c)).collect();
    let [x0, y0, x1, y1] = geom::path_bounds(&shapes).ok_or_else(|| EditError::new("unsupported", format!("{id} has an empty path")))?;
    let moved = item_transform(&ix, &t, i).then_after(&Mat([1.0, 0.0, 0.0, 1.0, point[0] * (x1 - x0 - size[0]), point[1] * (y1 - y0 - size[1])]));
    resize(w, id, Target::Box([x0, y0, x0 + size[0], y0 + size[1]]))?;
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let mut edits = vec![attr_edit(&ix, &t, i, "ItemTransform", &moved.text())];
    if (size[0] - (x1 - x0)).abs() > 1e-6 {
        if let Some(edit) = fit_column_count(w, &ix, &t, i, size[0]) {
            edits.push(edit);
        }
    }
    w.splice(&part, edits)
}

fn fit_column_count(w: &Work<'_>, ix: &Index, t: &str, frame: usize, width: f64) -> Option<(std::ops::Range<usize>, String)> {
    let pref = ix.named(t, frame, "TextFramePreference").first().copied();
    let setting = |name| pref.and_then(|p| ix.attr(t, p, name)).map(str::to_owned)
        .unwrap_or_else(|| inherited(w, ix, t, frame, name));
    if setting("UseFixedColumnWidth") != "true" || !matches!(setting("AutoSizingType").as_str(), "WidthOnly" | "HeightAndWidth") {
        return None;
    }
    let gutter = setting("TextColumnGutter").parse::<f64>().unwrap_or(12.0).max(0.0);
    let column = setting("TextColumnFixedWidth").parse::<f64>().ok()?;
    let [_, left, _, right] = pref.and_then(|pref| read_insets(ix, t, pref))
        .or_else(|| super::objectstyle::inherited_frame_insets(w, ix, t, frame))
        .unwrap_or([0.0; 4]);
    let count = (width - left - right - 2.0 * stroke_inset(w, ix, t, frame) + gutter) / (column + gutter);
    let rounded = count.round();
    if !(column > 0.0 && (1.0..=1000.0).contains(&rounded) && (count - rounded).abs() <= 1e-6) {
        return None;
    }
    if rounded == setting("TextColumnCount").parse::<f64>().unwrap_or(1.0).round().clamp(1.0, 1000.0) {
        return None;
    }
    let count = geom::num(rounded);
    if let Some(pref) = pref {
        Some(attr_edit(ix, t, pref, "TextColumnCount", &count))
    } else {
        let at = ix.nodes[frame].close_start;
        Some((at..at, format!("<TextFramePreference TextColumnCount=\"{count}\"/>")))
    }
}

