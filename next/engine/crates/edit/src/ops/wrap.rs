//! TextWrapPreference edits, measured by text-wrap/wrap-*.
use super::*;
pub(super) fn run(w:&mut Work<'_>,id:&str,mode:Option<&str>,offsets:Option<[f64;4]>,inverse:Option<bool>,master_only:Option<bool>,side:Option<&str>)->Result<(),EditError> {
    let mode=mode.map(|v|match v {
        "NONE"=>Ok("None"),"BOUNDING_BOX_TEXT_WRAP"=>Ok("BoundingBoxTextWrap"),
        "CONTOUR"=>Ok("Contour"),"JUMP_OBJECT_TEXT_WRAP"=>Ok("JumpObjectTextWrap"),
        "NEXT_COLUMN_TEXT_WRAP"=>Ok("NextColumnTextWrap"),
        _=>Err(EditError::new("invalid","invalid text wrap mode")),
    }).transpose()?;
    let side=side.map(|v|match v {
        "BOTH_SIDES"=>Ok("BothSides"),"LEFT_SIDE"=>Ok("LeftSide"),"RIGHT_SIDE"=>Ok("RightSide"),
        "SIDE_TOWARDS_SPINE"=>Ok("SideTowardsSpine"),"SIDE_AWAY_FROM_SPINE"=>Ok("SideAwayFromSpine"),"LARGEST_AREA"=>Ok("LargestArea"),
        _=>Err(EditError::new("invalid","invalid text wrap side")),
    }).transpose()?;
    if offsets.is_some_and(|v|v.iter().any(|x|!x.is_finite()||x.abs()>10000.0)) {return Err(EditError::new("invalid","invalid text wrap offsets"));}
    let (part,ix,i)=w.find(id)?;let t=w.text(&part)?.to_owned();
    let mut attrs=Vec::new();
    if let Some(v)=mode {attrs.push(("TextWrapMode",v.to_string()));}
    if let Some(v)=inverse {attrs.push(("Inverse",bool_text(v)));}
    if let Some(v)=master_only {attrs.push(("ApplyToMasterPageOnly",bool_text(v)));}
    if let Some(v)=side {attrs.push(("TextWrapSide",v.to_string()));}
    if let Some(p)=ix.path(&t,i,&["TextWrapPreference"]) {
        let mut edits:Vec<_>=attrs.iter().map(|(k,v)|attr_edit(&ix,&t,p,k,v)).collect();
        if let Some(values)=offsets {
            let o=ix.path(&t,p,&["Properties","TextWrapOffset"]);
            if let Some(o)=o {
                edits.extend(["Top","Left","Bottom","Right"].into_iter().zip(values).map(|(k,v)|attr_edit(&ix,&t,o,k,&geom::num(v))));
            } else {return Err(EditError::new("unsupported","missing wrap offset structure; original XML kept"));}
        }
        w.splice(&part,edits)?;
    } else {
        let mut xml="<TextWrapPreference".to_string();
        for (k,v) in attrs {xml+=&format!(" {k}=\"{}\"",escape(&v));}
        xml.push('>');
        let [t,l,b,r]=offsets.unwrap_or([0.0;4]);
        xml+=&format!("<Properties><TextWrapOffset Top=\"{}\" Left=\"{}\" Bottom=\"{}\" Right=\"{}\"/></Properties>",geom::num(t),geom::num(l),geom::num(b),geom::num(r));
        xml+="</TextWrapPreference>";
        let at=ix.nodes[i].close_start;w.splice(&part,vec![(at..at,xml)])?;
    }
    // A master item can affect document spreads too. Invalidate saved lines for
    // every story; the compositor limits each obstacle to the appropriate spread.
    let mut stories=Vec::new();
    for (part,t) in &w.ed.parts {if part.starts_with("Stories/") {
        let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
        stories.extend((0..ix.nodes.len()).filter(|&i|ix.local(t,i)=="Story").filter_map(|i|ix.value(t,i,"Self")));
    }}
    for id in stories {w.recompose(&id);}
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Command,Editor};
    use std::collections::BTreeMap;
    #[test]
    fn new_wrap_can_be_adjusted_and_one_undo_restores_all_source_bytes() {
        let mut e=Editor::new(BTreeMap::from([
            ("Spreads/a.xml".into(),r#"<Spread><Rectangle Self="r"><Private keep="yes"/></Rectangle></Spread>"#.into()),
            ("Stories/a.xml".into(),r#"<Story Self="a"/>"#.into()),
            ("Stories/b.xml".into(),r#"<Story Self="b"/>"#.into()),
        ]));
        let before=e.parts().clone();
        let r=e.apply("wrap",&[
            Command::TextWrap{id:"r".into(),mode:Some("BOUNDING_BOX_TEXT_WRAP".into()),offsets:None,inverse:None,master_only:None,side:None},
            Command::TextWrap{id:"r".into(),mode:None,offsets:Some([1.0,2.0,3.0,4.0]),inverse:Some(true),master_only:Some(true),side:Some("LEFT_SIDE".into())}
        ]).unwrap();
        assert_eq!(r.recompose,["a","b"]);
        assert!(e.part("Spreads/a.xml").unwrap().contains(r#"<Private keep="yes"/>"#));
        e.undo().unwrap();assert_eq!(&before,e.parts());
    }
}
