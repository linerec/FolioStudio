//! Edit ▸ Copy, Cut, Paste, Paste in Place, Paste Into (oracle basic-clipboard, measured with
//! the document in a window). Copying keeps the document as it was then, so a cut object can
//! still be pasted. Every paste is a copy as Edit ▸ Duplicate makes one (new ids, stories copied,
//! an image's "None" profile becoming the document default), placed as InDesign places it:
//! - Paste in Place: at the same spread coordinates, on the target spread (clip-copy-in-place,
//!   clip-other-spread);
//! - Paste: moved so the objects' middle is a given point, the middle of the view
//!   (clip-paste-fit-page, clip-paste-actual);
//! - both on the active layer, above everything on it (clip-active-layer: not the layer
//!   they came from), several keeping their order (clip-two-in-place);
//! - Paste Into: the frame's content, centred in it (clip-paste-into); a frame that already
//!   has content gets the new one in its place (clip-paste-into-full). Several objects cannot
//!   be pasted into a frame: InDesign 2025 refuses ("Unable to paste current scrap item");
//! - objects copied in another document bring the swatches and styles they use (`xdoc`,
//!   oracle basic-xdoc); the engine's server keeps the clipboard across documents and brings
//!   the graphics' files into the project (server `clip.rs`).
use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;

/// Where copied objects are read from: the document's parts and frame starts at a moment.
#[derive(Clone, Debug)]
pub(crate) struct Source {
    parts: Arc<BTreeMap<String, String>>,
    frame_starts: Arc<BTreeMap<String, Option<usize>>>,
    /// The editor copied from (`Editor::doc`).
    doc: u64,
}

impl Source {
    /// The editor this was copied from (`Editor::doc`).
    pub(crate) fn doc(&self) -> u64 {
        self.doc
    }

    pub(crate) fn parts(&self) -> &BTreeMap<String, String> {
        &self.parts
    }

    pub(crate) fn of(ed: &crate::Editor) -> Source {
        Source { parts: Arc::new(ed.parts.clone()), frame_starts: Arc::new(ed.frame_starts.clone()), doc: ed.doc }
    }

    /// Only the parts `keep` names (a copy within the document needs the part of the copied object and its stories, not the whole
    /// package: Override All on many pages made one whole-document copy each).
    pub(crate) fn of_parts(ed: &crate::Editor, keep: &std::collections::BTreeSet<String>) -> Source {
        let parts = ed.parts.iter().filter(|(n, _)| keep.contains(*n)).map(|(n, t)| (n.clone(), t.clone())).collect();
        Source { parts: Arc::new(parts), frame_starts: Arc::new(ed.frame_starts.clone()), doc: ed.doc }
    }

    pub(super) fn text(&self, part: &str) -> Result<&str, EditError> {
        self.parts.get(part).map(String::as_str).ok_or_else(|| EditError::new("notFound", format!("part {part}")))
    }

    /// The part, index and element of `Self` `id`.
    pub(super) fn find(&self, id: &str) -> Result<(String, Index, usize), EditError> {
        let pat = format!(" Self=\"{id}\"");
        for (name, t) in self.parts.iter().filter(|(n, t)| n.ends_with(".xml") && t.contains(&pat)) {
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", format!("{name}: {e}")))?;
            if let Some(i) = ix.by_self(t, id) {
                return Ok((name.clone(), ix, i));
            }
        }
        Err(EditError::new("notFound", format!("no object {id}")))
    }

    /// The first character a frame showed then (None: none, unknown: absent).
    pub(super) fn frame_start(&self, frame: &str) -> Option<Option<usize>> {
        self.frame_starts.get(frame).copied()
    }
}

