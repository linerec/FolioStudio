//! Object styles: what an object takes from its style (the one owner of `Resources/Styles.xml`
//! object-style resolution for the commands), and applying a style (next/oracle/scenarios/
//! basic-styles.json):
//! - a click sets `AppliedObjectStyle` only; the object keeps its own values (overrides);
//! - Alt-click (clear) and Clear Overrides also remove the object's own values of the settings
//!   the style defines, and InDesign then sets the gradient start and length to the bottom left
//!   and the width of the object's visible bounds (stroke as it was before);
//! - [None] removes the object's own values and its frame fitting options too;
//! - a text frame's `TextFramePreference` follows the style category by category: a category
//!   the style turns on is the style's (always-written settings take its values, the others are
//!   inherited), one it turns off keeps the frame's current values, written out; the column
//!   width is worked out again. The frame's story is composed again.
use super::*;
use std::collections::HashSet;
mod group;
mod clear_undefined;
mod paragraph;
mod effects;
mod application;

const NONE: &str = "ObjectStyle/$ID/[None]";

pub(crate) fn apply_to_content(w: &mut Work<'_>, ids: &Vec<String>, style: &str, clear: &bool) -> Result<(), EditError> {
    let mut contents = Vec::new();
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        check_item(&ix, t, i, id)?;
        let graphic = ix.children(i).iter().copied().find(|&child| GRAPHICS.contains(&ix.local(t, child)))
            .and_then(|child| ix.attr(t, child, "Self"))
            .ok_or_else(|| EditError::new("invalid", "frame has no graphic content"))?;
        contents.push(graphic.to_owned());
    }
    apply(w, &contents, style, *clear)
}

/// The document's object styles.
pub(super) struct Styles<'t> {
    t: &'t str,
    ix: Index,
}

impl<'t> Styles<'t> {
    pub fn load(w: &'t Work<'_>) -> Option<Styles<'t>> {
        let t = w.text("Resources/Styles.xml").ok()?;
        Some(Styles { t, ix: Index::scan(t).ok()? })
    }

    /// A style and the styles it is based on, nearest first.
    pub fn chain(&self, style: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut style = style.to_owned();
        for _ in 0..32 {
            let Some(s) = self.ix.by_self(self.t, &style) else { break };
            out.push(s);
            let Some(based) = self.ix.path(self.t, s, &["Properties", "BasedOn"]) else { break };
            let n = &self.ix.nodes[based];
            let b = crate::xml::unescape(self.t[n.open_end..n.close_start].trim());
            if b.is_empty() {
                break;
            }
            // `BasedOn` names the style without its `ObjectStyle/` prefix
            style = if b.starts_with("ObjectStyle/") { b } else { format!("ObjectStyle/{b}") };
        }
        out
    }

    /// A category of `style` is on (a style without the switch, such as [None], has them all).
    pub fn enabled(&self, style: &str, enable: &str) -> bool {
        self.chain(style).first().is_none_or(|&s| self.ix.attr(self.t, s, enable) != Some("false"))
    }

    /// The value `style` gives `name` (at `path` under it): the first style of its chain with the
    /// category on settles it (as the reader's `ObjectProps::get`; none there is the default).
    pub fn value(&self, style: &str, path: &[&str], name: &str, enable: &str) -> Option<String> {
        let s = self.chain(style).into_iter().find(|&s| self.ix.attr(self.t, s, enable) != Some("false"))?;
        let at = if path.is_empty() { Some(s) } else { self.ix.path(self.t, s, path) }?;
        self.ix.value(self.t, at, name)
    }

    /// The settings a style element carries as attributes.
    fn attribute_names(&self, style: &str) -> HashSet<String> {
        self.chain(style).first().map(|&s| self.ix.attrs(self.t, s).iter().map(|a| self.t[a.name.0..a.name.1].to_owned()).collect()).unwrap_or_default()
    }

    /// The text of a style's own element at `path` (its first style that has it).
    fn element(&self, style: &str, name: &str) -> Option<String> {
        self.chain(style).into_iter().find_map(|s| {
            let at = *self.ix.named(self.t, s, name).first()?;
            Some(self.t[self.ix.nodes[at].start..self.ix.nodes[at].end].to_owned())
        })
    }
}

/// What an object's style gives `name` (attributes of the style element; `enable` its category).
pub(super) fn from_style(w: &Work<'_>, ix: &Index, t: &str, i: usize, path: &[&str], name: &str, enable: &str) -> Option<String> {
    let style = ix.value(t, i, "AppliedObjectStyle")?;
    Styles::load(w)?.value(&style, path, name, enable)
}

