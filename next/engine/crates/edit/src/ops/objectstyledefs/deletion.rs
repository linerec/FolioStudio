//! Removing a definition, rebasing dependent styles and assigning its replacement.
use super::*;

pub(super) fn base(t: &str, ix: &Index, i: usize) -> String {
    ix.path(t,i,&["Properties","BasedOn"]).map(|j| {
        let s = crate::xml::unescape(element_text(ix,t,j));
        if s.starts_with("ObjectStyle/") { s } else { format!("ObjectStyle/{s}") }
    }).unwrap_or_else(|| NONE.to_owned())
}

pub(super) fn rebase_node(t: &str, ix: &Index, node: usize, old: usize, new: Option<usize>, edits: &mut Vec<(std::ops::Range<usize>, String)>) {
    rebase_node_skipping(t, ix, node, old, new, edits, &[])
}

/// `rebase_node` that leaves the attributes and groups named in `skip` of the dependent style alone (Redefine Style, see `propagation::update_redefined`).
pub(super) fn rebase_node_skipping(t: &str, ix: &Index, node: usize, old: usize, new: Option<usize>, edits: &mut Vec<(std::ops::Range<usize>, String)>, skip: &[&str]) {
    for attr in ix.attrs(t,node) {
        let key = &t[attr.name.0..attr.name.1];
        if key.starts_with("Enable") || ["Self","Name","ExtendedKeyboardShortcut","KeyboardShortcut","ApplyNextParagraphStyle"].contains(&key) || skip.contains(&key) { continue; }
        if key=="OverprintFill" && ix.attr(t,node,"FillColor")!=ix.attr(t,old,"FillColor") { continue; }
        if ix.attr(t,node,key) != ix.attr(t,old,key) { continue; }
        match new.and_then(|i| ix.value(t,i,key)) {
            Some(value) => if ix.value(t,node,key).as_deref()!=Some(&value) { edits.push(attr_edit(ix,t,node,key,&value)); },
            None => edits.extend(attr_removal(ix,t,node,key)),
        }
    }
    for &child in ix.children(node) {
        let name = ix.local(t,child);
        if name=="BasedOn" || name.starts_with("ObjectStyle") || skip.contains(&name) { continue; }
        let Some(&oldchild) = ix.named(t,old,name).first() else { continue; };
        let newchild = new.and_then(|n|ix.named(t,n,name).first().copied());
        let c=&ix.nodes[child];let o=&ix.nodes[oldchild];
        if name!="Properties" && t[c.start..c.end]==t[o.start..o.end] {
            if let Some(n)=newchild { let n=&ix.nodes[n]; edits.push((c.start..c.end,t[n.start..n.end].to_owned())); }
        } else { rebase_node_skipping(t,ix,child,oldchild,newchild,edits,skip); }
    }
}

pub(super) fn delete(w: &mut Work<'_>, style: &String, replacement: &String) -> Result<(), EditError> {
    mutable(style)?;
    if style==replacement { return Err(EditError::new("invalid","cannot replace a style by itself")); }
    let (t,ix)=scan(w)?;
    let source=find(&t,&ix,style)?;
    find(&t,&ix,replacement)?;
    let mut ancestor=base(&t,&ix,find(&t,&ix,replacement)?);
    let mut visited=std::collections::BTreeSet::new();
    let mut depth=1;
    while ancestor!=NONE {
        if !visited.insert(ancestor.clone()) {return Err(EditError::new("invalid","cyclic object style bases"));}
        if ancestor==*style {
            if depth>1 {return Err(EditError::new("unsupported","replacement by an indirect descendant is not measured"));}
            break;
        }
        ancestor=base(&t,&ix,find(&t,&ix,&ancestor)?);depth+=1;
    }
    let fallback=base(&t,&ix,source);
    let mut edits=Vec::new();
    for i in 0..ix.nodes.len() {
        if ix.local(&t,i)!="ObjectStyle" || base(&t,&ix,i)!=*style { continue; }
        let next=if ix.value(&t,i,"Self").as_deref()==Some(replacement) { &fallback } else { replacement };
        let nextnode=find(&t,&ix,next)?;
        rebase_node(&t,&ix,i,source,Some(nextnode),&mut edits);
        let b=ix.path(&t,i,&["Properties","BasedOn"]).unwrap();let n=&ix.nodes[b];
        let (ty,value)=if next==NONE {("string","$ID/[None]")} else {("object",next.as_str())};
        edits.push((n.start..n.end,format!("<BasedOn type=\"{ty}\">{}</BasedOn>",escape(value))));
    }
    if !edits.is_empty() {w.splice(PART,edits)?;}
    let mut users=Vec::new();
    for t in w.ed.parts.values() {
        if !t.contains(&escape(style)) {continue;}
        let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
        for i in 0..ix.nodes.len() {
            if is_item(&ix,t,i) && ix.value(t,i,"AppliedObjectStyle").as_deref()==Some(style) {
                users.push(ix.value(t,i,"Self").unwrap());
            }
        }
    }
    for id in &users {
        objectstyle::apply(w,std::slice::from_ref(id),replacement,false)?;
        let(part,ix,i)=w.find(id)?;let t=w.text(&part)?.to_owned();
        if replacement!=NONE && stroke_attr(w,&ix,&t,i,"StrokeColor").as_deref()==Some("Swatch/None") {
            set_paint(w,id,None,None,Some("Swatch/None"),None,None)?;
        }
    }
    references(w,style,replacement)?;
    let (t,ix)=scan(w)?;let i=find(&t,&ix,style)?;
    w.splice(PART,vec![(element_span(&ix,&t,i),String::new())])?;
    recompose(w);
    Ok(())
}
