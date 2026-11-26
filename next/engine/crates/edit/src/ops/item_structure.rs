//! Group, arrange, duplicate and remove page items.

use super::*;

#[cfg(test)]
mod layer_memory_tests;

/// Removes an object and what goes with it, as InDesign does (oracle: delete-*):
/// - a threaded text frame: its neighbours are joined (`PreviousTextFrame`/`NextTextFrame`);
///   the story goes with the last frame showing it (part, designmap entry, `StoryList`);
/// - a group's child: the group's `FillColor` is again the fill its children share
///   (written only when they all have the same).
pub(super) fn delete(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let kind = ix.local(&t, i).to_owned();
    // frames inside the deleted one (a group's text frames) take their threads along
    let mut frames = Vec::new();
    if matches!(kind.as_str(), "TextFrame" | "TextPath") {
        frames.push(i);
    }
    for d in i + 1..ix.nodes.len() {
        if ix.nodes[d].start >= ix.nodes[i].end {
            break;
        }
        if matches!(ix.local(&t, d), "TextFrame" | "TextPath") && ix.attr(&t, d, "Self").is_some() {
            frames.push(d);
        }
    }
    let threads: Vec<(String, String, String, String)> = frames
        .iter()
        .map(|&f| {
            let g = |k| ix.attr(&t, f, k).unwrap_or("n").to_owned();
            (ix.attr(&t, f, "Self").unwrap_or_default().to_owned(), g("PreviousTextFrame"), g("NextTextFrame"), g("ParentStory"))
        })
        .collect();
    let group = ix.nodes[i].parent.filter(|&p| ix.local(&t, p) == "Group").and_then(|p| ix.attr(&t, p, "Self")).map(str::to_owned);
    let inside: Vec<String> = (i..ix.nodes.len()).take_while(|&d| ix.nodes[d].start < ix.nodes[i].end).filter_map(|d| ix.attr(&t, d, "Self")).map(str::to_owned).collect();
    w.splice(&part, vec![(element_span(&ix, &t, i), String::new())])?;
    // a page's copy of a parent item (also inside a deleted group): its parent item stays off the page
    overrides::forget_copy(w, &part, &inside)?;

    // the neighbours skip the deleted frames
    let gone: Vec<String> = threads.iter().map(|x| x.0.clone()).collect();
    let live = |mut f: String, back: bool| {
        while f != "n" && gone.contains(&f) {
            let at = threads.iter().find(|x| x.0 == f).expect("deleted frame is listed");
            f = if back { at.1.clone() } else { at.2.clone() };
        }
        f
    };
    for (_, prev, next, story) in &threads {
        let p = live(prev.clone(), true);
        let n = live(next.clone(), false);
        if p != "n" {
            set_attrs(w, &p, &[("NextTextFrame", n.clone())])?;
        }
        if n != "n" {
            set_attrs(w, &n, &[("PreviousTextFrame", p.clone())])?;
        }
        if p == "n" && n == "n" && story != "n" {
            delete_story(w, story)?;
        }
    }
    if let Some(g) = group {
        group_fill(w, &g)?;
    }
    Ok(())
}

pub(super) fn delete_story(w: &mut Work<'_>, story: &str) -> Result<(), EditError> {
    // still shown by another frame or text path?
    let pat = format!("ParentStory=\"{story}\"");
    for p in w.parts_named("Spreads/").into_iter().chain(w.parts_named("MasterSpreads/")) {
        if w.text(&p)?.contains(&pat) {
            return Ok(());
        }
    }
    let dm = "designmap.xml";
    let ix = w.index(dm)?;
    let t = w.text(dm)?.to_owned();
    let root = ix.roots().find(|&r| ix.local(&t, r) == "Document").ok_or_else(|| EditError::new("xml", "designmap has no Document"))?;
    let mut src = None;
    let mut edits = Vec::new();
    for c in ix.named(&t, root, "Story") {
        let s = ix.attr(&t, c, "src").unwrap_or_default();
        if s.ends_with(&format!("_{story}.xml")) {
            src = Some(s.to_owned());
            edits.push((element_span(&ix, &t, c), String::new()));
        }
    }
    if let Some(list) = ix.attr(&t, root, "StoryList") {
        let kept: Vec<&str> = list.split_ascii_whitespace().filter(|s| *s != story).collect();
        edits.push(attr_edit(&ix, &t, root, "StoryList", &kept.join(" ")));
    }
    w.splice(dm, edits)?;
    if let Some(s) = src {
        w.remove_part(&s);
    }
    Ok(())
}

