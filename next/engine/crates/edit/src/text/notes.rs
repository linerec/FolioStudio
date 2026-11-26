//! Notes are opaque story atoms with independently styled editable text.
use super::*;
use crate::xml::escape;

fn header(user: &str, date: &str, reference: &str) -> String {
    format!("<Note Collapsed=\"false\" CreationDate=\"{}\" ModificationDate=\"{}\" UserName=\"{}\" AppliedDocumentUser=\"{}\">", escape(date), escape(date), escape(user),escape(reference))
}

fn plain(text: &str) -> String {
    let mut body = String::new();
    for (i, line) in text.split('\r').enumerate() {
        if i > 0 { body.push_str("<Br />"); }
        if !line.is_empty() { body.push_str(&format!("<Content>{}</Content>", line.chars().map(raw_of).collect::<String>())); }
    }
    format!("<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/[No paragraph style]\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\">{body}</CharacterStyleRange></ParagraphStyleRange>")
}

fn note_at(s: &Story, index: usize) -> Result<usize, EditError> {
    s.atoms.iter().enumerate().filter(|(_, a)| a.raw.starts_with("<Note ") || a.raw.starts_with("<Note>"))
        .nth(index).map(|(i, _)| i).ok_or_else(|| EditError::new("notFound", format!("note {index}")))
}

fn attr(raw: &mut String, key: &str, value: &str) -> Result<(), EditError> {
    let ix = Index::scan(raw).map_err(|e| EditError::new("xml", e))?;
    if let Some(a) = ix.attrs(raw, 0).into_iter().find(|a| &raw[a.name.0..a.name.1] == key) {
        raw.replace_range(a.value.0..a.value.1, &escape(value));
    } else {
        let at = ix.nodes[0].open_end - 1;
        raw.insert_str(at, &format!(" {key}=\"{}\"", escape(value)));
    }
    Ok(())
}

pub(crate) fn insert(w: &mut Work<'_>, id: &String, at: &usize, text: &String, user: &String, date: &String) -> Result<(), EditError> {
    let reference=author::reference(w,user)?;
    let raw = format!("{}{}</Note>", header(user, date,&reference), plain(text));
    let ix = Index::scan(&raw).map_err(|e|EditError::new("xml",e))?;
    let mut s = Story::read(&raw,&ix,0)?;
    let (p,c) = format::note_styles(w,&s.paragraphs[0],&s.characters[0])?;
    s.paragraphs[0]=p; s.characters[0]=c;
    let raw = format!("{}{}</Note>",header(user,date,&reference),s.serialize());
    track_edit::insert_note(w, id, *at, &raw,user,date)
}

pub(crate) fn convert(w: &mut Work<'_>, id: &String, start: &usize, end: &usize, user: &String, date: &String) -> Result<(), EditError> {
    let tracking=track_edit::enabled(w,id)?;
    let (part, ix, i, _) = locate(w, id)?;
    let revisions=tracking || w.text(&part)?.contains("<Change ");
    let mut s = if revisions {changes::visible(Story::read_mode(w.text(&part)?, &ix, i,true)?)?}
        else {Story::read(w.text(&part)?, &ix, i)?};
    if start >= end || *end > s.atoms.len() { return Err(EditError::new("invalid", "select text to convert")); }
    if s.atoms[*start..*end].iter().any(|a| !a.content && a.ch != '\r') { return Err(unsupported("note conversion requires text")); }
    let ch = |i:usize| s.atoms.get(i).map(|a|a.ch);
    let before = if *start==0 {None} else {ch(start-1)};
    let after = ch(*end);
    let single_after = after==Some(' ') && ch(end+1)!=Some(' ');
    let single_before = before==Some(' ') && (*start<2 || ch(start-2)!=Some(' '));
    let replace_end = if (before.is_none() || before==Some(' ')) && single_after {end+1} else {*end};
    let replace_start = if replace_end==*end && single_before && (after.is_none() || after==Some(' ')) {start-1} else {*start};
    let preserve_space = ch(*start)==Some(' ') && ch(end-1)==Some(' ');
    s.atoms = s.atoms[*start..*end].to_vec(); s.trailing = None;
    let reference=author::reference(w,user)?;
    let raw = format!("{}{}</Note>", header(user, date,&reference), s.serialize());
    if revisions {
        track_edit::replace(w,id,replace_start,replace_end,user,date,Some(&raw))?;
        if preserve_space {track_edit::insert(w,id,&replace_start,&" ".into(),user,date)?;}
    } else {
        replace_with_node(w, id, replace_start, replace_end, '\u{feff}', &raw)?;
        if preserve_space { super::insert(w,id,replace_start," ")?; }
    }
    Ok(())
}

pub(crate) fn update(w: &mut Work<'_>, id: &String, index: &usize, text: &String, date: &String) -> Result<(), EditError> {
    note_nodes::edit(w, id, *index, |raw| {
        let ix = Index::scan(raw).map_err(|e| EditError::new("xml", e))?;
        let mut inner = Story::read(raw, &ix, 0)?;
        let (p,c) = inner.atoms.first().map(|a| (a.p,a.c)).unwrap_or(inner.empty);
        inner.atoms = text.chars().map(|ch| Atom{p,c,ch,content:ch!='\r',raw:raw_of(ch)}).collect();
        inner.trailing=None;
        raw.replace_range(inner.range.clone(), &inner.serialize());
        attr(raw, "ModificationDate", date)
    })
}

