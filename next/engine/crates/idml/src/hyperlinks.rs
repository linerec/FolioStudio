//! Hyperlink definitions and story positions in Unicode scalar indices.
use crate::xml::{self, Element, Node};
use folio_model::hyperlinks::*;
use std::collections::BTreeMap;
pub mod formats;

/// How a page destination shows its page (IDML `ViewSetting`/`ViewPercentage`). `percent` is the fixed magnification in percent of actual size and is
/// only meaningful for `fixed`; the other settings fit the page to the window or keep the current zoom.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PageView {
    pub setting: &'static str,
    pub percent: f64,
}

/// Where Go To Destination goes, as structured data: the screen applies it (scroll, zoom, selection) and an automation client reads the same value.
/// `url` is only the destination's address: nothing here opens it.
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Text { story: String, start: usize, end: usize },
    Page { page: String, view: PageView },
    Url { url: String },
    MissingLink,
    MissingDestination,
    Unsupported { destination_kind: String },
}

/// IDML `ViewSetting` as the screen's zoom setting. InDesign measured (2025, a window, 2026-10-28): Fixed N = N percent of actual size; Fit View, Fit Height
/// and Fit Visible fit the whole page in the window; Fit Width fits its width; Inherit Zoom keeps the current zoom, and so does a value this version does not know. An absent value is InDesign's default (Fixed, 100).
fn page_view(dest: &Destination) -> PageView {
    let setting = match dest.view_setting.as_str() {
        "" | "Fixed" => "fixed",
        "FitView" => "fitView",
        "FitWidth" => "fitWidth",
        "FitHeight" => "fitHeight",
        "FitVisible" => "fitVisible",
        _ => "inheritZoom",
    };
    // An absent ViewSetting is InDesign's default: Fixed at 100 percent.
    PageView { setting, percent: dest.view_percentage.filter(|p| *p > 0.0).unwrap_or(100.0) }
}

/// Resolve a document-local position or address without selecting, editing, or opening anything.
pub fn target(data: &Hyperlinks, link: &str) -> Target { target_in(data, link, &|_| true) }

/// `target`, where a page destination whose page `has_page` does not know is a missing destination.
pub fn target_in(data: &Hyperlinks, link: &str, has_page: &dyn Fn(&str) -> bool) -> Target {
    let Some(link) = data.links.iter().find(|l| l.id == link) else { return Target::MissingLink };
    let Some(dest) = data.destinations.iter().find(|d| d.id == link.destination) else { return Target::MissingDestination };
    match dest.kind.as_str() {
        "HyperlinkTextDestination" | "ParagraphDestination" => {
            if dest.story.is_empty() { return Target::MissingDestination; }
            Target::Text { story: dest.story.clone(), start: dest.at, end: dest.at }
        }
        "HyperlinkPageDestination" => {
            if dest.value.is_empty() || !has_page(&dest.value) { return Target::MissingDestination; }
            Target::Page { page: dest.value.clone(), view: page_view(dest) }
        }
        "HyperlinkURLDestination" => Target::Url { url: dest.value.clone() },
        _ => Target::Unsupported { destination_kind: dest.kind.clone() },
    }
}

fn attr(e: &Element<'_>, name: &str) -> String { e.attr(name).unwrap_or_default().to_owned() }

pub(crate) fn from_elements(dm: &Element<'_>, stories: &[&Element<'_>]) -> Hyperlinks {
    let mut out = Hyperlinks::default();
    for e in dm.elements() {
        match e.name {
            "Hyperlink" => out.links.push(Hyperlink { id:attr(e,"Self"), name:attr(e,"Name"), source:attr(e,"Source"),
                destination:e.child("Properties").and_then(|p|p.child("Destination")).map(Element::text).unwrap_or_default() }),
            "HyperlinkURLDestination" | "HyperlinkPageDestination" | "HyperlinkExternalPageDestination" => out.destinations.push(Destination {
                id:attr(e,"Self"), name:attr(e,"Name"), kind:e.name.into(), value:e.attr("DestinationURL").or_else(||e.attr("DestinationPage")).unwrap_or_default().into(),
                view_setting:attr(e,"ViewSetting"), view_percentage:e.attr("ViewPercentage").and_then(|v|v.trim().parse().ok()), ..Default::default()
            }),
            "HyperlinkPageItemSource" => out.item_sources.push(ItemSource { id:attr(e,"Self"), item:attr(e,"SourcePageItem") }),
            "CrossReferenceFormat" => out.formats.push(CrossReferenceFormat { id:attr(e,"Self"), name:attr(e,"Name"), character_style:attr(e,"AppliedCharacterStyle"),
                blocks:e.children_named("BuildingBlock").map(|b|BuildingBlock {kind:attr(b,"BlockType"),text:attr(b,"CustomText"),character_style:attr(b,"AppliedCharacterStyle"),delimiter:attr(b,"AppliedDelimiter"),include_delimiter:b.attr("IncludeDelimiter")==Some("true")}).collect() }),
            _ => {}
        }
    }
    for root in stories {
        if let Some(s)=root.child("Story") { walk(s, &attr(s,"Self"), &mut 0, &mut out); }
    }
    out
}

fn walk(e: &Element<'_>, story: &str, at: &mut usize, out: &mut Hyperlinks) {
    if e.name=="HiddenText" || (e.name=="Change" && e.attr("ChangeType")==Some("DeletedText")) {return;}
    let start=*at;
    match e.name {
        "HyperlinkTextDestination" | "ParagraphDestination" => {out.destinations.push(Destination {id:attr(e,"Self"),name:attr(e,"Name"),kind:e.name.into(),story:story.into(),at:start,..Default::default()}); return;}
        "Content" => { *at += e.children.iter().map(|n|match n {Node::Text(t)=>t.chars().count(),Node::Pi(..)=>1,_=>0}).sum::<usize>(); return; }
        "Br" | "Table" | "Footnote" | "Endnote" | "Note" | "TextVariableInstance" | "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "Group" | "TextFrame" => {*at+=1;return;}
        "PageReference" if e.elements().all(|c| c.name == "Properties") => {*at+=1;return;}
        "Properties" => return,
        _ => {}
    }
    for child in e.elements() {walk(child,story,at,out);}
    if matches!(e.name,"HyperlinkTextSource"|"CrossReferenceSource") {
        out.sources.push(TextSource{id:attr(e,"Self"),story:story.into(),start,end:*at,format:attr(e,"AppliedFormat")});
    }
}

pub fn read(parts: &BTreeMap<String,String>) -> Result<Hyperlinks,String> {
    let dm=xml::parse(parts.get("designmap.xml").ok_or("missing designmap.xml")?).map_err(|e|e.0)?;
    let stories=parts.iter().filter(|(p,_)|p.starts_with("Stories/")).map(|(_,t)|xml::parse(t).map_err(|e|e.0)).collect::<Result<Vec<_>,_>>()?;
    Ok(from_elements(&dm,&stories.iter().collect::<Vec<_>>()))
}