/// A group carries its children's common `FillColor`, and none when they differ
/// (oracle: delete-group-child).
fn group_fill(w: &mut Work<'_>, group: &str) -> Result<(), EditError> {
    let (part, ix, g) = w.find(group)?;
    let t = w.text(&part)?;
    let fills: Vec<Option<&str>> = ix.children(g).iter().filter(|&&c| is_item(&ix, t, c)).map(|&c| ix.attr(t, c, "FillColor")).collect();
    let common = match fills.first() {
        Some(Some(f)) if fills.iter().all(|x| *x == Some(*f)) && *f != "Swatch/None" => Some(f.to_string()),
        _ => None,
    };
    let edit = match common {
        Some(f) => Some(attr_edit(&ix, t, g, "FillColor", &f)),
        None => attr_removal(&ix, t, g, "FillColor"),
    };
    match edit {
        Some(e) => w.splice(&part, vec![e]),
        None => Ok(()),
    }
}

/// Stacking order is element order among the siblings on the same layer: Bring to Front puts
/// the item after the last of them, Send to Back before the first, Forward and Backward swap
/// it with the next or previous one, overlapping or not (oracle: arrange-*).
pub(super) fn arrange(w: &mut Work<'_>, id: &str, how: Arrange) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let parent = ix.nodes[i].parent.ok_or_else(|| EditError::new("invalid", format!("{id} has no parent")))?;
    let layer = ix.attr(&t, i, "ItemLayer");
    let sibs: Vec<usize> = ix.children(parent).iter().copied().filter(|&c| is_item(&ix, &t, c) && ix.attr(&t, c, "ItemLayer") == layer).collect();
    let k = sibs.iter().position(|&c| c == i).expect("item is among its siblings");
    let target = match how {
        Arrange::Front if k + 1 < sibs.len() => Some((*sibs.last().unwrap(), true)),
        Arrange::Forward if k + 1 < sibs.len() => Some((sibs[k + 1], true)),
        Arrange::Backward if k > 0 => Some((sibs[k - 1], false)),
        Arrange::Back if k > 0 => Some((sibs[0], false)),
        _ => None,
    };
    let Some((other, after)) = target else { return Ok(()) };
    let span = element_span(&ix, &t, i);
    let text = t[span.clone()].to_owned();
    let at = if after { ix.nodes[other].end } else { element_span(&ix, &t, other).start };
    w.splice(&part, vec![(span, String::new()), (at..at, text)])
}

