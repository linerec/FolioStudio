use super::*;
use std::collections::BTreeMap;

pub(crate) fn load(w: &mut Work<'_>, parts: &BTreeMap<String, String>) -> Result<(), EditError> {
    let source = clipboard::Source::of(&crate::Editor::new(parts.clone()));
    let dm = parts.get("designmap.xml").ok_or_else(|| EditError::new("invalid", "missing source designmap"))?;
    let ix = Index::scan(dm).map_err(|e|EditError::new("xml",e))?;
    let mut spreads = Vec::new();
    let mut map = Vec::new();
    for i in 0..ix.nodes.len() {
        if ix.local(dm,i) != "MasterSpread" {continue;}
        let Some(part) = ix.attr(dm,i,"src") else {continue};
        let text = parts.get(part).ok_or_else(||EditError::new("invalid","missing parent definition"))?;
        let sx = Index::scan(text).map_err(|e|EditError::new("xml",e))?;
        let root = (0..sx.nodes.len()).find(|&j|sx.local(text,j)=="MasterSpread" && sx.attr(text,j,"Self").is_some()).ok_or_else(||EditError::new("xml","missing parent"))?;
        let n = &sx.nodes[root];
        let xml = text[n.start..n.end].to_owned();
        map.push((sx.attr(text,root,"Self").unwrap().to_owned(),w.new_id()));
        spreads.push(xml);
    }
    if spreads.is_empty() { return Err(EditError::new("invalid","no parent pages in source")); }
    layer_map(w, dm, &ix, &mut map)?;
    let before = w.parts_named("");
    let mut importer = xdoc::Importer::new(&source);
    for xml in spreads {
        let sx = Index::scan(&xml).map_err(|e|EditError::new("xml",e))?;
        let name = sx.value(&xml,0,"Name").unwrap_or_default();
        let existing = parents::parents(w)?;
        let clash = existing.iter().any(|(p,_,_)|w.text(p).is_ok_and(|t|Index::scan(t).is_ok_and(|ix|(0..ix.nodes.len()).any(|j|ix.local(t,j)=="MasterSpread"&&ix.value(t,j,"Name").as_deref()==Some(&name)))));
        let prefix = if clash {parents::free_prefix(&existing.iter().map(|p|p.2.clone()).collect::<Vec<_>>())} else {sx.value(&xml,0,"NamePrefix").unwrap_or_default()};
        let mut edits = Vec::new();
        if clash { edits.push(attr_edit(&sx,&xml,0,"BaseName","Parent")); }
        for j in (0..sx.nodes.len()).filter(|&j|sx.local(&xml,j)=="MarginPreference") {
            let page = sx.nodes[j].parent.ok_or_else(||EditError::new("xml","parent margins have no page"))?;
            let bounds: Vec<f64> = sx.attr(&xml,page,"GeometricBounds").unwrap_or("").split_whitespace().filter_map(|n|n.parse().ok()).collect();
            if bounds.len()!=4 {return Err(EditError::new("xml","parent page has no bounds"));}
            let mut values = BTreeMap::new();
            for key in ["Top","Bottom","Left","Right","ColumnCount","ColumnGutter","ColumnDirection"] {
                if let Some(v)=parents::doc_pref(w,"MarginPreference",key) {
                    edits.push(attr_edit(&sx,&xml,j,key,&v));values.insert(key,v);
                }
            }
            let num = |key:&str,default:f64| values.get(key).and_then(|v|v.parse::<f64>().ok()).unwrap_or(default);
            let positions=parents::column_positions(bounds[3]-bounds[1]-num("Left",36.0)-num("Right",36.0),num("ColumnCount",1.0) as u32,num("ColumnGutter",12.0));
            edits.push(attr_edit(&sx,&xml,j,"ColumnsPositions",&positions));
        }
        edits.sort_by_key(|e|std::cmp::Reverse(e.0.start));
        let mut xml=xml;
        for (range,value) in edits {xml.replace_range(range,&value);}
        let xml = importer.rewrite(w,&xml)?;
        parents::copy_parent_from(w,&xml,&source,map.clone(),&prefix)?;
    }
    for p in w.parts_named("Stories/").into_iter().filter(|p|!before.contains(p)) {
        let text = w.text(&p)?.to_owned();
        let imported = importer.rewrite(w,&text)?;
        w.splice(&p,vec![(0..text.len(),imported)])?;
    }
    Ok(())
}

/// Layers are matched by name; incoming definitions never keep the source path. Adds (source id, document id) pairs to `map`.
pub(crate) fn layer_map(w: &mut Work<'_>, dm: &str, ix: &Index, map: &mut Vec<(String, String)>) -> Result<(), EditError> {
    for i in (0..ix.nodes.len()).filter(|&i|ix.local(dm,i)=="Layer") {
        let old = ix.attr(dm,i,"Self").unwrap_or_default();
        let name = ix.value(dm,i,"Name").unwrap_or_default();
        let target = w.text("designmap.xml")?.to_owned();
        let tx = Index::scan(&target).map_err(|e|EditError::new("xml",e))?;
        let found = (0..tx.nodes.len()).find(|&j|tx.local(&target,j)=="Layer" && tx.value(&target,j,"Name").as_deref()==Some(&name));
        let id = if let Some(j)=found {tx.attr(&target,j,"Self").unwrap().to_owned()} else {
            let id = w.new_id();
            let mut xml = dm[ix.nodes[i].start..ix.nodes[i].end].to_owned();
            let lx = Index::scan(&xml).map_err(|e|EditError::new("xml",e))?;
            let (range,value)=attr_edit(&lx,&xml,0,"Self",&id);xml.replace_range(range,&value);
            let at = (0..tx.nodes.len()).rfind(|&j|tx.local(&target,j)=="Layer").map(|j|tx.nodes[j].end).unwrap_or(tx.nodes[0].open_end);
            w.splice("designmap.xml",vec![(at..at,format!("\n\t{xml}"))])?;
            id
        };
        map.push((old.to_owned(),id));
    }
    Ok(())
}