pub(super) fn inherited_frame_insets(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> Option<[f64; 4]> {
    let style = ix.value(t, i, "AppliedObjectStyle")?;
    let styles = Styles::load(w)?;
    let owner = styles.chain(&style).into_iter()
        .find(|&s| styles.ix.attr(styles.t, s, GENERAL.0) != Some("false"))?;
    let pref = styles.ix.path(styles.t, owner, &["TextFramePreference"])?;
    super::textframe::read_insets(&styles.ix, styles.t, pref)
}

/// What `style` itself gives a stroke setting.
fn from_style_named(w: &Work<'_>, style: &str, name: &str) -> Option<String> {
    Styles::load(w)?.value(style, &[], name, "EnableStroke")
}

/// Attributes of a style that are not settings of the object (identity, the panel, gradients
/// the object always carries).
fn not_a_setting(name: &str) -> bool {
    name.starts_with("Enable") || matches!(name, "Self" | "Name" | "EmitCss" | "IncludeClass" | "ExtendedKeyboardShortcut" | "KeyboardShortcut"
        | "AppliedParagraphStyle" | "ApplyNextParagraphStyle" | "GradientFillAngle" | "GradientStrokeAngle"
        | "GradientFillHiliteLength" | "GradientFillHiliteAngle" | "GradientStrokeHiliteLength" | "GradientStrokeHiliteAngle")
}

/// Text frame settings by style category: (switch, settings). `TextColumnCount` and
/// `TextColumnMaxWidth` are always written; the general category also carries the insets and the
/// column width (worked out).
const GENERAL: (&str, &[&str]) = ("EnableTextFrameGeneralOptions", &["TextColumnGutter", "UseFixedColumnWidth", "VerticalJustification", "VerticalThreshold", "IgnoreWrap", "UseFlexibleColumnWidth", "VerticalBalanceColumns"]);
const CATEGORIES: [(&str, &[&str]); 4] = [
    ("EnableTextFrameBaselineOptions", &["FirstBaselineOffset", "MinimumFirstBaselineOffset"]),
    ("EnableTextFrameAutoSizingOptions", &["AutoSizingType", "AutoSizingReferencePoint", "UseMinimumHeightForAutoSizing", "MinimumHeightForAutoSizing",
        "UseMinimumWidthForAutoSizing", "MinimumWidthForAutoSizing", "UseNoLineBreaksForAutoSizing"]),
    ("EnableTextFrameColumnRuleOptions", &["ColumnRuleOverride", "ColumnRuleOffset", "ColumnRuleTopInset", "ColumnRuleInsetChainOverride", "ColumnRuleBottomInset",
        "ColumnRuleStrokeWidth", "ColumnRuleStrokeColor", "ColumnRuleStrokeType", "ColumnRuleStrokeTint", "ColumnRuleOverprintOverride"]),
    ("EnableTextFrameFootnoteOptions", &["FootnotesEnableOverrides", "FootnotesSpanAcrossColumns", "FootnotesMinimumSpacing", "FootnotesSpaceBetween"]),
];
const ALWAYS: [&str; 2] = ["TextColumnCount", "TextColumnMaxWidth"];

pub(super) fn apply(w: &mut Work<'_>, ids: &[String], style: &str, clear: bool) -> Result<(), EditError> {
    apply_items(w, ids, style, clear, false)
}

/// What the Object Styles panel menu "Redefine Style" leaves on the object it redefined from (oracle os-redefine-menu-*): the style is applied with its
/// overrides cleared, but the gradient start and length stay as they were (Alt-click / Clear Overrides recompute them from the stroke weight, the
/// redefine does not: 161 after a 6 pt stroke was put on an object that was unstroked when the style went on, where the DOM path gives 166).
pub(super) fn apply_after_redefine(w: &mut Work<'_>, ids: &[String], style: &str) -> Result<(), EditError> {
    apply_core(w, ids, style, true, false, false)
}

fn apply_items(w: &mut Work<'_>, ids: &[String], style: &str, clear: bool, through_group: bool) -> Result<(), EditError> {
    apply_core(w, ids, style, clear, through_group, true)
}

use application::apply_core;

/// Break Link to Style (oracle os-break-link*): the object stands with [None] and keeps what its style gave it. Every setting of the style
/// (in a category that is on) that differs from [None]'s and that the object does not have itself becomes the object's own; so does the
/// opacity (a `TransparencySetting`, put before the text wrap). What the object has itself stays.
pub(super) fn break_link(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    materialize_style(w, ids, NONE, true)
}

fn materialize_style(w: &mut Work<'_>, ids: &[String], replacement: &str, remove_fitting: bool) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let old = ix.value(&t, i, "AppliedObjectStyle").unwrap_or_else(|| NONE.to_owned());
        if old == NONE {
            continue;
        }
        let Some(styles) = Styles::load(w) else { continue };
        let mut names: Vec<String> = styles.attribute_names(&old).into_iter().filter(|n| !not_a_setting(n)).collect();
        names.sort();
        let mut edits = Vec::new();
        for n in names {
            if ix.attr(&t, i, &n).is_some() { continue; }
            let enable = setting_category(&n);
            let (Some(mine), none) = (styles.value(&old, &[], &n, enable), styles.value(replacement, &[], &n, "")) else { continue };
            if !enable.is_empty() && !styles.enabled(&old, enable) { continue; }
            // [None] that does not say has the plain default (no overprint, no tint)
            if none.as_deref().unwrap_or(if matches!(mine.as_str(), "false" | "-1") { mine.as_str() } else { "" }) != mine.as_str() {
                edits.push(attr_edit(&ix, &t, i, &n, &mine));
            }
        }
        // the opacity
        let opacity = styles.value(&old, &["TransparencySetting", "BlendingSetting"], "Opacity", "")
            .filter(|o| styles.value(replacement, &["TransparencySetting", "BlendingSetting"], "Opacity", "").as_deref().unwrap_or("100") != o);
        if let Some(opacity) = opacity.filter(|_| ix.named(&t, i, "TransparencySetting").is_empty()) {
            let at = ix.named(&t, i, "TextFramePreference").first().copied().or_else(|| ix.named(&t, i, "TextWrapPreference").first().copied()).or_else(|| ix.children(i).last().copied());
            if let Some(at) = at {
                let indent = indent_before(&t, ix.nodes[at].start).to_owned();
                let start = t[..ix.nodes[at].start].rfind(char::from(10)).map_or(ix.nodes[at].start, |p| p + 1);
                edits.push((start..start, [format!("{indent}<TransparencySetting>"), format!("{indent}	<BlendingSetting Opacity=\"{}\" />", escape(&opacity)), format!("{indent}</TransparencySetting>"), String::new()].join("
")));
            }
        }
        // [None] has no frame fitting: the object loses its own (as when [None] is applied)
        if remove_fitting && let Some(&f) = ix.named(&t, i, "FrameFittingOption").first() {
            edits.push((element_span(&ix, &t, f), String::new()));
        }
        edits.push(attr_edit(&ix, &t, i, "AppliedObjectStyle", replacement));
        w.splice(&part, edits)?;
    }
    Ok(())
}

/// Object ▸ Clear Overrides (the Object Styles panel): as an Alt-click on the object's own style.
pub(super) fn clear_overrides(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        let style = ix.value(&t, i, "AppliedObjectStyle").unwrap_or_else(|| NONE.to_owned());
        drop((part, ix, i, t));
        apply(w, std::slice::from_ref(id), &style, true)?;
    }
    Ok(())
}

pub(super) use clear_undefined::clear_not_in_style;

fn setting_category(name: &str) -> &'static str {
    if name.starts_with("Fill") || matches!(name, "OverprintFill") || name.starts_with("GradientFill") {
        "EnableFill"
    } else if name.contains("Corner") {
        "EnableStrokeAndCornerOptions"
    } else if name.starts_with("Stroke") || name.starts_with("GradientStroke") ||
        matches!(name, "OverprintStroke" | "MiterLimit" | "EndCap" | "EndJoin" | "LeftLineEnd" | "RightLineEnd" | "GapColor" | "GapTint")
    {
        "EnableStroke"
    } else {
        ""
    }
}

/// InDesign's gradient start and length after clearing: bottom left and width of the visible
/// bounds in the object's own space, with the stroke weight it had (whatever its colour).
fn gradient_reset(w: &Work<'_>, ix: &Index, t: &str, i: usize, new: &str, through_group: bool) -> Vec<(std::ops::Range<usize>, String)> {
    if GRAPHICS.contains(&ix.local(t, i)) {
        let Some(b) = ix.path(t, i, &["Properties", "GraphicBounds"]) else { return Vec::new() };
        let number = |key| ix.attr(t, b, key).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let start = geom::pair((number("Left"), number("Bottom")));
        let length = geom::num(number("Right") - number("Left"));
        return vec![attr_edit(ix,t,i,"GradientFillStart",&start),attr_edit(ix,t,i,"GradientFillLength",&length)];
    }
    let Some(path) = own_path(ix, t, i) else { return Vec::new() };
    let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, q, c)| (q.clone(), *c)).collect();
    let Some([x0, _, x1, y1]) = geom::path_bounds(&shapes) else { return Vec::new() };
    let unstroked = stroke_attr(w, ix, t, i, "StrokeColor").is_none_or(|c| c == "Swatch/None");
    let weight = if through_group { from_style_named(w, new, "StrokeWeight") }
        else if unstroked { Some(if from_style_named(w, new, "StrokeColor").is_none_or(|c| c == "Swatch/None") { "0" } else { "1" }.to_owned()) }
        else { stroke_attr(w, ix, t, i, "StrokeWeight") }
        .and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).max(0.0);
    let out = match stroke_attr(w, ix, t, i, "StrokeAlignment").as_deref() {
        Some("InsideAlignment") => 0.0,
        Some("OutsideAlignment") => weight,
        _ => weight / 2.0,
    };
    let start = geom::pair((x0 - out, y1 + out));
    let length = geom::num(x1 - x0 + 2.0 * out);
    ["GradientFillStart", "GradientStrokeStart"].iter().map(|n| attr_edit(ix, t, i, n, &start))
        .chain(["GradientFillLength", "GradientStrokeLength"].iter().map(|n| attr_edit(ix, t, i, n, &length))).collect()
}

