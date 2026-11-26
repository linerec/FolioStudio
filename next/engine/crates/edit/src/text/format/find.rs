//! Literal text find/change. Matches use story scalar indexes, and replace-all is
//! one editor command, so every replaced story rolls back together on failure.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FindQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub scope: String,
    pub story: Option<String>,
    pub start: usize,
    pub end: Option<usize>,
    pub props: BTreeMap<String, Value>,
    /// Find/Change's search direction (tools-c2): false = forward (story order), true = backward; the matches come in that order
    pub backward: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoundText {
    pub story: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || matches!(c, '\u{0300}'..='\u{036f}' | '\u{0591}'..='\u{05bd}' | '\u{064b}'..='\u{065f}')
}
fn same(a: char, b: char, sensitive: bool) -> bool {
    a == b || !sensitive && a.to_lowercase().eq(b.to_lowercase())
}
pub(crate) fn search(w: &mut Work<'_>, q: &FindQuery) -> Result<Vec<FoundText>, EditError> {
    if !matches!(q.scope.as_str(), "" | "story" | "document" | "selection") {
        return Err(EditError::new("invalid", "invalid find scope"));
    }
    if q.text.is_empty() && q.props.is_empty() {
        return Err(EditError::new("invalid", "find text or formatting required"));
    }
    let cat = Catalog::read(&w.ed.parts)?;
    let wanted: Vec<_> = q.props.iter().map(|(k,v)| {
        if k == "paragraphStyle" || k == "characterStyle" {
            let para = k == "paragraphStyle";
            let tag = if para {"ParagraphStyle"} else {"CharacterStyle"};
            Ok((para, format!("Applied{tag}"), attr(cat.id(tag, v.as_str().ok_or_else(||EditError::new("invalid","style required"))?)?)))
        } else {direct(k,v)}
    }).collect::<Result<_,EditError>>()?;
    let ids: Vec<String> = if q.scope == "document" {
        let mut ids = Vec::new();
        for (part,t) in &w.ed.parts {
            if !part.starts_with("Stories/") {continue;}
            let ix = Index::scan(t).map_err(|e|EditError::new("xml",e))?;
            ids.extend((0..ix.nodes.len()).filter(|&i|ix.local(t,i)=="Story").filter_map(|i|ix.value(t,i,"Self")));
        }
        ids
    } else {
        vec![q.story.clone().ok_or_else(||EditError::new("invalid","story required"))?]
    };
    let needle: Vec<_> = q.text.chars().collect();
    let mut found = Vec::new();
    for id in ids {
        let (part,ix,i,id) = locate(w,&id)?;
        let s = Story::read(w.text(&part)?,&ix,i)?;
        let (start,end) = if q.scope=="selection" {
            (q.start,q.end.ok_or_else(||EditError::new("invalid","selection end required"))?)
        } else {(0,s.atoms.len())};
        validate(&s,start,end)?;
        let mut cache = BTreeMap::new();
        let mut matches_format = Vec::with_capacity(s.atoms.len());
        for a in &s.atoms {
            let key = (a.p,a.c);
            let yes = if let Some(&v)=cache.get(&key) {v} else {
                let pa=style_attrs(&s.paragraphs[a.p]);let ca=style_attrs(&s.characters[a.c]);
                let mut effective=cat.character(&pa,&ca)?;effective.extend(ca);
                effective.insert("AppliedParagraphStyle".into(),attr(val(&pa,"AppliedParagraphStyle")));
                let v=wanted.iter().all(|(_,k,v)|effective.get(k).is_some_and(|a|equal(a,v)));
                cache.insert(key,v);v
            };
            matches_format.push(yes);
        }
        let mut at=start;
        while at<end {
            let length=needle.len().max(1);
            let to=at+length;
            let text_matches=to<=end && needle.iter().enumerate().all(|(k,&c)|same(s.atoms[at+k].ch,c,q.case_sensitive));
            let boundary=!q.whole_word || (at==0 || !word(s.atoms[at-1].ch)) && (to==s.atoms.len() || to<s.atoms.len() && !word(s.atoms[to].ch));
            if text_matches && boundary && matches_format[at..to].iter().all(|&v|v) {
                let mut to=to;
                if needle.is_empty() {
                    while to<end && matches_format[to] {to+=1;}
                }
                found.push(FoundText {story:id.clone(),start:at,end:to,text:s.atoms[at..to].iter().map(|a|a.ch).collect()});
                at=to;
            } else {at+=1;}
        }
    }
    Ok(found)
}
pub(crate) fn replace(w:&mut Work<'_>,q:&FindQuery,text:Option<&str>,props:&BTreeMap<String,Value>)->Result<(),EditError> {
    let matches=search(w,q)?;
    for m in matches.into_iter().rev() {
        let end = if let Some(text)=text {
            let (part,ix,i,_)=locate(w,&m.story)?;
            let old=Story::read(w.text(&part)?,&ix,i)?;
            // find-mixed / find-insert-return: native Change Text retains the
            // formatting at each character position; added characters take the
            // last matched character's formatting. This differs from typing.
            let formats:Vec<_>=old.atoms[m.start..m.end].iter().map(|a|old.characters[a.c].clone()).collect();
            delete(w,&m.story,m.start,m.end)?;
            insert(w,&m.story,m.start,text)?;
            let end=m.start+text.chars().count();
            if end>m.start {
                let (part,ix,i,_)=locate(w,&m.story)?;
                let mut changed=Story::read(w.text(&part)?,&ix,i)?;
                let base=changed.characters.len();changed.characters.extend(formats);
                for (k,a) in changed.atoms[m.start..end].iter_mut().enumerate() {a.c=base+k.min(m.end-m.start-1);}
                write(w,&part,&m.story,&changed)?;
                w.ed.text_state.remove(&m.story);
            }
            end
        } else {m.end};
        let mut direct_props=props.clone();
        for (key,para) in [("paragraphStyle",true),("characterStyle",false)] {
            if let Some(style)=direct_props.remove(key) {
                apply_style(w,&m.story,m.start,end,style.as_str().ok_or_else(||EditError::new("invalid","style required"))?,para,false)?;
            }
        }
        if !direct_props.is_empty() {format(w,&m.story,m.start,end,&direct_props)?;}
    }
    Ok(())
}
impl crate::Editor {
    pub fn find_text(&mut self,q:&FindQuery)->Result<Vec<FoundText>,EditError> {
        let mut found=search(&mut Work{ed:self,splices:Vec::new(),recompose:Vec::new(),created:Vec::new()},q)?;
        if q.backward {found.reverse();}
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command,Editor};
    fn editor()->Editor {
        let story=|id:&str,text:&str|format!(r#"<Story Self="{id}"><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/P"><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/C"><Content>{text}</Content></CharacterStyleRange></ParagraphStyleRange></Story>"#);
        Editor::new(BTreeMap::from([("Stories/a.xml".into(),story("a","한글 Cat scatter CAT 😀cat")),("Stories/b.xml".into(),story("b","Cat"))]))
    }
    #[test]
    fn find_indexes_are_scalars_and_whole_words_use_the_story_outside_selection() {
        let mut e=editor();let q=FindQuery{text:"cat".into(),scope:"document".into(),whole_word:true,..Default::default()};
        let found=e.find_text(&q).unwrap();
        assert_eq!(found.iter().map(|m|(m.story.as_str(),m.start,m.end)).collect::<Vec<_>>(),[("a",3,6),("a",15,18),("a",20,23),("b",0,3)]);
        let back=e.find_text(&FindQuery{backward:true,..q.clone()}).unwrap();
        assert_eq!(back.iter().map(|m|(m.story.as_str(),m.start)).collect::<Vec<_>>(),[("b",0),("a",20),("a",15),("a",3)]);
        let q=FindQuery{scope:"selection".into(),story:Some("a".into()),start:8,end:Some(11),..q};
        assert!(e.find_text(&q).unwrap().is_empty());
    }
    #[test]
    fn replacing_a_document_is_one_undo_and_invalid_format_rolls_every_story_back() {
        let mut e=editor();let before=e.parts().clone();
        let q=FindQuery{text:"Cat".into(),scope:"document".into(),whole_word:true,..Default::default()};
        let cmd=Command::FindReplace{query:q,text:Some("고양이".into()),props:BTreeMap::new()};
        let out=e.apply("all",std::slice::from_ref(&cmd)).unwrap();assert_eq!(out.recompose.len(),2);
        assert!(contents(e.part("Stories/a.xml").unwrap(),"a").unwrap().contains("scatter"));
        e.undo().unwrap();assert_eq!(&before,e.parts());
        let Command::FindReplace{query,text,..}=cmd else {unreachable!()};
        let bad=Command::FindReplace{query,text,props:BTreeMap::from([("pointSize".into(),serde_json::json!(-3))])};
        assert!(e.apply("bad",&[bad]).is_err());assert_eq!(&before,e.parts());
        assert!(e.redo().is_some());
    }
}