/// The copied objects' graphic links: (part, value range of `LinkResourceURI`, the URI).
pub(crate) fn links(clip: &Clip) -> Vec<(String, std::ops::Range<usize>, String)> {
    let mut out = Vec::new();
    for id in &clip.ids {
        let Ok((part, ix, i)) = clip.source.find(id) else { continue };
        let Ok(t) = clip.source.text(&part) else { continue };
        let end = ix.nodes[i].end;
        for k in (i..ix.nodes.len()).take_while(|&k| ix.nodes[k].start < end) {
            if ix.local(t, k) != "Link" {
                continue;
            }
            for a in ix.attrs(t, k) {
                if &t[a.name.0..a.name.1] == "LinkResourceURI" {
                    out.push((part.clone(), a.value.0..a.value.1, crate::xml::unescape(&t[a.value.0..a.value.1])));
                }
            }
        }
    }
    out
}

/// The clipboard with its copied graphics' links replaced as `map` says.
pub(crate) fn with_links(clip: &Clip, map: &BTreeMap<String, String>) -> Clip {
    let mut parts = (*clip.source.parts).clone();
    let mut edits: BTreeMap<String, Vec<(std::ops::Range<usize>, String)>> = BTreeMap::new();
    for (part, range, uri) in links(clip) {
        if let Some(new) = map.get(&uri) {
            edits.entry(part).or_default().push((range, escape(new)));
        }
    }
    for (part, mut list) in edits {
        list.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        if let Some(t) = parts.get_mut(&part) {
            for (r, s) in list {
                t.replace_range(r, &s);
            }
        }
    }
    Clip { source: Source { parts: Arc::new(parts), ..clip.source.clone() }, ids: clip.ids.clone(), keep_profile: clip.keep_profile }
}

/// What Edit ▸ Copy took: the document then and the objects, bottom to top.
#[derive(Clone, Debug)]
pub(crate) struct Clip {
    pub(crate) source: Source,
    pub(crate) ids: Vec<String>,
    /// Pasting leaves an image's "None" profile as it is (a library item does: probe bookx-c4), where Paste makes it the document default.
    pub(crate) keep_profile: bool,
}

/// Edit ▸ Copy: the objects (page items, not inside each other) in their stacking order.
pub(crate) fn copy(ed: &mut crate::Editor, ids: &[String]) -> Result<(), EditError> {
    ed.clipboard = Some(take(ed, ids)?);
    Ok(())
}

/// What the Content Collector tool takes: the same copy as Edit ▸ Copy, handed back instead of kept as the clipboard
/// (the conveyor holds it; the clipboard stays as it was).
pub(crate) fn take(ed: &crate::Editor, ids: &[String]) -> Result<Clip, EditError> {
    if ids.is_empty() {
        return Err(EditError::new("invalid", "nothing to copy"));
    }
    let source = Source::of(ed);
    let mut placed: Vec<(String, usize, String)> = Vec::new();
    for id in ids {
        let (part, ix, i) = source.find(id)?;
        let t = source.text(&part)?;
        check_item(&ix, t, i, id)?;
        if ix.ancestors(i).any(|a| ix.attr(t, a, "Self").is_some_and(|s| ids.iter().any(|x| x == s))) {
            continue; // inside another copied object: it comes with it
        }
        placed.push((part, ix.nodes[i].start, id.clone()));
    }
    placed.sort();
    Ok(Clip { source, ids: placed.into_iter().map(|p| p.2).collect(), keep_profile: false })
}

/// A copy of one object per entry of `clip`, all reading from the same document state.
pub(crate) fn split(clip: &Clip) -> Vec<Clip> {
    clip.ids.iter().map(|id| Clip { source: clip.source.clone(), ids: vec![id.clone()], keep_profile: clip.keep_profile }).collect()
}

/// One copied object as the conveyor lists it: its `Self` in the document it came from, its IDML element
/// (`Rectangle`, `TextFrame`, `Oval`, `Polygon`, `GraphicLine`, `Group`, …) and its name (empty when it has none).
pub(crate) fn summary(clip: &Clip) -> Vec<(String, String, String)> {
    clip.ids.iter().filter_map(|id| {
        let (part, ix, i) = clip.source.find(id).ok()?;
        let t = clip.source.text(&part).ok()?;
        // `$ID/` is InDesign's marker of a name the object does not have (the string table's prefix, nothing after it)
        let name = ix.attr(t, i, "Name").map(crate::xml::unescape).unwrap_or_default();
        Some((id.clone(), ix.local(t, i).to_owned(), name.strip_prefix("$ID/").map(str::to_owned).unwrap_or(name)))
    }).collect()
}

