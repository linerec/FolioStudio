use crate::{hash, idmlparts, paths::checked_project_path};
use folio_model::library::{Dependency, Snapshot};
use std::{collections::BTreeMap, path::{Path, PathBuf}};

pub const SCHEME: &str = "folio:library/";

pub fn resolve(root: &Path, link: &Dependency) -> Result<PathBuf, String> {
    let path = checked_project_path(root, &link.path)?;
    if hash::sha256_file(&path).map_err(|e| e.to_string())?.0 != link.sha256 { return Err("library link hash mismatch".into()); }
    Ok(path)
}

/// Library files are independent copies: removing or replacing the original cannot change an asset.
pub fn capture(root: &Path, snapshot: &mut Snapshot, mut source: impl FnMut(&str) -> Option<PathBuf>) -> Result<(), String> {
    for folder in ["assets", "assets/library"] {
        std::fs::create_dir_all(checked_project_path(root, folder)?).map_err(|e| e.to_string())?;
    }
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut error = None;
    for (part, xml) in &mut snapshot.parts {
        let mut copy = |uri: &str| -> Option<String> {
            if let Some(new) = map.get(uri) { return Some(new.clone()); }
            let result = (|| -> Result<String, String> {
                let file = source(uri).ok_or_else(|| "library link source missing".to_owned())?;
                let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
                let sha256 = hash::sha256_bytes(&bytes);
                let ext = file.extension().and_then(|s| s.to_str()).filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or("bin").to_lowercase();
                let path = format!("assets/library/{sha256}.{ext}");
                let dest = checked_project_path(root, &path)?;
                std::fs::create_dir_all(dest.parent().expect("validated assets/library file has a parent directory")).map_err(|e| e.to_string())?;
                use std::io::Write;
                match std::fs::OpenOptions::new().write(true).create_new(true).open(&dest) {
                    Ok(mut out) => { if let Err(e) = out.write_all(&bytes).and_then(|_| out.sync_all()) { drop(out); let _=std::fs::remove_file(&dest); return Err(e.to_string()); } }
                    Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists => { if hash::sha256_file(&dest).map_err(|e| e.to_string())?.0 != sha256 { return Err("library link hash mismatch".into()); } }
                    Err(e) => return Err(e.to_string()),
                }
                let uri = format!("{SCHEME}{sha256}");
                snapshot.links.insert(uri.clone(), Dependency { path, sha256 });
                Ok(uri)
            })();
            match result { Ok(new) => { map.insert(uri.to_owned(), new.clone()); Some(new) }, Err(e) => { error=Some(e); None } }
        };
        let mut edits=idmlparts::Edits { other_uri:Some(&mut copy), native_paths:true, prefixes:vec!["file:","folio:ref/"], ..Default::default() };
        if let Some(new)=idmlparts::rewrite(part,xml,&mut edits).0 { *xml=new; }
    }
    if let Some(error)=error { return Err(error); }
    validate(root,snapshot)
}

pub fn validate(root: &Path, snapshot: &Snapshot) -> Result<(), String> {
    for (uri, link) in &snapshot.links {
        if uri != &format!("{SCHEME}{}",link.sha256) { return Err("invalid library link".into()); }
        resolve(root,link)?;
    }
    for (part,xml) in &snapshot.parts {
        let mut invalid=false;
        let mut other=false;
        let mut inspect=|uri:&str| { if !uri.is_empty() && !snapshot.links.contains_key(uri) {invalid=true;} };
        let mut check=|_:&str| {other=true;None};
        let mut edits=idmlparts::Edits {inspect_link:Some(&mut inspect),other_uri:Some(&mut check),native_paths:true,prefixes:vec!["file:","folio:ref/"],..Default::default()};
        idmlparts::rewrite(part,xml,&mut edits);
        if invalid || other {return Err("library_external_references".into());}
    }
    Ok(())
}
