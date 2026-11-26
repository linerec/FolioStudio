use serde_json::{Value,json};
pub fn run(r:&Value,s:Option<&crate::Session>,_:&crate::Profiles)->Result<Value,String>{read(r,s)}
/// The custom stroke styles of an IDML file of the open project (the Stroke Styles dialog's Load… reads them).
pub fn read(r:&Value,s:Option<&crate::Session>)->Result<Value,String>{
    let f=s.and_then(|s|s.folio.as_ref()).ok_or("open a project document first")?;
    let root=f.opened.project.root.canonicalize().map_err(|e|e.to_string())?;
    let rel=folio_project::RelPath::parse(r["path"].as_str().ok_or("project-relative IDML path required")?).map_err(|e|e.to_string())?;
    let path=rel.to_native(&root).canonicalize().map_err(|e|e.to_string())?;
    if !path.starts_with(&root)||path.extension().and_then(|s|s.to_str())!=Some("idml"){return Err("choose an IDML file inside this project".into());}
    let doc=folio_idml::read_package(&path,None).map_err(|e|e.to_string())?;
    let(sha256,size)=folio_project::hash::sha256_file(&path).map_err(|e|e.to_string())?;
    Ok(json!({"styles":doc.stroke_styles,"path":rel,"sha256":sha256,"size":size}))
}
