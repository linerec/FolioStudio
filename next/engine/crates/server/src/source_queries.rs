//! Read-only file queries, registered independently of document commands.
use crate::{Out, send};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

type Query = fn(&Value) -> Result<Value, String>;
const QUERIES: &[(&str, Query)] = &[("placeInfo", place_info), ("fileMetadata", file_metadata)];

pub fn dispatch(req: &Value, out: &Out) -> bool {
    let Some((name, query)) = QUERIES.iter().find(|(name, _)| req["cmd"].as_str() == Some(*name)) else { return false };
    let req = req.clone(); let out = out.clone(); let query = *query; let name = *name;
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| query(&req)).unwrap_or_else(|_| Err("Cannot inspect this source file".into()));
        let mut reply = match result {
            Ok(info) => json!({"ok":true,"type":name,"info":info}),
            Err(error) => json!({"ok":false,"code":"place","error":error}),
        };
        reply["id"] = req["id"].clone();send(&out, reply, &[]);
    });
    true
}

/// `{"cmd":"fileMetadata","file":…}`: the XMP packet a file carries and the properties captions use (title, description, author,
/// copyright, keywords, IPTC headline, credit …), with the file's size and modified time (ms). Nothing is written.
fn file_metadata(req: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Request { file: String }
    let req: Request = serde_json::from_value(req.clone()).map_err(|e| e.to_string())?;
    let path = Path::new(&req.file);
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as f64);
    let packet = folio_links::probe::xmp::read_packet(path);
    let fields = packet.as_deref().map(folio_links::probe::xmp::XmpFields::parse);
    Ok(json!({"size": meta.len(), "modified": modified, "packet": packet, "fields": fields}))
}

fn place_info(req: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Request { file: String }
    let req: Request = serde_json::from_value(req.clone()).map_err(|e| e.to_string())?;
    let info = match folio_links::probe::probe(Path::new(&req.file)) {
        Ok(info) => info,
        // a format that is not read: said by name, not as a failure of the query (the screen shows the sentence and does not load the file)
        Err(e @ folio_links::probe::ProbeError::Unsupported(_)) => {
            return Ok(json!({"format":null,"pdf":false,"pages":null,"pageBoxes":[],"width":0,"height":0,"fonts":[],"unsupported":crate::place::probe_message(&e)}));
        }
        Err(e) => return Err(e.to_string()),
    };
    let pdf = !info.page_boxes.is_empty();
    // The engine validates the format, colour depth and supported SVG features before placement.
    let unsupported = if pdf { None } else { folio_edit::GraphicSource::read(Path::new(&req.file), String::new()).and_then(|source| { if matches!(source.format.as_str(), "SVG" | "EMF" | "WMF") { folio_render::svg::read(Path::new(&req.file))?; } Ok(source) }).err() };
    Ok(json!({"format":info.format,"pdf":pdf,"pages":info.pages,"pageBoxes":info.page_boxes,"width":info.width,"height":info.height,"fonts":info.fonts,"unsupported":unsupported}))
}
