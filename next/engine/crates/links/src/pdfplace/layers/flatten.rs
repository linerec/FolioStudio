//! Remove optional-content markers and suppress hidden painting without dropping state changes.
use super::*;
use lopdf::content::{Content, Operation};

fn inherited(doc: &Document, mut id: ObjectId) -> Result<Dictionary,String> {
    for _ in 0..64 {
        let d = doc.get_dictionary(id).map_err(|e| e.to_string())?;
        if let Ok(r) = d.get(b"Resources") { return dict(doc,r).cloned(); }
        let Ok(parent) = d.get(b"Parent").and_then(|p| p.as_reference()) else { return Ok(Dictionary::new()) };
        id = parent;
    }
    Err("Cyclic PDF page tree".into())
}

pub(super) fn run(doc: &mut Document, state: &BTreeMap<ObjectId,bool>) -> Result<(),String> {
    let source = doc.clone();
    for (_,id) in source.get_pages() {
        let resources = inherited(&source,id)?;
        let data = source.get_page_content_with_limit(id,256<<20).map_err(|e| e.to_string())?;
        let data = rewrite(&source,&data,&resources,state)?;
        let stream = doc.add_object(lopdf::Stream::new(Dictionary::new(),data));
        doc.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Contents",stream);
    }
    for (id,o) in &source.objects {
        let Object::Stream(s) = o else { continue };
        let form = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Form");
        let pattern = s.dict.get(b"PatternType").ok().and_then(|o| o.as_i64().ok()) == Some(1);
        if !form && !pattern { continue; }
        let resources = s.dict.get(b"Resources").ok().map(|o| dict(&source,o)).transpose()?.cloned().unwrap_or_default();
        let raw = if s.dict.has(b"Filter") { s.decompressed_content().map_err(|e| e.to_string())? } else { s.content.clone() };
        let data = rewrite(&source,&raw,&resources,state)?;
        let target = doc.get_object_mut(*id).map_err(|e| e.to_string())?.as_stream_mut().map_err(|e| e.to_string())?;
        target.set_plain_content(data);
        target.dict.remove(b"OC");
    }
    // The visibility is now baked into paint operations. Resources no longer need OCGs.
    for object in doc.objects.values_mut() {
        let d = match object { Object::Dictionary(d) => d, Object::Stream(s) => &mut s.dict, _ => continue };
        d.remove(b"OC");
        d.remove(b"OCProperties");
        if let Ok(Object::Dictionary(properties)) = d.get_mut(b"Properties") {
            let keys = properties.iter().filter(|(_,value)| value.as_reference().is_ok_and(|id| state.contains_key(&id))).map(|(key,_)| key.clone()).collect::<Vec<_>>();
            for key in keys { properties.remove(&key); }
        }
    }
    Ok(())
}

fn resource<'a>(doc: &'a Document, resources: &'a Dictionary, category: &[u8], name: &[u8]) -> Result<&'a Object,String> {
    dict(doc,resources.get(category).map_err(|e| e.to_string())?)?.get(name).map_err(|e| e.to_string())
}

fn rewrite(doc: &Document, data: &[u8], resources: &Dictionary, state: &BTreeMap<ObjectId,bool>) -> Result<Vec<u8>,String> {
    let content = Content::decode(data).map_err(|e| format!("PDF layer content: {e}"))?;
    let mut out = Vec::new();
    let mut stack = Vec::new();
    let mut text_mode = 0;
    let mut modes = Vec::new();
    for mut op in content.operations {
        let showing = stack.last().map(|(v,_)| *v).unwrap_or(true);
        match op.operator.as_str() {
            "BDC" | "BMC" => {
                let oc = op.operands.first().is_some_and(|o| o.as_name().ok() == Some(b"OC"));
                let v = if oc {
                    let o = op.operands.get(1).ok_or("PDF layer marker without properties")?;
                    let o = if let Ok(name) = o.as_name() { resource(doc,resources,b"Properties",name)? } else { o };
                    visible(doc,o,state)?
                } else { true };
                stack.push((showing && v,oc));
                if oc { continue; }
            }
            "EMC" if stack.pop().ok_or("Unbalanced PDF layer marker")?.1 => continue,
            "q" => modes.push(text_mode),
            "Q" => text_mode = modes.pop().ok_or("Unbalanced PDF graphics state")?,
            "Tr" => text_mode = op.operands.first().and_then(|o| o.as_i64().ok()).ok_or("Invalid PDF text mode")?,
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" if !showing => op = Operation::new("n",vec![]),
            "sh" if !showing => continue,
            "Do" => {
                let name = op.operands.first().and_then(|o| o.as_name().ok()).ok_or("Invalid PDF XObject")?;
                let object = object(doc,resource(doc,resources,b"XObject",name)?)?;
                let d = object.as_stream().map(|s| &s.dict).map_err(|e| e.to_string())?;
                if !showing || d.get(b"OC").ok().map(|o| visible(doc,o,state)).transpose()?.is_some_and(|v| !v) { continue; }
            }
            "Tj" | "TJ" | "'" | "\"" if !showing => {
                out.push(Operation::new("Tr",vec![3.into()]));
                out.push(op);
                out.push(Operation::new("Tr",vec![text_mode.into()]));
                continue;
            }
            "BI" | "ID" | "EI" => return Err("Inline images in layered PDF are not supported yet".into()),
            _ => {}
        }
        out.push(op);
    }
    if !stack.is_empty() || !modes.is_empty() { return Err("Unbalanced layered PDF content".into()); }
    Content { operations:out }.encode().map_err(|e| e.to_string())
}
