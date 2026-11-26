use crate::{EditError, Work, xml::{Index, escape}};
use std::collections::BTreeMap;
mod ranges;
mod story;
mod convert;
mod insert;
pub(crate) use ranges::remove_inserted;
pub(crate) use convert::run as convert_notes;
pub(crate) use story::text_escape;
pub(crate) use insert::run as insert_endnote;
pub(crate) use insert::{add_bodies, marker_look};
pub(crate) use story::RichRun;

pub(crate) fn set_options(w: &mut Work<'_>, props: &BTreeMap<String, serde_json::Value>) -> Result<(), EditError> {
    if props.is_empty() { return Ok(()); }
    let changes = props.iter().map(|(key,value)| {
        let value = value.as_str().map(str::to_owned).unwrap_or_else(||value.to_string());
        folio_idml::endnotes::option_value(key,&value).map_err(|e|EditError::new("invalid",e))
    }).collect::<Result<Vec<_>,_>>()?;
    const PART: &str = "designmap.xml";
    let source = w.text(PART)?;
    let ix = Index::scan(source).map_err(|e|EditError::new("xml",e))?;
    let option = (0..ix.nodes.len()).find(|&i|ix.local(source,i)=="EndnoteOption");
    if option.is_none() {
        let root = (0..ix.nodes.len()).find(|&i|ix.local(source,i)=="Document").ok_or_else(||EditError::new("xml","missing Document"))?;
        let at = ix.nodes[root].close_start;
        w.splice(PART,vec![(at..at,folio_idml::endnotes::DEFAULT_OPTIONS.into())])?;
    }
    // the enumerations are elements of the option's `Properties` (numbering style, restart, marker position, scope, frame, prefix/suffix)
    let (properties, attributes): (Vec<_>, Vec<_>) = changes.into_iter().partition(|(name, _)| folio_idml::endnotes::is_property(name));
    let source = w.text(PART)?;
    let ix = Index::scan(source).map_err(|e|EditError::new("xml",e))?;
    let option = (0..ix.nodes.len()).find(|&i|ix.local(source,i)=="EndnoteOption").unwrap();
    let mut edits = Vec::new();
    let mut missing = String::new();
    for (name,value) in attributes {
        if let Some(attr) = ix.attrs(source,option).into_iter().find(|a|&source[a.name.0..a.name.1]==name) {
            edits.push((attr.value.0..attr.value.1,escape(&value)));
        } else { missing.push_str(&format!(" {name}=\"{}\"",escape(&value))); }
    }
    if !missing.is_empty() {
        let node = &ix.nodes[option];
        let at = node.open_end - if node.empty {2} else {1};
        edits.push((at..at,missing));
    }
    if !properties.is_empty() {
        let list = ix.named(source, option, "Properties").first().copied();
        let mut added = String::new();
        for (name, value) in properties {
            let existing = list.and_then(|l| ix.named(source, l, name).first().copied());
            match existing {
                Some(e) if !ix.nodes[e].empty => edits.push((ix.nodes[e].open_end..ix.nodes[e].close_start, escape(&value))),
                Some(e) => edits.push((ix.nodes[e].start..ix.nodes[e].end, format!("<{name} type=\"enumeration\">{}</{name}>", escape(&value)))),
                None => added.push_str(&format!("<{name} type=\"enumeration\">{}</{name}>", escape(&value))),
            }
        }
        if !added.is_empty() {
            match list {
                Some(l) if !ix.nodes[l].empty => { let at = ix.nodes[l].close_start; edits.push((at..at, added)); }
                Some(l) => edits.push((ix.nodes[l].start..ix.nodes[l].end, format!("<Properties>{added}</Properties>"))),
                None => {
                    let node = &ix.nodes[option];
                    if node.empty {
                        // (an empty option element `<EndnoteOption …/>` gets a body)
                        edits.push((node.open_end - 2..node.end, format!("><Properties>{added}</Properties></EndnoteOption>")));
                    } else { let at = node.open_end; edits.push((at..at, format!("<Properties>{added}</Properties>"))); }
                }
            }
        }
    }
    w.splice(PART,edits)?;
    let stories: Vec<_> = w.ed.parts.keys().filter_map(|p|p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
    for story in stories { w.recompose(&story); }
    Ok(())
}

impl crate::Editor {
    pub fn endnote_options(&self) -> Result<BTreeMap<String,String>, EditError> {
        let source = self.part("designmap.xml").ok_or_else(||EditError::new("notFound","designmap.xml"))?;
        folio_idml::endnotes::settings(source).map_err(|e|EditError::new("xml",e))
    }
}
