use super::{styles, xml::*};
use crate::{EditError, Work, xml::unescape};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn import(w: &mut Work<'_>, parts: &BTreeMap<String,String>) -> Result<(), EditError> {
    let source = parts.get("Resources/Styles.xml").ok_or_else(||EditError::new("invalid","스타일 정보가 없습니다"))?;
    let ix=scan(source)?;
    let mut names:BTreeSet<_>=styles::all(w.ed.parts())?.into_iter().map(|s|s.name).collect();
    let mut count=0;
    for i in 0..ix.nodes.len() {
        if ix.local(source,i)!="TOCStyle" {continue}
        let original=unescape(ix.attr(source,i,"Name").unwrap_or_default());
        let mut name=original.clone();let mut suffix=2;
        while names.contains(&name) {name=format!("{original} {suffix}");suffix+=1;}
        names.insert(name.clone());
        let mut xml=fragment(source,&ix,i);
        let id=w.new_id();
        attribute(&mut xml,"Self",&id)?;
        attribute(&mut xml,"Name",&name)?;
        // Included paragraph styles are referenced by name rather than object ID.
        let ei=scan(&xml)?;
        for entry in ei.named(&xml,0,"TOCStyleEntry") {
            let name=unescape(ei.attr(&xml,entry,"Name").unwrap_or_default());
            if let Some(p)=(0..ix.nodes.len()).find(|&p|ix.local(source,p)=="ParagraphStyle" && ix.attr(source,p,"Name").map(unescape)==Some(name.clone())) {
                let reference=ix.attr(source,p,"Self").unwrap_or_default();
                crate::ops::bring_resources(w,parts,&format!("<Included AppliedParagraphStyle=\"{reference}\"/>"))?;
            }
        }
        xml=crate::ops::bring_resources(w,parts,&xml)?;
        let ei=scan(&xml)?;
        let edits:Vec<_>=ei.named(&xml,0,"TOCStyleEntry").into_iter().enumerate().map(|(n,e)| {
            let mut entry=fragment(&xml,&ei,e);
            attribute(&mut entry,"Self",&format!("{id}TOCStyleEntry{n}"))?;
            Ok((ei.nodes[e].start..ei.nodes[e].end,entry))
        }).collect::<Result<_,EditError>>()?;
        for (range,entry) in edits.into_iter().rev() {xml.replace_range(range,&entry);}
        let target=w.index("Resources/Styles.xml")?;
        let at=target.nodes[0].close_start;
        w.splice("Resources/Styles.xml",vec![(at..at,xml)])?;
        count+=1;
    }
    if count==0 {return Err(EditError::new("notFound","불러올 목차 스타일이 없습니다"));}
    Ok(())
}
