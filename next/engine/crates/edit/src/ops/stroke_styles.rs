use super::*;
use folio_model::stroke_styles::Definition;
use super::swatches::{scan, self_of, insert, rereference};
const GRAPHIC:&str="Resources/Graphic.xml";

pub(crate) fn save(w:&mut Work<'_>, p:&Definition)->Result<(),EditError>{
    let key=match p.kind.as_str(){"DashedStrokeStyle"=>"DashArray","DottedStrokeStyle"=>"DotArray","StripedStrokeStyle"=>"StripeArray",_=>return Err(EditError::new("invalid","unknown stroke style kind"))};
    if p.name.trim().is_empty() || p.values.is_empty() || p.values.iter().any(|v|!v.is_finite()||*v<0.0)
        || (p.kind!="DottedStrokeStyle" && !p.values.len().is_multiple_of(2))
        || (p.kind=="StripedStrokeStyle" && p.values.iter().any(|v|*v>100.0))
        || (p.kind=="StripedStrokeStyle" && p.values.windows(2).any(|v|v[0]>=v[1]))
        || !["None","Dashes","Gaps","DashesAndGaps"].contains(&p.adjustment.as_str())
        || !["ButtEndCap","RoundEndCap","ProjectingEndCap"].contains(&p.cap.as_str()) {
        return Err(EditError::new("invalid","invalid stroke style values"));
    }
    let id=self_of(&p.kind,&p.name);
    let (t,ix)=scan(w,GRAPHIC)?;
    if (0..ix.nodes.len()).any(|i|ix.value(&t,i,"Name").as_deref()==Some(&p.name) && ix.attr(&t,i,"Self")!=Some(p.id.as_str())) {
        return Err(EditError::new("invalid","stroke style name already exists"));
    }
    if !p.id.is_empty(){
        let i=ix.by_self(&t,&p.id).ok_or_else(||EditError::new("notFound","stroke style missing"))?;
        if ix.local(&t,i)!=p.kind{return Err(EditError::new("invalid","style kind cannot change"));}
        let vals=p.values.iter().map(|v|geom::num(*v)).collect::<Vec<_>>().join(" ");
        let mut edits=vec![attr_edit(&ix,&t,i,"Name",&p.name),attr_edit(&ix,&t,i,"Self",&id),attr_edit(&ix,&t,i,key,&vals)];
        if p.kind!="StripedStrokeStyle"{edits.push(attr_edit(&ix,&t,i,"StrokeCornerAdjustment",&p.adjustment));}
        if p.kind=="DashedStrokeStyle"{edits.push(attr_edit(&ix,&t,i,"EndCap",&p.cap));}
        w.splice(GRAPHIC,edits)?;
        if p.id!=id{
            let (t,ix)=scan(w,GRAPHIC)?;
            let i=ix.by_self(&t,&id).unwrap();
            let element=t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
            w.splice(GRAPHIC,vec![(element_span(&ix,&t,i),String::new())])?;
            insert(w,&p.kind,&id,&element,"StrokeStyle")?;
            rereference(w,&p.id,&id)?;
        }
    }else{
        let vals=p.values.iter().map(|v|geom::num(*v)).collect::<Vec<_>>().join(" ");
        let mut e=format!("<{} Self=\"{}\" {}=\"{}\" Name=\"{}\"",p.kind,escape(&id),key,vals,escape(&p.name));
        if p.kind!="StripedStrokeStyle"{e.push_str(&format!(" StrokeCornerAdjustment=\"{}\"",p.adjustment));}
        if p.kind=="DashedStrokeStyle"{e.push_str(&format!(" EndCap=\"{}\"",p.cap));}
        e.push_str(" />");
        insert(w,&p.kind,&id,&e,"StrokeStyle")?;
    }
    w.created.push(id);
    Ok(())
}

pub(crate) fn delete(w:&mut Work<'_>,id:&str)->Result<(),EditError>{
    let(t,ix)=scan(w,GRAPHIC)?;
    let i=ix.by_self(&t,id).ok_or_else(||EditError::new("notFound","stroke style missing"))?;
    if !["DashedStrokeStyle","DottedStrokeStyle","StripedStrokeStyle"].contains(&ix.local(&t,i)){return Err(EditError::new("locked","built-in style"));}
    w.splice(GRAPHIC,vec![(element_span(&ix,&t,i),String::new())])?;
    let parts:Vec<String>=w.ed.parts.keys().filter(|p|p.ends_with(".xml")).cloned().collect();
    for part in parts {
        let(t,ix)=scan(w,&part)?;
        let edits=(0..ix.nodes.len()).filter(|&i|ix.attr(&t,i,"StrokeType")==Some(id))
            .filter_map(|i|attr_removal(&ix,&t,i,"StrokeType")).collect();
        w.splice(&part,edits)?;
    }
    rereference(w,id,"StrokeStyle/$ID/Solid")
}
