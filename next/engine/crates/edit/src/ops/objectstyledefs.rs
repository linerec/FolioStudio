//! Object style definitions and their command registration.
use super::*;
use serde_json::{Map, Value};
mod properties;
mod deletion;
use deletion::delete;
mod snapshot;
mod text_snapshot;
mod groups;
mod defaults;
mod propagation;
use defaults::set_default;
use groups::{copy_to_group, create_group, delete_group, move_style, rename_group, sort_styles};

const PART: &str = "Resources/Styles.xml";
const NONE: &str = "ObjectStyle/$ID/[None]";
const GRAPHICS: &str = "ObjectStyle/$ID/[Normal Graphics Frame]";

macro_rules! definitions {
    ($($variant:ident { $($field:ident: $ty:ty),* } => $handler:ident;)+) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
        pub enum Definition { $($variant { $($field: $ty),* }),+ }
        pub(super) fn run(w: &mut Work<'_>, command: &Definition) -> Result<(), EditError> {
            match command { $(Definition::$variant { $($field),* } => $handler(w, $($field),*)),+ }
        }
    };
}
definitions! {
    CreateObjectStyle { name: String, based_on: Option<String>, from: Option<String>, props: Map<String, Value> } => create;
    EditObjectStyle { style: String, rename: Option<String>, props: Map<String, Value> } => edit;
    DuplicateObjectStyle { style: String, name: String } => duplicate;
    DeleteObjectStyle { style: String, replacement: String } => delete;
    RedefineObjectStyle { style: String, from: String } => redefine;
    ClearOverridesNotInStyle { ids: Vec<String> } => clear_not_in_style;
    CreateObjectStyleGroup { name: String, parent: Option<String> } => create_group;
    MoveObjectStyle { style: String, group: Option<String> } => move_style;
    CopyObjectStyleToGroup { style: String, group: String } => copy_to_group;
    SetDefaultFrameStyle { kind: String, style: String } => set_default;
    SortObjectStyles { } => sort_styles;
    BreakLinkToStyle { ids: Vec<String> } => break_link;
    // Style Group Options… of an object style group (`group`: its Self or its path "Grp:Sub")
    RenameObjectStyleGroup { group: String, name: String } => rename_group;
    DeleteObjectStyleGroup { group: String } => delete_group;
}

fn scan(w: &Work<'_>) -> Result<(String, Index), EditError> {
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    Ok((t, ix))
}

fn find(t: &str, ix: &Index, id: &str) -> Result<usize, EditError> {
    ix.by_self(t, id).filter(|&i| ix.local(t, i) == "ObjectStyle")
        .ok_or_else(|| EditError::new("notFound", format!("object style {id}")))
}

fn new_id(t: &str, ix: &Index, name: &str, old: Option<&str>) -> Result<String, EditError> {
    if name.trim().is_empty() || name.chars().any(|c| c.is_control() || c == '/') || name.starts_with('$') {
        return Err(EditError::new("invalid", "invalid object style name"));
    }
    let groups = old.map(|id| groups::split_style(id).0).unwrap_or_default();
    let (id, _) = groups::style_attrs(&groups, name);
    if Some(id.as_str()) != old && ix.by_self(t, &id).is_some() {
        return Err(EditError::new("invalid", "object style name exists"));
    }
    Ok(id)
}

fn mutable(style: &str) -> Result<(), EditError> {
    if style.contains("/$ID/") {
        return Err(EditError::new("unsupported", "built-in object style editing is not measured"));
    }
    if style.matches('/').count()!=1 {return Err(EditError::new("unsupported","grouped object style management is not measured"));}
    Ok(())
}

fn local_edit(text: &mut String, mut edits: Vec<(std::ops::Range<usize>, String)>) {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    for (range, value) in edits { text.replace_range(range, &value); }
}

fn set_attr(text: &mut String, path: &[&str], key: &str, value: &str) -> Result<(), EditError> {
    let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    let root = 0;
    let i = if path.is_empty() { Some(root) } else { ix.path(text, root, path) }
        .ok_or_else(|| EditError::new("unsupported", format!("missing style options {}", path.join("/"))))?;
    let edit = attr_edit(&ix, text, i, key, value);
    local_edit(text, vec![edit]);
    Ok(())
}

