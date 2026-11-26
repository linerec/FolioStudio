//! Table Options ▸ Row Strokes, Column Strokes and Fills: the alternating patterns (oracle `tables-alternating`, measured with the
//! DOM's table properties, one assignment per setting as the dialog makes them).
//! - The pattern is nothing but the counts: a table has `StartRowFillCount` rows of the start fill, then `EndRowFillCount` of the end
//!   fill, and so on (the same for columns and for strokes). The DOM's `alternatingFills` writes no attribute of its own.
//! - Every setting is an attribute of the `Table` element, written where it differs from what the table's applied table style
//!   gives (the style's `BasedOn` chain; `[No table style]` holds InDesign's defaults) and left out where it does not, so a value
//!   set back to the default goes away. A table keeps whatever else it carries.
//! - InDesign's scripting takes any number (a tint of 150, a count of -1); the dialog does not, so the limits here are the dialog's.
//!   Not followed (rendering is the same either way): a colour assigned while its count is above 0 is written even where it equals the
//!   default (`StartRowFillColor="Color/Black"`), and a fill setting equal to a non-default style value is written too.
use super::*;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// A whole number of rows or columns (a pattern's count, the rows or columns skipped).
    Count,
    /// A stroke weight in points.
    Weight,
    /// A tint in percent.
    Tint,
    Flag,
    /// A swatch's `Self` (`Swatch/None` for none).
    Swatch,
    /// A stroke style's `Self`.
    StrokeType,
    /// A length in points (a space before or after the table can be negative).
    Length,
}

/// Every pattern setting: its attribute, its kind and InDesign's default ([No table style]).
fn known() -> &'static BTreeMap<String, (Kind, &'static str)> {
    static KNOWN: OnceLock<BTreeMap<String, (Kind, &'static str)>> = OnceLock::new();
    KNOWN.get_or_init(|| {
        let mut m = BTreeMap::new();
        for dim in ["Row", "Column"] {
            for side in ["Start", "End"] {
                let start = side == "Start";
                let stroke = format!("{side}{dim}Stroke");
                m.insert(format!("{stroke}Count"), (Kind::Count, "0"));
                m.insert(format!("{stroke}Color"), (Kind::Swatch, "Color/Black"));
                m.insert(format!("{stroke}Weight"), (Kind::Weight, if start { "1" } else { "0.25" }));
                // the only attribute that names the type "LineStyle"
                let kind = if side == "End" && dim == "Column" { format!("{side}{dim}LineStyle") } else { format!("{stroke}Type") };
                m.insert(kind, (Kind::StrokeType, "StrokeStyle/$ID/Solid"));
                m.insert(format!("{stroke}Tint"), (Kind::Tint, "100"));
                m.insert(format!("{stroke}Overprint"), (Kind::Flag, "false"));
                m.insert(format!("{stroke}GapColor"), (Kind::Swatch, "Color/Paper"));
                m.insert(format!("{stroke}GapTint"), (Kind::Tint, "100"));
                m.insert(format!("{stroke}GapOverprint"), (Kind::Flag, "false"));
                let fill = format!("{side}{dim}Fill");
                m.insert(format!("{fill}Count"), (Kind::Count, "0"));
                m.insert(format!("{fill}Color"), (Kind::Swatch, if start { "Color/Black" } else { "Swatch/None" }));
                m.insert(format!("{fill}Tint"), (Kind::Tint, if start { "20" } else { "100" }));
                m.insert(format!("{fill}Overprint"), (Kind::Flag, "false"));
            }
            for skip in ["First", "Last"] {
                for what in ["Stroke", "Fill"] {
                    m.insert(format!("Skip{skip}Alternating{what}{dim}s"), (Kind::Count, "0"));
                }
            }
        }
        m.insert("ColumnFillsPriority".into(), (Kind::Flag, "false"));
        // Table Setup: the table border and the space around the table (a table style sets them too)
        for side in ["Top", "Left", "Bottom", "Right"] {
            let border = format!("{side}BorderStroke");
            m.insert(format!("{border}Weight"), (Kind::Weight, "1"));
            m.insert(format!("{border}Type"), (Kind::StrokeType, "StrokeStyle/$ID/Solid"));
            m.insert(format!("{border}Color"), (Kind::Swatch, "Color/Black"));
            m.insert(format!("{border}Tint"), (Kind::Tint, "100"));
            m.insert(format!("{border}Overprint"), (Kind::Flag, "false"));
            m.insert(format!("{border}GapColor"), (Kind::Swatch, "Color/Paper"));
            m.insert(format!("{border}GapTint"), (Kind::Tint, "100"));
            m.insert(format!("{border}GapOverprint"), (Kind::Flag, "false"));
        }
        m.insert("SpaceBefore".into(), (Kind::Length, "4"));
        m.insert("SpaceAfter".into(), (Kind::Length, "-4"));
        m
    })
}

const NO_STYLE: &str = "TableStyle/$ID/[No table style]";

/// The values the document's table styles give.
struct TableStyles<'t> {
    t: &'t str,
    ix: Option<Index>,
}

impl<'t> TableStyles<'t> {
    fn new(t: &'t str) -> Self {
        TableStyles { t, ix: Index::scan(t).ok() }
    }

