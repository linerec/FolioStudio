//! Document-local author references for notes and tracked edits.
use super::*;
use crate::xml::escape;

pub(super) fn reference(w: &mut Work<'_>, user: &str) -> Result<String, EditError> {
    // Story-only editor fixtures have no package author table.
    let Some(text) = w.ed.parts.get("designmap.xml") else { return Ok("dDocumentUser0".into()); };
    let ix=Index::scan(text).map_err(|e|EditError::new("xml",e))?;
    let mut ids=std::collections::BTreeSet::new();
    let mut at=ix.nodes[0].open_end;
    for (i,n) in ix.nodes.iter().enumerate() {
        if ix.local(text,i)!="DocumentUser" {continue;}
        let id=ix.value(text,i,"Self").unwrap_or_default();
        if ix.value(text,i,"UserName").unwrap_or_default().trim_start_matches("$ID/")==user.trim_start_matches("$ID/") {return Ok(id);}
        ids.insert(id);at=n.end;
    }
    let mut number=0;
    while ids.contains(&format!("dDocumentUser{number}")){number+=1;}
    let id=format!("dDocumentUser{number}");
    let raw=format!("<DocumentUser Self=\"{id}\" UserName=\"{}\"><Properties><UserColor type=\"enumeration\">Gold</UserColor></Properties></DocumentUser>",escape(user));
    w.splice("designmap.xml",vec![(at..at,raw)])?;
    Ok(id)
}
