use super::*;
use super::render::{self, Piece};
use folio_model::{hyperlinks::{CrossReferenceFormat, Destination}, toc::TocSource};

/// The character just before the source node `i` in its story (none at a paragraph's start): the last character of the text
/// written before it (`Content`, a return, a variable).
fn before(t: &str, ix: &Index, i: usize) -> Option<char> {
    let start = ix.nodes[i].start;
    let mut best: Option<(usize, Option<char>)> = None;
    for k in 0..ix.nodes.len() {
        let n = &ix.nodes[k];
        if n.end > start { continue; }
        let c = match ix.local(t, k) {
            "Content" => xml::unescape(&t[n.open_end..n.close_start]).chars().last(),
            "Br" | "ParagraphStyleRange" => None,
            "TextVariableInstance" | "CrossReferenceSource" => Some('\u{18}'),
            _ => continue,
        };
        if best.is_none_or(|(e, _)| n.end >= e) { best = Some((n.end, c)); }
    }
    best.and_then(|(_, c)| c)
}

pub(super) fn source(w: &mut Work<'_>, id: &str, dest: &Destination, fmt: &CrossReferenceFormat, locations: &[TocSource], force: bool) -> Result<(),EditError> {
    let (p,ix,i)=w.find(id)?;let t=w.text(&p)?.to_owned();let n=&ix.nodes[i];
    let descendants=(0..ix.nodes.len()).filter(|&c|ix.nodes[c].start>n.start&&ix.nodes[c].end<=n.end).collect::<Vec<_>>();
    let current=descendants.iter().filter_map(|&c|match ix.local(&t,c) {
        "Content"=>Some(xml::unescape(&t[ix.nodes[c].open_end..ix.nodes[c].close_start])),
        "TextVariableInstance"=>Some(format!("\u{18}{}",ix.value(&t,c,"ResultText").unwrap_or_default())),
        _=>None,
    }).collect::<String>();
    let prev=before(&t,&ix,i);
    let original=render::render_after(w,dest,fmt,locations,prev)?;
    let desired=original.iter().map(|x|match x {Piece::Text(s,_)=>s.clone(),Piece::Page(s,_)|Piece::Chapter(s,_)=>format!("\u{18}{s}")}).collect::<String>();
    if !force&&current==desired {return Ok(());}
    let parent=n.parent.filter(|&r|ix.local(&t,r)=="CharacterStyleRange");
    let first=parent.or_else(||descendants.iter().copied().find(|&c|ix.local(&t,c)=="CharacterStyleRange"));
    let mut effective=fmt.clone();
    if matches!(effective.character_style.as_str(),""|"n") {
        effective.character_style=first.and_then(|r|ix.value(&t,r,"AppliedCharacterStyle")).unwrap_or_else(||render::NO_STYLE.into());
    }
    let pieces=render::render_after(w,dest,&effective,locations,prev)?;
    if render::has_page(&pieces) {render::check_variable(w)?;}
    if render::has_chapter(&pieces) {render::check_chapter_variable(w)?;}
    let mut ids=descendants.iter().filter(|&&c|ix.local(&t,c)=="TextVariableInstance").filter_map(|&c|ix.value(&t,c,"Self")).collect::<Vec<_>>();
    let open=&t[n.start..n.open_end];let close="</CrossReferenceSource>";
    let single=match pieces.as_slice() {
        []=>Some(("<Content> </Content>".into(),effective.character_style.as_str(),false)),
        [Piece::Text(text,style)]=>Some((format!("<Content>{}</Content>",esc(text)),style.as_str(),false)),
        [Piece::Page(page,style)]=>Some((render::instance_xml(w,ids.pop(),page),style.as_str(),true)),
        _=>None,
    };
    if let (Some(r),Some((inner,style,page)))=(parent,&single)
        && ix.value(&t,r,"AppliedCharacterStyle").as_deref()==Some(*style) && (ix.attr(&t,r,"PageNumberType")==Some("TextVariable"))==*page {
        return w.splice(&p,vec![(n.open_end..n.close_start,inner.clone())]);
    }
    let replacement=if let Some((inner,style,page))=single {
        let attr=if page {" PageNumberType=\"TextVariable\""}else{""};
        format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\"{attr}>{open}{inner}{close}</CharacterStyleRange>",esc(style))
    }else{format!("{}{open}{}{close}",render::empty_leading_range(&pieces),render::runs(w,&pieces,&mut ids))};
    let (range,replacement)=if let Some(r)=parent {
        let r=&ix.nodes[r];let range_open=&t[r.start..r.open_end];
        let half=|s:&str|if s.trim().is_empty(){String::new()}else{format!("{range_open}{s}</CharacterStyleRange>")};
        (r.start..r.end,format!("{}{replacement}{}",half(&t[r.open_end..n.start]),half(&t[n.end..r.close_start])))
    }else{
        let preceding=n.parent.and_then(|r|ix.children(r).iter().copied().take_while(|&c|c!=i).last());
        let start=preceding.filter(|&c|ix.local(&t,c)=="CharacterStyleRange"&&ix.nodes[c].empty&&t[ix.nodes[c].end..n.start].trim().is_empty()).map_or(n.start,|c|ix.nodes[c].start);
        (start..n.end,replacement)
    };
    w.splice(&p,vec![(range,replacement)])
}