/// What InDesign writes for a new group (oracle: group-*), around its members.
fn group_open(id: &str, layer: Option<&str>, indent: &str) -> String {
    let layer = layer.map(|l| format!(" ItemLayer=\"{}\"", escape(l))).unwrap_or_default();
    format!(
        "<Group Self=\"{id}\" ParentInterfaceChangeCount=\"\" TargetInterfaceChangeCount=\"\" LastUpdatedInterfaceChangeCount=\"\" OverriddenPageItemProps=\"\" \
HorizontalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" VerticalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" \
GradientFillStart=\"0 0\" GradientFillLength=\"0\" GradientFillAngle=\"0\" GradientStrokeStart=\"0 0\" GradientStrokeLength=\"0\" GradientStrokeAngle=\"0\"{layer} \
Locked=\"false\" LocalDisplaySetting=\"Default\" GradientFillHiliteLength=\"0\" GradientFillHiliteAngle=\"0\" GradientStrokeHiliteLength=\"0\" GradientStrokeHiliteAngle=\"0\" \
AppliedObjectStyle=\"ObjectStyle/$ID/[None]\" Visible=\"true\" Name=\"$ID/\" ItemTransform=\"1 0 0 1 0 0\">\n\
{indent}\t<TextWrapPreference Inverse=\"false\" ApplyToMasterPageOnly=\"false\" TextWrapSide=\"BothSides\" TextWrapMode=\"None\">\n\
{indent}\t\t<Properties>\n{indent}\t\t\t<TextWrapOffset Top=\"0\" Left=\"0\" Bottom=\"0\" Right=\"0\" />\n{indent}\t\t</Properties>\n\
{indent}\t</TextWrapPreference>\n\
{indent}\t<ObjectExportOption EpubType=\"$ID/\" SizeType=\"DefaultSize\" CustomSize=\"$ID/\" PreserveAppearanceFromLayout=\"PreserveAppearanceDefault\" \
AltTextSourceType=\"SourceXMLStructure\" ActualTextSourceType=\"SourceXMLStructure\" CustomAltText=\"$ID/\" CustomActualText=\"$ID/\" ApplyTagType=\"TagFromStructure\" \
ImageConversionType=\"JPEG\" ImageExportResolution=\"Ppi300\" GIFOptionsPalette=\"AdaptivePalette\" GIFOptionsInterlaced=\"true\" JPEGOptionsQuality=\"High\" \
JPEGOptionsFormat=\"BaselineEncoding\" ImageAlignment=\"AlignLeft\" ImageSpaceBefore=\"0\" ImageSpaceAfter=\"0\" UseImagePageBreak=\"false\" ImagePageBreak=\"PageBreakBefore\" \
CustomImageAlignment=\"false\" SpaceUnit=\"CssPixel\" CustomLayout=\"false\" CustomLayoutType=\"AlignmentAndSpacing\">\n\
{indent}\t\t<Properties>\n{indent}\t\t\t<AltMetadataProperty NamespacePrefix=\"$ID/\" PropertyPath=\"$ID/\" />\n\
{indent}\t\t\t<ActualMetadataProperty NamespacePrefix=\"$ID/\" PropertyPath=\"$ID/\" />\n{indent}\t\t</Properties>\n\
{indent}\t</ObjectExportOption>"
    )
}

