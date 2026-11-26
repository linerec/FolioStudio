//! OCG source defaults and placement overrides. Preview and print use the same state.
mod flatten;
use folio_model::graphic_layers::{Layer, Options};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeMap;

fn object<'a>(doc: &'a Document, o: &'a Object) -> Result<&'a Object, String> {
    doc.dereference(o).map(|(_,o)| o).map_err(|e| e.to_string())
}
fn dict<'a>(doc: &'a Document, o: &'a Object) -> Result<&'a Dictionary, String> {
    object(doc,o)?.as_dict().map_err(|e| e.to_string())
}

pub fn read(doc: &Document) -> Result<Option<Options>, String> {
    let catalog = doc.catalog().map_err(|e| e.to_string())?;
    let Ok(properties) = catalog.get(b"OCProperties") else { return Ok(None) };
    let properties = dict(doc, properties)?;
    let config = properties.get(b"D").ok().map(|o| dict(doc,o)).transpose()?.cloned().unwrap_or_default();
    let base = config.get(b"BaseState").ok().and_then(|o| o.as_name().ok()) != Some(b"OFF");
    let refs = |key| config.get(key).ok().and_then(|o| object(doc,o).ok()).and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect::<Vec<_>>()).unwrap_or_default();
    let on = refs(b"ON"); let off = refs(b"OFF");
    let groups = object(doc, properties.get(b"OCGs").map_err(|e| e.to_string())?)?.as_array().map_err(|e| e.to_string())?;
    let mut layers = Vec::new();
    for group in groups {
        let id = group.as_reference().map_err(|e| e.to_string())?;
        let g = dict(doc,group)?;
        let name = lopdf::decode_text_string(g.get(b"Name").map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let visible = (base || on.contains(&id)) && !off.contains(&id);
        layers.push(Layer { id:id.0, name, original:visible, visible, children:Vec::new(), adjustment:false });
    }
    Ok(Some(Options { update_link:"KeepOverrides".into(), layers, ..Default::default() }))
}

pub fn apply(doc: &mut Document, options: Option<&Options>) -> Result<(), String> {
    let Some(defaults) = read(doc)? else {
        return if options.is_some_and(|o| !o.layers.is_empty()) { Err("PDF layer source no longer matches placement".into()) } else { Ok(()) };
    };
    let mut state = BTreeMap::new();
    if let Some(options) = options
        && options.layers.iter().any(|l| !defaults.layers.iter().any(|d| d.id == l.id && d.name == l.name))
    {
        return Err("PDF layer source no longer matches placement".into());
    }
    for layer in defaults.layers {
        let visible = options.and_then(|o| o.layers.iter().find(|l| l.id == layer.id && l.name == layer.name)).map(|l| l.visible).unwrap_or(layer.visible);
        let id = doc.objects.keys().find(|id| id.0 == layer.id).copied().ok_or("PDF layer missing")?;
        state.insert(id,visible);
    }
    if let Some(options) = options {
        for layer in &options.layers {
            if !state.keys().any(|id| id.0 == layer.id) { return Err("PDF layer source no longer matches placement".into()); }
        }
    }
    flatten::run(doc,&state)
}

fn visible(doc: &Document, value: &Object, state: &BTreeMap<ObjectId,bool>) -> Result<bool,String> {
    visibility(doc,value,state,0)
}

fn visibility(doc: &Document, value: &Object, state: &BTreeMap<ObjectId,bool>, depth: usize) -> Result<bool,String> {
    if depth > 32 { return Err("Cyclic PDF layer membership".into()); }
    if let Ok(id) = value.as_reference()
        && let Some(visible) = state.get(&id) { return Ok(*visible); }
    let d = dict(doc,value)?;
    if d.has(b"VE") { return Err("PDF layer visibility expressions are not supported yet".into()); }
    if d.get(b"Type").ok().and_then(|o| o.as_name().ok()) != Some(b"OCMD") { return Err("Unknown PDF layer reference".into()); }
    let groups = d.get(b"OCGs").map_err(|e| e.to_string())?;
    let groups = match groups { Object::Array(a) => a.clone(), other => vec![other.clone()] };
    let values = groups.iter().map(|g| visibility(doc,g,state,depth+1)).collect::<Result<Vec<_>,_>>()?;
    Ok(match d.get(b"P").ok().and_then(|o| o.as_name().ok()).unwrap_or(b"AnyOn") {
        b"AllOn" => values.iter().all(|v| *v), b"AnyOn" => values.iter().any(|v| *v),
        b"AllOff" => values.iter().all(|v| !v), b"AnyOff" => values.iter().any(|v| !v),
        _ => return Err("Unknown PDF layer membership policy".into()),
    })
}