/// Type ▸ Notes ▸ Split Note (InDesign 20.5.5.119, rules/notes3-split.ko.md): the note keeps the text before `at` and
/// keeps all its own attributes (author, dates, collapsed); the text from `at` on goes into a new note placed right
/// after it, made now by `user` (author and both dates). `at` counts the note's characters (a return is one) and must be
/// inside the text: InDesign leaves Split Note off at the very start, at the very end and with a range selected.
/// Reports the new note as `<story>#note<index>`.
pub(crate) fn split(w: &mut Work<'_>, id: &String, index: &usize, at: &usize, user: &String, date: &String) -> Result<(), EditError> {
    track_edit::require_untracked(w, id)?;
    let (part, ix, i, owner) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    let pos = note_at(&s, *index)?;
    let raw = s.atoms[pos].raw.clone();
    let nix = Index::scan(&raw).map_err(|e| EditError::new("xml", e))?;
    let inner = Story::read(&raw, &nix, 0)?;
    if inner.atoms.iter().any(|a| !a.content && a.ch != '\r') { return Err(unsupported("splitting a note that contains objects")); }
    if *at == 0 || *at >= inner.atoms.len() { return Err(EditError::new("invalid", "Split Note needs a caret inside the note's text")); }
    let (mut head, mut tail) = (inner.clone(), inner);
    head.atoms.truncate(*at);
    tail.atoms.drain(..*at);
    head.trailing = None;
    tail.trailing = None;
    let mut first = raw.clone();
    first.replace_range(head.range.clone(), &head.serialize());
    let reference = author::reference(w, user)?;
    let second = format!("{}{}</Note>", header(user, date, &reference), tail.serialize());
    let mut marker = s.atoms[pos].clone();
    s.atoms[pos].raw = first;
    marker.raw = second;
    s.atoms.insert(pos + 1, marker);
    write(w, &part, &owner, &s)?;
    w.created.push(format!("{owner}#note{}", index + 1));
    Ok(())
}

fn edit(w: &mut Work<'_>, id: &str, f: impl FnOnce(&mut Story) -> Result<(), EditError>) -> Result<(), EditError> {
    let (part, ix, i, owner) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    f(&mut s)?;
    write(w, &part, &owner, &s)
}

pub(crate) fn remove(w: &mut Work<'_>, id: &String, index: &usize) -> Result<(), EditError> {
    track_edit::require_untracked(w,id)?;
    edit(w, id, |s| { let at = note_at(s, *index)?; s.atoms.remove(at); Ok(()) })
}

pub(crate) fn collapse(w: &mut Work<'_>, id: &String, index: &usize, collapsed: &bool) -> Result<(), EditError> {
    note_nodes::edit(w, id, *index, |raw| attr(raw,"Collapsed",if *collapsed {"true"}else{"false"}))
}

pub(crate) fn collapse_all(w: &mut Work<'_>, id: &String, collapsed: &bool) -> Result<(), EditError> {
    let (part,ix,i,_)=locate(w,id)?;
    let count=note_nodes::nodes(w.text(&part)?,&ix,i).len();
    for index in 0..count {collapse(w,id,&index,collapsed)?;}Ok(())
}

pub(crate) fn remove_all(w: &mut Work<'_>, id: &String, document: &bool) -> Result<(), EditError> {
    let ids = if *document { w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect() } else {vec![id.clone()]};
    for owner in ids {
        track_edit::require_untracked(w,&owner)?;
        let (part,_,_,_) = locate(w,&owner)?;
        if w.text(&part)?.contains("<Note") {
            edit(w,&owner,|s| {s.atoms.retain(|a|!a.raw.starts_with("<Note"));Ok(())})?;
        }
    }
    Ok(())
}

pub(crate) fn to_text(w: &mut Work<'_>, id: &String, index: &usize) -> Result<(), EditError> {
    track_edit::require_untracked(w,id)?;
    let (part, ix, i, owner) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    let at = note_at(&s, *index)?;
    let marker = s.atoms[at].clone();
    let before=at.checked_sub(1).map(|i|s.atoms[i].ch);
    let after=s.atoms.get(at+1).map(|a|a.ch);
    let inner=expand(w,&mut s,&marker,&marker.raw,before,after)?;
    s.atoms.splice(at..at+1,inner);
    write(w,&part,&owner,&s)
}

pub(super) fn expand(w:&mut Work<'_>,s:&mut Story,marker:&Atom,raw:&str,before:Option<char>,after:Option<char>)->Result<Vec<Atom>,EditError>{
        let ix = Index::scan(raw).map_err(|e| EditError::new("xml", e))?;
        let mut inner = Story::read(raw, &ix, 0)?;
        let po=s.paragraphs.len(); let co=s.characters.len();
        s.paragraphs.extend(inner.paragraphs);
        for c in inner.characters { s.characters.push(format::rebase_character(w,&s.paragraphs[marker.p],&c)?); }
        for a in &mut inner.atoms { a.p+=po; a.c+=co; }
        if !inner.atoms.is_empty() {
            let space = || Atom {p:marker.p,c:marker.c,ch:' ',content:true,raw:" ".into()};
            if before.is_some_and(|c|!c.is_whitespace()) && !inner.atoms[0].ch.is_whitespace() { inner.atoms.insert(0,space()); }
            if after.is_some_and(|c|!c.is_whitespace()) && !inner.atoms.last().unwrap().ch.is_whitespace() { inner.atoms.push(space()); }
            // Joining a paragraph retains the receiving paragraph style.
            for a in &mut inner.atoms { a.p=marker.p; }
        }
    Ok(inner.atoms)
}
