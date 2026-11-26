//! Atomic, project-relative local libraries. The container is not an INDL or an IDML file.
use crate::{RelPath, atomic, hash, paths::checked_project_path as checked_path};
use folio_model::library::{Asset, Library, Snapshot};
use std::path::{Path, PathBuf};
pub mod links;

fn container(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !relative.ends_with(".foliolibrary") { return Err("use a .foliolibrary file".into()); }
    checked_path(root, relative)
}

pub fn create(root: &Path, relative: &str, name: &str) -> Result<Library, String> {
    let path = container(root, relative)?;
    if path.exists() { return Err("library already exists".into()); }
    if name.trim().is_empty() { return Err("library name required".into()); }
    let library = Library { format: "folio-library".into(), version: 1, name: name.into(), assets: vec![], extra: Default::default() };
    save(&path, &library)?;
    Ok(library)
}

/// What File > New > Library... made (see `book::Created`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Created { pub path: String, pub name: String, pub replaced: bool }
/// The answer of the New Library dialog: an empty library in `folder` named `file_name` (`.foliolibrary` is added when it is not typed); a file of
/// that name needs `replace`, otherwise the error is `exists: <path>`; `open` lists the paths open as libraries (replacing one is `open: <path>`).
pub fn create_in(root: &Path, folder: &str, file_name: &str, replace: bool, open: &[String]) -> Result<Created, String> {
    let target = crate::newfile::resolve(root, folder, file_name, ".foliolibrary")?;
    if target.exists && !replace { return Err(format!("exists: {}", target.path)); }
    // a file that is open as a library is not saved over (InDesign: "Cannot save to <path> because it is already open"), after the replace question
    if target.exists && open.iter().any(|p| crate::paths::collision_key(p) == crate::paths::collision_key(&target.path)) { return Err(format!("open: {}", target.path)); }
    let library = Library { format: "folio-library".into(), version: 1, name: target.file_name.clone(), assets: vec![], extra: Default::default() };
    save(&container(root, &target.path)?, &library)?;
    Ok(Created { path: target.path, name: target.file_name, replaced: target.exists })
}

