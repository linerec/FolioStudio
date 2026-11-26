use serde_json::{Value, json};
use folio_project::library;
pub mod links;

fn field<'a>(r: &'a Value, key: &str) -> Result<&'a str, String> {
    r[key].as_str().ok_or_else(|| format!("{key} required"))
}

pub fn run(r: &Value, session: Option<&crate::Session>, _: &crate::Profiles) -> Result<Value, String> {
    let root = std::path::Path::new(field(r, "root")?);
    // `new`: the answer of File > New > Library... (folder, fileName, replace): the library is made and then read like any other
    let made = if field(r, "action")? == "new" { Some(library::create_in(root, r["folder"].as_str().unwrap_or(""), field(r, "fileName")?, r["replace"].as_bool().unwrap_or(false), &r["open"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default())?) } else { None };
    let path = match &made { Some(m) => m.path.as_str(), None => field(r, "path")? };
    let mut created = None;
    let mut extracted = None;
    match field(r, "action")? {
        "create" => { library::create(root, path, field(r, "name")?)?; }
        "read" | "new" => {}
        "store" => {
            let session = session.ok_or("open a document first")?;
            let state = session.edit.as_ref().ok_or("open a document first")?.lock().expect("shared state must remain unpoisoned after each operation");
            let ids: Vec<String> = serde_json::from_value(r["ids"].clone()).map_err(|e| e.to_string())?;
            let mut snapshot = state.editor.library_snapshot(&ids).map_err(|e| e.to_string())?;
            library::links::capture(root, &mut snapshot, |uri| crate::clip::source_file(session, &state, uri))?;
            // a name left out is "Untitled", as in InDesign (probe bookx-c3-lib); `kind` names the asset type, as `assetType` does
            created = Some(library::store_as(root, path, r["name"].as_str().unwrap_or("Untitled"), r["description"].as_str().unwrap_or(""), r["label"].as_str().unwrap_or(""), r["kind"].as_str(), &snapshot)?.id);
        }
        "remove" => library::remove(root, path, field(r, "asset")?)?,
        "update" => library::update(root, path, field(r, "asset")?, r["name"].as_str(), r["description"].as_str(), r["label"].as_str())?,
        // keyed script labels (InDesign `insertLabel` / `extractLabel`), apart from the `label` of Item Information
        "insertLabel" => library::insert_label(root, path, field(r, "asset")?, field(r, "key")?, r["value"].as_str().unwrap_or(""))?,
        "extractLabel" => {
            let lib = library::read(root, path)?;
            let asset = lib.assets.iter().find(|a| Some(a.id.as_str()) == r["asset"].as_str()).ok_or("asset not found")?;
            extracted = Some(library::extract_label(asset, field(r, "key")?));
        }
        "relink" => library::relink(root, path, field(r, "asset")?, field(r, "replacement")?)?,
        "place" => {
            let lib = library::read(root, path)?;
            let asset = lib.assets.iter().find(|a| Some(a.id.as_str()) == r["asset"].as_str()).ok_or("asset not found")?;
            let snapshot = library::snapshot(root, asset)?;
            // `story` places it in text (InDesign `placeAsset(Text)`): over the characters `start..end` (an insertion point when `end` is left out)
            if let Some(story) = r["story"].as_str() {
                let start = r["start"].as_u64().ok_or("start required")?;
                let end = r["end"].as_u64().unwrap_or(start);
                return Ok(json!({"command":{"op":"placeLibraryAssetInText","parts":snapshot.parts,"ids":snapshot.ids,"story":story,"start":start,"end":end,"libraryRoot":root,"libraryLinks":snapshot.links}}));
            }
            return Ok(json!({"command":{"op":"placeLibraryAsset","parts":snapshot.parts,"ids":snapshot.ids,"page":field(r,"page")?,"libraryRoot":root,"libraryLinks":snapshot.links}}));
        }
        _ => return Err("unknown library action".into()),
    }
    let lib = library::read(root, path)?;
    let status: Vec<_> = lib.assets.iter().map(|asset| match library::snapshot(root, asset) {
        Ok(_) => json!({"id":asset.id,"ok":true}),
        Err(error) => json!({"id":asset.id,"ok":false,"error":error}),
    }).collect();
    let view = library::view(&lib, r["search"].as_str().unwrap_or(""), r["sort"].as_str().unwrap_or("newest"));
    Ok(json!({"library":lib,"status":status,"created":created,"file":made,"path":path,"label":extracted,"view":view,
        "format": {"kind": "folio-library", "indl": false, "cloud": false, "note": "Folio format: the library is a .foliolibrary of project-relative paths and SHA-256; InDesign .indl and Creative Cloud libraries are not read or written"}}))
}
