use super::*;
use folio_model::preflight::Profiles;
const DM: &str = "designmap.xml";

pub(crate) fn set(w: &mut Work<'_>, state: &Profiles) -> Result<(), EditError> {
    folio_preflight::profiles::validate(state).map_err(|e|EditError::new("invalid", e))?;
    write_label(w,folio_idml::preflight::LABEL,&serde_json::to_string(state).unwrap())
}
pub(crate) fn settings(w:&mut Work<'_>,state:&folio_model::preflight::Settings)->Result<(),EditError>{
    folio_preflight::scope::validate(state).map_err(|e|EditError::new("invalid",e))?;
    write_label(w,folio_idml::preflight::SETTINGS,&serde_json::to_string(state).unwrap())
}
fn write_label(w:&mut Work<'_>,key:&str,value:&str)->Result<(),EditError>{
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e|EditError::new("xml", e))?;
    let pair = format!("<KeyValuePair Key=\"{key}\" Value=\"{}\" />", escape(value));
    let doc = (0..ix.nodes.len()).find(|&i|ix.local(&t,i)=="Document").ok_or_else(||EditError::new("xml","Document missing"))?;
    let props = ix.named(&t,doc,"Properties").first().copied();
    if let Some(label) = props.and_then(|p|ix.named(&t,p,"Label").first().copied()) {
        if let Some(i) = ix.named(&t,label,"KeyValuePair").into_iter().find(|&i|ix.attr(&t,i,"Key")==Some(key)) {
            return w.splice(DM,vec![(ix.nodes[i].start..ix.nodes[i].end,pair)]);
        }
        return insert(w,&t,&ix,label,&pair);
    }
    let label = format!("<Label>{pair}</Label>");
    if let Some(p)=props {return insert(w,&t,&ix,p,&label);}
    insert(w,&t,&ix,doc,&format!("<Properties>{label}</Properties>"))
}
fn insert(w:&mut Work<'_>,t:&str,ix:&Index,node:usize,child:&str)->Result<(),EditError>{
    let n=&ix.nodes[node];
    if t[n.start..n.end].trim_end().ends_with("/>") {
        let open=t[n.start..n.end].trim_end().trim_end_matches("/>");
        return w.splice(DM,vec![(n.start..n.end,format!("{open}>{child}</{}>",ix.local(t,node)))]);
    }
    let at=t[..n.end].rfind("</").ok_or_else(||EditError::new("xml","missing closing tag"))?;
    w.splice(DM,vec![(at..at,child.to_owned())])
}
