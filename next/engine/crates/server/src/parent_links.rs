//! Resolve the selected parent document, then import its links in the edit transaction.
use crate::{Session, edit::EditState, place::Prepared};
use folio_edit::xml::Index;
use folio_project::{format::REF_SCHEME, idmlparts};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

type Parts = BTreeMap<String, String>;

fn parent_parts(all: Parts) -> Result<Parts, String> {
    let mut parts: Parts = all
        .iter()
        .filter(|(n, _)| {
            n.as_str() == "designmap.xml"
                || n.starts_with("MasterSpreads/")
                || n.starts_with("Resources/")
        })
        .map(|(n, t)| (n.clone(), t.clone()))
        .collect();
    // Only stories reachable from these parents belong to this import.
    let mut pending: Vec<String> = parts
        .iter()
        .filter(|(n, _)| n.starts_with("MasterSpreads/"))
        .map(|(_, t)| t.clone())
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(text) = pending.pop() {
        let ix = Index::scan(&text)?;
        for i in 0..ix.nodes.len() {
            if let Some(story) = ix.attr(&text, i, "ParentStory") {
                if story == "n" || !seen.insert(story.to_owned()) {
                    continue;
                }
                let name = format!("Stories/Story_{story}.xml");
                let xml = all.get(&name).ok_or("missing parent story")?.clone();
                pending.push(xml.clone());
                parts.insert(name, xml);
            }
        }
    }
    Ok(parts)
}

fn rewrite(
    parts: &mut Parts,
    mut resolve: impl FnMut(&str) -> Result<String, String>,
) -> Result<(), String> {
    let mut mapped = BTreeMap::new();
    for (name, text) in parts {
        let mut error = None;
        let mut replace = |uri: &str| {
            if let Some(new) = mapped.get(uri) {
                return Some(String::clone(new));
            }
            match resolve(uri) {
                Ok(new) => {
                    mapped.insert(uri.to_owned(), new.clone());
                    Some(new)
                }
                Err(e) => {
                    error = Some(e);
                    None
                }
            }
        };
        let mut edits = idmlparts::Edits {
            other_uri: Some(&mut replace),
            native_paths: true,
            prefixes: vec!["file:", REF_SCHEME],
            ..Default::default()
        };
        if let Some(new) = idmlparts::rewrite(name, text, &mut edits).0 {
            *text = new;
        }
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(())
}

pub fn file(req: &Value) -> Result<Value, String> {
    let path = Path::new(req["path"].as_str().ok_or("missing file path")?);
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "idml" | "folio") {
        return Err("Choose an IDML or Folio document".into());
    }
    let mut parts = parent_parts(crate::edit::package_parts(path)?)?;
    let opened = if extension == "folio" {
        Some(folio_project::open::resolve(path).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let search = folio_links::LinkSearch {
        document_dir: path.parent().map(Path::to_path_buf),
        roots: vec![],
    };
    let index = folio_links::FileIndex::scan(&[]);
    rewrite(&mut parts, |uri| {
        let file = if let Some(id) = uri.strip_prefix(REF_SCHEME) {
            opened
                .as_ref()
                .and_then(|o| o.state(id))
                .and_then(|r| r.file.clone())
        } else {
            folio_links::resolve(uri, &search, &index)
                .path()
                .map(Path::to_path_buf)
        };
        file.map(|p| folio_project::paths::file_uri(&p))
            .ok_or_else(|| format!("Parent link source missing: {uri}"))
    })?;
    Ok(json!({"parts":parts}))
}

pub fn prepare(
    cur: &Session,
    es: &EditState,
    command: &mut Value,
    prepared: &mut Prepared,
) -> Result<(), String> {
    let Some(folio) = &cur.folio else {
        return Ok(());
    };
    let mut parts =
        parent_parts(serde_json::from_value(command["parts"].clone()).map_err(|e| e.to_string())?)?;
    rewrite(&mut parts, |uri| {
        let file = crate::clip::source_file(cur, es, uri)
            .ok_or_else(|| format!("Parent link source missing: {uri}"))?;
        let ext = file
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
            .to_ascii_lowercase();
        let (reference, _) = crate::place::import_link(&file, &ext, folio, es, prepared)?;
        Ok(format!("{REF_SCHEME}{}", reference.id))
    })?;
    command["parts"] = json!(parts);
    Ok(())
}
