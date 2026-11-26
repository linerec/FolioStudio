//! Joins exported document page trees, preserving each document's resources and page boxes.
//! The documents' Acrobat layers (optional content) are joined as InDesign 2025 joins them (buildc2-ocg-probe, book-merge.pdf and
//! book-nomerge.pdf): with Merge Identically Named Layers on Export one group per name (the first document's, the others' marks point
//! at it) listed under the book's name; without it every document's groups listed under the document's name.
use lopdf::{Document, Object, ObjectId, dictionary};
use std::collections::HashMap;

fn profile(doc: &Document) -> Result<Option<Vec<u8>>, String> {
    let catalog = doc.catalog().map_err(|e| e.to_string())?;
    let Ok(intents) = catalog.get(b"OutputIntents") else { return Ok(None) };
    let intents = intents.as_array().map_err(|e| e.to_string())?;
    if intents.len() != 1 { return Err("book requires one output intent per document".into()); }
    let intent = doc.dereference(&intents[0]).map_err(|e| e.to_string())?.1.as_dict().map_err(|e| e.to_string())?;
    let value = intent.get(b"DestOutputProfile").map_err(|e| e.to_string())?;
    let stream = doc.dereference(value).map_err(|e| e.to_string())?.1.as_stream().map_err(|e| e.to_string())?;
    Ok(Some(stream.get_plain_content().map_err(|e| e.to_string())?))
}

/// How a book's Acrobat layers are joined: the book's file name (the label of the merged list), each document's file name (the labels
/// of its own list) and Merge Identically Named Layers on Export.
pub struct BookLayers<'a> {
    pub book_name: &'a str,
    pub document_names: &'a [String],
    pub merge: bool,
}

/// One document's groups: (object, name, on in the default view), in its /OCGs order ("Guides and Grids" first, then the layers from
/// the bottom one up, as the exporter writes them).
fn groups(doc: &Document) -> Vec<(ObjectId, Vec<u8>, bool)> {
    let Ok(cat) = doc.catalog() else { return Vec::new() };
    let Ok(oc) = cat.get(b"OCProperties").and_then(|o| doc.dereference(o)).and_then(|(_, o)| o.as_dict()) else { return Vec::new() };
    let on: Vec<ObjectId> = oc.get(b"D").and_then(|d| doc.dereference(d)).and_then(|(_, d)| d.as_dict()).and_then(|d| d.get(b"ON")).and_then(Object::as_array)
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect()).unwrap_or_default();
    oc.get(b"OCGs").and_then(Object::as_array).map(|a| a.iter().filter_map(|o| {
        let id = o.as_reference().ok()?;
        let name = doc.get_dictionary(id).ok()?.get(b"Name").ok()?.as_str().ok()?.to_vec();
        Some((id, name, on.contains(&id)))
    }).collect()).unwrap_or_default()
}

fn remap(o: &mut Object, to: &HashMap<ObjectId, ObjectId>) {
    match o {
        Object::Reference(r) => if let Some(n) = to.get(r) { *r = *n; },
        Object::Array(a) => a.iter_mut().for_each(|x| remap(x, to)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| remap(x, to)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| remap(x, to)),
        _ => {}
    }
}

/// A PDF text string: ASCII as it is, anything else UTF-16BE with its byte order mark.
fn text(s: &str) -> Object {
    if s.is_ascii() { return Object::string_literal(s); }
    let mut b = vec![0xfe, 0xff];
    for u in s.encode_utf16() { b.extend(u.to_be_bytes()); }
    Object::String(b, lopdf::StringFormat::Hexadecimal)
}

/// A labelled list of a document's or the book's groups: the label, the layers from the top one down, "Guides and Grids" last.
fn order_list(label: &str, groups: &[ObjectId]) -> Object {
    let mut list = vec![text(label)];
    list.extend(groups.iter().skip(1).rev().map(|g| Object::Reference(*g)));
    if let Some(first) = groups.first() { list.push(Object::Reference(*first)); }
    Object::Array(list)
}

