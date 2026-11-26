use super::*;

/// Insert an inline node or wrap one continuous Content range. Refuse unsupported
/// boundaries instead of flattening style ranges or nested objects.
pub(super) fn wrap(w: &mut Work<'_>, owner: &str, start: usize, end: usize, open: &str, close: &str, replacement: Option<&str>) -> Result<(),EditError> {
    let story=crate::text::story_of(w,owner)?;
    let (p,ix,i)=w.find(&story)?;let t=w.text(&p)?;
    let mut at=0;let mut chosen=None;
    scan(t,&ix,i,&mut at,start,end,&mut chosen)?;
    let (node,a,b)=chosen.ok_or_else(||EditError::new("unsupported","select one continuous text range without nested objects"))?;
    let n=&ix.nodes[node];
    let raw=&t[n.open_end..n.close_start];
    let inside=replacement.map(esc).unwrap_or_else(||raw[a..b].to_owned());
    let content=|s:&str|if s.is_empty(){String::new()}else{format!("<Content>{s}</Content>")};
    let body=format!("{}{open}{}{close}{}",content(&raw[..a]),content(&inside),content(&raw[b..]));
    w.splice(&p,vec![(n.start..n.end,body)])?;w.recompose(&story);Ok(())
}
fn scan(t: &str,ix: &Index,i: usize,at: &mut usize,start: usize,end: usize,chosen: &mut Option<(usize,usize,usize)>) -> Result<(),EditError> {
    let tag=ix.local(t,i);let n=&ix.nodes[i];
    if tag=="Properties" || tag=="HiddenText" || (tag=="Change" && ix.value(t,i,"ChangeType").as_deref()==Some("DeletedText")) {return Ok(());}
    if tag=="Content" {
        let raw=&t[n.open_end..n.close_start];let bounds=boundaries(raw)?;let len=bounds.len()-1;
        if start>=*at&&end<=*at+len&&start<=end { *chosen=Some((i,bounds[start-*at],bounds[end-*at])); }
        *at+=len;return Ok(());
    }
    if matches!(tag,"Br"|"Table"|"Footnote"|"Endnote"|"Note"|"TextVariableInstance"|"Rectangle"|"Oval"|"Polygon"|"GraphicLine"|"Group"|"TextFrame") {*at+=1;return Ok(());}
    if tag=="PageReference" && ix.children(i).iter().all(|&c|ix.local(t,c)=="Properties") {*at+=1;return Ok(());}
    if matches!(tag,"HyperlinkTextSource"|"CrossReferenceSource") {
        // Count existing linked text but never nest a new link in it.
        let mut ignored=None;
        for &c in ix.children(i){scan(t,ix,c,at,usize::MAX,usize::MAX,&mut ignored)?;}return Ok(());
    }
    for &c in ix.children(i){scan(t,ix,c,at,start,end,chosen)?;}Ok(())
}
fn boundaries(raw: &str) -> Result<Vec<usize>,EditError> {
    let mut out=vec![0];let mut at=0;
    while at<raw.len() {
        let rest=&raw[at..];
        let n=if rest.starts_with('&'){rest.find(';').ok_or_else(||invalid("invalid entity"))?+1}
            else if rest.starts_with("<?"){rest.find("?>").ok_or_else(||invalid("invalid processing instruction"))?+2}
            else{rest.chars().next().expect("at is below raw.len() and advances by complete UTF-8 characters or ASCII delimiters").len_utf8()};
        at+=n;out.push(at);
    }Ok(out)
}

/// Put `make(range)` between the two halves of the character range holding the insertion point; `range` is that range's start tag.
/// A half with nothing in it is left out (as InDesign writes it).
pub(super) fn split(w: &mut Work<'_>, owner: &str, at: usize, make: &mut dyn FnMut(&mut Work<'_>,&str)->Result<String,EditError>) -> Result<(),EditError> {
    let story=crate::text::story_of(w,owner)?;
    let (p,ix,i)=w.find(&story)?;let t=w.text(&p)?.to_owned();
    let mut seen=0;let mut chosen=None;
    scan(&t,&ix,i,&mut seen,at,at,&mut chosen)?;
    let (node,a,_)=chosen.ok_or_else(||EditError::new("unsupported","select one continuous text range without nested objects"))?;
    let n=&ix.nodes[node];
    let range=n.parent.filter(|&r|ix.local(&t,r)=="CharacterStyleRange").ok_or_else(||EditError::new("unsupported","the insertion point is not in a plain character range"))?;
    let r=&ix.nodes[range];
    let open=&t[r.start..r.open_end];
    let raw=&t[n.open_end..n.close_start];
    let content=|s:&str|if s.is_empty(){String::new()}else{format!("<Content>{s}</Content>")};
    let (before,after)=(format!("{}{}",&t[r.open_end..n.start],content(&raw[..a])),format!("{}{}",content(&raw[a..]),&t[n.end..r.close_start]));
    let half=|inside:&str|if inside.trim().is_empty(){String::new()}else{format!("{open}{inside}</CharacterStyleRange>")};
    let xref=make(w,open)?;
    w.splice(&p,vec![(r.start..r.end,format!("{}{xref}{}",half(&before),half(&after)))])?;w.recompose(&story);Ok(())
}

pub(super) fn anchor(w: &mut Work<'_>,story: &str,at: usize,id: &str,name: &str,k: usize) -> Result<(),EditError> {
    // An empty wrapper is removed after insertion so the destination is zero-width.
    let raw=format!("<HyperlinkTextDestination Self=\"{}\" Name=\"{}\" Hidden=\"false\" DestinationUniqueKey=\"{k}\" />",esc(id),esc(name));
    wrap(w,story,at,at,&raw,"",Some(""))
}
