//! Authored text edits preserve existing revision XML and its visible character space.
use super::*;
use crate::xml::escape;

pub(super) fn enabled(w: &mut Work<'_>, id: &str) -> Result<bool,EditError> {
    let (part,ix,_,owner)=locate(w,id)?;
    let t=w.text(&part)?;
    Ok(ix.nodes.iter().enumerate().find(|(i,_)|ix.value(t,*i,"Self").as_deref()==Some(&owner))
        .is_some_and(|(i,_)|ix.value(t,i,"TrackChanges").as_deref()==Some("true")))
}

pub(super) fn require_untracked(w: &mut Work<'_>, id: &str) -> Result<(),EditError> {
    if enabled(w,id)? {Err(unsupported("tracked text requires the current user's name and date"))} else {Ok(())}
}

fn change(a: &Atom) -> bool {a.raw.starts_with("<Change ")}
fn attribute(a: &Atom,key:&str) -> Result<String,EditError> {
    let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
    Ok(ix.value(&a.raw,0,key).unwrap_or_default())
}
fn body(a: &Atom) -> Result<Vec<Atom>,EditError> {
    let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
    let mut shell=Story{range:0..0,separator:String::new(),paragraphs:Vec::new(),characters:Vec::new(),atoms:Vec::new(),empty:(0,0),trailing:None,tail_range:false};
    let atoms=changes::insertion(&mut shell,a,&ix)?;
    Ok(atoms)
}
pub(super) fn inline(atoms:&[Atom])->String {
    let (mut raw,mut text)=(String::new(),String::new());
    for a in atoms {if a.content{text.push_str(&a.raw);}else{if !text.is_empty(){raw.push_str(&format!("<Content>{text}</Content>"));text.clear();}raw.push_str(&a.raw);}}
    if !text.is_empty(){raw.push_str(&format!("<Content>{text}</Content>"));}raw
}
pub(super) fn with_body(a: &Atom,atoms:&[Atom]) -> Atom {
    let open=a.raw.find('>').unwrap()+1;
    Atom{raw:format!("{}{}</Change>",&a.raw[..open],inline(atoms)),..a.clone()}
}

/// Split an insertion record at a visible offset. Deleted records have zero width.
pub(super) fn boundary(s:&mut Story,position:usize) -> Result<usize,EditError> {
    let mut cursor=0;
    for i in 0..s.atoms.len() {
        if cursor==position {return Ok(i);}
        let a=&s.atoms[i];
        let len=if change(a) {match attribute(a,"ChangeType")?.as_str() {
            "DeletedText"=>0,"InsertedText"=>body(a)?.len(),_=>return Err(unsupported("unsupported change type")),
        }} else {1};
        if cursor+len>position {
            let atoms=body(a)?;let (left,right)=atoms.split_at(position-cursor);
            let halves=[with_body(a,left),with_body(a,right)];s.atoms.splice(i..i+1,halves);return Ok(i+1);
        }
        cursor+=len;
    }
    if cursor==position {Ok(s.atoms.len())} else {Err(EditError::new("invalid","text position outside story"))}
}

pub(super) fn new_atom(p:usize,c:usize,kind:&str,user:&str,date:&str,reference:&str,body:&str) -> Atom {
    Atom{p,c,ch:'\u{feff}',content:false,raw:format!("<Change Date=\"{}\" ChangeType=\"{kind}\" UserName=\"{}\" AppliedDocumentUser=\"{}\">{body}</Change>",escape(date),escape(user),escape(reference))}
}

fn metadata(a:&Atom)->Result<std::collections::BTreeMap<String,String>,EditError>{
    let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
    Ok(ix.attrs(&a.raw,0).into_iter().filter_map(|v|{let key=&a.raw[v.name.0..v.name.1];(key!="Date").then(||(key.to_owned(),unescape(&a.raw[v.value.0..v.value.1])))}).collect())
}

pub(super) fn join_insertions(s:&mut Story,user:&str) -> Result<(),EditError> {
    let mut i=1;
    while i<s.atoms.len() {
        let (a,b)=(&s.atoms[i-1],&s.atoms[i]);
        if change(a)&&change(b)&&attribute(a,"UserName")?.trim_start_matches("$ID/")==user.trim_start_matches("$ID/")&&metadata(a)?==metadata(b)?
            &&attribute(a,"UserName")?==attribute(b,"UserName")?&&s.characters[a.c].key==s.characters[b.c].key&&s.paragraphs[a.p].key==s.paragraphs[b.p].key {
            let kind=attribute(a,"ChangeType")?;
            let mut joined=if attribute(a,"Date")?<=attribute(b,"Date")?{a.clone()}else{b.clone()};
            if kind=="InsertedText" {let mut atoms=body(a)?;atoms.extend(body(b)?);joined=with_body(&joined,&atoms);}
            else if kind=="DeletedText" {
                let mut fragment=deleted_story(a)?;let inner=deleted_story(b)?;append(&mut fragment,inner);
                let open=joined.raw.find('>').unwrap()+1;
                joined.raw=format!("{}{}</Change>",&joined.raw[..open],fragment.serialize());
            } else {return Err(unsupported("unsupported change type"));}
            s.atoms[i-1]=joined;s.atoms.remove(i);
        } else {i+=1;}
    }
    Ok(())
}