/// A copied object: its XML with new ids (stories copied into the document) and where the
/// original was (its transform and its parent's, the page item bounds through them).
pub(super) struct Copied {
    pub xml: String,
    pub id: String,
    /// The original's own transform and its parents' (spread ← parent).
    pub transform: Mat,
    pub parent: Mat,
    /// The original's path bounds in spread coordinates.
    pub bounds: Option<[f64; 4]>,
}

/// A copy of `id` as `src` has it: every object in it with a new id, references between them
/// following, stories copied (oracle duplicate-*).
pub(super) fn copy_element(w: &mut Work<'_>, src: &Source, id: &str) -> Result<Copied, EditError> {
    copy_element_with(w, src, id, false)
}

/// `copy_element`; `keep_profile` leaves an image's "None" profile as it is (a library item placed in text keeps it, probe bookx-c4).
pub(super) fn copy_element_with(w: &mut Work<'_>, src: &Source, id: &str, keep_profile: bool) -> Result<Copied, EditError> {
    let (part, ix, i) = src.find(id)?;
    let t = src.text(&part)?;
    check_item(&ix, t, i, id)?;
    let n = &ix.nodes[i];
    let s = &t[n.start..n.end];
    let cx = Index::scan(s).map_err(|e| EditError::new("xml", e))?;
    let mut map: Vec<(String, String)> = Vec::new();
    for k in 0..cx.nodes.len() {
        if let Some(old) = cx.attr(s, k, "Self") {
            let new = w.new_id();
            map.push((old.to_owned(), new));
        }
    }
    let top_new = map.first().map(|m| m.1.clone()).ok_or_else(|| EditError::new("xml", "copy without id"))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    story_copy::copy_stories(w, src, &cx, s, &mut map, &mut edits)?;
    for k in 0..cx.nodes.len() {
        for a in cx.attrs(s, k) {
            let v = &s[a.value.0..a.value.1];
            if let Some((_, new)) = map.iter().find(|(old, _)| old == v) {
                edits.push((a.value.0..a.value.1, new.clone()));
            }
        }
        if let Some(edit) = copied_profile(&cx, s, k).filter(|_| !keep_profile) { edits.push(edit); }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut xml = s.to_owned();
    for (r, s2) in edits {
        xml.replace_range(r, &s2);
    }
    Ok(Copied { xml, id: top_new, transform: item_transform(&ix, t, i), parent: parent_transform(&ix, t, i), bounds: spread_bounds(&ix, t, i) })
}

pub(super) fn copied_profile(ix: &Index, text: &str, k: usize) -> Option<(std::ops::Range<usize>, String)> {
    let n = &ix.nodes[k];
    (ix.local(text, k) == "Profile"
        && n.parent.and_then(|p| ix.nodes[p].parent).is_some_and(|g| GRAPHICS.contains(&ix.local(text, g)))
        && text[n.open_end..n.close_start].trim() == "$ID/None")
        .then(|| (n.open_end..n.close_start, "$ID/Use Document Default".into()))
}

/// A copy's top element with one attribute set (added when absent) or removed.
pub(super) fn with_attr(xml: &str, name: &str, value: Option<&str>) -> Result<String, EditError> {
    let ix = Index::scan(xml).map_err(|e| EditError::new("xml", e))?;
    let edit = match value {
        Some(v) => Some(attr_edit(&ix, xml, 0, name, v)),
        None => attr_removal(&ix, xml, 0, name),
    };
    let mut out = xml.to_owned();
    if let Some((r, s)) = edit {
        out.replace_range(r, &s);
    }
    Ok(out)
}

/// The clipboard's one object as a copy for this document: new ids, its stories copied, the swatches and styles it uses brought in from
/// another document. Library ▸ place in text anchors it inline (`inline_paste`); several objects cannot be (InDesign places nothing).
pub(super) fn single_copy(w: &mut Work<'_>) -> Result<Copied, EditError> {
    let clip = w.ed.clipboard.clone().ok_or_else(|| EditError::new("invalid", "nothing has been copied"))?;
    let [id] = clip.ids.as_slice() else {
        return Err(EditError::new("unsupported", "an item of several objects cannot be placed in text (InDesign places nothing)"));
    };
    let mut c = copy_element_with(w, &clip.source, id, true)?;
    if clip.source.doc != w.ed.doc {
        c.xml = xdoc::Importer::new(&clip.source).bring(w, &c.xml)?;
    }
    Ok(c)
}

/// Edit ▸ Paste / Paste in Place / Paste Into. `page`: a page of the spread pasted on; `at`:
/// where the objects' middle goes, in that page's coordinates (none: in place); `layer`: the
/// layer (none: the active one); `into`: the frame pasted into.
pub(crate) fn paste(w: &mut Work<'_>, page: Option<&str>, at: Option<[f64; 2]>, layer: Option<&str>, into: Option<&str>, remember_layers: bool) -> Result<(), EditError> {
    let clip = w.ed.clipboard.clone().ok_or_else(|| EditError::new("invalid", "nothing has been copied"))?;
    if let Some(frame) = into {
        return paste_into(w, &clip, frame);
    }
    let page = page.ok_or_else(|| EditError::new("invalid", "paste needs a page"))?;
    let mut import = (clip.source.doc != w.ed.doc).then(|| xdoc::Importer::new(&clip.source));
    let layer = match layer {
        Some(l) => l.to_owned(),
        None => active_layer(w)?,
    };
    let (part, ix, pi) = w.find(page)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, pi) != "Page" {
        return Err(EditError::new("invalid", format!("{page} is not a page")));
    }
    let spread = ix.ancestors(pi).find(|&a| matches!(ix.local(&t, a), "Spread" | "MasterSpread")).ok_or_else(|| EditError::new("xml", "a page outside a spread"))?;
    let indent = format!("{}\t", indent_before(&t, ix.nodes[spread].start));
    let mut xml = String::new();
    let mut ids = Vec::new();
    let mut on_layer: Vec<(String, String)> = Vec::new();
    let mut bounds: Option<[f64; 4]> = None;
    for id in &clip.ids {
        let mut c = copy_element_with(w, &clip.source, id, clip.keep_profile)?;
        if let Some(im) = import.as_mut() {
            c.xml = im.bring(w, &c.xml)?;
        }
        // Paste Remembers Layers: the layer it was copied from (here or, in another document, the
        // layer of that name) if it can take it, else the layer pasted on
        let target = if remember_layers { remembered_layer(w, &clip.source, &c.xml).unwrap_or_else(|| layer.clone()) } else { layer.clone() };
        if layer_locked(w, &target)? {
            return Err(EditError::new("invalid", "the layer is locked or hidden: nothing can be pasted on it"));
        }
        // top level on the spread: its transform through its group, on the layer pasted on
        let full = c.parent.then_after(&c.transform);
        let top = with_attr(&with_attr(&c.xml, "ItemTransform", Some(&full.text()))?, "ItemLayer", Some(&target))?;
        on_layer.push((c.id.clone(), target));
        xml.push_str(&format!("\n{indent}{top}"));
        bounds = match (bounds, c.bounds) {
            (Some(a), Some(b)) => Some([a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]),
            (a, b) => a.or(b),
        };
        ids.push(c.id);
    }
    let close = ix.nodes[spread].close_start;
    let back = indent_before(&t, close).len();
    w.splice(&part, vec![(close - back..close - back, xml)])?;
    // above everything on the layer, in their order (as Object ▸ to a layer puts them)
    let mut layers_in_order: Vec<&String> = Vec::new();
    for (_, l) in &on_layer {
        if !layers_in_order.contains(&l) {
            layers_in_order.push(l);
        }
    }
    for l in layers_in_order {
        let group: Vec<String> = on_layer.iter().filter(|(_, x)| x == l).map(|(i, _)| i.clone()).collect();
        layers::to_layer(w, &group, l)?;
    }
    if let (Some(at), Some(b)) = (at, bounds) {
        let p = page_to_spread(&ix, &t, pi, at)?;
        let by = (p.0 - (b[0] + b[2]) / 2.0, p.1 - (b[1] + b[3]) / 2.0);
        if by != (0.0, 0.0) {
            move_items(w, &ids, by)?;
        }
    }
    w.created.extend(ids);
    Ok(())
}