fn based(text: &mut String, base: &str) -> Result<(), EditError> {
    let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    let i = ix.path(text, 0, &["Properties", "BasedOn"])
        .ok_or_else(|| EditError::new("unsupported", "object style has no base field"))?;
    let n = &ix.nodes[i];
    let (ty, value) = if base == NONE { ("string", "$ID/[None]") } else { ("object", base) };
    text.replace_range(n.start..n.end, &format!("<BasedOn type=\"{ty}\">{}</BasedOn>", escape(value)));
    Ok(())
}

fn append(w: &mut Work<'_>, t: &str, ix: &Index, text: String) -> Result<(), EditError> {
    let group = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "RootObjectStyleGroup")
        .ok_or_else(|| EditError::new("xml", "no root object style group"))?;
    let at = ix.nodes[group].close_start;
    let at = t[..at].rfind('\n').map_or(at, |i| i + 1);
    w.splice(PART, vec![(at..at, format!("\t\t{text}\n"))])
}

fn create(w: &mut Work<'_>, name: &str, base: &Option<String>, from: &Option<String>, props: &Map<String, Value>) -> Result<(), EditError> {
    properties::validate_resources(w,props)?;
    let (t, ix) = scan(w)?;
    let id = new_id(&t, &ix, name, None)?;
    let source = from.as_deref().map(|id| text_snapshot::source_style(w, id)).transpose()?.flatten();
    let base = base.as_deref().unwrap_or(source.as_deref().unwrap_or(NONE));
    find(&t, &ix, base)?;
    let i = find(&t, &ix, source.as_deref().unwrap_or(GRAPHICS))?;
    let mut text = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
    set_attr(&mut text, &[], "Self", &id)?;
    set_attr(&mut text, &[], "Name", name)?;
    based(&mut text, base)?;
    if source.is_some() { text_snapshot::capture(w, from.as_deref().unwrap(), &mut text, true)?; }
    if source.is_none() && let Some(id) = from { properties::apply(&mut text, &snapshot::item(w,id)?)?; }
    properties::apply(&mut text, props)?;
    if source.is_some() {
        let last = if source.as_deref().is_some_and(|s| !s.contains("/$ID/")) { Some(i) } else {
            (0..ix.nodes.len()).rfind(|&i| ix.local(&t, i) == "ObjectStyle" && ix.value(&t, i, "Self").is_some_and(|s| s.contains("/$ID/")))
        };
        if let Some(last) = last {
            let at = ix.nodes[last].end;
            return w.splice(PART, vec![(at..at, format!("\n\t\t{text}"))]);
        }
    }
    append(w, &t, &ix, text)
}

fn duplicate(w: &mut Work<'_>, style: &str, name: &str) -> Result<(), EditError> {
    mutable(style)?;
    let (t, ix) = scan(w)?;
    let id = new_id(&t, &ix, name, None)?;
    let i = find(&t, &ix, style)?;
    let mut text = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
    set_attr(&mut text, &[], "Self", &id)?;
    set_attr(&mut text, &[], "Name", name)?;
    append(w, &t, &ix, text)
}

fn edit(w: &mut Work<'_>, style: &str, rename: &Option<String>, props: &Map<String, Value>) -> Result<(), EditError> {
    mutable(style)?;
    properties::validate_resources(w, props)?;
    let (xml, index) = scan(w)?;
    let item = find(&xml, &index, style)?;
    let mut replacement = xml[index.nodes[item].start..index.nodes[item].end].to_owned();
    let mut options = props.clone();
    if let Some(value) = options.remove("basedOn") {
        let base = value.as_str().ok_or_else(|| EditError::new("invalid", "expected an object style"))?;
        let mut parent = base.to_owned();
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let node = find(&xml, &index, &parent)?;
            if parent == style || !seen.insert(parent.clone()) {
                return Err(EditError::new("invalid", "cyclic object style bases"));
            }
            if parent == NONE { break; }
            parent = deletion::base(&xml, &index, node);
        }
        based(&mut replacement, base)?;
    }
    properties::apply(&mut replacement, &options)?;
    let renamed = rename.as_ref().map(|name| new_id(&xml, &index, name, Some(style))).transpose()?;
    if let (Some(name), Some(id)) = (rename, &renamed) {
        let (groups, _) = groups::split_style(style);
        let (_, full_name) = groups::style_attrs(&groups, name);
        set_attr(&mut replacement, &[], "Name", &full_name)?;
        set_attr(&mut replacement, &[], "Self", id)?;
    }
    propagation::preserve_opacity(w, style, &xml, &index, item, &replacement)?;
    propagation::update(w, &xml, &index, item, &replacement)?;
    replace(w, style, replacement)?;
    if let Some(id) = renamed.filter(|id| id != style) {
        references(w, style, &id)?;
    }
    recompose(w);
    Ok(())
}

