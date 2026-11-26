//! Value-based editing of cross-reference format definitions.
use super::*;
use folio_model::hyperlinks::CrossReferenceFormat;
use folio_idml::hyperlinks::formats as idml;

fn find(w: &Work<'_>, id: &str) -> Result<CrossReferenceFormat,EditError> {
    data(w)?.formats.into_iter().find(|f|f.id==id||f.name==id).ok_or_else(||invalid("cross-reference format not found"))
}

fn validate(w: &mut Work<'_>, f: &CrossReferenceFormat) -> Result<(),EditError> {
    if f.name.trim().is_empty() || data(w)?.formats.iter().any(|x|x.name==f.name&&x.id!=f.id) { return Err(invalid("format name must be unique and nonempty")); }
    for style in std::iter::once(&f.character_style).chain(f.blocks.iter().map(|b|&b.character_style)) {
        if !matches!(style.as_str(),""|"n") { let (p,ix,i)=w.find(style)?; if ix.local(w.text(&p)?,i)!="CharacterStyle" { return Err(invalid("expected character style")); } }
    }
    for b in &f.blocks {
        if !matches!(b.kind.as_str(),"CustomStringBuildingBlock"|"ParagraphTextBuildingBlock"|"ParagraphNumberBuildingBlock"|"FullParagraphBuildingBlock"|"BookmarkNameBuildingBlock"|"PageNumberBuildingBlock"|"ChapterNumberBuildingBlock"|"FileNameBuildingBlock") {return Err(invalid("unknown cross-reference building block"));}
    }
    Ok(())
}

pub(crate) fn save(w: &mut Work<'_>, format: &CrossReferenceFormat, locations: &[folio_model::toc::TocSource]) -> Result<(),EditError> {
    let mut f=format.clone();
    let previous=if f.id.is_empty(){None}else{Some(find(w,&f.id)?)};
    if let Some(old)=&previous { f.id=old.id.clone(); }
    validate(w,&f)?;
    if let Some(old)=previous
        && old.blocks==f.blocks && old.character_style==f.character_style { return set_attr(w,&f.id,"Name",&f.name); }
    if f.id.is_empty() {
        f.id=w.new_id();
        let ix=w.index("designmap.xml")?;let t=w.text("designmap.xml")?;
        let root=ix.roots().next().ok_or_else(||invalid("missing document"))?;
        let at=ix.children(root).iter().rev().find(|&&i|ix.local(t,i)=="CrossReferenceFormat").map(|&i|ix.nodes[i].end).unwrap_or(ix.nodes[root].close_start);
        w.splice("designmap.xml",vec![(at..at,idml::xml(&f))])?;
        w.created.push(f.id.clone());
    } else {
        set_attr(w,&f.id,"Name",&f.name)?;
        set_attr(w,&f.id,"AppliedCharacterStyle",idml::style(&f.character_style))?;
        let(p,ix,i)=w.find(&f.id)?;let t=w.text(&p)?;
        if ix.nodes[i].empty {
            let node=&ix.nodes[i];
            let opening=t[node.start..node.open_end].trim_end_matches('>').trim_end().trim_end_matches('/');
            let replacement=format!("{opening}>{}</CrossReferenceFormat>",idml::blocks_xml(&f));
            w.splice(&p,vec![(node.start..node.end,replacement)])?;
        } else {
            let nodes=ix.children(i).iter().copied().filter(|&c|ix.local(t,c)=="BuildingBlock").collect::<Vec<_>>();
            let at=nodes.first().map_or(ix.nodes[i].close_start,|&c|ix.nodes[c].start);
            let mut edits=nodes.iter().map(|&c|(ix.nodes[c].start..ix.nodes[c].end,String::new())).collect::<Vec<_>>();
            if let Some(first)=edits.first_mut(){first.1=idml::blocks_xml(&f);}else{edits.push((at..at,idml::blocks_xml(&f)));}
            w.splice(&p,edits)?;
        }
    }
    let d=data(w)?;
    for source in d.sources.iter().filter(|s|s.format==f.id) {
        let Some(link)=d.links.iter().find(|l|l.source==source.id) else {continue};
        let dest=d.destinations.iter().find(|x|x.id==link.destination).ok_or_else(||invalid("cross-reference destination is missing"))?;
        super::refresh::source(w,&source.id,dest,&f,locations,true)?;
        w.recompose(&source.story);
    }
    Ok(())
}

pub(crate) fn duplicate(w: &mut Work<'_>, id: &str, name: &str) -> Result<(),EditError> {
    let mut f=find(w,id)?;f.id.clear();f.name=name.to_owned();save(w,&f,&Vec::new())
}

pub(crate) fn delete(w: &mut Work<'_>, id: &str) -> Result<(),EditError> {
    let f=find(w,id)?;
    if data(w)?.sources.iter().any(|s|s.format==f.id) {return Err(invalid("cross-reference format is in use"));}
    remove_node(w,&f.id,false)
}