/// A point on a page in spread coordinates (the page's top left corner is 0, 0).
pub(super) fn page_to_spread(ix: &Index, t: &str, page: usize, at: [f64; 2]) -> Result<(f64, f64), EditError> {
    let b: Vec<f64> = ix.attr(t, page, "GeometricBounds").unwrap_or("0 0 0 0").split_whitespace().filter_map(|v| v.parse().ok()).collect();
    let (top, left) = (b.first().copied().unwrap_or(0.0), b.get(1).copied().unwrap_or(0.0));
    Ok(item_transform(ix, t, page).apply((left + at[0], top + at[1])))
}

/// Paste Into: one object becomes the frame's content, centred in it, in place of what the
/// frame held.
fn paste_into(w: &mut Work<'_>, clip: &Clip, frame: &str) -> Result<(), EditError> {
    let [id] = clip.ids.as_slice() else {
        return Err(EditError::new("invalid", "several objects cannot be pasted into a frame (InDesign 2025 refuses too)"));
    };
    let (part, ix, fi) = w.find(frame)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, fi, frame)?;
    if !matches!(ix.local(&t, fi), "Rectangle" | "Oval" | "Polygon") {
        return Err(EditError::new("invalid", "paste into a graphic frame"));
    }
    replace_content(w, &part, &ix, &t, fi)?;
    let (part, ix, fi) = w.find(frame)?;
    let t = w.text(&part)?.to_owned();
    let frame_full = parent_transform(&ix, &t, fi).then_after(&item_transform(&ix, &t, fi));
    let fb = spread_bounds(&ix, &t, fi).ok_or_else(|| EditError::new("unsupported", "a frame without bounds"))?;
    let mut c = copy_element(w, &clip.source, id)?;
    if clip.source.doc != w.ed.doc {
        c.xml = xdoc::Importer::new(&clip.source).bring(w, &c.xml)?;
    }
    let cb = c.bounds.ok_or_else(|| EditError::new("unsupported", "an object without bounds"))?;
    // centred: the content's middle on the frame's, in spread coordinates
    let by = ((fb[0] + fb[2]) / 2.0 - (cb[0] + cb[2]) / 2.0, (fb[1] + fb[3]) / 2.0 - (cb[1] + cb[3]) / 2.0);
    let full = Mat([1.0, 0.0, 0.0, 1.0, by.0, by.1]).then_after(&c.parent.then_after(&c.transform));
    let local = frame_full.invert().ok_or_else(|| EditError::new("unsupported", "the frame cannot be inverted"))?.then_after(&full);
    // content carries neither a layer nor a lock of its own
    let xml = with_attr(&with_attr(&with_attr(&c.xml, "ItemTransform", Some(&local.text()))?, "ItemLayer", None)?, "Locked", None)?;
    let close = ix.nodes[fi].close_start;
    let indent = format!("{}\t", indent_before(&t, ix.nodes[fi].start));
    let back = indent_before(&t, close).len();
    let mut edits = vec![(close - back..close - back, format!("\n{indent}{xml}"))];
    // an unassigned frame becomes a graphic frame
    if ix.attr(&t, fi, "ContentType") == Some("Unassigned") {
        edits.push(attr_edit(&ix, &t, fi, "ContentType", "GraphicType"));
    }
    w.splice(&part, edits)?;
    // the frame's fitting crops follow its new content
    let (part, ix, fi) = w.find(frame)?;
    let t = w.text(&part)?.to_owned();
    if let Some(b) = own_bounds(&ix, &t, fi, &Mat::ID) {
        let crops = fitting_crops(&ix, &t, fi, b);
        w.splice(&part, crops)?;
    }
    w.created.push(c.id);
    Ok(())
}

