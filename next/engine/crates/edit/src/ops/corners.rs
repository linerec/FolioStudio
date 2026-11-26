//! Object ▸ Corner Options and the control panel's corner fields (next/oracle/scenarios/
//! basic-corners.json). Each corner has a shape and a size. InDesign writes
//! `<Corner>CornerOption` only for a shape other than None and `<Corner>CornerRadius` only for a
//! size other than 12; the item's own `CornerOption` / `CornerRadius` repeat the top left's.
//! Where the object style sets a corner, a default is written out so the style does not show
//! through. The command carries all four corners (the reader owns what an item has now).
//! A text frame with a shaped corner keeps one inset: four equal insets are written as one
//! value, and back as four when the frame is rectangular again (corner-text-frame,
//! corner-text-frame-back; four different insets stay four — measured with 2/4/6/8).
use super::*;

pub(super) const CORNERS: [&str; 4] = ["TopLeft", "TopRight", "BottomLeft", "BottomRight"];
const SHAPES: [&str; 6] = ["None", "RoundedCorner", "InverseRoundedCorner", "InsetCorner", "BevelCorner", "FancyCorner"];
const DEFAULT_RADIUS: f64 = 12.0;
const ENABLE: &str = "EnableStrokeAndCornerOptions";
/// Items with corners (lines and groups have none of their own).
const SHAPED: [&str; 4] = ["Rectangle", "Oval", "Polygon", "TextFrame"];

/// One corner: `shape` as IDML names it ("None", "RoundedCorner", …), `radius` in points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Corner {
    pub shape: String,
    pub radius: f64,
}

pub(super) fn set_corners(w: &mut Work<'_>, ids: &[String], corners: &[Corner; 4]) -> Result<(), EditError> {
    for c in corners {
        if !SHAPES.contains(&c.shape.as_str()) {
            return Err(EditError::new("invalid", format!("unknown corner shape {}", c.shape)));
        }
        if !c.radius.is_finite() || c.radius < 0.0 {
            return Err(EditError::new("invalid", "corner size must be a number of points, 0 or more"));
        }
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        if !SHAPED.contains(&ix.local(&t, i)) {
            return Err(EditError::new("unsupported", format!("{id} ({}) has no corners of its own", ix.local(&t, i))));
        }
        // (attribute, value, default)
        let mut wanted: Vec<(String, String, bool)> = Vec::new();
        let shape = |c: &Corner| (c.shape.clone(), c.shape == "None");
        let radius = |c: &Corner| (geom::num(c.radius), (c.radius - DEFAULT_RADIUS).abs() < 1e-9);
        for (name, c) in CORNERS.iter().zip(corners) {
            let (v, d) = shape(c);
            wanted.push((format!("{name}CornerOption"), v, d));
            let (v, d) = radius(c);
            wanted.push((format!("{name}CornerRadius"), v, d));
        }
        let (v, d) = shape(&corners[0]);
        wanted.push(("CornerOption".into(), v, d));
        let (v, d) = radius(&corners[0]);
        wanted.push(("CornerRadius".into(), v, d));
        let mut edits = Vec::new();
        for (name, value, default) in &wanted {
            let styled = style_attr(w, &ix, &t, i, name, ENABLE);
            let keep_out = *default && styled.as_deref().is_none_or(|s| s == value || same_number(s, value));
            if keep_out {
                edits.extend(attr_removal(&ix, &t, i, name));
            } else if ix.value(&t, i, name).as_deref() != Some(value.as_str()) {
                edits.push(attr_edit(&ix, &t, i, name, value));
            }
        }
        let story = (ix.local(&t, i) == "TextFrame").then(|| ix.attr(&t, i, "ParentStory").map(str::to_owned)).flatten();
        if ix.local(&t, i) == "TextFrame" {
            let shaped = corners.iter().any(|c| c.shape != "None" && c.radius > 1e-9);
            edits.extend(inset_form(&ix, &t, i, shaped));
        }
        if !edits.is_empty() {
            w.splice(&part, edits)?;
            // the frame's shape holds its lines: they are composed again
            if let Some(s) = story {
                w.recompose(&s);
            }
        }
    }
    Ok(())
}

/// A text frame's `InsetSpacing`: its element and values as written.
fn insets_of(ix: &Index, t: &str, frame: usize) -> Option<(usize, Vec<String>)> {
    let at = ix.path(t, frame, &["TextFramePreference", "Properties", "InsetSpacing"])?;
    let n = &ix.nodes[at];
    let values: Vec<String> = if ix.attr(t, at, "type") == Some("list") {
        ix.children(at).iter().map(|&c| { let m = &ix.nodes[c]; t[m.open_end..m.close_start].trim().to_owned() }).collect()
    } else {
        vec![t[n.open_end..n.close_start].trim().to_owned(); 4]
    };
    (values.len() == 4).then_some((at, values))
}

/// `InsetSpacing` with `values`: one value when `shaped` and all four are equal, else four.
fn inset_edit(ix: &Index, t: &str, at: usize, values: &[String], shaped: bool) -> Option<(std::ops::Range<usize>, String)> {
    let n = &ix.nodes[at];
    let one = shaped && values.iter().all(|v| same_number(v, &values[0]));
    let ind = indent_before(t, n.start);
    let text = if one {
        format!("<InsetSpacing type=\"unit\">{}</InsetSpacing>", values[0])
    } else {
        let items: String = values.iter().map(|v| format!("\n{ind}\t<ListItem type=\"unit\">{v}</ListItem>")).collect();
        format!("<InsetSpacing type=\"list\">{items}\n{ind}</InsetSpacing>")
    };
    (t[n.start..n.end] != text).then_some((n.start..n.end, text))
}

/// The frame's insets in the form its corners ask for (after a corner change).
fn inset_form(ix: &Index, t: &str, frame: usize, shaped: bool) -> Option<(std::ops::Range<usize>, String)> {
    let (at, values) = insets_of(ix, t, frame)?;
    inset_edit(ix, t, at, &values, shaped)
}

/// New insets (top, left, bottom, right) for a text frame, in the form its shape asks for.
pub(super) fn write_insets(ix: &Index, t: &str, frame: usize, insets: [f64; 4]) -> Option<(std::ops::Range<usize>, String)> {
    let (at, _) = insets_of(ix, t, frame)?;
    let values: Vec<String> = insets.iter().map(|v| geom::num(*v)).collect();
    inset_edit(ix, t, at, &values, shaped(ix, t, frame))
}

/// The frame has a shaped corner, as the attributes InDesign writes say (a corner option
/// other than None, of a size above 0; before per-corner options, the item's own).
pub(super) fn shaped(ix: &Index, t: &str, frame: usize) -> bool {
    let size = |name: &str| ix.attr(t, frame, name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(DEFAULT_RADIUS);
    let per_corner = CORNERS.iter().any(|c| ix.attr(t, frame, &format!("{c}CornerOption")).is_some() || ix.attr(t, frame, &format!("{c}CornerRadius")).is_some());
    if per_corner {
        CORNERS.iter().any(|c| ix.attr(t, frame, &format!("{c}CornerOption")).is_some_and(|o| o != "None") && size(&format!("{c}CornerRadius")) > 1e-9)
    } else {
        ix.attr(t, frame, "CornerOption").is_some_and(|o| o != "None") && size("CornerRadius") > 1e-9
    }
}
