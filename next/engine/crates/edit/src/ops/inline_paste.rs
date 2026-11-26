//! Library ▸ place an item in text: `Asset.placeAsset(Text)` of InDesign 2025 (probes bookx-c4-inline and bookx-c4-inline2,
//! rules/bookx-c4.ko.md). The item's one object becomes an inline anchored object at a story position:
//! - on an insertion point it goes in front of the character there; on a range of characters it takes the range's place, a paragraph
//!   return inside the range too (`on<OBJ>wo` for "one\rtwo" over characters 2..4); in an empty story it is the only character;
//! - its anchor character is formatted like the one before it in the paragraph (else the one at it), as every inserted object is;
//! - the settings are the inline defaults (`AnchoredPosition="InlinePosition"`, pinned), a group has none of its own and each member
//!   gets the same with `VerticalAlignment="BottomAlign"` (what InDesign writes);
//! - the object keeps its own turn and scale, its layer is dropped and its lock kept; its `ItemTransform` is moved so that the bottom
//!   left corner of its visible bounds (the stroke included, joins as drawn) is the origin: the anchor point on the baseline;
//! - an item of several objects places nothing in text (the call returns nothing, the story stays as it was): refused here.
use super::*;
use folio_idml::anchored::FIELDS;

/// The settings InDesign writes for an inline object (`bottom`: a group member's vertical alignment).
fn inline_settings(bottom: bool) -> String {
    let attrs: String = FIELDS
        .iter()
        .map(|f| format!(" {}=\"{}\"", f.attr, if bottom && f.key == "verticalAlignment" { "BottomAlign" } else { f.default }))
        .collect();
    format!("<AnchoredObjectSetting{attrs}/>")
}

/// Where an object's `AnchoredObjectSetting` goes: after its Properties and (a text frame) its TextFramePreference.
fn settings_at(ix: &Index, t: &str, i: usize) -> usize {
    ix.nodes.iter().enumerate().find(|(n, node)| node.parent == Some(i) && !matches!(ix.local(t, *n), "Properties" | "TextFramePreference")).map(|(_, n)| n.start).unwrap_or(ix.nodes[i].close_start)
}

/// `xml` (one object) with its anchored settings made inline: any it had are dropped.
fn with_inline_settings(xml: &str) -> Result<String, EditError> {
    let mut raw = xml.to_owned();
    // an object that was anchored already starts over: its own settings (and its members') go
    loop {
        let ix = Index::scan(&raw).map_err(|e| EditError::new("xml", e))?;
        let Some(n) = (0..ix.nodes.len()).find(|&n| ix.local(&raw, n) == "AnchoredObjectSetting") else { break };
        raw.replace_range(ix.nodes[n].start..ix.nodes[n].end, "");
    }
    let ix = Index::scan(&raw).map_err(|e| EditError::new("xml", e))?;
    let mut inserts: Vec<(usize, String)> = Vec::new();
    if ix.local(&raw, 0) == "Group" {
        for n in 1..ix.nodes.len() {
            if matches!(ix.local(&raw, n), "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "TextFrame") {
                inserts.push((settings_at(&ix, &raw, n), inline_settings(true)));
            }
        }
    } else {
        inserts.push((settings_at(&ix, &raw, 0), inline_settings(false)));
    }
    inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, s) in inserts {
        raw.insert_str(at, &s);
    }
    Ok(raw)
}

/// The `src` of every story the designmap lists.
fn story_parts(w: &Work<'_>) -> Result<Vec<String>, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    Ok((0..ix.nodes.len()).filter(|&i| &t[ix.nodes[i].name.clone()] == "idPkg:Story").filter_map(|i| ix.attr(t, i, "src").map(str::to_owned)).collect())
}

/// A story the item brought (a text frame's) is listed last in the designmap: InDesign appends it there when a library item is placed,
/// where Duplicate and Paste list it first (probe bookx-c4-inline: the new `Story_u19b.xml` after `Story_ue7.xml`).
fn new_stories_last(w: &mut Work<'_>, before: &[String]) -> Result<(), EditError> {
    let t = w.text("designmap.xml")?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let nodes: Vec<usize> = (0..ix.nodes.len()).filter(|&i| &t[ix.nodes[i].name.clone()] == "idPkg:Story").collect();
    let fresh = |i: usize| ix.attr(&t, i, "src").is_some_and(|s| !before.iter().any(|b| b == s));
    let k = nodes.iter().take_while(|&&i| fresh(i)).count();
    if k == 0 || k == nodes.len() || nodes[k..].iter().any(|&i| fresh(i)) {
        return Ok(());
    }
    let moved: String = nodes[..k].iter().map(|&i| format!("\n\t{}", &t[ix.nodes[i].start..ix.nodes[i].end])).collect();
    let last_end = ix.nodes[*nodes.last().unwrap()].end;
    w.splice("designmap.xml", vec![(ix.nodes[nodes[0]].start..ix.nodes[nodes[k]].start, String::new()), (last_end..last_end, moved)])
}

/// `placeLibraryAssetInText`: the clipboard's one object (the library item's, see `commands/library.rs`) goes into `story` over the
/// characters `start..end` (an insertion point when they are equal).
pub(crate) fn run(w: &mut Work<'_>, story: &str, start: usize, end: usize) -> Result<(), EditError> {
    let story = crate::text::story_of(w, story)?;
    let stories = story_parts(w)?;
    let c = clipboard::single_copy(w)?;
    let full = c.parent.then_after(&c.transform);
    // Only an object that is not turned, flipped or sheared has its position worked out: InDesign lifts a turned one by the rise of its
    // bottom edge (30 degrees: width times sine) and not by its bounding box; the rule for every angle is not found (rules/bookx-c4.ko.md)
    let l = full.0;
    if l[1].abs() > 1e-9 || l[2].abs() > 1e-9 || l[0] <= 0.0 || l[3] <= 0.0 {
        return Err(EditError::new("unsupported", "a turned, flipped or sheared object cannot be placed in text yet (its position in the line is not worked out)"));
    }
    let cx = Index::scan(&c.xml).map_err(|e| EditError::new("xml", e))?;
    // the visible bounds as Align counts them: the stroke outline first, then the transform (the join of a stroked corner counts)
    let b = alignment::visible(w, &cx, &c.xml, 0, full).ok_or_else(|| EditError::new("unsupported", "an object without bounds cannot be placed in text"))?;
    let mut m = full.0;
    m[4] -= b[0];
    m[5] -= b[3];
    let xml = clipboard::with_attr(&clipboard::with_attr(&c.xml, "ItemTransform", Some(&Mat(m).text()))?, "ItemLayer", None)?;
    let xml = with_inline_settings(&xml)?;
    crate::text::replace_with_node(w, &story, start, end, '\u{fffc}', &xml)?;
    new_stories_last(w, &stories)?;
    w.created.push(c.id);
    w.recompose(&story);
    Ok(())
}
