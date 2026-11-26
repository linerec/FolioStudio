//! Content links: what the Content Placer's Create Link makes (oracle conveyor-c2-*, measured in InDesign 2025
//! with `Page.contentPlace`, artifacts/conveyor-c2-20261023).
//!
//! InDesign's IDML for a placed object linked to its source in the same document:
//! - the source frame (and, for a text frame, its story) carries `LinkResourceId="<n>"`;
//! - the placed frame gets a `LinkedPageItemOption` (after `TextWrapPreference`) and a `Link` with
//!   `LinkResourceURI="idinternalSharedArticle:<n>"`, `LinkResourceFormat="$ID/Internal Linked Object"`, `LinkClassID="138546"`
//!   (after the frame's options, before its graphic);
//! - the placed story gets a `LinkedStoryOption` (after `StoryPreference`; `ApplyStyleMappings` is the Content Placer's Map
//!   Styles) and a `Link` with `idinternalcontent:<n>`, `Internal Linked Story`, `LinkClassID="138499"`.
//!
//! The link's status is not stored by InDesign but computed: the source changed since the link was made or updated is
//! `LinkResourceModified="true"` (the object link follows the frame, the story link the text: the text of a frame changing
//! leaves the frame's link as it was). Here the `LinkImportStamp` is a hash of the source as it was then and the status is
//! computed from it ([`Editor::content_links`]). Update re-copies the source (the placed object keeps its top left corner, a
//! text frame's story keeps its place), Unlink removes the `Link` and the option.
//!
//! Measured in conveyor-c3 (artifacts/conveyor-c3-20261023, probes b1~b3):
//! - InDesign takes the state a link compares the source with when the link is first evaluated, not when it is made: in a
//!   script the source changed before any read of the link's status is never out of date. In the application the panel reads
//!   the link at once, so Folio compares with the state at the making of the link (and the oracle runner reads the status after
//!   placing);
//! - Update Link on a link that is up to date does nothing (also when the placed object was edited); on an out-of-date one it
//!   replaces the placed object by the source's (also its script label; the corner and the ids stay) and clears `edited`;
//! - `edited` (DOM `Link.edited`, a placed object changed after the link: fill, stroke, bounds, label, a text frame's story link
//!   when its text changed; the object link of a text frame stays as it was) is not in the IDML: `LinkObjectModified` is `false`
//!   even then, and InDesign computes it again when it opens a file. Folio keeps the state of the placed object as it was when the
//!   link was made or updated in the document label `folio.contentLinkEdits` of the designmap (Link `Self` → hash) and compares;
//! - an edited copy of a link whose source changed is replaced without a question by a script (`userInteractionLevel` never
//!   interact); the option `WarnOnUpdateOfEditedPageItem` / `…Story` is the application's question, so `updateContentLink` refuses
//!   with `edited` unless it gets `overwrite`;
//! - the name of a link in the Links panel is the source's name: the object's name, or `<` + the first 32 characters of the text
//!   (`...` after them when longer; paragraph breaks left out; `<text frame>` when empty), `<` + the file of a placed graphic
//!   + `>`, or `<rectangle>`, `<oval>`, `<polygon>`, `<line>`, `<group>`; a story link has the name of its frame;
//! - a link to another document: `Link` with `LinkResourceURI="idexternalSharedArticle:<path of the document>?<n>"` (a story:
//!   `idexternalcontent:`), format `External Linked Object` / `External Linked Story`, the source document gets `LinkResourceId`
//!   (the open document is marked modified), name `<file>#<name>`; the status follows the open source document at once; updating
//!   with the source closed did nothing in a script; with the source open Update and the status work as inside a document.
//!
//! Links to an object of another document (conveyor-c3): an import keeps `idexternalSharedArticle:<document>?<n>` (a story:
//! `idexternalcontent:`) without the address: the document is a project reference (`folio:path-ref/<id>`, hint only, the document is
//! not copied) and the IDML export gives InDesign's spelling back (`project/idmlparts.rs`). They are listed (`external`, status
//! `external`: the other document is not read, so the state is not known) and can be unlinked; updating one is refused.
//!
//! Correspondence with InDesign (the interop table takes model declarations only, so it is written here): the object and story links,
//! Unlink and Map Styles are exact (oracle `basic/cc-*`); Update Link is an approximation (Preserve Size and Shape only; a copy whose
//! local fill the source does not have gets the style's fill here, `Swatch/None` in InDesign); the state of a link (up to date, out of
//! date, source gone), `edited` and the Links panel's names are Folio's own (hash comparison; InDesign computes them again when it
//! opens the file).
//!
//! A group that holds text frames is updated as a group: the frames keep their own stories (the copy's), as InDesign 2025 does (oracle
//! basic-conveyor-c6: Update Link leaves the copy's story text as it was; the stories have their own links).
//!
//! Not supported (declared 2026-11-13; refused with `unsupported` and a plain message, nothing is half done): a new link to an object of a
//! source document that is open (the link number has to be written into the source, which another window may be editing: the server asks to
//! save and close it first), objects with inner objects other than the graphic in the story's text, and the options Preserve Appearance /
//! Interactivity / Frame Content / Others of an update (Preserve Size and Shape is).
use super::*;
use crate::xml::unescape;
use serde::Serialize;
use std::collections::BTreeMap;

pub mod foreign;

const OBJECT_PREFIX: &str = "idinternalSharedArticle:";
const STORY_PREFIX: &str = "idinternalcontent:";
/// A link to an object of another document names the document: `<prefix><path of the document>?<resource number>`. In a `.folio` the
/// path is a project reference (`folio:path-ref/<id>`), never an address of this computer (principle 1).
const EXTERNAL_OBJECT_PREFIX: &str = "idexternalSharedArticle:";
const EXTERNAL_STORY_PREFIX: &str = "idexternalcontent:";
const PAGE_ITEM_OPTION: &str = r#"<LinkedPageItemOption UpdateLinkWhileSaving="false" WarnOnUpdateOfEditedPageItem="true" PreserveSizeAndShape="false" PreserveAppearance="false" PreserveInteractivity="false" PreserveFrameContent="false" PreserveOthers="false" />"#;
/// What an element of a frame holds before its graphic: a `Link` goes after these.
const STORY_HEAD: &[&str] = &["Properties", "StoryPreference", "LinkedStoryOption", "InCopyExportOption", "Link"];

