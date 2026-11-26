use crate::{Session, edit::EditState, place::Prepared};
use folio_model::library::Dependency;
use folio_project::{idmlparts, library::links};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub fn prepare(cur: &Session, es: &EditState, command: &mut Value, prepared: &mut Prepared) -> Result<(), String> {
    let dependencies: BTreeMap<String, Dependency> = command.get("libraryLinks").map(|v| serde_json::from_value(v.clone())).transpose().map_err(|e| e.to_string())?.unwrap_or_default();
    let mut mapped = BTreeMap::new();
    if !dependencies.is_empty() {
        let root = Path::new(command["libraryRoot"].as_str().ok_or("library root required")?);
        let folio = cur.folio.as_ref().ok_or("linked library assets require a project")?;
        for (uri, dependency) in dependencies {
            if uri != format!("{}{}", links::SCHEME, dependency.sha256) { return Err("invalid library link".into()); }
            let file = links::resolve(root, &dependency)?;
            let source = folio_edit::GraphicSource::read(&file, String::new())?;
            let (reference, _) = crate::place::import_link(&file, source.extension(), folio, es, prepared)?;
            mapped.insert(uri, format!("{}{}", folio_project::format::REF_SCHEME, reference.id));
        }
    }
    let mut parts: BTreeMap<String, String> = serde_json::from_value(command["parts"].clone()).map_err(|e| e.to_string())?;
    for (part, xml) in &mut parts {
        let mut missing = false;
        let mut replace = |uri: &str| { let found = mapped.get(uri).cloned(); if found.is_none() { missing = true; } found };
        let mut edits = idmlparts::Edits { other_uri: Some(&mut replace), prefixes: vec![links::SCHEME], ..Default::default() };
        if let Some(new) = idmlparts::rewrite(part, xml, &mut edits).0 { *xml = new; }
        if missing { return Err("library link source missing".into()); }
    }
    command["parts"] = json!(parts);
    let object = command.as_object_mut().expect("parts were decoded from this command object");
    object.remove("libraryRoot");
    object.remove("libraryLinks");
    Ok(())
}