/// A text frame's `TextFramePreference` (and baseline grid) after `new` replaces `old`.
fn frame_settings(w: &Work<'_>, ix: &Index, t: &str, i: usize, old: &str, new: &str, clear: bool, through_group: bool) -> Vec<(std::ops::Range<usize>, String)> {
    let Some(styles) = Styles::load(w) else { return Vec::new() };
    let Some(&pref) = ix.named(t, i, "TextFramePreference").first() else { return Vec::new() };
    let mut edits = Vec::new();
    let tfp = ["TextFramePreference"];
    // the value the frame has now: its own, else its old style's, else [None]'s
    let current = |name: &str, enable: &str| ix.value(t, pref, name).or_else(|| styles.value(old, &tfp, name, enable)).or_else(|| styles.value(NONE, &tfp, name, enable));
    let category = |enable: &str, names: &[&str], edits: &mut Vec<(std::ops::Range<usize>, String)>| {
        let on = styles.enabled(new, enable);
        for &name in names {
            let own = ix.value(t, pref, name);
            if on {
                let theirs = styles.value(new, &tfp, name, enable);
                if own.is_some() && (clear || own == theirs) {
                    edits.extend(attr_removal(ix, t, pref, name));
                }
            } else if !through_group && own.is_none() && let Some(v) = current(name, enable) {
                edits.push(attr_edit(ix, t, pref, name, &v));
            }
        }
    };
    category(GENERAL.0, GENERAL.1, &mut edits);
    for (enable, names) in CATEGORIES {
        category(enable, names, &mut edits);
    }
    if styles.enabled(new, GENERAL.0) {
        for name in ALWAYS {
            if let Some(v) = styles.value(new, &tfp, name, GENERAL.0) {
                edits.push(attr_edit(ix, t, pref, name, &v));
            }
        }
        if let Some(insets) = style_insets(&styles, new) {
            edits.extend(super::corners::write_insets(ix, t, i, insets));
        }
    }
    // elements that go with a category (after the TextFramePreference, in this order): written
    // out when the new style turns the category off and the frame has none
    let n = &ix.nodes[pref];
    let ind = indent_before(t, n.start);
    let mut block = String::new();
    for (enable, name) in [("EnableTextFrameBaselineOptions", "BaselineFrameGridOption"), ("EnableTextFrameFootnoteOptions", "TextFrameFootnoteOptionsObject")] {
        if !through_group && !styles.enabled(new, enable) && ix.named(t, i, name).is_empty()
            && let Some(text) = styles.element(old, name).or_else(|| styles.element(NONE, name))
        {
            block.push_str(&format!("{ind}{}\n", reindent(&text, ind)));
        }
    }
    if !block.is_empty() {
        let at = t[n.end..].find('\n').map_or(n.end, |k| n.end + k + 1);
        edits.push((at..at, block));
    }
    edits
}

/// The insets a style's text frame preference gives (top, left, bottom, right).
fn style_insets(styles: &Styles<'_>, style: &str) -> Option<[f64; 4]> {
    styles.chain(style).into_iter().find_map(|s| {
        let at = styles.ix.path(styles.t, s, &["TextFramePreference", "Properties", "InsetSpacing"])?;
        let v: Vec<f64> = if styles.ix.attr(styles.t, at, "type") == Some("list") {
            styles.ix.named(styles.t, at, "ListItem").iter().filter_map(|&li| element_text(&styles.ix, styles.t, li).parse().ok()).collect()
        } else {
            vec![element_text(&styles.ix, styles.t, at).parse().ok()?; 4]
        };
        (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
    })
}

/// An element copied from Styles.xml, its inner lines indented for the object.
fn reindent(text: &str, ind: &str) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim().to_owned();
    let rest: Vec<&str> = lines.collect();
    let base = rest.last().map_or(0, |l| l.len() - l.trim_start().len());
    let mut out = first;
    for l in rest {
        let cut = l.len() - l.trim_start().len();
        out.push('\n');
        out.push_str(ind);
        out.push_str(&"\t".repeat(cut.saturating_sub(base)));
        out.push_str(l.trim_start());
    }
    out
}