fn story_option(map_styles: bool) -> String {
    format!(r#"<LinkedStoryOption UpdateWhileSaving="false" WarnOnUpdateOfEditedStory="true" RemoveForcedLineBreaks="false" ApplyStyleMappings="{map_styles}" />"#)
}

/// The `Link` to the resource `resource` of this document, or (`document`) of another one: `idexternal…:<document>?<n>`.
fn link_xml_to(id: &str, story: bool, document: Option<&str>, resource: u64, stamp: u32, date: &str) -> String {
    let (uri, format, class) = match (story, document) {
        (true, None) => (format!("{STORY_PREFIX}{resource}"), "Internal Linked Story", "138499"),
        (false, None) => (format!("{OBJECT_PREFIX}{resource}"), "Internal Linked Object", "138546"),
        (true, Some(d)) => (format!("{EXTERNAL_STORY_PREFIX}{d}?{resource}"), "External Linked Story", "138499"),
        (false, Some(d)) => (format!("{EXTERNAL_OBJECT_PREFIX}{d}?{resource}"), "External Linked Object", "138546"),
    };
    format!(
        r#"<Link Self="{id}" AssetURL="$ID/" AssetID="$ID/" RenditionData="Actual" LinkResourceURI="{uri}" LinkResourceFormat="$ID/{format}" StoredState="Contained" LinkClassID="{class}" LinkClientID="257" LinkResourceModified="false" LinkObjectModified="false" ShowInUI="true" CanEmbed="true" CanUnembed="true" CanPackage="true" ImportPolicy="NoAutoImport" ExportPolicy="NoAutoExport" LinkImportStamp="{stamp}" LinkImportModificationTime="{date}" LinkImportTime="{date}" LinkResourceSize="0~0" />"#
    )
}

fn hash32(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

/// The resource a content `Link` element names: (is a story link, the number, the link is to another document).
fn content_uri(uri: &str) -> Option<(bool, u64, bool)> {
    if let Some(n) = uri.strip_prefix(OBJECT_PREFIX) {
        return Some((false, n.parse().ok()?, false));
    }
    if let Some(n) = uri.strip_prefix(STORY_PREFIX) {
        return Some((true, n.parse().ok()?, false));
    }
    for (prefix, story) in [(EXTERNAL_OBJECT_PREFIX, false), (EXTERNAL_STORY_PREFIX, true)] {
        if let Some(rest) = uri.strip_prefix(prefix) {
            let n = rest.rsplit_once('?').and_then(|(_, n)| n.parse().ok())?;
            return Some((story, n, true));
        }
    }
    None
}

fn is_content_link(ix: &Index, t: &str, k: usize) -> bool {
    ix.local(t, k) == "Link" && ix.attr(t, k, "LinkResourceURI").is_some_and(|u| content_uri(&unescape(u)).is_some())
}

fn is_option(ix: &Index, t: &str, k: usize) -> bool {
    matches!(ix.local(t, k), "LinkedPageItemOption" | "LinkedStoryOption")
}

/// `range` with the whitespace before it (so a removed element leaves no blank line).
fn with_leading_space(t: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let mut s = range.start;
    while s > 0 && t.as_bytes()[s - 1].is_ascii_whitespace() {
        s -= 1;
    }
    s..range.end
}

/// An element's text without what a link adds to it (its resource id, content `Link` and options): the state a link compares.
fn state(xml: &str) -> String {
    state_cut(xml, true)
}

/// [`state`]; `ignore_move`: moving does not change it (a source moved is not a source changed, but a placed object moved is edited).
fn state_cut(xml: &str, ignore_move: bool) -> String {
    let Ok(ix) = Index::scan(xml) else { return xml.to_owned() };
    let mut cuts: Vec<std::ops::Range<usize>> = Vec::new();
    // the resource id of the element and, in a group, of its members
    for k in 0..ix.nodes.len() {
        for a in ix.attrs(xml, k) {
            if &xml[a.name.0..a.name.1] == "LinkResourceId" {
                cuts.push(with_leading_space(xml, a.name.0..a.value.1 + 1));
            }
        }
    }
    for k in 1..ix.nodes.len() {
        if is_option(&ix, xml, k) || is_content_link(&ix, xml, k) {
            cuts.push(with_leading_space(xml, ix.nodes[k].start..ix.nodes[k].end));
        }
    }
    let mut edits: Vec<(std::ops::Range<usize>, String)> = cuts.into_iter().map(|c| (c, String::new())).collect();
    // moving the source does not make a link out of date (measured: Move); turning, scaling or shearing it does
    if ignore_move && let Some(a) = ix.attrs(xml, 0).into_iter().find(|a| &xml[a.name.0..a.name.1] == "ItemTransform") {
        let first: Vec<&str> = xml[a.value.0..a.value.1].split_whitespace().take(4).collect();
        edits.push((a.value.0..a.value.1, first.join(" ")));
    }
    edits.sort_by_key(|c| std::cmp::Reverse(c.0.start));
    let mut out = xml.to_owned();
    for (r, v) in edits {
        out.replace_range(r, &v);
    }
    out
}

// ---------------------------------------------------------------------------------------
// the state of a placed object as it was when its link was made or updated (what `edited` compares with)

const DM: &str = "designmap.xml";
const EDITS_KEY: &str = "folio.contentLinkEdits";

fn read_stamps(parts: &BTreeMap<String, String>) -> BTreeMap<String, u32> {
    let Some(t) = parts.get(DM).filter(|t| t.contains(EDITS_KEY)) else { return BTreeMap::new() };
    let Ok(ix) = Index::scan(t) else { return BTreeMap::new() };
    (0..ix.nodes.len())
        .find(|&i| ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some(EDITS_KEY))
        .and_then(|i| ix.value(t, i, "Value"))
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn write_stamps(w: &mut Work<'_>, stamps: &BTreeMap<String, u32>) -> Result<(), EditError> {
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let json = serde_json::to_string(stamps).map_err(|e| EditError::new("invalid", e.to_string()))?;
    let pair = format!("<KeyValuePair Key=\"{EDITS_KEY}\" Value=\"{}\" />", escape(&json));
    if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some(EDITS_KEY)) {
        return w.splice(DM, vec![(ix.nodes[i].start..ix.nodes[i].end, pair)]);
    }
    if stamps.is_empty() {
        return Ok(());
    }
    let doc = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document element"))?;
    let label = ix.named(&t, doc, "Properties").first().and_then(|&p| ix.named(&t, p, "Label").first().copied());
    if let Some(label) = label {
        // after the last pair, with its indentation (so taking the pair away gives the text back)
        if let Some(&last) = ix.children(label).last() {
            let at = ix.nodes[last].end;
            return w.splice(DM, vec![(at..at, format!("
{}{pair}", indent_before(&t, ix.nodes[last].start)))]);
        }
        let close = t[..ix.nodes[label].end].rfind("</").ok_or_else(|| EditError::new("xml", "label without end"))?;
        return w.splice(DM, vec![(close..close, format!("	{pair}
"))]);
    }
    let open_end = t[ix.nodes[doc].start..].find('>').map(|k| ix.nodes[doc].start + k + 1).ok_or_else(|| EditError::new("xml", "Document start tag"))?;
    w.splice(DM, vec![(open_end..open_end, format!("
	<Properties><Label>{pair}</Label></Properties>"))])
}

/// The hash a placed object (a frame, a story) is compared by: what it is, not the link it carries.
fn target_stamp(xml: &str) -> u32 {
    hash32(&state_cut(xml, false))
}

/// Remembers the state of the elements the links `(link, target)` made now have.
fn record_stamps(w: &mut Work<'_>, pairs: &[(String, String)]) -> Result<(), EditError> {
    let mut stamps = read_stamps(&w.ed.parts);
    for (link, target) in pairs {
        let (part, ix, i) = w.find(target)?;
        let t = w.text(&part)?;
        stamps.insert(link.clone(), target_stamp(&t[ix.nodes[i].start..ix.nodes[i].end]));
    }
    write_stamps(w, &stamps)
}

/// Where the option and the `Link` go in a frame or a story: byte offsets with the text to insert there.
fn with_link(xml: &str, option: &str, link: &str, story: bool) -> Result<String, EditError> {
    let ix = Index::scan(xml).map_err(|e| EditError::new("xml", e))?;
    let kids = ix.children(0).to_vec();
    let name = |k: usize| ix.local(xml, k);
    let top = &ix.nodes[0];
    let tag = xml[top.name.clone()].to_owned();
    let ind = indent_before(xml, 0).to_owned();
    if top.empty {
        let head = &xml[..top.open_end - 2];
        return Ok(format!("{}>\n{ind}\t{option}\n{ind}\t{link}\n{ind}</{tag}>", head.trim_end()));
    }
    enum At { After(usize), Before(usize), End }
    let option_at = if story {
        kids.iter().find(|&&k| name(k) == "StoryPreference").map(|&k| At::After(k))
            .or_else(|| kids.first().map(|&k| At::Before(k))).unwrap_or(At::End)
    } else {
        let later = ["InCopyExportOption", "FrameFittingOption", "ObjectExportOption"];
        kids.iter().rev().find(|&&k| name(k) == "TextWrapPreference").map(|&k| At::After(k))
            .or_else(|| kids.iter().find(|&&k| later.contains(&name(k)) || GRAPHICS.contains(&name(k)) || ITEMS.contains(&name(k))).map(|&k| At::Before(k)))
            .or_else(|| kids.iter().rev().find(|&&k| name(k) == "Properties").map(|&k| At::After(k)))
            .unwrap_or(At::End)
    };
    let link_at = if story {
        kids.iter().find(|&&k| !STORY_HEAD.contains(&name(k))).map(|&k| At::Before(k)).unwrap_or(At::End)
    } else {
        kids.iter().find(|&&k| GRAPHICS.contains(&name(k)) || ITEMS.contains(&name(k))).map(|&k| At::Before(k)).unwrap_or(At::End)
    };
    let mut edits: Vec<(usize, String)> = [(option_at, option), (link_at, link)].into_iter().map(|(at, text)| match at {
        At::After(k) => (ix.nodes[k].end, format!("\n{}{text}", indent_before(xml, ix.nodes[k].start))),
        At::Before(k) => (ix.nodes[k].start, format!("{text}\n{}", indent_before(xml, ix.nodes[k].start))),
        At::End => {
            let back = indent_before(xml, top.close_start).len();
            (top.close_start - back, format!("{}\t{text}\n", indent_before(xml, top.close_start)))
        }
    }).collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = xml.to_owned();
    for (at, text) in edits {
        out.insert_str(at, &text);
    }
    Ok(out)
}

/// The next unused resource number (`LinkResourceId` and the numbers of content `Link`s).
fn next_resource(w: &Work<'_>) -> u64 {
    let mut max = 0u64;
    for t in w.ed.parts.values() {
        for key in ["LinkResourceId=\"", OBJECT_PREFIX, STORY_PREFIX] {
            let mut rest = t.as_str();
            while let Some(p) = rest.find(key) {
                rest = &rest[p + key.len()..];
                let n: String = rest.chars().take_while(char::is_ascii_digit).collect();
                max = max.max(n.parse().unwrap_or(0));
            }
        }
    }
    max + 1
}

/// The element of `kind` (a story or a page item) whose `LinkResourceId` is `n`, in the parts of any document.
fn find_source_in(parts: &BTreeMap<String, String>, story: bool, n: u64) -> Option<(String, Index, usize)> {
    let pat = format!(" LinkResourceId=\"{n}\"");
    for (name, t) in parts.iter().filter(|(p, t)| p.ends_with(".xml") && t.contains(&pat)) {
        let Ok(ix) = Index::scan(t) else { continue };
        let found = (0..ix.nodes.len()).find(|&k| (ix.local(t, k) == "Story") == story && ix.attr(t, k, "LinkResourceId").is_some_and(|v| v == n.to_string()));
        if let Some(k) = found {
            return Some((name.clone(), ix, k));
        }
    }
    None
}

/// Create Link for what the last paste placed: each placed object links to the object it was copied from, a text frame's
/// story to the story. `map_styles`: the Content Placer's Map Styles (`ApplyStyleMappings` of the story link); `date`: the
/// time written into the links.
pub(crate) fn link_placed(w: &mut Work<'_>, map_styles: &bool, date: &str) -> Result<(), EditError> {
    let clip = w.ed.clipboard.clone().ok_or_else(|| EditError::new("invalid", "nothing was placed"))?;
    let n = clip.ids.len();
    if n == 0 || w.created.len() < n {
        return Err(EditError::new("invalid", "linkPlaced follows the paste of the objects it links"));
    }
    let placed: Vec<String> = w.created[w.created.len() - n..].to_vec();
    if clip.source.doc() != w.ed.doc {
        // an object of another document of the project: its link numbers come from that document (foreign.rs)
        return foreign::link_foreign(w, &clip.ids, &placed, *map_styles, date);
    }
    // check everything before changing anything
    for (src, dst) in clip.ids.iter().zip(&placed) {
        let (part, ix, i) = w.find(src)?;
        let _ = (part, ix, i);
        w.find(dst)?;
    }
    for (src, dst) in clip.ids.iter().zip(&placed) {
        link_object(w, src, dst, *map_styles, date)?;
    }
    Ok(())
}

/// Edit ▸ Place and Link (oracle `conveyor-c4-*`; the menu action 139008 and a real click on the page, measured in InDesign 2025
/// with the input harness, artifacts/conveyor-c4-20261023/out/s1_place*): the objects the engine holds (the loaded ones, see the
/// server's conveyor) are placed on `page` with the top left corner of their visible bounds (stroke included) at `at`, the
/// relative places kept, and linked as Create Link does, Map Styles on (`ApplyStyleMappings="true"` in the real click's IDML; the
/// DOM's `contentPlace` with the same flags gives the same). One undo step; the placed objects are in `created`.
pub(crate) fn place_and_link(w: &mut Work<'_>, page: &str, at: &[f64; 2], map_styles: &Option<bool>, date: &str) -> Result<(), EditError> {
    let clip = w.ed.clipboard.clone().ok_or_else(|| EditError::new("invalid", "nothing is loaded to place"))?;
    let n = clip.ids.len();
    if n == 0 {
        return Err(EditError::new("invalid", "nothing is loaded to place"));
    }
    clipboard::paste(w, Some(page), None, None, None, false)?;
    if w.created.len() < n {
        return Err(EditError::new("invalid", "nothing was placed"));
    }
    let placed: Vec<String> = w.created[w.created.len() - n..].to_vec();
    let mut union: Option<[f64; 4]> = None;
    for id in &placed {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        if let Some(b) = outline_bounds(w, &ix, t, i, false) {
            union = Some(match union {
                None => b,
                Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
            });
        }
    }
    if let Some(u) = union {
        let (part, ix, pi) = w.find(page)?;
        let t = w.text(&part)?;
        let to = clipboard::page_to_spread(&ix, t, pi, *at)?;
        let by = (to.0 - u[0], to.1 - u[1]);
        if by != (0.0, 0.0) {
            move_items(w, &placed, by)?;
        }
    }
    link_placed(w, &map_styles.unwrap_or(true), date)
}

/// Sets `LinkResourceId` on a source element (or finds the one it has) and returns it with the state hash.
fn mark_source(w: &mut Work<'_>, id: &str) -> Result<(u64, u32), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    let xml = &t[ix.nodes[i].start..ix.nodes[i].end];
    let stamp = hash32(&state(xml));
    if let Some(n) = ix.attr(t, i, "LinkResourceId").and_then(|v| v.parse().ok()) {
        return Ok((n, stamp));
    }
    let n = next_resource(w);
    let edit = attr_edit(&ix, t, i, "LinkResourceId", &n.to_string());
    w.splice(&part, vec![edit])?;
    Ok((n, stamp))
}

/// The page items inside the element `id` (a group's members, at any depth) that have a `Self`.
fn members(w: &mut Work<'_>, id: &str) -> Result<Vec<String>, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    Ok((0..ix.nodes.len()).filter(|&k| k != i && ITEMS.contains(&ix.local(t, k)) && ix.ancestors(k).any(|a| a == i))
        .filter_map(|k| ix.attr(t, k, "Self").map(str::to_owned)).collect())
}

/// InDesign gives the members of a group `LinkResourceId` too when it links the group (measured conveyor-c4, a real Place and Link
/// of a group): a source group is marked with everything in it.
fn mark_members(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    for m in members(w, id)? {
        mark_source(w, &m)?;
    }
    Ok(())
}

/// The `LinkResourceId` of each member of a group, in document order (0: it has none).
fn member_resources(w: &mut Work<'_>, id: &str) -> Result<Vec<u64>, EditError> {
    let mut out = Vec::new();
    for m in members(w, id)? {
        let (part, ix, i) = w.find(&m)?;
        let t = w.text(&part)?;
        out.push(ix.attr(t, i, "LinkResourceId").and_then(|v| v.parse().ok()).unwrap_or(0));
    }
    Ok(out)
}

/// The members of a linked group copy name the members of the source they were copied from (`LinkedSourceTableId` = the source member's
/// `LinkResourceId`; measured conveyor-c4, oracle `pal-group`: the copy's members carry it, the copy's group has the `Link`).
fn name_sources(w: &mut Work<'_>, dst: &str, resources: &[u64]) -> Result<(), EditError> {
    let copies = members(w, dst)?;
    if copies.len() != resources.len() {
        return Ok(());
    }
    for (m, n) in copies.iter().zip(resources) {
        if *n == 0 {
            continue;
        }
        let (part, ix, i) = w.find(m)?;
        let t = w.text(&part)?;
        let edit = attr_edit(&ix, t, i, "LinkedSourceTableId", &n.to_string());
        w.splice(&part, vec![edit])?;
    }
    Ok(())
}

/// A copy of a source is not a source, nor are the members of a copied group.
fn strip_member_ids(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    for m in members(w, id)? {
        strip_resource_id(w, &m)?;
    }
    Ok(())
}

/// Puts `option` and a new `Link` into the element `id`.
fn add_link(w: &mut Work<'_>, id: &str, story: bool, resource: u64, stamp: u32, map_styles: bool, date: &str) -> Result<String, EditError> {
    add_link_to(w, id, story, None, resource, stamp, map_styles, date)
}

/// [`add_link`]; `document`: the link is to an object of another document (the reference to it, as the `Link` writes it).
// Each argument supplies one existing link field; retain the writer's call contract.
#[allow(clippy::too_many_arguments)]
fn add_link_to(w: &mut Work<'_>, id: &str, story: bool, document: Option<&str>, resource: u64, stamp: u32, map_styles: bool, date: &str) -> Result<String, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    let range = ix.nodes[i].start..ix.nodes[i].end;
    let xml = t[range.clone()].to_owned();
    let option = if story { story_option(map_styles) } else { PAGE_ITEM_OPTION.to_owned() };
    let link_id = w.new_id();
    let link = link_xml_to(&link_id, story, document, resource, stamp, date);
    let new = with_link(&xml, &option, &link, story)?;
    w.splice(&part, vec![(range, new)])?;
    Ok(link_id)
}

/// A copy of a source is not a source: the resource id the copied element had (it was linked before) does not come along.
fn strip_resource_id(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    let Some(a) = ix.attrs(t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == "LinkResourceId") else { return Ok(()) };
    let range = with_leading_space(t, a.name.0..a.value.1 + 1);
    w.splice(&part, vec![(range, String::new())])
}

fn link_object(w: &mut Work<'_>, src: &str, dst: &str, map_styles: bool, date: &str) -> Result<(), EditError> {
    strip_resource_id(w, dst)?;
    strip_member_ids(w, dst)?;
    let (sp, six, si) = w.find(src)?;
    let st = w.text(&sp)?;
    let source_story = (six.local(st, si) == "TextFrame").then(|| six.attr(st, si, "ParentStory").filter(|s| *s != "n").map(str::to_owned)).flatten();
    let (resource, stamp) = mark_source(w, src)?;
    mark_members(w, src)?;
    let member_ids = member_resources(w, src)?;
    let mut made = vec![(add_link(w, dst, false, resource, stamp, map_styles, date)?, dst.to_owned())];
    name_sources(w, dst, &member_ids)?;
    if let Some(story) = source_story {
        let (dp, dix, di) = w.find(dst)?;
        let dt = w.text(&dp)?;
        let placed_story = dix.attr(dt, di, "ParentStory").filter(|s| *s != "n").map(str::to_owned)
            .ok_or_else(|| EditError::new("xml", format!("{dst} has no story")))?;
        strip_resource_id(w, &placed_story)?;
        let (resource, stamp) = mark_source(w, &story)?;
        made.push((add_link(w, &placed_story, true, resource, stamp, map_styles, date)?, placed_story));
    }
    record_stamps(w, &made)
}

/// A content link as the Links panel lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentLink {
    /// The `Link` element's `Self`.
    pub link: String,
    /// `object` (a page item) or `story` (a text frame's text).
    pub kind: &'static str,
    /// The linked (placed) frame or story.
    pub target: String,
    /// What it links to: the frame or story with that `LinkResourceId`; none when it is gone.
    pub source: Option<String>,
    /// `normal`, `outOfDate` (the source changed since the link was made or updated), `missing` (the source is gone) or `external` (the
    /// source is in another document, which is not read: its state is not known).
    pub status: &'static str,
    /// The story link's Map Styles.
    pub map_styles: Option<bool>,
    /// The placed object (or story) was changed after the link was made or updated (DOM `Link.edited`).
    pub edited: bool,
    /// The name the Links panel shows: the source's name (an object's own, the start of its text, the file of its graphic) or
    /// `<rectangle>`, `<oval>`… for what has none; a story link has the name of its frame.
    pub name: String,
    /// When `name` is the general word of a kind (`textFrame`, `rectangle`, `oval`, `polygon`, `line`, `group`) that word, so a
    /// screen can say it in its language.
    pub generic: Option<&'static str>,
    /// The frame the link is shown at (the placed frame, or the first frame of the placed story).
    pub frame: Option<String>,
    /// The `LinkResourceURI`.
    pub uri: String,
    /// The link is to an object of another document (`status` is `external`: that document is not read yet).
    pub external: bool,
}

/// An object's name as InDesign's Layers and Links panels give it: its own, the start of the text of a text frame, the file of
/// a placed graphic, else the kind of the object (`generic`).
fn element_name(parts: &BTreeMap<String, String>, xml: &str) -> (String, Option<&'static str>) {
    let Ok(ix) = Index::scan(xml) else { return (String::new(), None) };
    if let Some(n) = ix.attr(xml, 0, "Name").map(unescape).filter(|n| !n.is_empty() && n != "$ID/") {
        return (n, None);
    }
    let general = |kind: &'static str| (format!("<{}>", if kind == "textFrame" { "text frame" } else { kind }), Some(kind));
    match ix.local(xml, 0) {
        "TextFrame" => {
            let text = ix.attr(xml, 0, "ParentStory").map(|s| story_text(parts, s)).unwrap_or_default();
            if text.is_empty() {
                return general("textFrame");
            }
            let cut: String = text.chars().take(32).collect();
            (format!("<{cut}{}>", if text.chars().count() > 32 { "..." } else { "" }), None)
        }
        tag @ ("Rectangle" | "Oval" | "Polygon") => {
            let file = (0..ix.nodes.len()).filter(|&k| GRAPHICS.contains(&ix.local(xml, k))).find_map(|g| {
                ix.children(g).iter().find(|&&c| ix.local(xml, c) == "Link").and_then(|&c| ix.attr(xml, c, "LinkResourceURI")).map(|u| super::captions::link_name(&unescape(u))).filter(|n| !n.is_empty())
            });
            match file {
                Some(f) => (format!("<{f}>"), None),
                None => general(match tag { "Rectangle" => "rectangle", "Oval" => "oval", _ => "polygon" }),
            }
        }
        "GraphicLine" => general("line"),
        "Group" => general("group"),
        _ => general("rectangle"),
    }
}

/// The text of a story (its `Content`s; paragraph breaks are not text).
fn story_text(parts: &BTreeMap<String, String>, story: &str) -> String {
    let Some(t) = parts.get(&format!("Stories/Story_{story}.xml")) else { return String::new() };
    let Ok(ix) = Index::scan(t) else { return String::new() };
    (0..ix.nodes.len()).filter(|&k| ix.local(t, k) == "Content").map(|k| unescape(&t[ix.nodes[k].open_end..ix.nodes[k].close_start])).collect()
}

/// The first text frame of a story: its `Self` and its element text.
fn frame_of_story(parts: &BTreeMap<String, String>, story: &str) -> Option<(String, String)> {
    let pat = format!("ParentStory=\"{story}\"");
    for (_, t) in parts.iter().filter(|(n, t)| n.starts_with("Spreads/") && t.contains(&pat)) {
        let Ok(ix) = Index::scan(t) else { continue };
        if let Some(k) = (0..ix.nodes.len()).find(|&k| ix.local(t, k) == "TextFrame" && ix.attr(t, k, "ParentStory") == Some(story)) {
            return Some((ix.attr(t, k, "Self")?.to_owned(), t[ix.nodes[k].start..ix.nodes[k].end].to_owned()));
        }
    }
    None
}

/// Every content link in the parts, in the order of the parts.
pub(crate) fn list(parts: &BTreeMap<String, String>) -> Vec<ContentLink> {
    list_with(parts, &|_| foreign::Foreign::Unknown)
}

/// [`list`]; `others` reads the document a link to another document names (the reference as the `Link` writes it): its state is then
/// the state of the link (`normal`, `outOfDate`, `missing`), else it stays `external`.
pub(crate) fn list_with(parts: &BTreeMap<String, String>, others: &dyn Fn(&str) -> foreign::Foreign) -> Vec<ContentLink> {
    let mut out = Vec::new();
    let stamps = read_stamps(parts);
    let sources: Vec<(&str, &String)> = parts.iter().filter(|(n, t)| n.ends_with(".xml") && t.contains(" LinkResourceId=\"")).map(|(n, t)| (n.as_str(), t)).collect();
    for (_, t) in parts.iter().filter(|(n, t)| (n.starts_with("Spreads/") || n.starts_with("Stories/")) && (t.contains("idinternal") || t.contains("idexternal"))) {
        let Ok(ix) = Index::scan(t) else { continue };
        for k in (0..ix.nodes.len()).filter(|&k| is_content_link(&ix, t, k)) {
            let Some((story, n, external)) = ix.attr(t, k, "LinkResourceURI").and_then(|u| content_uri(&unescape(u))) else { continue };
            let Some(parent) = ix.nodes[k].parent else { continue };
            let target = ix.attr(t, parent, "Self").unwrap_or_default().to_owned();
            let link_id = ix.attr(t, k, "Self").unwrap_or_default().to_owned();
            let stamp = ix.attr(t, k, "LinkImportStamp").and_then(|s| s.parse::<u32>().ok());
            let map_styles = story.then(|| ix.children(parent).iter().find(|&&c| ix.local(t, c) == "LinkedStoryOption").and_then(|&c| ix.attr(t, c, "ApplyStyleMappings")).map(|v| v == "true")).flatten();
            let target_xml = &t[ix.nodes[parent].start..ix.nodes[parent].end];
            let edited = stamps.get(&link_id).is_some_and(|&s| s != target_stamp(target_xml));
            let pat = format!(" LinkResourceId=\"{n}\"");
            let mut source = None;
            let mut source_xml: Option<String> = None;
            let mut status = if external { "external" } else { "missing" };
            let mut foreign_name: Option<String> = None;
            if external && let Some(doc) = foreign::external_document(&unescape(ix.attr(t, k, "LinkResourceURI").unwrap_or_default())).map(str::to_owned) {
                match others(&doc) {
                    foreign::Foreign::Unknown => {}
                    foreign::Foreign::Missing => status = "missing",
                    foreign::Foreign::Doc { name, parts: fp } => {
                        let (st, found) = foreign::status_in(&fp, &name, story, n, stamp);
                        status = st;
                        if let Some((id, shown, _)) = found {
                            source = Some(id);
                            foreign_name = Some(shown);
                        }
                    }
                }
            }
            for (_, st) in sources.iter().filter(|(_, st)| !external && st.contains(&pat)) {
                let Ok(six) = Index::scan(st) else { continue };
                let found = (0..six.nodes.len()).find(|&c| (six.local(st, c) == "Story") == story && six.attr(st, c, "LinkResourceId").is_some_and(|v| v == n.to_string()));
                if let Some(c) = found {
                    source = six.attr(st, c, "Self").map(str::to_owned);
                    let xml = &st[six.nodes[c].start..six.nodes[c].end];
                    status = if stamp == Some(hash32(&state(xml))) { "normal" } else { "outOfDate" };
                    source_xml = Some(xml.to_owned());
                    break;
                }
            }
            // the name is the source's (the target's while the source is gone); a story has the name of its frame
            let (name, generic) = if let Some(shown) = foreign_name {
                (shown, None)
            } else if story {
                let story_id = source.as_deref().unwrap_or(&target);
                frame_of_story(parts, story_id).map_or((String::new(), None), |(_, frame)| element_name(parts, &frame))
            } else {
                element_name(parts, source_xml.as_deref().unwrap_or(target_xml))
            };
            let frame = if story { frame_of_story(parts, &target).map(|(id, _)| id) } else { Some(target.clone()) };
            let uri = unescape(ix.attr(t, k, "LinkResourceURI").unwrap_or_default());
            out.push(ContentLink { link: link_id, kind: if story { "story" } else { "object" }, target, source, status, map_styles, edited, name, generic, frame, uri, external });
        }
    }
    out
}

/// What an update or an unlink works on: the `Link` (its part, index, parent) and what it names.
struct Found {
    part: String,
    ix: Index,
    link: usize,
    parent: usize,
    story: bool,
    resource: u64,
    external: bool,
}

fn find_link(w: &mut Work<'_>, link: &str) -> Result<Found, EditError> {
    let (part, ix, k) = w.find(link)?;
    let t = w.text(&part)?;
    let (story, resource, external) = is_content_link(&ix, t, k).then(|| ix.attr(t, k, "LinkResourceURI").and_then(|u| content_uri(&unescape(u)))).flatten()
        .ok_or_else(|| EditError::new("invalid", format!("{link} is not a content link")))?;
    let parent = ix.nodes[k].parent.ok_or_else(|| EditError::new("xml", "a link outside an object"))?;
    Ok(Found { part, ix, link: k, parent, story, resource, external })
}

/// Updates the link's placed object or story from its source (Links panel ▸ Update Link). A link that is up to date is left as it
/// is (measured: also when the placed object was edited). An edited placed object is replaced only with `overwrite` when the
/// link's option asks for a warning (InDesign asks; a script's `never interact` answers yes).
pub(crate) fn update(w: &mut Work<'_>, link: &str, date: &str, overwrite: &Option<bool>) -> Result<(), EditError> {
    let f = find_link(w, link)?;
    // the source: in this document, or (a link to another document of the project) in that document's parts, which the host reads
    // (the server, `set_foreign_documents`) and whose swatches and styles come along as they do in a paste
    let other = if f.external { Some(foreign::document_of(w, &f)?) } else { None };
    let (mut source_xml, full, stamp) = {
        let parts = other.as_deref().unwrap_or(&w.ed.parts);
        let (sp, six, si) = find_source_in(parts, f.story, f.resource).ok_or_else(|| EditError::new("notFound", "the source of the link is missing"))?;
        let st = &parts[&sp];
        let x = st[six.nodes[si].start..six.nodes[si].end].to_owned();
        let full = parent_transform(&six, st, si).then_after(&item_transform(&six, st, si)).text();
        let stamp = hash32(&state(&x));
        (x, full, stamp)
    };
    let (target, up_to_date, edited, warn) = {
        let t = w.text(&f.part)?;
        let target_xml = &t[f.ix.nodes[f.parent].start..f.ix.nodes[f.parent].end];
        let made = f.ix.attr(t, f.link, "LinkImportStamp").and_then(|s| s.parse::<u32>().ok());
        let link_id = f.ix.attr(t, f.link, "Self").unwrap_or_default().to_owned();
        let edited = read_stamps(&w.ed.parts).get(&link_id).is_some_and(|&s| s != target_stamp(target_xml));
        let warn_name = if f.story { "WarnOnUpdateOfEditedStory" } else { "WarnOnUpdateOfEditedPageItem" };
        let warn = !f.ix.children(f.parent).iter().any(|&k| is_option(&f.ix, t, k) && f.ix.attr(t, k, warn_name) == Some("false"));
        (f.ix.attr(t, f.parent, "Self").unwrap_or_default().to_owned(), made == Some(stamp), edited, warn)
    };
    if up_to_date {
        return Ok(());
    }
    if edited && warn && !overwrite.unwrap_or(false) {
        return Err(EditError::new("edited", "the placed object was changed after the link was made: updating replaces its changes (overwrite)"));
    }
    if let Some(parts) = &other {
        source_xml = xdoc::bring_resources(w, parts, &source_xml)?;
    }
    if f.story {
        update_story(w, &f, &source_xml)?;
        w.recompose(&target);
    } else {
        update_object(w, &f, &target, &source_xml, &full)?;
    }
    // the link now matches the source
    let (part, ix, k) = w.find(link)?;
    let t = w.text(&part)?;
    let edits = [("LinkResourceModified", "false".to_owned()), ("LinkObjectModified", "false".to_owned()), ("LinkImportStamp", stamp.to_string()),
        ("LinkImportModificationTime", date.to_owned()), ("LinkImportTime", date.to_owned())]
        .iter().map(|(name, v)| attr_edit(&ix, t, k, name, v)).collect();
    w.splice(&part, edits)?;
    record_stamps(w, &[(link.to_owned(), target)])
}

fn flag(ix: &Index, t: &str, option: usize, name: &str) -> bool {
    ix.attr(t, option, name) == Some("true")
}

fn update_object(w: &mut Work<'_>, f: &Found, target: &str, source_xml: &str, full: &str) -> Result<(), EditError> {
    let t = w.text(&f.part)?;
    let placed_range = f.ix.nodes[f.parent].start..f.ix.nodes[f.parent].end;
    let placed_xml = t[placed_range.clone()].to_owned();
    let before = spread_bounds(&f.ix, t, f.parent);
    let pix = Index::scan(&placed_xml).map_err(|e| EditError::new("xml", e))?;
    let option = (0..pix.nodes.len()).find(|&k| pix.local(&placed_xml, k) == "LinkedPageItemOption");
    let mut keep_geometry = false;
    if let Some(o) = option {
        for name in ["PreserveAppearance", "PreserveInteractivity", "PreserveFrameContent", "PreserveOthers"] {
            if flag(&pix, &placed_xml, o, name) {
                return Err(EditError::new("unsupported", format!("updating a link with {name} is not built yet")));
            }
        }
        keep_geometry = flag(&pix, &placed_xml, o, "PreserveSizeAndShape");
    }
    let new = rebuilt_object(source_xml, &placed_xml, full, keep_geometry)?;
    w.splice(&f.part, vec![(placed_range, new)])?;
    // the placed object keeps its top left corner
    if let Some(old) = before {
        let (part, ix, i) = w.find(target)?;
        let t = w.text(&part)?;
        if let Some(now) = spread_bounds(&ix, t, i) {
            let by = (old[0] - now[0], old[1] - now[1]);
            if by != (0.0, 0.0) {
                move_items(w, &[target.to_owned()], by)?;
            }
        }
    }
    Ok(())
}

/// The ids of the elements inside `xml` that have one, but not a content `Link`.
fn own_ids(ix: &Index, xml: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&k| ix.attr(xml, k, "Self").is_some() && !is_content_link(ix, xml, k)).collect()
}

/// The source's element as the placed object: the placed object's ids, story, layer and (for a text frame) threading, its
/// link and its option kept; its geometry kept when `keep_geometry`.
fn rebuilt_object(source: &str, placed: &str, full: &str, keep_geometry: bool) -> Result<String, EditError> {
    let six = Index::scan(source).map_err(|e| EditError::new("xml", e))?;
    let pix = Index::scan(placed).map_err(|e| EditError::new("xml", e))?;
    let (sids, pids) = (own_ids(&six, source), own_ids(&pix, placed));
    if sids.len() != pids.len() {
        return Err(EditError::new("unsupported", "the object inside differs from the source's: updating it is not built yet"));
    }
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for (&s, &p) in sids.iter().zip(&pids) {
        let new = pix.attr(placed, p, "Self").unwrap_or_default();
        if let Some(a) = six.attrs(source, s).into_iter().find(|a| &source[a.name.0..a.name.1] == "Self") {
            edits.push((a.value.0..a.value.1, new.to_owned()));
        }
    }
    for name in ["ParentStory", "PreviousTextFrame", "NextTextFrame", "ItemLayer"] {
        let mine = six.attrs(source, 0).into_iter().find(|a| &source[a.name.0..a.name.1] == name);
        if let (Some(a), Some(v)) = (mine, pix.attr(placed, 0, name)) {
            edits.push((a.value.0..a.value.1, v.to_owned()));
        }
    }
    // the text frames inside a group keep their own stories and threading: InDesign 2025 updates the group, not the text of the frames in it
    // (oracle basic-conveyor-c6 cc-update-group-text: after Update Link the copy's stories read as before, each still with its own story link)
    let source_frames: Vec<usize> = (1..six.nodes.len()).filter(|&k| six.local(source, k) == "TextFrame").collect();
    let placed_frames: Vec<usize> = (1..pix.nodes.len()).filter(|&k| pix.local(placed, k) == "TextFrame").collect();
    if source_frames.len() != placed_frames.len() {
        return Err(EditError::new("unsupported", "the text frames inside differ from the source's: updating it is not built yet"));
    }
    for (&sf, &pf) in source_frames.iter().zip(&placed_frames) {
        for name in ["ParentStory", "PreviousTextFrame", "NextTextFrame"] {
            let mine = six.attrs(source, sf).into_iter().find(|a| &source[a.name.0..a.name.1] == name);
            if let (Some(a), Some(v)) = (mine, pix.attr(placed, pf, name)) {
                edits.push((a.value.0..a.value.1, v.to_owned()));
            }
        }
    }
    // the source's resource id (and its members') stays the source's; the placed object keeps the one it has (it may be a source itself)
    for k in 0..six.nodes.len() {
        if let Some(a) = six.attrs(source, k).into_iter().find(|a| &source[a.name.0..a.name.1] == "LinkResourceId") {
            edits.push((with_leading_space(source, a.name.0..a.value.1 + 1), String::new()));
        }
    }
    let transform = if keep_geometry { pix.attr(placed, 0, "ItemTransform").map(str::to_owned) } else { Some(full.to_owned()) };
    if let (Some(v), Some(a)) = (transform, six.attrs(source, 0).into_iter().find(|a| &source[a.name.0..a.name.1] == "ItemTransform")) {
        edits.push((a.value.0..a.value.1, v));
    }
    if keep_geometry {
        let old = pix.path(placed, 0, &["Properties", "PathGeometry"]).map(|k| placed[pix.nodes[k].start..pix.nodes[k].end].to_owned());
        if let (Some(old), Some(k)) = (old, six.path(source, 0, &["Properties", "PathGeometry"])) {
            edits.push((six.nodes[k].start..six.nodes[k].end, old));
        }
    }
    // the members of a group copy keep the names of their sources (`LinkedSourceTableId`)
    for (&sm, &pm) in sids.iter().zip(&pids).skip(1) {
        if let Some(v) = pix.attr(placed, pm, "LinkedSourceTableId") {
            edits.push(attr_edit(&six, source, sm, "LinkedSourceTableId", v));
        }
    }
    // the label is the source's too (measured conveyor-c3, oracle cc-update-rect: the copy's script label is replaced)
    // drop what a link added to the source (it may itself be a linked copy) and put the placed object's own back
    for k in 1..six.nodes.len() {
        if is_option(&six, source, k) || is_content_link(&six, source, k) {
            edits.push((with_leading_space(source, six.nodes[k].start..six.nodes[k].end), String::new()));
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut xml = source.to_owned();
    for (r, v) in edits {
        xml.replace_range(r, &v);
    }
    if let Some(id) = pix.attr(placed, 0, "LinkResourceId") {
        xml = clipboard::with_attr(&xml, "LinkResourceId", Some(id))?;
    }
    let option = pix.children(0).iter().find(|&&k| is_option(&pix, placed, k)).map(|&k| placed[pix.nodes[k].start..pix.nodes[k].end].to_owned());
    let link = pix.children(0).iter().find(|&&k| is_content_link(&pix, placed, k)).map(|&k| placed[pix.nodes[k].start..pix.nodes[k].end].to_owned());
    match (option, link) {
        (Some(o), Some(l)) => with_link(&xml, &o, &l, false),
        _ => Ok(xml),
    }
}

fn update_story(w: &mut Work<'_>, f: &Found, source_xml: &str) -> Result<(), EditError> {
    let t = w.text(&f.part)?;
    let placed_range = f.ix.nodes[f.parent].start..f.ix.nodes[f.parent].end;
    let placed_xml = &t[placed_range.clone()];
    let six = Index::scan(source_xml).map_err(|e| EditError::new("xml", e))?;
    let pix = Index::scan(placed_xml).map_err(|e| EditError::new("xml", e))?;
    // the text after the head (preferences, options, link): replaced as it is
    let body = |ix: &Index, x: &str| -> (usize, usize) {
        let first = ix.children(0).iter().find(|&&k| !STORY_HEAD.contains(&ix.local(x, k))).map(|&k| ix.nodes[k].start);
        let end = ix.nodes[0].close_start - indent_before(x, ix.nodes[0].close_start).len();
        (first.unwrap_or(end), end)
    };
    let (ss, se) = body(&six, source_xml);
    let (ps, pe) = body(&pix, placed_xml);
    let new_body = &source_xml[ss..se];
    if new_body.contains(" Self=\"") {
        return Err(EditError::new("unsupported", "a story with objects, tables or notes in it cannot be updated from its source (not supported)"));
    }
    let at = placed_range.start;
    w.splice(&f.part, vec![(at + ps..at + pe, new_body.to_owned())])
}

/// Links panel ▸ Unlink for a content link: the `Link` and the option go, the placed object stays as it is.
pub(crate) fn unlink(w: &mut Work<'_>, link: &str) -> Result<(), EditError> {
    let f = find_link(w, link)?;
    let t = w.text(&f.part)?;
    let mut edits = vec![with_leading_space(t, f.ix.nodes[f.link].start..f.ix.nodes[f.link].end)];
    edits.extend(f.ix.children(f.parent).iter().filter(|&&k| is_option(&f.ix, t, k)).map(|&k| with_leading_space(t, f.ix.nodes[k].start..f.ix.nodes[k].end)));
    let edits = edits.into_iter().map(|r| (r, String::new())).collect();
    w.splice(&f.part, edits)?;
    let mut stamps = read_stamps(&w.ed.parts);
    if stamps.remove(link).is_some() {
        write_stamps(w, &stamps)?;
    }
    Ok(())
}

#[cfg(test)]
mod group_tests {
    use super::rebuilt_object;

    /// Updating a group that holds text frames (InDesign 2025, oracle basic-conveyor-c6 `cc-update-group-text`): the group takes the source's shapes but each
    /// text frame of the copy keeps its own story and threading, so the text of the copy stays as it was.
    #[test]
    fn a_group_of_text_frames_keeps_the_stories_of_the_copy() {
        let source = r#"<Group Self="g1" ItemLayer="l1" ItemTransform="1 0 0 1 0 0"><TextFrame Self="a1" ParentStory="s1" PreviousTextFrame="n" NextTextFrame="n" ContentType="TextType"/><Rectangle Self="r1"/><TextFrame Self="b1" ParentStory="s2" PreviousTextFrame="n" NextTextFrame="n" ContentType="TextType"/></Group>"#;
        let placed = r#"<Group Self="g2" ItemLayer="l2" ItemTransform="1 0 0 1 30 40"><TextFrame Self="a2" ParentStory="p1" PreviousTextFrame="n" NextTextFrame="n" ContentType="TextType"/><Rectangle Self="r2"/><TextFrame Self="b2" ParentStory="p2" PreviousTextFrame="n" NextTextFrame="n" ContentType="TextType"/></Group>"#;
        let out = rebuilt_object(source, placed, "1 0 0 1 30 40", false).expect("a group with text frames is updated");
        assert!(out.contains("ParentStory=\"p1\"") && out.contains("ParentStory=\"p2\""), "{out}");
        assert!(!out.contains("ParentStory=\"s1\"") && !out.contains("ParentStory=\"s2\""), "{out}");
        assert!(out.contains("Self=\"a2\"") && out.contains("Self=\"b2\"") && out.contains("Self=\"g2\""), "{out}");
        // the text frames must match one for one
        let fewer = r#"<Group Self="g2"><TextFrame Self="a2" ParentStory="p1"/><Rectangle Self="r2"/><Rectangle Self="x2"/></Group>"#;
        assert!(rebuilt_object(source, fewer, "1 0 0 1 0 0", false).is_err());
    }
}
