use serde_json::{Value, json};
pub fn run(r:&Value,s:Option<&crate::Session>,_:&crate::Profiles)->Result<Value,String>{ file(r,s) }

/// A profile file of the project: `export` writes `profile` to `path`, `import` reads it (also Define Profiles' Load and Export).
pub fn file(r:&Value,s:Option<&crate::Session>)->Result<Value,String>{
    let f=s.and_then(|s|s.folio.as_ref()).ok_or("open a project document first")?;
    if f.read_only(){return Err("read-only document".into());}
    let root=&f.opened.project.root;
    let rel=folio_project::RelPath::parse(r["path"].as_str().ok_or("project-relative JSON path required")?).map_err(|e|e.to_string())?;
    let path=rel.to_native(root);
    if path.extension().and_then(|s|s.to_str())!=Some("json"){return Err("JSON file required".into());}
    let parent=path.parent().ok_or("missing parent")?.canonicalize().map_err(|e|e.to_string())?;
    if !parent.starts_with(root.canonicalize().map_err(|e|e.to_string())?) {return Err("path leaves project".into());}
    if path.exists() && (std::fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() || !path.canonicalize().map_err(|e|e.to_string())?.starts_with(&parent)){return Err("linked profile files are not allowed".into());}
    let profile:folio_model::preflight::Profile;
    if r["action"]=="export" {
        profile=serde_json::from_value(r["profile"].clone()).map_err(|e|e.to_string())?;
        folio_preflight::profiles::validate(&folio_model::preflight::Profiles{active:profile.name.clone(),profiles:vec![profile.clone()]})?;
        let bytes=serde_json::to_vec_pretty(&profile).map_err(|e|e.to_string())?;
        folio_project::atomic::write_with(&path,&mut |w|std::io::Write::write_all(w,&bytes)).map_err(|e|e.to_string())?;
    } else if r["action"]=="import" {
        let bytes=std::fs::read(&path).map_err(|e|e.to_string())?;
        profile=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
        folio_preflight::profiles::validate(&folio_model::preflight::Profiles{active:profile.name.clone(),profiles:vec![profile.clone()]})?;
    } else {return Err("unknown profile file action".into());}
    let (sha256,size)=folio_project::hash::sha256_file(&path).map_err(|e|e.to_string())?;
    Ok(json!({"profile":profile,"path":rel,"sha256":sha256,"size":size}))
}
