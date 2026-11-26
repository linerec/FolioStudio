use crate::{Finding, Options, geom::{Doc,Placed}};
use folio_model::{Document,preflight::Settings};

fn ranges(s:&str)->Result<Vec<(usize,usize)>,String>{
    if s.trim().is_empty(){return Ok(vec![]);}
    s.split(',').map(|p|{
        let mut a=p.trim().split('-');
        let start=a.next().unwrap_or("").trim().parse::<usize>().map_err(|_|"invalid page range")?;
        let end=a.next().map(|v|v.trim().parse::<usize>()).transpose().map_err(|_|"invalid page range")?.unwrap_or(start);
        if start==0 || end<start || a.next().is_some(){return Err("invalid page range".into());}
        Ok((start-1,end-1))
    }).collect()
}
pub fn validate(s:&Settings)->Result<(),String>{
    if !["all","visible","visiblePrintable"].contains(&s.layers.as_str()){return Err("invalid layer scope".into());}
    ranges(&s.page_range)?;Ok(())
}
fn page(s:&Settings,p:usize)->bool{
    ranges(&s.page_range).is_ok_and(|r|r.is_empty()||r.iter().any(|&(a,b)|p>=a&&p<=b))
}
pub(crate) fn checked(p:&Placed<'_>,g:&Doc<'_>,_:&Document,o:&Options)->bool{
    let s=&o.scope;
    let layer=match s.layers.as_str(){"all"=>true,"visible"=>p.layer.is_none_or(|l|l.visible),_=>p.layer_prints};
    let visible=s.layers=="all" || p.visible;
    let printable=s.include_nonprinting || !p.nonprinting;
    let onpage=p.page.is_some() || o.include_pasteboard || s.include_pasteboard;
    let inrange=g.doc_page(p).is_none_or(|i|page(s,i));
    layer&&visible&&printable&&onpage&&inrange
}
pub(crate) fn finding(f:&Finding,g:&Doc<'_>,doc:&Document,o:&Options)->bool{
    if f.page.is_some_and(|p|!page(&o.scope,p)){return false;}
    // Default checks also report hidden content and informational glyph diagnostics.
    // Keep those existing diagnostics unless the caller explicitly selects a scope.
    if o.scope==Settings::default(){return true;}
    f.item.as_deref().and_then(|id|g.by_id.get(id)).is_none_or(|&i|checked(&g.items[i],g,doc,o))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn ranges_are_validated(){
        let mut s=Settings{page_range:"1,3-5".into(),..Default::default()};
        assert!(validate(&s).is_ok());assert!(page(&s,0));assert!(!page(&s,1));assert!(page(&s,4));
        for v in ["0","3-1","1-2-3","abc"]{s.page_range=v.into();assert!(validate(&s).is_err());}
    }
}
