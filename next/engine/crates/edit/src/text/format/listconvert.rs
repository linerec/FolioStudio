use super::*;

pub(crate) fn run(w:&mut Work<'_>,id:&str,start:&usize,end:&usize,labels:&BTreeMap<String,String>)->Result<(),EditError>{
    convert(w,id,start,end,labels,true)
}
fn convert(w:&mut Work<'_>,id:&str,start:&usize,end:&usize,labels:&BTreeMap<String,String>,clear:bool)->Result<(),EditError>{
    let (part,ix,i,_)=locate(w,id)?;
    let s=Story::read(w.text(&part)?,&ix,i)?;
    validate(&s,*start,*end)?;
    let range=paragraph_range(&s,*start,*end);
    let catalog=Catalog::read(&w.ed.parts)?;
    let mut edits=Vec::new();
    for at in range.clone().filter(|&at|at==0||s.atoms[at-1].ch=='\r') {
        let Some(label)=labels.get(&at.to_string()) else {continue};
        if label.contains(['\r','\n']) {return Err(unsupported("multiline list label"));}
        let (p,_)=inherit(&s,at);
        let mut attrs=catalog.para(&style_attrs(&s.paragraphs[p]))?;
        attrs.extend(style_attrs(&s.paragraphs[p]));
        let key=match val(&attrs,"BulletsAndNumberingListType") {"BulletList"=>"BulletsCharacterStyle","NumberedList"=>"NumberingCharacterStyle",_=>continue};
        let marker=val(&attrs,key).to_owned();
        edits.push((at,label.clone(),marker));
    }
    for (at,label,marker) in edits.into_iter().rev() {
        super::super::insert(w,id,at,&label)?;
        let end=at+label.chars().count();
        if !marker.is_empty() && !marker.ends_with("/$ID/[No character style]") {
            apply_style(w,id,at,end,&marker,false,false)?;
        }
        if clear {format(w,id,at,end,&BTreeMap::from([("bulletsAndNumberingListType".into(),Value::String("NO_LIST".into()))]))?;}
    }
    Ok(())
}

/// Convert every paragraph using a style or one of its descendants in one edit.
pub(crate) fn style(w:&mut Work<'_>,style:&str,labels:&BTreeMap<String,BTreeMap<String,String>>)->Result<(),EditError>{
    let catalog=Catalog::read(&w.ed.parts)?;
    let style=catalog.id("ParagraphStyle",style)?;
    let mut related=std::collections::BTreeSet::from([style.clone()]);
    loop {
        let count=related.len();
        for (id,a) in &catalog.styles {if id.starts_with("ParagraphStyle/")&&related.contains(val(a,"BasedOn")){related.insert(id.clone());}}
        if related.len()==count {break}
    }
    for (id,prefixes) in labels {
        let (part,ix,i,_)=locate(w,id)?;
        let story=Story::read(w.text(&part)?,&ix,i)?;
        let starts:Vec<_>=(0..story.atoms.len()).filter(|&at|at==0||story.atoms[at-1].ch=='\r').filter(|&at|{
            let (p,_)=inherit(&story,at);related.contains(val(&style_attrs(&story.paragraphs[p]),"AppliedParagraphStyle"))
        }).collect();
        for at in starts.into_iter().rev(){
            let (p,_)=inherit(&story,at);let attrs=style_attrs(&story.paragraphs[p]);
            let clear=val(&attrs,"AppliedParagraphStyle")!=style||attrs.contains_key("BulletsAndNumberingListType");
            convert(w,id,&at,&at,prefixes,clear)?;
        }
    }
    super::styles::run_with_refresh(w,&crate::Command::StyleEdit{kind:"paragraph".into(),style,props:BTreeMap::from([("bulletsAndNumberingListType".into(),Value::String("NO_LIST".into()))])},false)
}