fn replace(w: &mut Work<'_>, style: &str, replacement: String) -> Result<(), EditError> {
    let (xml, index) = scan(w)?;
    let item = &index.nodes[find(&xml, &index, style)?];
    w.splice(PART, vec![(item.start..item.end, replacement)])
}

fn references(w: &mut Work<'_>, from: &str, to: &str) -> Result<(), EditError> {
    let parts: Vec<String> = w.ed.parts.keys().cloned().collect();
    for part in parts {
        let t = w.text(&part)?.to_owned();
        if !t.contains(&escape(from)) { continue; }
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for (i, n) in ix.nodes.iter().enumerate() {
            for key in ["AppliedObjectStyle", "AppliedGraphicObjectStyle", "AppliedTextObjectStyle"] {
                if ix.value(&t, i, key).as_deref() == Some(from) { edits.push(attr_edit(&ix, &t, i, key, to)); }
            }
            if ix.local(&t, i) == "BasedOn" && crate::xml::unescape(element_text(&ix, &t, i)) == from {
                edits.push((n.open_end..n.close_start, escape(to)));
            }
        }
        if !edits.is_empty() { w.splice(&part, edits)?; }
    }
    Ok(())
}

fn recompose(w: &mut Work<'_>) {
    let stories: Vec<String> = w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
    for story in stories { w.recompose(&story); }
}

fn redefine(w: &mut Work<'_>, style: &str, from: &String) -> Result<(), EditError> {
    mutable(style)?;
    let (xml, index) = scan(w)?;
    let item = find(&xml, &index, style)?;
    let (part, source_index, source) = w.find(from)?;
    super::textpath::unlocked(w, &source_index, w.text(&part)?, source)?;
    let source_style = text_snapshot::source_style(w, from)?;
    let same_style = source_style.as_deref() == Some(style);
    let mut replacement = xml[index.nodes[item].start..index.nodes[item].end].to_owned();
    if same_style || text_snapshot::is_text_frame(w, from)? {
        text_snapshot::capture(w, from, &mut replacement, false)?;
        propagation::update_redefined(w, &xml, &index, item, &replacement)?;
    } else {
        properties::apply(&mut replacement, &snapshot::item(w, from)?)?;
        propagation::update(w, &xml, &index, item, &replacement)?;
    }
    replace(w, style, replacement)?;
    if same_style {
        objectstyle::apply_after_redefine(w, std::slice::from_ref(from), style)?;
    } else {
        objectstyle::apply(w, std::slice::from_ref(from), style, true)?;
    }
    recompose(w);
    Ok(())
}

fn break_link(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    objectstyle::break_link(w, ids)
}

fn clear_not_in_style(w: &mut Work<'_>, ids: &[String]) -> Result<(),EditError> {
    objectstyle::clear_not_in_style(w,ids)
}

impl crate::Editor {
    pub fn object_style_info(&self, id: &str) -> Result<Value,EditError> {
        let t=self.parts.get(PART).ok_or_else(||EditError::new("xml","no styles"))?;
        let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;let i=find(t,&ix,id)?;
        let props=properties::read(t,&ix,i);
        let base=ix.path(t,i,&["Properties","BasedOn"]).map(|b|crate::xml::unescape(element_text(&ix,t,b))).unwrap_or_else(||"$ID/[None]".into());
        let base=if base.starts_with("ObjectStyle/"){base}else{format!("ObjectStyle/{base}")};
        Ok(serde_json::json!({"id":id,"name":ix.value(t,i,"Name"),"basedOn":base,"props":props,"protected":id.contains("/$ID/")}))
    }
}