fn deleted_story(a:&Atom)->Result<Story,EditError>{
    let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
    Story::read(&a.raw,&ix,0)
}
fn append(s:&mut Story,mut other:Story) {
    let (p,c)=(s.paragraphs.len(),s.characters.len());
    s.paragraphs.extend(other.paragraphs);s.characters.extend(other.characters);
    for a in &mut other.atoms{a.p+=p;a.c+=c;}s.atoms.extend(other.atoms);
}

pub(crate) fn insert(w:&mut Work<'_>,id:&String,at:&usize,text:&String,user:&String,date:&String) -> Result<(),EditError> {
    let tracking=enabled(w,id)?;
    if !tracking {let (part,_,_,_)=locate(w,id)?;if !w.text(&part)?.contains("<Change "){return super::insert(w,id,*at,text);}}
    if text.chars().any(|c|c<' '&&c!='\t'&&c!='\r') {return Err(unsupported("tracked input supports text, tabs and paragraph breaks"));}
    let (part,ix,i,owner)=locate(w,id)?;
    let mut s=Story::read_mode(w.text(&part)?,&ix,i,true)?;
    let pos=boundary(&mut s,*at)?;
    if text.is_empty(){return Ok(());}
    let (p,c)=typing_at(w,&owner,&mut s,pos,*at);
    let typed:Vec<Atom>=text.chars().map(|ch|Atom{p,c,ch,content:ch!='\r',raw:raw_of(ch)}).collect();
    if tracking {
        let reference=author::reference(w,user)?;
        s.atoms.insert(pos,new_atom(p,c,"InsertedText",user,date,&reference,&inline(&typed)));
    } else {
        s.atoms.splice(pos..pos,typed);
    }
    join_insertions(&mut s,user)?;
    write(w,&part,&owner,&s)?;
    w.ed.text_state.insert(owner,InsertionStyle{at:at+text.chars().count(),character:s.characters[c].clone()});
    Ok(())
}

pub(crate) fn delete(w:&mut Work<'_>,id:&String,start:&usize,end:&usize,user:&String,date:&String) -> Result<(),EditError> {
    let tracking=enabled(w,id)?;
    if !tracking {let (part,_,_,_)=locate(w,id)?;if !w.text(&part)?.contains("<Change "){return super::delete(w,id,*start,*end);}}
    replace(w,id,*start,*end,user,date,None)
}