/// Group members drop `ItemLayer` and `Locked` (the group carries them) and keep their
/// order; the group takes the frontmost member's place and layer (oracle: group-two,
/// group-apart, group-same-fill).
pub(super) fn group(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    if ids.len() < 2 {
        return Err(EditError::new("invalid", "a group needs two objects or more"));
    }
    let (part, _, _) = w.find(&ids[0])?;
    let ix = w.index(&part)?;
    let t = w.text(&part)?.to_owned();
    let mut members: Vec<usize> = Vec::new();
    for id in ids {
        let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("unsupported", format!("{id} is on another spread")))?;
        check_item(&ix, &t, i, id)?;
        members.push(i);
    }
    let parent = ix.nodes[members[0]].parent;
    if members.iter().any(|&m| ix.nodes[m].parent != parent) {
        return Err(EditError::new("unsupported", "objects in different groups cannot be grouped yet"));
    }
    members.sort_by_key(|&m| ix.nodes[m].start);
    let front = *members.last().unwrap();
    let layer = ix.attr(&t, front, "ItemLayer").map(str::to_owned);
    // what Ungroup Remembers Layers needs: each member's layer and the item below it there
    let siblings: Vec<usize> = parent.map(|p| ix.children(p).iter().copied().filter(|&c| is_item(&ix, &t, c)).collect()).unwrap_or_default();
    let remember: Option<Vec<(String, String)>> = members
        .iter()
        .map(|&m| {
            let layer = ix.attr(&t, m, "ItemLayer")?;
            let below = siblings.iter().take_while(|&&s| s != m).filter(|&&s| !members.contains(&s) && ix.attr(&t, s, "ItemLayer") == Some(layer)).last();
            Some((layer.to_owned(), below.and_then(|&s| ix.attr(&t, s, "Self")).unwrap_or_default().to_owned()))
        })
        .collect();
    let indent = indent_before(&t, ix.nodes[front].start).to_owned();
    let id = w.new_id();
    let mut text = group_open(&id, layer.as_deref(), &indent);
    for &m in &members {
        text.push('\n');
        text.push_str(&indent);
        text.push('\t');
        text.push_str(&element_with(&ix, &t, m, &[], &["ItemLayer", "Locked"]));
    }
    text.push('\n');
    text.push_str(&indent);
    text.push_str("</Group>");
    let mut edits: Vec<(std::ops::Range<usize>, String)> = members[..members.len() - 1].iter().map(|&m| (element_span(&ix, &t, m), String::new())).collect();
    edits.push((ix.nodes[front].start..ix.nodes[front].end, text));
    w.splice(&part, edits)?;
    w.created.push(id.clone());
    group_fill(w, &id)?;
    // Ungroup Remembers Layers: members on other layers than the group's go back there, to
    // where they were. IDML has no place for it (InDesign keeps it in its own model; basic-layers3
    // ungroup-mixed-remembers: group + ungroup leaves the spread as it was), so it rides on the
    // group as a Folio-only script label: `<group layer>|<layer>:<item below it on that layer>...`.
    if let (Some(group_layer), Some(list)) = (&layer, remember)
        && list.iter().any(|(l, _)| l != group_layer) {
        let list: Vec<String> = list.iter().map(|(l, p)| format!("{l}:{p}")).collect();
        set_label(w, &id, MEMBER_LAYERS, &format!("{group_layer}|{}", list.join(" ")))?;
    }
    Ok(())
}

/// The group's label for Ungroup Remembers Layers.
/// Valid only while the group is still on that layer.
const MEMBER_LAYERS: &str = "folio.memberLayers";

/// The members take the group's place, its transform folded into theirs, and its layer and
/// lock back (oracle: ungroup, ungroup-rotated).
pub(super) fn ungroup(w: &mut Work<'_>, id: &str, remember_layers: bool) -> Result<(), EditError> {
    let (part, ix, g) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, g) != "Group" {
        return Err(EditError::new("invalid", format!("{id} is not a group")));
    }
    let gm = item_transform(&ix, &t, g);
    let layer = ix.attr(&t, g, "ItemLayer").map(str::to_owned);
    let remembered = if remember_layers { remembered_layers(w, &ix, &t, g, layer.as_deref())? } else { Vec::new() };
    let indent = indent_before(&t, ix.nodes[g].start).to_owned();
    let kids: Vec<usize> = ix.children(g).iter().copied().filter(|&c| is_item(&ix, &t, c)).collect();
    let mut text = String::new();
    // members that go back to another layer land where they were: above the item that was
    // below them there, else under the layer's first item (else they stay in the group's place)
    let mut landed: std::collections::BTreeMap<usize, String> = std::collections::BTreeMap::new();
    for (k, &c) in kids.iter().enumerate() {
        let m = gm.then_after(&item_transform(&ix, &t, c));
        let mut set = vec![("ItemTransform", m.text()), ("Locked", "false".to_string())];
        let back = remembered.get(k);
        if let Some(l) = back.map(|(l, _)| l).or(layer.as_ref()) {
            set.push(("ItemLayer", l.clone()));
        }
        let element = element_with(&ix, &t, c, &set, &[]);
        let at = back.filter(|(l, _)| Some(l) != layer.as_ref()).and_then(|(l, below)| landing(&ix, &t, g, l, below));
        match at {
            Some(at) => landed.entry(at).or_default().push_str(&format!("\n{indent}{element}")),
            None => {
                if !text.is_empty() {
                    text.push('\n');
                    text.push_str(&indent);
                }
                text.push_str(&element);
            }
        }
    }
    let group_span = if text.is_empty() { element_span(&ix, &t, g) } else { ix.nodes[g].start..ix.nodes[g].end };
    let mut edits = vec![(group_span, text)];
    edits.extend(landed.into_iter().map(|(at, el)| (at..at, el)));
    w.splice(&part, edits)?;
    if !remembered.is_empty() {
        // a member put back on a layer with nothing else of the group's neighbours: keep layers in order
        layers::regroup_spreads(w)?;
    }
    Ok(())
}

