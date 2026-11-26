//! Locate notes through revision wrappers without rewriting those wrappers.
use super::*;

pub(super) fn nodes(source:&str,ix:&Index,story:usize)->Vec<usize>{
    ix.nodes.iter().enumerate().filter_map(|(i,_)|{
        if ix.local(source,i)!="Note"{return None;}
        let mut parent=ix.nodes[i].parent;
        while let Some(p)=parent {
            if p==story{return Some(i);}
            if ix.local(source,p)=="Note" || (ix.local(source,p)=="Change"&&ix.value(source,p,"ChangeType").as_deref()==Some("DeletedText")){return None;}
            parent=ix.nodes[p].parent;
        }
        None
    }).collect()
}

pub(super) fn edit(w:&mut Work<'_>,id:&str,index:usize,f:impl FnOnce(&mut String)->Result<(),EditError>)->Result<(),EditError>{
    let (part,ix,i,owner)=locate(w,id)?;
    let source=w.text(&part)?;
    let note=*nodes(source,&ix,i).get(index).ok_or_else(||EditError::new("notFound",format!("note {index}")))?;
    let n=&ix.nodes[note];let mut raw=source[n.start..n.end].to_owned();
    f(&mut raw)?;w.splice(&part,vec![(n.start..n.end,raw)])?;w.recompose(&owner);Ok(())
}

pub(crate) fn remove(w:&mut Work<'_>,id:&String,index:&usize,user:&String,date:&String)->Result<(),EditError>{
    let (part,_,_,owner)=locate(w,id)?;
    let notes=folio_idml::editorial::metadata(w.text(&part)?,&owner).map_err(|e|EditError::new("xml",e))?.0;
    let at=notes.get(*index).ok_or_else(||EditError::new("notFound",format!("note {index}")))?.at;
    track_edit::delete(w,id,&at,&(at+1),user,date)
}

pub(crate) fn remove_all(w:&mut Work<'_>,id:&String,document:&bool,user:&String,date:&String)->Result<(),EditError>{
    let ids=if *document {w.ed.parts.keys().filter_map(|p|p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect()}else{vec![id.clone()]};
    for id in ids {
        let (part,ix,i,_)=locate(w,&id)?;
        let count=nodes(w.text(&part)?,&ix,i).len();
        for index in (0..count).rev(){remove(w,&id,&index,user,date)?;}
    }
    Ok(())
}

pub(crate) fn to_text(w:&mut Work<'_>,id:&String,index:&usize,user:&String,date:&String)->Result<(),EditError>{
    let tracking=track_edit::enabled(w,id)?;
    let (part,ix,i,owner)=locate(w,id)?;
    let source=w.text(&part)?;
    if !source.contains("<Change ")&&!tracking {return notes::to_text(w,id,index);}
    let node=*nodes(source,&ix,i).get(*index).ok_or_else(||EditError::new("notFound",format!("note {index}")))?;
    let raw=source[ix.nodes[node].start..ix.nodes[node].end].to_owned();
    let metadata=folio_idml::editorial::metadata(source,&owner).map_err(|e|EditError::new("xml",e))?.0;
    let at=metadata[*index].at;
    let visible=folio_idml::editorial::contents(source,&owner).map_err(|e|EditError::new("xml",e))?;
    let chars:Vec<_>=visible.chars().collect();
    let mut s=Story::read_mode(source,&ix,i,true)?;
    track_edit::boundary(&mut s,at+1)?;
    let first=track_edit::boundary(&mut s,at)?;
    let last=track_edit::boundary(&mut s,at+1)?;
    let marker=s.atoms[first].clone();
    let before=at.checked_sub(1).map(|i|chars[i]);let after=chars.get(at+1).copied();
    let mut atoms=notes::expand(w,&mut s,&marker,&raw,before,after)?;
    if atoms.iter().any(|a|!a.content&&a.ch!='\r'){return Err(unsupported("conversion of a tracked note containing objects"));}
    if tracking&&!atoms.is_empty(){
        let reference=author::reference(w,user)?;
        let marker_ix=Index::scan(&marker.raw).map_err(|e|EditError::new("xml",e))?;
        let own=marker.raw.starts_with("<Change ")&&marker_ix.value(&marker.raw,0,"UserName").unwrap_or_default().trim_start_matches("$ID/")==user.trim_start_matches("$ID/");
        let mut records=Vec::new();let mut start=0;
        for end in 1..=atoms.len(){
            if end<atoms.len()&&s.characters[atoms[start].c].key==s.characters[atoms[end].c].key {continue;}
            let mut record=if own {track_edit::with_body(&marker,&atoms[start..end])}
                else {track_edit::new_atom(marker.p,atoms[start].c,"InsertedText",user,date,&reference,&track_edit::inline(&atoms[start..end]))};
            record.c=atoms[start].c;records.push(record);
            start=end;
        }
        atoms=records;
    }
    s.atoms.splice(first..last,atoms);
    track_edit::join_insertions(&mut s,user)?;
    write(w,&part,&owner,&s)
}