/// What a frame holds goes before Paste Into puts something else there (clip-paste-into-full):
/// a placed graphic with its link, and the frame's wrap contour options that went with it
/// (InDesign writes `ContourOption` only for a frame holding a graphic); a page item as
/// Delete removes it (its text threads and stories too).
fn replace_content(w: &mut Work<'_>, part: &str, ix: &Index, t: &str, fi: usize) -> Result<(), EditError> {
    let content: Vec<usize> = ix.children(fi).iter().copied().filter(|&c| is_item(ix, t, c) || GRAPHICS.contains(&ix.local(t, c))).collect();
    if content.is_empty() {
        return Ok(());
    }
    let mut edits = Vec::new();
    let mut items = Vec::new();
    for &c in &content {
        if GRAPHICS.contains(&ix.local(t, c)) {
            edits.push((element_span(ix, t, c), String::new()));
        } else if let Some(id) = ix.attr(t, c, "Self") {
            items.push(id.to_owned());
        }
    }
    if !edits.is_empty() {
        for wrap in ix.named(t, fi, "TextWrapPreference") {
            for co in ix.named(t, wrap, "ContourOption") {
                edits.push((element_span(ix, t, co), String::new()));
            }
        }
        w.splice(part, edits)?;
    }
    for id in items {
        super::delete(w, &id)?;
    }
    Ok(())
}

