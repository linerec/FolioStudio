//! Hyperlinks panel ▸ destinations (build-c, 2026-11-21): New Hyperlink Destination (a text anchor at a story position, a page, a URL),
//! Hyperlink Destination Options (name, address, page, zoom), Delete Unused Destinations, and Reset Hyperlink (the link's text source moves
//! to the selected text). The elements are InDesign's own (`HyperlinkTextDestination` in the story, `HyperlinkPageDestination` and
//! `HyperlinkURLDestination` in designmap.xml), so they go to InDesign as they are; every change is one undo step.
use super::*;

const ZOOMS: &[&str] = &["Fixed", "FitView", "FitWidth", "FitHeight", "FitVisible", "InheritZoom"];

fn unique_name(d: &folio_model::hyperlinks::Hyperlinks, name: &str, except: &str) -> Result<(), EditError> {
    if name.trim().is_empty() { return Err(invalid("a destination needs a name")); }
    if d.destinations.iter().any(|x| x.id != except && x.name == name) { return Err(invalid("destination name must be unique")); }
    Ok(())
}

fn zoom(view_setting: &str) -> Result<&str, EditError> {
    if view_setting.is_empty() { return Ok("FitView"); }
    ZOOMS.iter().copied().find(|z| *z == view_setting).ok_or_else(|| invalid("view_setting is Fixed, FitView, FitWidth, FitHeight, FitVisible or InheritZoom"))
}

/// The `Self` of the page `page` (a page id, or a 1-based page number among the document's pages in order).
fn page_id(w: &mut Work<'_>, page: &str) -> Result<String, EditError> {
    if page.is_empty() { return Err(invalid("page required")); }
    if w.find(page).is_ok() { return Ok(page.to_owned()); }
    let n: usize = page.parse().map_err(|_| invalid("page is a page id or a page number"))?;
    let dm = w.text("designmap.xml")?.to_owned();
    let ix = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    let mut pages = Vec::new();
    for &c in ix.children(root) {
        if ix.local(&dm, c) != "Spread" { continue; }
        let Some(src) = ix.attr(&dm, c, "src") else { continue };
        let t = w.text(src)?;
        let sx = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        for k in 0..sx.nodes.len() {
            if sx.local(t, k) == "Page" && let Some(id) = sx.attr(t, k, "Self") { pages.push(id.to_owned()); }
        }
    }
    pages.get(n.wrapping_sub(1)).cloned().ok_or_else(|| invalid("no such page"))
}

/// Puts a page or URL destination with the others of its kind in designmap.xml (InDesign writes page destinations, then URL
/// destinations, then the hyperlinks, after the stories).
fn append_destination(w: &mut Work<'_>, tag: &str, raw: &str) -> Result<(), EditError> {
    let ix = w.index("designmap.xml")?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    let t = w.text("designmap.xml")?;
    let kids = ix.children(root);
    let last = |name: &str| kids.iter().rev().find(|&&i| ix.local(t, i) == name).map(|&i| ix.nodes[i].end);
    let first = |name: &str| kids.iter().find(|&&i| ix.local(t, i) == name).map(|&i| ix.nodes[i].start);
    let neighbour = if tag == "HyperlinkPageDestination" { first("HyperlinkURLDestination") } else { last("HyperlinkPageDestination") };
    let at = last(tag).or(neighbour).or_else(|| first("Hyperlink")).or_else(|| first("IndexingSortOption"))
        .or_else(|| last("Story")).unwrap_or(ix.nodes[root].close_start);
    w.splice("designmap.xml", vec![(at..at, format!("\n{raw}\n"))])
}

/// New Hyperlink Destination: `kind` text (a text anchor at `story`/`at`), page (`page`: a page id or number, `view_setting` its zoom) or
/// url (`url`). The new destination's id is in `created`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create(w: &mut Work<'_>, kind: &str, name: &String, story: &str, at: &usize, page: &str, url: &str, view_setting: &str) -> Result<(), EditError> {
    let d = data(w)?;
    unique_name(&d, name, "")?;
    let k = key(w)?;
    let id = match kind {
        "text" => {
            let id = format!("HyperlinkTextDestination/{name}");
            let target = crate::text::story_of(w, story)?;
            nodes::anchor(w, &target, *at, &id, name, k)?;
            id
        }
        "page" => {
            let id = format!("HyperlinkPageDestination/{name}");
            let page = page_id(w, page)?;
            let z = zoom(view_setting)?;
            append_destination(w, "HyperlinkPageDestination", &format!("<HyperlinkPageDestination Self=\"{}\" Name=\"{}\" DestinationPage=\"{}\" ViewSetting=\"{z}\" ViewPercentage=\"100\" Hidden=\"false\" DestinationUniqueKey=\"{k}\" />", esc(&id), esc(name), esc(&page)))?;
            id
        }
        "url" => {
            validate_url(url)?;
            let id = format!("HyperlinkURLDestination/{name}");
            append_destination(w, "HyperlinkURLDestination", &format!("<HyperlinkURLDestination Self=\"{}\" DestinationUniqueKey=\"{k}\" Name=\"{}\" DestinationURL=\"{}\" Hidden=\"false\" />", esc(&id), esc(name), esc(url)))?;
            id
        }
        _ => return Err(invalid("kind is text, page or url")),
    };
    if w.find(&id).is_err() { return Err(invalid("the destination could not be made")); }
    w.created.push(id);
    Ok(())
}

