//! Changes retain their source XML until the user accepts or rejects them.
use super::*;

fn stories(w: &Work<'_>, id: &str, document: bool) -> Vec<String> {
    if document { w.ed.parts.keys().filter_map(|p|p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect() }
    else {vec![id.to_owned()]}
}

pub(super) fn insertion(s:&mut Story,a:&Atom,ix:&Index)->Result<Vec<Atom>,EditError>{
    let original=s.atoms.len();
    for &child in ix.children(0) {
        let n=&ix.nodes[child];
        match ix.local(&a.raw,child) {
            "Content"=>s.content(&a.raw[n.open_end..n.close_start],a.p,a.c)?,
            "Br"=>s.atoms.push(Atom{p:a.p,c:a.c,ch:'\r',content:false,raw:a.raw[n.start..n.end].into()}),
            "Note"=>s.atoms.push(Atom{p:a.p,c:a.c,ch:'\u{feff}',content:false,raw:a.raw[n.start..n.end].into()}),
            "Endnote"=>s.atoms.push(Atom{p:a.p,c:a.c,ch:'\u{4}',content:false,raw:a.raw[n.start..n.end].into()}),
            _=>return Err(unsupported("inserted change contains an unsupported object")),
        }
    }
    Ok(s.atoms.drain(original..).collect())
}

/// Read-only character/style view. Never write this projection back to IDML.
pub(super) fn visible(mut s:Story)->Result<Story,EditError>{
    for at in (0..s.atoms.len()).rev() {
        let a=s.atoms[at].clone();
        if !a.raw.starts_with("<Change "){continue;}
        let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
        let added=match ix.value(&a.raw,0,"ChangeType").as_deref(){
            Some("InsertedText")=>insertion(&mut s,&a,&ix)?,
            Some("DeletedText")=>Vec::new(),
            _=>return Err(unsupported("unsupported change type")),
        };
        s.atoms.splice(at..at+1,added);
    }
    Ok(s)
}

pub(crate) fn set_tracking(w: &mut Work<'_>, id: &String, document: &bool, enabled: &bool) -> Result<(),EditError> {
    for id in stories(w,id,*document) {
        let (part,ix,i,owner)=locate(w,&id)?;
        let source=w.text(&part)?;
        let value=enabled.to_string();
        let patch=if let Some(a)=ix.attrs(source,i).iter().find(|a|&source[a.name.0..a.name.1]=="TrackChanges") {
            (a.value.0..a.value.1,value)
        } else {let at=ix.nodes[i].open_end-1;(at..at,format!(" TrackChanges=\"{value}\""))};
        w.splice(&part,vec![patch])?;w.recompose(&owner);
    }
    Ok(())
}

pub(crate) fn resolve(w: &mut Work<'_>, id: &String, document: &bool, index: &i32, user: &String, accept: &bool) -> Result<(),EditError> {
    if *index < -1 {return Err(EditError::new("invalid","change index must be -1 or nonnegative"));}
    for id in stories(w,id,*document) {
        let (part,ix,i,owner)=locate(w,&id)?;
        let source=w.text(&part)?;
        if !source.contains("<Change ") {continue;}
        let mut s=Story::read_mode(source,&ix,i,true)?;
        let selected:Vec<_>=s.atoms.iter().enumerate().filter(|(_,a)|a.raw.starts_with("<Change "))
            .enumerate().filter(|(n,_)|*index<0||*n==*index as usize).map(|(_, (at,_))|at).collect();
        let mut changed=false;
        for at in selected.into_iter().rev() {
            let a=s.atoms[at].clone();
            let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
            let author=ix.value(&a.raw,0,"UserName").unwrap_or_default();
            if !user.is_empty() && author.trim_start_matches("$ID/")!=user.trim_start_matches("$ID/") {continue;}
            let kind=ix.value(&a.raw,0,"ChangeType").unwrap_or_default();
            let keep=match kind.as_str() {"InsertedText"=>*accept,"DeletedText"=>!*accept,_=>return Err(unsupported(format!("change type {kind}; original XML kept")))};
            let mut added=Vec::new();
            if keep && kind=="InsertedText" {
                added=insertion(&mut s,&a,&ix)?;
            } else if keep {
                let mut inner=Story::read(&a.raw,&ix,0)?;
                let po=s.paragraphs.len();let co=s.characters.len();
                s.paragraphs.extend(inner.paragraphs);s.characters.extend(inner.characters);
                for a in &mut inner.atoms {a.p+=po;a.c+=co;}
                added=inner.atoms;
            } else if kind=="InsertedText" {
                for &child in ix.children(0) {
                    if ix.local(&a.raw,child)=="Endnote" {
                        let reference=ix.value(&a.raw,child,"Self").ok_or_else(||unsupported("endnote has no identity"))?;
                        let range=ix.value(&a.raw,child,"EndnoteTextRange").ok_or_else(||unsupported("endnote has no body range"))?;
                        crate::ops::endnotes::remove_inserted(w,&reference,&range)?;
                    }
                }
            }
            s.atoms.splice(at..at+1,added);changed=true;
        }
        if changed {write(w,&part,&owner,&s)?;}
    }
    Ok(())
}