pub fn read(root: &Path, relative: &str) -> Result<Library, String> {
    let library: Library = serde_json::from_slice(&std::fs::read(container(root, relative)?).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if library.format != "folio-library" || library.version != 1 { return Err("unsupported library format".into()); }
    for asset in &library.assets { RelPath::parse(&asset.path).map_err(|e| e.to_string())?; }
    Ok(library)
}

fn save(path: &Path, library: &Library) -> Result<(), String> {
    atomic::write_bytes(path, &serde_json::to_vec_pretty(library).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn store(root: &Path, relative: &str, name: &str, description: &str, snapshot: &Snapshot) -> Result<Asset, String> {
    store_with(root, relative, name, description, "", snapshot)
}

/// The item kinds of InDesign's `AssetType` (an item can be stored with one of them named, as `Library.store` takes `assetType`).
pub const KINDS: [&str; 8] = ["geometry", "text", "image", "pdf", "eps", "page", "structure", "indesignFile"];

/// What an item is (measured with `Library.store`, probes bookx-c1-library a1/l1 and bookx-c3-lib2, InDesign 2025): a single frame
/// holding a placed PDF or Illustrator file is a PDF, an EPS is an EPS, any other placed graphic an image, a single text frame is
/// text, anything else, several objects included, is geometry.
pub fn kind_of(snapshot: &Snapshot) -> &'static str {
    let [id] = snapshot.ids.as_slice() else { return "geometry" };
    for xml in snapshot.parts.values() {
        let key = format!("Self=\"{id}\"");
        let Some(at) = xml.find(&key) else { continue };
        let Some(open) = xml[..at].rfind('<') else { continue };
        let tag: String = xml[open + 1..].chars().take_while(|c| c.is_alphanumeric()).collect();
        let end = xml[open..].find(&format!("</{tag}>")).map_or(xml.len(), |e| open + e);
        let body = &xml[open..end];
        if body.contains("<PDF ") { return "pdf"; }
        if body.contains("<EPS ") { return "eps"; }
        if ["<Image ", "<ImportedPage "].iter().any(|g| body.contains(g)) {
            return "image";
        }
        return if tag == "TextFrame" { "text" } else { "geometry" };
    }
    "geometry"
}

/// Item Information: the name, the description and the script label of a stored item.
pub fn update(root: &Path, relative: &str, id: &str, name: Option<&str>, description: Option<&str>, label: Option<&str>) -> Result<(), String> {
    let mut library = read(root, relative)?;
    let asset = library.assets.iter_mut().find(|a| a.id == id).ok_or("asset not found")?;
    // any name is kept as given, an empty one and one of spaces too (InDesign stores them as they come, probe bookx-c4-names)
    if let Some(name) = name { asset.name = name.into(); }
    if let Some(description) = description { asset.description = description.into(); }
    if let Some(label) = label { asset.label = label.into(); }
    save(&container(root, relative)?, &library)
}

/// `Asset.insertLabel(key, value)`: a keyed script label of an item. A key may be empty and a value hold any text (line breaks, any length); a key
/// given again takes the new value; an empty value is the same as none (InDesign answers `''` for both, probe bookx-c4-labels), so it removes the key.
pub fn insert_label(root: &Path, relative: &str, id: &str, key: &str, value: &str) -> Result<(), String> {
    let mut library = read(root, relative)?;
    let asset = library.assets.iter_mut().find(|a| a.id == id).ok_or("asset not found")?;
    if value.is_empty() { asset.labels.remove(key); } else { asset.labels.insert(key.into(), value.into()); }
    save(&container(root, relative)?, &library)
}

/// `Asset.extractLabel(key)`: the keyed label, `''` when the item has none under that key.
pub fn extract_label(asset: &Asset, key: &str) -> String {
    asset.labels.get(key).cloned().unwrap_or_default()
}

/// The items to show, in order: `sort` is `newest` (InDesign's order, the default), `oldest`, `name` or `type`; `search` keeps the
/// items whose name, description, label or kind holds the text (capitals do not matter).
pub fn view(library: &Library, search: &str, sort: &str) -> Vec<String> {
    let needle = search.trim().to_lowercase();
    let mut items: Vec<(usize, &Asset)> = library.assets.iter().enumerate().collect();
    if !needle.is_empty() {
        items.retain(|(_, a)| [&a.name, &a.description, &a.label, &a.kind].iter().any(|f| f.to_lowercase().contains(&needle)));
    }
    match sort {
        "oldest" => {}
        "name" => items.sort_by_key(|(i, a)| (a.name.to_lowercase(), std::cmp::Reverse(*i))),
        "type" => items.sort_by_key(|(i, a)| (a.kind.clone(), std::cmp::Reverse(*i))),
        _ => items.reverse(),
    }
    items.into_iter().map(|(_, a)| a.id.clone()).collect()
}

pub fn store_with(root: &Path, relative: &str, name: &str, description: &str, label: &str, snapshot: &Snapshot) -> Result<Asset, String> {
    store_as(root, relative, name, description, label, None, snapshot)
}

/// `store_with` with the kind named (`geometry`, `text`, `image`, `pdf`, `eps`, `page`, `structure`, `indesignFile`), as InDesign's
/// `Library.store(items, {assetType})` takes it: the name is only a tag and the stored objects are the same (probe bookx-c3-lib).
pub fn store_as(root: &Path, relative: &str, name: &str, description: &str, label: &str, kind: Option<&str>, snapshot: &Snapshot) -> Result<Asset, String> {
    if let Some(kind) = kind && !KINDS.contains(&kind) { return Err(format!("unknown asset kind {kind} (use {})", KINDS.join(", "))); }
    let mut library = read(root, relative)?;
    // the name is kept as given, empty or only spaces included, 1200 characters too (InDesign: probe bookx-c4-names); a name left out is "Untitled" (the server)
    if snapshot.ids.is_empty() { return Err("asset objects required".into()); }
    links::validate(root, snapshot)?;
    let bytes = serde_json::to_vec(snapshot).map_err(|e| e.to_string())?;
    let sha256 = hash::sha256_bytes(&bytes);
    let folder = checked_path(root, "assets")?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = format!("assets/library-{sha256}.json");
    let dest = checked_path(root, &path)?;
    if dest.exists() {
        if hash::sha256_file(&dest).map_err(|e| e.to_string())?.0 != sha256 { return Err("asset hash mismatch".into()); }
    } else { atomic::write_bytes(&dest, &bytes).map_err(|e| e.to_string())?; }
    let asset = Asset { id: crate::util::short_id(), name: name.into(), description: description.into(), path, sha256, kind: kind.unwrap_or_else(|| kind_of(snapshot)).into(), label: label.into(), labels: Default::default(), created: crate::util::now_rfc3339() };
    library.assets.push(asset.clone());
    save(&container(root, relative)?, &library)?;
    Ok(asset)
}

pub fn snapshot(root: &Path, asset: &Asset) -> Result<Snapshot, String> {
    let bytes = std::fs::read(checked_path(root, &asset.path)?).map_err(|e| e.to_string())?;
    if hash::sha256_bytes(&bytes) != asset.sha256 { return Err("asset hash mismatch".into()); }
    let snapshot = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    links::validate(root, &snapshot)?;
    Ok(snapshot)
}

pub fn remove(root: &Path, relative: &str, id: &str) -> Result<(), String> {
    let mut library = read(root, relative)?;
    let index = library.assets.iter().position(|a| a.id == id).ok_or("asset not found")?;
    library.assets.remove(index);
    save(&container(root, relative)?, &library)
}

pub fn relink(root: &Path, relative: &str, id: &str, path: &str) -> Result<(), String> {
    let mut library = read(root, relative)?;
    let asset = library.assets.iter_mut().find(|a| a.id == id).ok_or("asset not found")?;
    let mut candidate = asset.clone(); candidate.path = path.into();
    snapshot(root, &candidate)?;
    asset.path = candidate.path;
    save(&container(root, relative)?, &library)
}