/// Hyperlink Destination Options: a new `name`, and for a URL destination its `url`, for a page destination its `page` and `view_setting`;
/// an empty value leaves that one as it is.
pub(crate) fn set(w: &mut Work<'_>, id: &String, name: &String, url: &str, page: &str, view_setting: &str) -> Result<(), EditError> {
    let d = data(w)?;
    let dest = d.destinations.iter().find(|x| &x.id == id).ok_or_else(|| invalid("destination not found"))?.clone();
    if !name.is_empty() && *name != dest.name {
        unique_name(&d, name, id)?;
        set_attr(w, id, "Name", name)?;
    }
    match dest.kind.as_str() {
        "HyperlinkURLDestination" if !url.is_empty() => { validate_url(url)?; set_attr(w, id, "DestinationURL", url)?; }
        "HyperlinkPageDestination" => {
            if !page.is_empty() { let p = page_id(w, page)?; set_attr(w, id, "DestinationPage", &p)?; }
            if !view_setting.is_empty() { let z = zoom(view_setting)?.to_owned(); set_attr(w, id, "ViewSetting", &z)?; }
        }
        _ if !url.is_empty() || !page.is_empty() || !view_setting.is_empty() => return Err(invalid("only a URL destination has an address, only a page destination a page and zoom")),
        _ => {}
    }
    Ok(())
}

/// How many times `id` is named in the package other than by its own `Self` (a hyperlink's `Destination`, a bookmark, a cross-reference).
fn uses(w: &Work<'_>, id: &str) -> usize {
    let e = esc(id);
    let (quoted, text, own) = (format!("\"{e}\""), format!(">{e}<"), format!("Self=\"{e}\""));
    w.ed.parts().values().map(|t| t.matches(&quoted).count() + t.matches(&text).count() - t.matches(&own).count()).sum()
}

/// Delete Unused Destinations: every destination no hyperlink, bookmark or other object names. The ids that went are in `created`
/// (the screen says how many).
pub(crate) fn delete_unused(w: &mut Work<'_>) -> Result<(), EditError> {
    let d = data(w)?;
    let unused: Vec<String> = d.destinations.iter().filter(|x| uses(w, &x.id) == 0).map(|x| x.id.clone()).collect();
    for id in &unused {
        let story = d.destinations.iter().find(|x| &x.id == id).map(|x| x.story.clone()).unwrap_or_default();
        remove_node(w, id, false)?;
        if !story.is_empty() { w.recompose(&story); }
    }
    w.created.extend(unused);
    Ok(())
}

/// Reset Hyperlink: the text source of hyperlink `id` moves to `story` [`start`, `end`) (the selected text), with its name and settings.
pub(crate) fn reset(w: &mut Work<'_>, id: &String, story: &str, start: &usize, end: &usize) -> Result<(), EditError> {
    if start >= end { return Err(invalid("select text for the hyperlink")); }
    let d = data(w)?;
    let link = d.links.iter().find(|l| &l.id == id).ok_or_else(|| invalid("hyperlink not found"))?;
    let source = d.sources.iter().find(|s| s.id == link.source).ok_or_else(|| EditError::new("unsupported", "only a hyperlink whose source is text can be reset"))?;
    if !source.format.is_empty() { return Err(EditError::new("unsupported", "a cross-reference is relinked with its options, not reset")); }
    let (p, ix, i) = w.find(&source.id)?;
    let t = w.text(&p)?;
    let n = &ix.nodes[i];
    if ix.local(t, i) != "HyperlinkTextSource" { return Err(EditError::new("unsupported", "only a hyperlink whose source is text can be reset")); }
    let open = t[n.start..n.open_end].to_owned();
    let old_story = source.story.clone();
    remove_node(w, &source.id, true)?;
    w.recompose(&old_story);
    nodes::wrap(w, story, *start, *end, &open, "</HyperlinkTextSource>", None)
}

#[cfg(test)]
mod tests {
    use crate::{Command, Editor, commands::RegisteredCommand};
    use std::collections::BTreeMap;

    fn doc() -> Editor {
        let mut parts = BTreeMap::new();
        parts.insert("designmap.xml".into(), "<?xml version=\"1.0\"?>\n<Document Self=\"d\">\n<idPkg:Spread src=\"Spreads/Spread_s1.xml\" />\n<idPkg:Story src=\"Stories/Story_st.xml\" />\n</Document>".into());
        parts.insert("Spreads/Spread_s1.xml".into(), "<idPkg:Spread><Spread Self=\"s1\"><Page Self=\"pg1\" GeometricBounds=\"0 0 100 100\" ItemTransform=\"1 0 0 1 0 0\" /><Page Self=\"pg2\" GeometricBounds=\"0 0 100 100\" ItemTransform=\"1 0 0 1 100 0\" /></Spread></idPkg:Spread>".into());
        parts.insert("Stories/Story_st.xml".into(), "<idPkg:Story><Story Self=\"st\"><ParagraphStyleRange><CharacterStyleRange AppliedCharacterStyle=\"n\"><Content>Hello world</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>".into());
        Editor::new(parts)
    }

