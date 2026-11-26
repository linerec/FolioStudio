//! Keep hyperlink markers when editing the surrounding text. Computed cross-reference
//! text and edits crossing source boundaries are intentionally refused.
use super::*;

pub(crate) fn edit(w: &mut Work<'_>, owner: &str, start: usize, end: usize, inserted: Option<&str>) -> Result<bool,EditError> {
    // footnote text is nested in its story: the story's hyperlink offsets do not cover it
    if folio_idml::footnotes::address(owner).is_some(){return Ok(false);}
    let story=crate::text::story_of(w,owner)?;
    if folio_idml::footnotes::address(&story).is_some(){return Ok(false);}
    let (part,_,_)=w.find(&story)?;
    let text=w.text(&part)?;
    if !(text.contains("<Hyperlink")||text.contains("<CrossReferenceSource")){return Ok(false);}
    let length=folio_idml::editorial::contents(text,&story).map_err(|e|EditError::new("xml",e))?.chars().count();
    if start>end||end>length{return Err(invalid("edit outside story"));}
    let d=data(w)?;
    let sources=d.sources.iter().filter(|s|s.story==story).collect::<Vec<_>>();
    let anchors=d.destinations.iter().filter(|s|s.story==story).collect::<Vec<_>>();
    if sources.is_empty()&&anchors.is_empty(){return Ok(false);}
    for s in &sources {
        if (start< s.end&&end>s.start)||(start==end&&start>=s.start&&start<=s.end){
            return Err(EditError::new("unsupported","editing hyperlink source text is not supported yet"));
        }
    }
    let (part,ix,_)=w.find(&story)?;let t=w.text(&part)?;
    let mut splices=Vec::new();let mut restore=Vec::new();
    let added=inserted.map_or(0,|s|s.chars().count());
    let moved=|at:usize|if at>=end {at-(end-start)+added}else if at>start {start}else{at};
    for s in sources {
        let i=ix.by_self(t,&s.id).ok_or_else(||invalid("source not found"))?;let n=&ix.nodes[i];
        restore.push((moved(s.start),moved(s.end),t[n.start..n.open_end].to_owned(),t[n.close_start..n.end].to_owned()));
        splices.push((n.start..n.open_end,String::new()));splices.push((n.close_start..n.end,String::new()));
    }
    for a in anchors {
        let i=ix.by_self(t,&a.id).ok_or_else(||invalid("anchor not found"))?;let n=&ix.nodes[i];
        if !(start<end&&a.at>=start&&a.at<end) {restore.push((moved(a.at),moved(a.at),t[n.start..n.end].to_owned(),String::new()));}
        splices.push((n.start..n.end,String::new()));
    }
    w.splice(&part,splices)?;
    match inserted {Some(text)=>crate::text::insert(w,&story,start,text)?,None=>crate::text::delete(w,&story,start,end)?}
    restore.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (start,end,open,close) in restore {nodes::wrap(w,&story,start,end,&open,&close,None)?;}
    Ok(true)
}