/// Paste Remembers Layers: the layer the copied element sat on (its `ItemLayer`), in this
/// document by id, in another by the name; none when it has none, the layer is not there or
/// is locked or hidden (then the layer pasted on is used). Not measured against InDesign
/// (it was not running): the preference defaults off.
fn remembered_layer(w: &Work<'_>, source: &Source, xml: &str) -> Option<String> {
    let ix = Index::scan(xml).ok()?;
    let from = ix.attr(xml, 0, "ItemLayer")?;
    let id = if source.doc == w.ed.doc {
        from.to_owned()
    } else {
        let name = layer_name(source.parts.get("designmap.xml")?, from)?;
        layer_by_name(w.text("designmap.xml").ok()?, &name)?
    };
    (!layer_locked(w, &id).ok()?).then_some(id)
}

/// The `Name` of the layer `id` in a designmap.
fn layer_name(dm: &str, id: &str) -> Option<String> {
    let ix = Index::scan(dm).ok()?;
    (0..ix.nodes.len()).find(|&i| ix.local(dm, i) == "Layer" && ix.attr(dm, i, "Self") == Some(id)).and_then(|i| ix.value(dm, i, "Name"))
}

/// The `Self` of the layer named `name` in a designmap.
fn layer_by_name(dm: &str, name: &str) -> Option<String> {
    let ix = Index::scan(dm).ok()?;
    (0..ix.nodes.len()).find(|&i| ix.local(dm, i) == "Layer" && ix.value(dm, i, "Name").as_deref() == Some(name)).and_then(|i| ix.attr(dm, i, "Self")).map(str::to_owned)
}

fn active_layer(w: &Work<'_>) -> Result<String, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let doc = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document"))?;
    ix.attr(t, doc, "ActiveLayer").map(str::to_owned)
        .or_else(|| (0..ix.nodes.len()).rev().find(|&i| ix.local(t, i) == "Layer").and_then(|i| ix.attr(t, i, "Self")).map(str::to_owned))
        .ok_or_else(|| EditError::new("xml", "no layer"))
}

fn layer_locked(w: &Work<'_>, layer: &str) -> Result<bool, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let i = ix.by_self(t, layer).ok_or_else(|| EditError::new("notFound", format!("layer {layer}")))?;
    Ok(ix.attr(t, i, "Locked") == Some("true") || ix.attr(t, i, "Visible") == Some("false"))
}