    /// What `style` (following `BasedOn`, nearest first) gives `name`; none where no style in the chain sets it.
    fn value(&self, style: &str, name: &str) -> Option<String> {
        let ix = self.ix.as_ref()?;
        let mut style = style.to_owned();
        for _ in 0..32 {
            let s = ix.by_self(self.t, &style)?;
            if let Some(v) = ix.value(self.t, s, name) {
                return Some(v);
            }
            let based = ix.path(self.t, s, &["Properties", "BasedOn"])?;
            let n = &ix.nodes[based];
            let b = crate::xml::unescape(self.t[n.open_end..n.close_start].trim());
            if b.is_empty() {
                return None;
            }
            // `BasedOn` names `[No table style]` without its `TableStyle/` prefix
            style = if b.starts_with("TableStyle/") { b } else { format!("TableStyle/{b}") };
        }
        None
    }
}

fn normalize(name: &str, kind: Kind, raw: &str) -> Result<String, EditError> {
    let bad = |m: String| Err(EditError::new("invalid", m));
    let raw = raw.trim();
    match kind {
        Kind::Count => match raw.parse::<f64>() {
            Ok(v) if v.fract() == 0.0 && (0.0..=999.0).contains(&v) => Ok(geom::num(v)),
            _ => bad(format!("{name}: a whole number from 0 to 999")),
        },
        Kind::Weight => match raw.parse::<f64>() {
            Ok(v) if (0.0..=800.0).contains(&v) => Ok(geom::num(v)),
            _ => bad(format!("{name}: 0 to 800 pt")),
        },
        Kind::Length => match raw.parse::<f64>() {
            Ok(v) if v.is_finite() && v.abs() <= 10000.0 => Ok(geom::num(v)),
            _ => bad(format!("{name}: a length in points")),
        },
        Kind::Tint => match raw.parse::<f64>() {
            Ok(v) if (0.0..=100.0).contains(&v) => Ok(geom::num(v)),
            _ => bad(format!("{name}: 0 to 100 %")),
        },
        Kind::Flag => match raw {
            "true" | "false" => Ok(raw.to_owned()),
            _ => bad(format!("{name}: true or false")),
        },
        Kind::Swatch if raw.contains('/') => Ok(raw.to_owned()),
        Kind::Swatch => bad(format!("{name}: a swatch")),
        Kind::StrokeType if raw.starts_with("StrokeStyle/") => Ok(raw.to_owned()),
        Kind::StrokeType => bad(format!("{name}: a stroke style")),
    }
}

pub(super) fn set_options(w: &mut Work<'_>, table: &str, options: &BTreeMap<String, String>) -> Result<(), EditError> {
    if options.is_empty() {
        return Ok(());
    }
    let mut values: Vec<(&str, String, &'static str)> = Vec::new();
    for (name, raw) in options {
        let &(kind, default) = known().get(name).ok_or_else(|| EditError::new("invalid", format!("{name} is not a table pattern setting")))?;
        values.push((name, normalize(name, kind, raw)?, default));
    }
    let (part, ix, i) = w.find(table)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "Table" {
        return Err(EditError::new("invalid", format!("{table} is not a table")));
    }
    let applied = ix.attr(&t, i, "AppliedTableStyle").unwrap_or(NO_STYLE).to_owned();
    let styles_text = w.text("Resources/Styles.xml").map(str::to_owned).unwrap_or_default();
    let styles = TableStyles::new(&styles_text);
    let mut edits = Vec::new();
    for (name, value, default) in &values {
        let base = styles.value(&applied, name).or_else(|| styles.value(NO_STYLE, name)).unwrap_or_else(|| (*default).to_owned());
        if value == &base || same_number(value, &base) {
            edits.extend(attr_removal(&ix, &t, i, name));
        } else {
            edits.push(attr_edit(&ix, &t, i, name, value));
        }
    }
    if !edits.is_empty() {
        w.splice(&part, edits)?;
        let story = part.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned).or_else(|| {
            ix.ancestors(i).find(|&a| ix.local(&t, a) == "Story").and_then(|a| ix.attr(&t, a, "Self")).map(str::to_owned)
        });
        if let Some(story) = story {
            w.recompose(&story);
        }
    }
    Ok(())
}

/// The pattern settings a table has now (its own values, else its table style's, else InDesign's defaults), as IDML writes them.
/// Every attribute a table style sets on a table (Clear Table Style Overrides removes them from the table).
pub(super) fn style_attributes() -> impl Iterator<Item = &'static str> {
    known().keys().map(String::as_str)
}

pub(crate) fn effective(parts: &BTreeMap<String, String>, table: &str) -> Option<BTreeMap<String, String>> {
    let (text, ix, i) = parts.values().filter(|t| t.contains(&format!(" Self=\"{table}\""))).find_map(|t| {
        let ix = Index::scan(t).ok()?;
        let i = ix.by_self(t, table)?;
        (ix.local(t, i) == "Table").then_some((t, ix, i))
    })?;
    let empty = String::new();
    let styles_text = parts.get("Resources/Styles.xml").unwrap_or(&empty);
    let styles = TableStyles::new(styles_text);
    let applied = ix.attr(text, i, "AppliedTableStyle").unwrap_or(NO_STYLE);
    Some(known().iter().map(|(name, &(_, default))| {
        let value = ix.value(text, i, name).or_else(|| styles.value(applied, name)).or_else(|| styles.value(NO_STYLE, name)).unwrap_or_else(|| default.to_owned());
        (name.clone(), value)
    }).collect())
}