pub fn join(documents: &[Vec<u8>]) -> Result<(Vec<u8>, usize), String> {
    join_layers(documents, None)
}

pub fn join_layers(documents: &[Vec<u8>], layers: Option<&BookLayers>) -> Result<(Vec<u8>, usize), String> {
    if documents.is_empty() { return Err("book has no documents".into()); }
    let mut output = Document::with_version("1.7");
    let pages_id = output.new_object_id();
    let mut kids = Vec::new();
    let mut total = 0;
    let mut expected = None;
    let mut first_catalog = None;
    // the joined groups: (object, name, on), and each document's list
    let mut kept: Vec<(ObjectId, Vec<u8>, bool)> = Vec::new();
    let mut lists: Vec<(String, Vec<ObjectId>)> = Vec::new();
    let mut dropped: Vec<ObjectId> = Vec::new();
    let merge = layers.is_some_and(|l| l.merge);
    for (i, bytes) in documents.iter().enumerate() {
        let mut doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
        let intent = profile(&doc)?;
        if i == 0 { expected = intent; }
        else if intent != expected { return Err("book documents have different output profiles".into()); }
        doc.renumber_objects_with(output.max_id + 1);
        let own = groups(&doc);
        if merge {
            // a group of a name already joined: this document's marks point at that one
            let mut to = HashMap::new();
            for (id, name, on) in &own {
                match kept.iter_mut().find(|k| &k.1 == name) {
                    Some(k) => { k.2 |= *on; to.insert(*id, k.0); dropped.push(*id); }
                    None => kept.push((*id, name.clone(), *on)),
                }
            }
            if !to.is_empty() { doc.objects.values_mut().for_each(|o| remap(o, &to)); }
        } else if !own.is_empty() {
            let name = layers.and_then(|l| l.document_names.get(i)).cloned().unwrap_or_else(|| format!("Document {}", i + 1));
            lists.push((name, own.iter().map(|g| g.0).collect()));
            kept.extend(own);
        }
        let catalog = doc.catalog().map_err(|e| e.to_string())?.clone();
        let root = catalog.get(b"Pages").and_then(Object::as_reference).map_err(|e| e.to_string())?;
        total += doc.get_pages().len();
        doc.get_object_mut(root).and_then(Object::as_dict_mut).map_err(|e| e.to_string())?.set("Parent", pages_id);
        kids.push(Object::Reference(root));
        if i == 0 {
            first_catalog = Some(catalog);
            output.trailer = doc.trailer.clone();
        }
        output.max_id = doc.max_id;
        output.objects.extend(doc.objects);
    }
    for id in dropped { output.objects.remove(&id); }
    output.objects.insert(pages_id, dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => total as i64}.into());
    let mut catalog = first_catalog.ok_or("missing book catalog")?;
    catalog.set("Pages", pages_id);
    catalog.remove(b"OCProperties");
    if !kept.is_empty() {
        let order: Vec<Object> = if merge {
            let ids: Vec<ObjectId> = kept.iter().map(|k| k.0).collect();
            vec![order_list(layers.map_or("Book", |l| l.book_name), &ids)]
        } else {
            lists.iter().map(|(name, ids)| order_list(name, ids)).collect()
        };
        let refs = |on: bool| Object::Array(kept.iter().filter(|k| k.2 == on).map(|k| Object::Reference(k.0)).collect());
        catalog.set("OCProperties", dictionary! {
            "OCGs" => Object::Array(kept.iter().map(|k| Object::Reference(k.0)).collect()),
            "D" => dictionary! {"Name" => Object::string_literal("Default"), "ON" => refs(true), "OFF" => refs(false), "Order" => Object::Array(order), "RBGroups" => Object::Array(Vec::new())},
        });
    }
    let catalog_id = output.add_object(catalog);
    output.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    output.save_to(&mut bytes).map_err(|e| e.to_string())?;
    Ok((bytes, total))
}
