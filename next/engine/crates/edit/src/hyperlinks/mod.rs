//! Hyperlink commands operate on the original IDML package and share editor history.
mod nodes;
pub(crate) mod datamerge;
pub(crate) mod datasource;
pub(crate) mod qr;
pub(crate) mod merge;
pub(crate) mod merged;
pub(crate) mod computed;
mod references;
mod queries;
mod render;
mod refresh;
pub(crate) mod formats;
pub(crate) use queries::query_xml;
pub(crate) mod text_edit;
pub(crate) mod dbsource;
pub(crate) mod destinations;
use crate::{EditError, Work};
use crate::xml::{self, Index};
pub(crate) use references::{insert_reference, update_reference, set_reference};

fn data(w: &Work<'_>) -> Result<folio_model::hyperlinks::Hyperlinks,EditError> {
    folio_idml::hyperlinks::read(w.ed.parts()).map_err(|e|EditError::new("xml",e))
}
fn invalid(s: &str) -> EditError {EditError::new("invalid",s)}
fn esc(s: &str) -> String {xml::escape(s)}
fn append(w: &mut Work<'_>, raw: &str) -> Result<(),EditError> {
    let ix=w.index("designmap.xml")?;
    let root=ix.roots().next().ok_or_else(||invalid("missing document"))?;
    let t=w.text("designmap.xml")?;
    let tag=if raw.starts_with("<HyperlinkURLDestination "){"HyperlinkURLDestination"}else{"Hyperlink"};
    let at=ix.children(root).iter().rev().find(|&&i|ix.local(t,i)==tag).map(|&i|ix.nodes[i].end)
        .or_else(||ix.children(root).iter().find(|&&i|ix.local(t,i)=="Hyperlink").map(|&i|ix.nodes[i].start))
        // the first one in a document: InDesign writes it after the stories, before the indexing sort options (measured)
        .or_else(||ix.children(root).iter().find(|&&i|ix.local(t,i)=="IndexingSortOption").map(|&i|ix.nodes[i].start))
        .or_else(||ix.children(root).iter().rev().find(|&&i|ix.local(t,i)=="Story").map(|&i|ix.nodes[i].end))
        .unwrap_or(ix.nodes[root].close_start);
    w.splice("designmap.xml",vec![(at..at,format!("\n{raw}\n"))])
}
fn remove_node(w: &mut Work<'_>, id: &str, unwrap: bool) -> Result<(),EditError> {
    let (p,ix,i)=w.find(id)?;let n=&ix.nodes[i];let t=w.text(&p)?;
    let body=if unwrap {t[n.open_end..n.close_start].to_owned()}else{String::new()};
    w.splice(&p,vec![(n.start..n.end,body)])
}
fn key(w: &Work<'_>) -> Result<usize,EditError> {
    let mut max=0;
    for t in w.ed.parts().values() {let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
        for i in 0..ix.nodes.len() {if let Some(n)=ix.value(t,i,"DestinationUniqueKey").and_then(|s|s.parse::<usize>().ok()){max=max.max(n);}}
    }
    max.checked_add(1).ok_or_else(||invalid("destination key overflow"))
}
fn add_link(w: &mut Work<'_>, name: &str, source: &str, dest: &str, key: usize) -> Result<String,EditError> {
    if name.trim().is_empty() || data(w)?.links.iter().any(|l|l.name==name) {return Err(invalid("hyperlink name must be unique and nonempty"));}
    let id=w.new_id();
    append(w,&format!("<Hyperlink Self=\"{}\" Name=\"{}\" Source=\"{}\" Visible=\"false\" Highlight=\"None\" Width=\"Thin\" BorderStyle=\"Solid\" Hidden=\"false\" DestinationUniqueKey=\"{key}\"><Properties><BorderColor type=\"enumeration\">Black</BorderColor><Destination type=\"object\">{}</Destination></Properties></Hyperlink>",esc(&id),esc(name),esc(source),esc(dest)))?;
    w.created.push(id.clone());Ok(id)
}

pub(crate) fn insert_url(w: &mut Work<'_>, story: &str, start: &usize, end: &usize, name: &str, url: &str) -> Result<(),EditError> {
    if start>=end {return Err(invalid("select text for a hyperlink"));}
    validate_url(url)?;
    let k=key(w)?;let source=w.new_id();let source_name=source_name(w)?;
    let d=data(w)?;let mut number=1;
    while d.destinations.iter().any(|d|d.name==format!("URL {number}")){number+=1;}
    let dest=format!("HyperlinkURLDestination/URL {number}");
    nodes::wrap(w,story,*start,*end,&format!("<HyperlinkTextSource Self=\"{source}\" Name=\"{source_name}\" Hidden=\"false\" AppliedCharacterStyle=\"n\">"),"</HyperlinkTextSource>",None)?;
    append(w,&format!("<HyperlinkURLDestination Self=\"{dest}\" DestinationUniqueKey=\"{k}\" Name=\"URL {number}\" DestinationURL=\"{}\" Hidden=\"false\" />",esc(url)))?;
    add_link(w,name,&source,&dest,k)?;Ok(())
}
fn source_name(w: &mut Work<'_>) -> Result<String,EditError> {
    let d=data(w)?;let mut names=Vec::new();
    for s in d.sources {let(p,ix,i)=w.find(&s.id)?;names.push(ix.value(w.text(&p)?,i,"Name").unwrap_or_default());}
    let mut n=1;while names.contains(&format!("Hyperlink {n}")){n+=1;}Ok(format!("Hyperlink {n}"))
}
fn validate_url(url: &str) -> Result<(),EditError> {
    if !(url.starts_with("https://")||url.starts_with("http://")||url.starts_with("mailto:")) || url.chars().any(char::is_control) {return Err(invalid("expected an http, https or mailto URL"));}Ok(())
}

pub(crate) fn set_url(w: &mut Work<'_>, id: &String, name: &str, url: &str) -> Result<(),EditError> {
    validate_url(url)?;
    let d=data(w)?;let link=d.links.iter().find(|l|&l.id==id).ok_or_else(||invalid("hyperlink not found"))?;
    let dest=d.destinations.iter().find(|d|d.id==link.destination&&d.kind=="HyperlinkURLDestination").ok_or_else(||invalid("expected URL destination"))?;
    rename(w,id,name)?;
    set_attr(w,&dest.id,"DestinationURL",url)
}
fn rename(w: &mut Work<'_>, id: &str, name: &str) -> Result<(),EditError> {
    if name.trim().is_empty()||data(w)?.links.iter().any(|l|l.id!=id&&l.name==name){return Err(invalid("hyperlink name must be unique and nonempty"));}
    set_attr(w,id,"Name",name)
}
fn set_attr(w: &mut Work<'_>, id: &str, name: &str, value: &str) -> Result<(),EditError> {
    let (p,ix,i)=w.find(id)?;let t=w.text(&p)?;
    let edit=crate::ops::attr_edit(&ix,t,i,name,value);
    w.splice(&p,vec![edit])
}
pub(crate) fn delete(w: &mut Work<'_>, id: &String) -> Result<(),EditError> {
    let d=data(w)?;let l=d.links.iter().find(|l|&l.id==id).ok_or_else(||invalid("hyperlink not found"))?;
    if !d.sources.iter().any(|s|s.id==l.source) {return Err(EditError::new("unsupported","only text hyperlink sources can be deleted"));}
    remove_node(w,id,false)
}