pub(super) fn replace(w:&mut Work<'_>,id:&str,start:usize,end:usize,user:&str,date:&str,note:Option<&str>)->Result<(),EditError>{
    let tracking=enabled(w,id)?;
    if start>end{return Err(EditError::new("invalid","reversed deletion range"));}
    let (part,ix,i,owner)=locate(w,id)?;
    let mut s=Story::read_mode(w.text(&part)?,&ix,i,true)?;
    boundary(&mut s,end)?;let first=boundary(&mut s,start)?;let last=boundary(&mut s,end)?;
    if first==last{return Ok(());}
    for a in s.atoms[first..last].iter().filter(|a|!a.content) {
        let ix=Index::scan(&a.raw).map_err(|e|EditError::new("xml",e))?;
        if (0..ix.nodes.len()).any(|i|ix.local(&a.raw,i)=="Endnote") {
            return Err(unsupported("endnote reference deletion requires linked body editing"));
        }
    }
    let (p,c)=(s.atoms[first].p,s.atoms[first].c);
    if !tracking {
        if s.atoms[first..last].iter().any(|a|!a.content&&!change(a)&&!a.raw.starts_with("<Note")&&a.ch!='\r'){return Err(unsupported("deletion of inline objects with change records"));}
        breaks_share_style(&s,&s.atoms[first..last],last,p)?;
        s.atoms.splice(first..last,note.map(|raw|Atom{p,c,ch:'\u{feff}',content:false,raw:raw.into()}));join_insertions(&mut s,user)?;
        write(w,&part,&owner,&s)?;
        w.ed.text_state.insert(owner,InsertionStyle{at:start,character:s.characters[c].clone()});
        return Ok(());
    }
    let retained_insertion=if last==first+1 && change(&s.atoms[first])
        && attribute(&s.atoms[first],"ChangeType")?=="InsertedText"
        && attribute(&s.atoms[first],"UserName")?.trim_start_matches("$ID/")==user.trim_start_matches("$ID/") {
        Some(s.atoms[first].clone())
    } else {None};
    let deleted_record=|a:&Atom|change(a)&&attribute(a,"ChangeType").is_ok_and(|k|k=="DeletedText");
    // measured (changes-merged-*): another user's deleted record that the new deletion follows or crosses joins
    // the new record, which then belongs to the new user (the owner is that of the rightmost piece)
    let mut from=first;
    let mut removed=Vec::new();
    if note.is_none() && first>0 && deleted_record(&s.atoms[first-1])
        && attribute(&s.atoms[first-1],"UserName")?.trim_start_matches("$ID/")!=user.trim_start_matches("$ID/") {
        let mut inner=deleted_story(&s.atoms[first-1])?;let (po,co)=(s.paragraphs.len(),s.characters.len());
        s.paragraphs.extend(inner.paragraphs);s.characters.extend(inner.characters);
        for a in &mut inner.atoms{a.p+=po;a.c+=co;}removed.extend(inner.atoms);from=first-1;
    }
    #[allow(clippy::unnecessary_to_owned, reason = "copy the entire atom range before extending the story styles")]
    for a in s.atoms[first..last].to_vec() {
        if change(&a) {
            let kind=attribute(&a,"ChangeType")?;let own=attribute(&a,"UserName")?.trim_start_matches("$ID/")==user.trim_start_matches("$ID/");
            if kind=="DeletedText" {
                let mut inner=deleted_story(&a)?;let (po,co)=(s.paragraphs.len(),s.characters.len());
                s.paragraphs.extend(inner.paragraphs);s.characters.extend(inner.characters);
                for a in &mut inner.atoms{a.p+=po;a.c+=co;}removed.extend(inner.atoms);continue;
            }
            if kind!="InsertedText"{return Err(unsupported("unsupported change type"));}
            if own{continue;}
            removed.extend(body(&a)?);
        } else if a.content || a.raw.starts_with("<Note") || a.ch=='\r' {removed.push(a.clone());}
        else {return Err(unsupported("tracked deletion of inline objects is not yet supported"));}
    }
    breaks_share_style(&s,&removed,last,p)?;
    let mut last=last;
    let mut replacement=if removed.is_empty(){Vec::new()}else{
        let mut fragment=Story{range:0..0,separator:s.separator.clone(),paragraphs:s.paragraphs.clone(),characters:s.characters.clone(),atoms:removed,empty:(p,c),trailing:None,tail_range:false};
        // measured (changes-absorbed-*): text deleted right before another user's deleted record joins that
        // record, which keeps its author and date
        let next=s.atoms.get(last).filter(|n|note.is_none()&&change(n)&&attribute(n,"ChangeType").is_ok_and(|k|k=="DeletedText")
            &&attribute(n,"UserName").is_ok_and(|u|u.trim_start_matches("$ID/")!=user.trim_start_matches("$ID/"))).cloned();
        if let Some(next)=next {
            author::reference(w,user)?; // InDesign still registers the deleting user in the document
            append(&mut fragment,deleted_story(&next)?);
            let open=next.raw.find('>').unwrap()+1;
            last+=1;
            vec![Atom{raw:format!("{}{}</Change>",&next.raw[..open],fragment.serialize()),..next}]
        } else {
            let reference=author::reference(w,user)?;
            vec![new_atom(p,c,"DeletedText",user,date,&reference,&fragment.serialize())]
        }
    };
    if let Some(raw)=note {
        let reference=author::reference(w,user)?;
        replacement.push(if let Some(original)=retained_insertion {
            with_body(&original,&[Atom{p,c,ch:'\u{feff}',content:false,raw:raw.into()}])
        } else {new_atom(p,c,"InsertedText",user,date,&reference,raw)});
    }
    s.atoms.splice(from..last,replacement);join_insertions(&mut s,user)?;
    write(w,&part,&owner,&s)?;
    w.ed.text_state.insert(owner,InsertionStyle{at:start,character:s.characters[c].clone()});
    Ok(())
}

/// A deleted paragraph break leaves one paragraph range only while both sides share a style (measured in
/// changes-break-delete and changes-off-break-delete); other merges stay with InDesign's own merge rule.
fn breaks_share_style(s:&Story,atoms:&[Atom],next:usize,p:usize)->Result<(),EditError>{
    if !atoms.iter().any(|a|a.ch=='\r'&&!a.content) {return Ok(());}
    let touched=atoms.iter().map(|a|a.p).chain(s.atoms.get(next).map(|a|a.p));
    if touched.into_iter().any(|q|s.paragraphs[q].key!=s.paragraphs[p].key) {return Err(unsupported("deleting a paragraph break between different paragraph styles"));}
    Ok(())
}

pub(super) fn insert_note(w:&mut Work<'_>,id:&str,at:usize,raw:&str,user:&str,date:&str)->Result<(),EditError>{
    let tracking=enabled(w,id)?;
    let (part,ix,i,owner)=locate(w,id)?;
    let mut s=Story::read_mode(w.text(&part)?,&ix,i,true)?;
    let pos=boundary(&mut s,at)?;
    let (p,c)=typing_at(w,&owner,&mut s,pos,at);
    let atom=if tracking {let reference=author::reference(w,user)?;new_atom(p,c,"InsertedText",user,date,&reference,raw)}
        else {Atom{p,c,ch:'\u{feff}',content:false,raw:raw.into()}};
    s.atoms.insert(pos,atom);join_insertions(&mut s,user)?;
    write(w,&part,&owner,&s)
}