/// Where a member of an ungrouped group goes back on `layer`: after the item that was below
/// it (`below`), else before the layer's first item on the spread; none if neither is there.
fn landing(ix: &Index, t: &str, g: usize, layer: &str, below: &str) -> Option<usize> {
    let parent = ix.nodes[g].parent;
    let siblings = || ix.nodes.iter().enumerate().filter(|(i, n)| n.parent == parent && *i != g && is_item(ix, t, *i)).map(|(i, _)| i);
    if !below.is_empty()
        && let Some(b) = siblings().find(|&i| ix.attr(t, i, "Self") == Some(below) && ix.attr(t, i, "ItemLayer") == Some(layer)) {
        return Some(element_span(ix, t, b).end);
    }
    let first = siblings().find(|&i| ix.attr(t, i, "ItemLayer") == Some(layer))?;
    Some(element_span(ix, t, first).start)
}

/// The layers the members of group `g` had when grouped (Ungroup Remembers Layers), one per
/// member, when the label is there, the group is still on the layer it was made on, the count
/// fits and every layer still exists; else none (all on the group's layer).
fn remembered_layers(w: &Work<'_>, ix: &Index, t: &str, g: usize, layer: Option<&str>) -> Result<Vec<(String, String)>, EditError> {
    let Some(value) = label_value(ix, t, g, MEMBER_LAYERS) else { return Ok(Vec::new()) };
    let Some((on, list)) = value.split_once('|') else { return Ok(Vec::new()) };
    let list: Vec<(String, String)> = list.split(' ').filter(|l| !l.is_empty()).filter_map(|l| l.split_once(':')).map(|(l, b)| (l.to_owned(), b.to_owned())).collect();
    let members = ix.children(g).iter().filter(|&&c| is_item(ix, t, c)).count();
    if Some(on) != layer || list.len() != members {
        return Ok(Vec::new());
    }
    let dm = w.text("designmap.xml")?;
    if list.iter().any(|(l, _)| !dm.contains(&format!("<Layer Self=\"{}\"", escape(l)))) {
        return Ok(Vec::new());
    }
    Ok(list)
}

/// A copy right above the original (the next element), every object in it with a new id
/// and references between them following, moved by `by` (oracle: duplicate-*).
pub(super) fn duplicate(w: &mut Work<'_>, src: &clipboard::Source, id: &str, by: (f64, f64)) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    let n = &ix.nodes[i];
    let c = clipboard::copy_element(w, src, id)?;
    let mut copy = c.xml;
    // the copy moves by `by` in its parent's space
    let v = c.parent.unapply_vector(by).ok_or_else(|| EditError::new("unsupported", format!("{id}: its group cannot be inverted")))?;
    if v != (0.0, 0.0) {
        let mut m = c.transform;
        m.0[4] += v.0;
        m.0[5] += v.1;
        let cx = Index::scan(&copy).map_err(|e| EditError::new("xml", e))?;
        let a = cx.attrs(&copy, 0).into_iter().find(|a| &copy[a.name.0..a.name.1] == "ItemTransform");
        match a {
            Some(a) => copy.replace_range(a.value.0..a.value.1, &m.text()),
            None => return Err(EditError::new("unsupported", format!("{id} has no transform"))),
        }
    }
    let indent = indent_before(&t, n.start).to_owned();
    w.splice(&part, vec![(n.end..n.end, format!("\n{indent}{copy}"))])?;
    w.created.push(c.id);
    Ok(())
}