    fn run(ed: &mut Editor, c: RegisteredCommand) -> Result<Vec<String>, crate::EditError> {
        ed.apply("x", &[Command::Registered(c)]).map(|o| o.created)
    }

    fn create(kind: &str, name: &str, at: usize, page: &str, url: &str) -> RegisteredCommand {
        RegisteredCommand::CreateHyperlinkDestination { kind: kind.into(), name: name.into(), story: "st".into(), at, page: page.into(), url: url.into(), view_setting: String::new() }
    }

    #[test]
    fn destinations_are_made_edited_and_unused_ones_removed() {
        let mut ed = doc();
        let original = ed.parts().clone();
        assert_eq!(run(&mut ed, create("text", "Top", 6, "", "")).unwrap(), ["HyperlinkTextDestination/Top"]);
        assert!(ed.parts()["Stories/Story_st.xml"].contains("<HyperlinkTextDestination Self=\"HyperlinkTextDestination/Top\" Name=\"Top\""));
        run(&mut ed, create("page", "Second", 0, "2", "")).unwrap();
        assert!(ed.parts()["designmap.xml"].contains("DestinationPage=\"pg2\" ViewSetting=\"FitView\""));
        run(&mut ed, create("url", "Site", 0, "", "https://example.com/")).unwrap();
        assert!(run(&mut ed, create("url", "Site", 0, "", "https://example.com/")).is_err(), "names are unique");
        assert!(run(&mut ed, create("url", "Bad", 0, "", "javascript:x")).is_err(), "only http, https, mailto");
        let dm = ed.parts()["designmap.xml"].clone();
        assert!(dm.find("<HyperlinkPageDestination").unwrap() < dm.find("<HyperlinkURLDestination").unwrap(), "page destinations before URL ones");
        run(&mut ed, RegisteredCommand::SetHyperlinkDestination { id: "HyperlinkURLDestination/Site".into(), name: "Web".into(), url: "https://example.org/".into(), page: String::new(), view_setting: String::new() }).unwrap();
        run(&mut ed, RegisteredCommand::SetHyperlinkDestination { id: "HyperlinkPageDestination/Second".into(), name: String::new(), url: String::new(), page: "1".into(), view_setting: "FitWidth".into() }).unwrap();
        let data = folio_idml::hyperlinks::read(ed.parts()).unwrap();
        let web = data.destinations.iter().find(|d| d.name == "Web").unwrap();
        assert_eq!(web.value, "https://example.org/");
        let page = data.destinations.iter().find(|d| d.name == "Second").unwrap();
        assert_eq!((page.value.as_str(), page.view_setting.as_str()), ("pg1", "FitWidth"));
        assert_eq!(data.destinations.iter().find(|d| d.name == "Top").unwrap().at, 6);
        // a link to "Web" keeps it; the other two are not named by anything and go
        run(&mut ed, RegisteredCommand::InsertHyperlink { story: "st".into(), start: 0, end: 5, name: "L".into(), url: "https://a.example/".into() }).unwrap();
        let link_dest = folio_idml::hyperlinks::read(ed.parts()).unwrap().links[0].destination.clone();
        let gone = run(&mut ed, RegisteredCommand::DeleteUnusedHyperlinkDestinations {}).unwrap();
        assert_eq!(gone.len(), 3, "{gone:?}");
        let left = folio_idml::hyperlinks::read(ed.parts()).unwrap().destinations;
        assert_eq!(left.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), [link_dest]);
        for _ in 0..7 { ed.undo(); }
        assert_eq!(ed.parts(), &original, "every step undoes");
    }

    #[test]
    fn reset_moves_the_link_to_the_selected_text() {
        let mut ed = doc();
        run(&mut ed, RegisteredCommand::InsertHyperlink { story: "st".into(), start: 0, end: 5, name: "L".into(), url: "https://a.example/".into() }).unwrap();
        let id = folio_idml::hyperlinks::read(ed.parts()).unwrap().links[0].id.clone();
        run(&mut ed, RegisteredCommand::ResetHyperlink { id: id.clone(), story: "st".into(), start: 6, end: 11 }).unwrap();
        let data = folio_idml::hyperlinks::read(ed.parts()).unwrap();
        let s = &data.sources[0];
        assert_eq!((s.start, s.end), (6, 11));
        assert_eq!(data.links[0].source, s.id, "the same source, moved");
        assert_eq!(folio_edit_text(&ed), "Hello world");
    }

    fn folio_edit_text(ed: &Editor) -> String { crate::text::contents(&ed.parts()["Stories/Story_st.xml"], "st").unwrap() }
}
