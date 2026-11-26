use super::*;
use folio_model::{hyperlinks::{CrossReferenceFormat, Destination}, toc::TocSource};
use super::render;

fn format<'a>(d: &'a folio_model::hyperlinks::Hyperlinks,id: &str) -> Result<&'a CrossReferenceFormat,EditError> {
    d.formats.iter().find(|f|f.id==id||f.name==id).ok_or_else(||invalid("cross-reference format not found"))
}
pub(crate) fn insert_reference(w: &mut Work<'_>,story: &str,at: &usize,target_story: &str,target_at: &usize,destination: &String,format_id: &str,name: &String,locations: &[TocSource]) -> Result<(),EditError> {
    let d=data(w)?;let fmt=format(&d,format_id)?.clone();
    let (dest,k)=if destination.is_empty() {
        let target=crate::text::story_of(w,target_story)?;
        if d.destinations.iter().any(|d|d.name==*name) {return Err(invalid("destination name must be unique"));}
        let id=format!("HyperlinkTextDestination/{name}");let k=key(w)?;
        nodes::anchor(w,&target,*target_at,&id,name,k)?;
        (Destination{id,name:name.clone(),kind:"HyperlinkTextDestination".into(),story:target,at:*target_at,..Default::default()},k)
    }else{
        let dest=d.destinations.iter().find(|d|&d.id==destination).ok_or_else(||invalid("destination not found"))?.clone();
        let (p,ix,i)=w.find(&dest.id)?;
        let k=ix.value(w.text(&p)?,i,"DestinationUniqueKey").and_then(|s|s.parse().ok()).unwrap_or(0);
        (dest,k)
    };
    // the character before the insertion point (none at a paragraph's start): the quotes of the format follow it
    let prev=if *at==0 {None} else {let (p,_,_)=w.find(story)?;crate::text::contents(w.text(&p)?,story).ok().and_then(|t|t.chars().nth(*at-1)).filter(|&c|c!='\r')};
    let pieces=render::render_after(w,&dest,&fmt,locations,prev)?;let source=w.new_id();let source_name=source_name(w)?;
    let open=format!("<CrossReferenceSource Self=\"{source}\" AppliedFormat=\"{}\" Name=\"{source_name}\" Hidden=\"false\" AppliedCharacterStyle=\"n\">",esc(&fmt.id));
    let close="</CrossReferenceSource>";
    if let Some(value)=render::only_text(&pieces) {
        nodes::wrap(w,story,*at,*at,&open,close,Some(&value))?;
    }else{
        if render::has_page(&pieces) { render::check_variable(w)?; }
        if render::has_chapter(&pieces) { render::check_chapter_variable(w)?; }
        nodes::split(w,story,*at,&mut |w,range|{
            render::check_range(range)?;
            Ok(if let Some((text,style))=render::styled_text(&pieces) {
                format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\">{open}<Content>{}</Content>{close}</CharacterStyleRange>",esc(style),esc(text))
            }else if let Some((page,style))=render::only_page(&pieces) {
                let wrapper=format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\" PageNumberType=\"TextVariable\">",esc(style));
                format!("{wrapper}{open}{}{close}</CharacterStyleRange>",render::instance_xml(w,None,page))
            }else{format!("{}{open}{}{close}",render::empty_leading_range(&pieces),render::runs(w,&pieces,&mut Vec::new()))})
        })?;
    }
    add_link(w,name,&source,&dest.id,k)?;Ok(())
}

pub(crate) fn update_reference(w: &mut Work<'_>,id: &str,locations: &[TocSource]) -> Result<(),EditError> {
    refresh_links(w,id,locations,false)
}

fn refresh_links(w: &mut Work<'_>,id: &str,locations: &[TocSource],force: bool) -> Result<(),EditError> {
    let d=data(w)?;
    let links=d.links.iter().filter(|l|id.is_empty()||l.id==id).cloned().collect::<Vec<_>>();
    if !id.is_empty()&&links.is_empty(){return Err(invalid("cross-reference not found"));}
    for link in links {
        let d=data(w)?;
        let Some(source)=d.sources.iter().find(|s|s.id==link.source&&!s.format.is_empty())else{continue};
        let dest=d.destinations.iter().find(|d|d.id==link.destination).ok_or_else(||invalid("cross-reference destination is missing"))?;
        super::refresh::source(w,&source.id,dest,format(&d,&source.format)?,locations,force)?;
        w.recompose(&source.story);
    }Ok(())
}

pub(crate) fn set_reference(w: &mut Work<'_>,id: &String,name: &str,destination: &String,format_id: &str,locations: &[TocSource]) -> Result<(),EditError> {
    let d=data(w)?;let link=d.links.iter().find(|l|&l.id==id).ok_or_else(||invalid("cross-reference not found"))?;
    if !d.sources.iter().any(|s|s.id==link.source&&!s.format.is_empty()){return Err(invalid("expected cross-reference source"));}
    let fmt=format(&d,format_id)?;
    let dest=d.destinations.iter().find(|d|&d.id==destination).ok_or_else(||invalid("destination not found"))?;
    render::render(w,dest,fmt,locations)?;
    rename(w,id,name)?;set_attr(w,&link.source,"AppliedFormat",&fmt.id)?;
    let (p,ix,i)=w.find(id)?;let t=w.text(&p)?;
    let n=ix.path(t,i,&["Properties","Destination"]).ok_or_else(||invalid("missing destination property"))?;
    w.splice(&p,vec![(ix.nodes[n].open_end..ix.nodes[n].close_start,esc(destination))])?;
    let (p,ix,i)=w.find(destination)?;
    let k=ix.value(w.text(&p)?,i,"DestinationUniqueKey").unwrap_or_default();
    set_attr(w,id,"DestinationUniqueKey",&k)?;
    refresh_links(w,id,locations,true)
}
