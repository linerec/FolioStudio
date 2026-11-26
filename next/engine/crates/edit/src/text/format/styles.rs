//! Style definitions and references.
use super::*;
use crate::{Command, Editor};
mod identity;
pub(crate) mod groups;
pub(crate) mod group_manage;
mod definition;
pub(crate) mod grep;
pub(crate) mod nested;
pub(crate) mod shortcut;
const PART: &str = "Resources/Styles.xml";
const SOURCE: &[&str] = &["appliedFont", "fontStyle", "pointSize", "leading", "tracking",
    "baselineShift", "underline", "strikeThru", "capitalization", "justification",
    "leftIndent", "rightIndent", "firstLineIndent", "spaceBefore", "spaceAfter"];
const BASIC_OPTIONS: &[&str] = &["kerningMethod", "ligatures", "noBreak", "position", "horizontalScale", "verticalScale", "skew", "appliedLanguage"];

fn kind(k: &str) -> Result<&'static str, EditError> {
    match k { "paragraph"=>Ok("ParagraphStyle"), "character"=>Ok("CharacterStyle"),
        _=>Err(EditError::new("invalid","style kind must be paragraph or character")) }
}
fn none(tag: &str) -> String {
    format!("{tag}/$ID/[No {} style]",if tag=="ParagraphStyle" {"paragraph"} else {"character"})
}
fn reference(id: &str, tag: &str) -> Property {
    if id==none(tag) { property("BasedOn","string",id.split_once('/').unwrap().1) }
    else { property("BasedOn","object",id) }
}
fn based(a: &Attrs, tag: &str) -> String {
    let b=val(a,"BasedOn");
    if b.is_empty() {none(tag)} else if b.starts_with("$ID/") {format!("{tag}/{b}")} else {b.into()}
}
fn protected(id: &str) -> bool { id.contains("/$ID/") }
/// `[No paragraph style]` and `[None]`: nothing of them can be changed ("Invalid request on a root style."); `[Basic Paragraph]` takes values.
fn root(id: &str) -> bool { id.ends_with("/$ID/[No paragraph style]") || id.ends_with("/$ID/[No character style]") }
/// The panel name of a built-in style, as the style options window shows it.
fn shown_name(name: &str) -> &str {
    match name { "$ID/NormalParagraphStyle" => "[Basic Paragraph]", "$ID/[No paragraph style]" => "[No Paragraph Style]", "$ID/[No character style]" => "[None]", other => other }
}
/// `[Basic Paragraph]` edited: the document's text default (`TextDefault` of Preferences.xml) takes the values that changed (oracle bs-edit-*):
/// what the style gives now for each setting that differs between `old` and `new`, in the attribute of that name or in the `Properties` element.
fn mirror_text_default(w: &mut Work<'_>, id: &str, old: &Attrs, new: &Attrs) -> Result<(), EditError> {
    const PREFERENCES: &str = "Resources/Preferences.xml";
    let Ok(t) = w.text(PREFERENCES).map(str::to_owned) else { return Ok(()) };
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let Some(td) = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "TextDefault") else { return Ok(()) };
    let given = Catalog::read(&w.ed.parts)?.resolved(id)?;
    let touched: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).filter(|k| !META.contains(&k.as_str()) && old.get(*k) != new.get(*k)).collect();
    let own: std::collections::BTreeSet<String> = ix.attrs(&t, td).iter().map(|a| t[a.name.0..a.name.1].to_owned()).collect();
    let properties = ix.children(td).iter().copied().find(|&c| ix.local(&t, c) == "Properties");
    let mut edits = Vec::new();
    for key in touched {
        let element = properties.and_then(|p| ix.children(p).iter().copied().find(|&c| ix.local(&t, c) == key.as_str()));
        match (given.get(key), element) {
            (Some(v), Some(c)) if v.xml.is_some() => edits.push((ix.nodes[c].start..ix.nodes[c].end, v.xml.clone().unwrap_or_default())),
            (None, Some(c)) if key == "Leading" => edits.push((ix.nodes[c].start..ix.nodes[c].end, "<Leading type=\"enumeration\">Auto</Leading>".to_owned())),
            (Some(v), None) if v.xml.is_none() && own.contains(key.as_str()) => edits.push(crate::ops::attr_edit(&ix, &t, td, key, &v.value)),
            _ => {}
        }
    }
    if edits.is_empty() { return Ok(()); }
    w.splice(PREFERENCES, edits)
}
fn definition(tag: &str, a: &Attrs, extra: &str) -> String {
    let mut t=format!("<{tag}");
    for (k,v) in a.iter().filter(|(_,v)|v.xml.is_none()) {t+=&format!(" {k}=\"{}\"",escape(&v.value));}
    t.push('>');
    let mut p:Vec<_>=a.iter().filter_map(|(k,v)|v.xml.as_ref().map(|v|(k,v))).collect();
    // style-create-*, style-paragraph-font: InDesign's property element order.
    p.sort_by_key(|(k,_)| (match k.as_str() {"BasedOn"=>0,"PreviewColor"=>1,"Leading"=>2,"AppliedFont"=>3,_=>4},k.replace("AllNestedStyles","AllA")));
    if !p.is_empty() { t+="\n<Properties>"; for (_,v) in p {t+="\n";t+=v;} t+="\n</Properties>"; }
    t+=extra; t+=&format!("\n</{tag}>"); t
}
fn put(w:&mut Work<'_>,id:&str,tag:&str,a:&Attrs) -> Result<(),EditError> {
    let t=w.text(PART)?;
    let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
    let i=ix.by_self(t,id).ok_or_else(||EditError::new("notFound",id))?;
    if let Some(edits)=definition::attribute_edits(t,&ix,i,a) {
        return w.splice(PART,edits);
    }
    let extra=ix.children(i).iter().filter(|&&c|ix.local(t,c)!="Properties")
        .map(|&c| &t[ix.nodes[c].start..ix.nodes[c].end]).collect::<Vec<_>>().join("\n");
    let replacement=definition(tag,a,&extra);
    w.splice(PART,vec![(ix.nodes[i].start..ix.nodes[i].end,replacement)])
}
fn source(w:&mut Work<'_>,story:&str,start:usize,end:usize,tag:&str)->Result<(Attrs,String),EditError> {
    let (part,ix,i,id)=locate(w,story)?;
    let s=Story::read(w.text(&part)?,&ix,i)?; validate(&s,start,end)?;
    // A range starts with its first character; an insertion point uses typing format.
    let (p,c)=if start<end {(s.atoms[start].p,s.atoms[start].c)} else {inherit(&s,start)};
    let pa=style_attrs(&s.paragraphs[p]);
    let cs=if start==end { w.ed.text_state.get(&id).filter(|x|x.at==start).map(|x|&x.character).unwrap_or(&s.characters[c]) } else {&s.characters[c]};
    let ca=style_attrs(cs);
    let cat=Catalog::read(&w.ed.parts)?;
    let mut effective=cat.character(&pa,&ca)?;effective.extend(ca.clone());
    let mut a=Attrs::new();
    for (n,&key) in SOURCE.iter().enumerate() {
        if tag=="CharacterStyle" && n>=9 {break;}
        let key=control_key(key);
        if let Some(v)=effective.get(&key) {a.insert(key,v.clone());}
    }
    if tag=="ParagraphStyle" {
        for p in folio_idml::paragraph_columns::PROPERTIES {
            if let Some(v)=effective.get(p.xml) {a.insert(p.xml.into(),v.clone());}
        }
        for p in folio_idml::paragraph_options::PROPERTIES {
            if let Some(v)=effective.get(p.xml) {a.insert(p.xml.into(),v.clone());}
        }
        if let Some(v)=effective.get("Composer") {a.insert("Composer".into(),v.clone());}
        for p in folio_idml::parcomp::FLAGS {if let Some(v)=effective.get(p.xml) {a.insert(p.xml.into(),v.clone());}}
        for key in ["BulletsAndNumberingListType","NumberingContinue"].into_iter().chain(folio_idml::listprops::PROPERTIES.iter().map(|p|p.xml)) {
            if let Some(v)=effective.get(key) {a.insert(key.into(),v.clone());}
        }
    }
    Ok((a,val(if tag=="ParagraphStyle" {&pa} else {&ca},if tag=="ParagraphStyle" {"AppliedParagraphStyle"} else {"AppliedCharacterStyle"}).into()))
}
fn properties(cat:&Catalog,tag:&str,a:&mut Attrs,props:&BTreeMap<String,Value>) -> Result<(),EditError> {
    cat.list_ref(props)?;
    for (key,v) in props {
        if key=="basedOn" || key=="nextStyle" {
            if key=="nextStyle" && tag!="ParagraphStyle" {return Err(EditError::new("invalid","character style has no next style"));}
            let id=cat.id(tag,v.as_str().ok_or_else(||EditError::new("invalid","style reference required"))?)?;
            a.insert(if key=="basedOn" {"BasedOn"} else {"NextStyle"}.into(),if key=="basedOn" {reference(&id,tag)} else {attr(id)});
        } else {
            if matches!(key.as_str(),"bulletsCharacterStyle"|"numberingCharacterStyle") {
                cat.id("CharacterStyle",v.as_str().ok_or_else(||EditError::new("invalid","character style reference required"))?)?;
            }
            if key=="kerningValue" {return Err(EditError::new("invalid","manual kerning is not a style property"));}
            let (para,key,value)=direct(key,v)?;
            if para && tag!="ParagraphStyle" {return Err(EditError::new("invalid","paragraph property in character style"));}
            if tag=="CharacterStyle" && key=="KerningMethod" {
                a.insert("KerningValue".into(),attr(folio_idml::style_values::AUTOMATIC_KERNING));
            }
            a.insert(key,value);
        }
    }
    if tag=="ParagraphStyle" { columns::normalize(a,&cat.resolved(&based(a,tag))?,props)?; }
    Ok(())
}
fn validate_base(cat:&Catalog,id:&str,a:&Attrs,tag:&str)->Result<(),EditError> {
    let mut next=based(a,tag);let mut seen=vec![id.to_string()];
    while !next.is_empty() {
        if seen.contains(&next) {return Err(EditError::new("invalid","cyclic style inheritance"));}
        seen.push(next.clone());
        let Some(p)=cat.styles.get(&next) else {break};
        let b=val(p,"BasedOn");if b.is_empty(){break;}
        next=based(p,tag);
    }
    Ok(())
}
fn normalize_definition(cat:&Catalog,tag:&str,a:&mut Attrs)->Result<(),EditError> {
    let base=cat.resolved(&based(a,tag))?;
    if tag=="ParagraphStyle" { validate_paragraph_options(a,&base)?; }
    a.retain(|k,v|META.contains(&k.as_str()) || !base.get(k).is_some_and(|b|equal(v,b)));
    Ok(())
}
/// Native style edits remove direct overrides that now equal inherited values,
/// even in other stories; all other raw range properties remain (style-character-size).
pub(crate) fn refresh_stories(w:&mut Work<'_>)->Result<(),EditError> {
    let cat=Catalog::read(&w.ed.parts)?;
    let stories:Vec<_>=w.ed.parts.iter().filter(|(p,_)|p.starts_with("Stories/"))
        .map(|(p,t)|(p.clone(),t.clone())).collect();
    for (part,t) in stories {
        let ix=Index::scan(&t).map_err(|e|EditError::new("xml",e))?;
        let Some(i)=(0..ix.nodes.len()).find(|&i|ix.local(&t,i)=="Story" && ix.attr(&t,i,"Self").is_some()) else {continue};
        let id=ix.value(&t,i,"Self").unwrap_or_default();
        let mut s=match Story::read(&t,&ix,i) {
            Ok(s)=>s,
            // Style definitions can still affect stories with unmeasured inline
            // containers. Retain their exact XML (including redundant overrides).
            Err(e) if e.code=="unsupported"=>{w.recompose(&id);continue;}
            Err(e)=>return Err(e),
        };
        let before=s.serialize();
        for p in &mut s.paragraphs {
            let mut a=style_attrs(p);let base=cat.para(&a)?;
            let composer=a.get("Composer").cloned();
            normalize(&mut a,&base);
            if let Some(v)=composer {a.insert("Composer".into(),v);}
            *p=make_style(p,&a);
        }
        let mut cache=BTreeMap::new();
        for a in &mut s.atoms {
            let key=(a.p,a.c);
            let c=if let Some(&c)=cache.get(&key) {c} else {
                let pa=style_attrs(&s.paragraphs[a.p]);let mut ca=style_attrs(&s.characters[a.c]);
                let base=cat.character(&pa,&ca)?;normalize(&mut ca,&base);
                let c=s.characters.len();s.characters.push(make_style(&s.characters[a.c],&ca));cache.insert(key,c);c
            };a.c=c;
        }
        if s.serialize()!=before {write(w,&part,&id,&s)?;}
        w.recompose(&id);
    }
    // Pending typing formats may have depended on a definition that just changed.
    w.ed.text_state.clear();
    Ok(())
}
pub(crate) fn run(w:&mut Work<'_>,command:&Command)->Result<(),EditError> {
    run_with_refresh(w,command,true)
}
pub(crate) fn run_with_refresh(w:&mut Work<'_>,command:&Command,refresh:bool)->Result<(),EditError> {
    let cat=Catalog::read(&w.ed.parts)?;
    match command {
        Command::StyleCreate{kind:k,name,story,start,end,props,apply,group}=>{
            let tag=kind(k)?;
            if name.trim().is_empty() || name.chars().any(|c|c.is_control()||c=='/') {return Err(EditError::new("invalid","style name is empty or contains a slash/control"));}
            let path=groups::path(group.as_deref());
            let (id,full)=groups::join_id(tag,&path,name);
            if cat.styles.contains_key(&id) {return Err(EditError::new("invalid","style name already exists"));}
            let (mut a,base)=if let Some(s)=story {source(w,s,*start,*end,tag)?} else {(Attrs::new(),none(tag))};
            for (k,v) in [("Self",id.as_str()),("Name",full.as_str()),("Imported","false"),("SplitDocument","false"),("EmitCss","true"),
                ("IncludeClass","true"),("ExtendedKeyboardShortcut","0 0 0"),("KeyboardShortcut","0 0")] {a.insert(k.into(),attr(v));}
            if tag=="ParagraphStyle" {
                a.insert("NextStyle".into(),attr(&id));
                for k in ["EmptyNestedStyles","EmptyLineStyles","EmptyGrepStyles"] {a.insert(k.into(),attr("true"));}
            }
            a.insert("BasedOn".into(),reference(&base,tag));
            a.insert("PreviewColor".into(),property("PreviewColor","enumeration","Nothing"));
            properties(&cat,tag,&mut a,props)?;validate_base(&cat,&id,&a,tag)?;normalize_definition(&cat,tag,&mut a)?;
            if path.is_empty() {
                let t=w.text(PART)?;let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
                let root=format!("Root{tag}Group");
                let i=(0..ix.nodes.len()).find(|&i|ix.local(t,i)==root).ok_or_else(||EditError::new("notFound",root))?;
                let at=ix.nodes[i].close_start;
                w.splice(PART,vec![(at..at,definition(tag,&a,""))])?;
            } else {groups::insert_in(w,tag,&path.join(":"),definition(tag,&a,""))?;}
            crate::text::languages::register(w,props)?;
            if *apply&& let Some(story)=story {apply_style(w,story,*start,*end,&id,tag=="ParagraphStyle",true)?;}
            refresh_stories(w)
        }
        Command::StyleEdit{kind:k,style,props}=>{
            let tag=kind(k)?;let id=cat.id(tag,style)?;
            // the DOM (oracle probe builtin-probe.json): a root style takes nothing, the Basic Paragraph style takes values and a base but not a name
            if root(&id) {return Err(EditError::new("invalid","Invalid request on a root style."));}
            if protected(&id) && props.contains_key("name") {return Err(EditError::new("invalid","Invalid request on a root style."));}
            let formatting=props.iter().filter(|(k,_)|k.as_str()!="name").map(|(k,v)|(k.clone(),v.clone())).collect::<BTreeMap<_,_>>();
            if !formatting.is_empty() {
                let mut a=cat.styles[&id].clone();properties(&cat,tag,&mut a,&formatting)?;
                validate_base(&cat,&id,&a,tag)?;normalize_definition(&cat,tag,&mut a)?;
                put(w,&id,tag,&a)?;
                crate::text::languages::register(w,&formatting)?;
                if protected(&id) {mirror_text_default(w,&id,&cat.styles[&id],&a)?;}
                if refresh {refresh_stories(w)?;} else {w.ed.text_state.clear();}
            }
            if let Some(name)=props.get("name") {identity::rename(w,&cat,&id,name)?;}
            Ok(())
        }
        Command::StyleRedefine{kind:k,style,story,start,end}=>{
            let tag=kind(k)?;let id=cat.id(tag,style)?;
            if root(&id) {return Err(EditError::new("invalid","Invalid request on a root style."));}
            let (values,_)=source(w,story,*start,*end,tag)?;
            let mut a=cat.styles[&id].clone();a.extend(values);
            normalize_definition(&cat,tag,&mut a)?;put(w,&id,tag,&a)?;
            if protected(&id) {mirror_text_default(w,&id,&cat.styles[&id],&a)?;}
            refresh_stories(w)
        }
        Command::StyleDelete{kind:k,style,replacement}=>{
            let tag=kind(k)?;let id=cat.id(tag,style)?;let replacement=cat.id(tag,replacement)?;
            if protected(&id) {return Err(EditError::new("invalid","The default style cannot be deleted."));}
            if id==replacement {return Err(EditError::new("invalid","cannot replace a style with itself"));}
            let t=w.text(PART)?;let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
            let i=ix.by_self(t,&id).unwrap();w.splice(PART,vec![(ix.nodes[i].start..ix.nodes[i].end,String::new())])?;
            // style-delete-parent: prevent the replacement becoming based on itself.
            // All explicit formatting survives; style references change in place.
            let parts:Vec<_>=w.ed.parts.iter().filter(|(p,_)|p.ends_with(".xml")).map(|(p,t)|(p.clone(),t.clone())).collect();
            for (part,t) in parts {
                let ix=Index::scan(&t).map_err(|e|EditError::new("xml",e))?;let mut edits=Vec::new();
                for (i,n) in ix.nodes.iter().enumerate() {
                    for a in ix.attrs(&t,i) {
                        let key=&t[a.name.0..a.name.1];
                        if folio_idml::style_identity::is_reference_attribute(key)
                            && unescape(&t[a.value.0..a.value.1])==id {edits.push((a.value.0..a.value.1,escape(&replacement)));}
                    }
                    // Only object references, never a Content/Label string that
                    // happens to spell the deleted style's identifier.
                    if ix.attr(&t,i,"type")!=Some("object") || n.empty || !ix.children(i).is_empty() || unescape(t[n.open_end..n.close_start].trim())!=id {continue;}
                    if ix.local(&t,i)=="BasedOn" && ix.ancestors(i).any(|p|ix.attr(&t,p,"Self")==Some(&replacement)) {
                        edits.push((n.start..n.end,reference(&none(tag),tag).xml.unwrap()));
                    } else {edits.push((n.open_end..n.close_start,escape(&replacement)));}
                }
                w.splice(&part,edits)?;
            }
            refresh_stories(w)
        }
        _=>Err(EditError::new("invalid","not a style command")),
    }
}

impl Editor {
    pub fn style_info(&self,k:&str,id:&str)->Result<Value,EditError> {
        if k=="object" {return self.object_style_info(id);}
        // the Object Styles panel menu's Default Text Frame Style / Default Graphic Frame Style (the document's, in its preferences)
        if k=="objectDefaults" {let (text,graphic)=self.default_frame_styles();return Ok(serde_json::json!({"text":text,"graphic":graphic}));}
        let tag=kind(k)?;let cat=Catalog::read(&self.parts)?;let id=cat.id(tag,id)?;let a=&cat.styles[&id];
        let resolved=cat.resolved(&id)?;let mut props=serde_json::Map::new();
        for key in SOURCE.iter().chain(BASIC_OPTIONS).copied().chain(folio_idml::opentype::PROPERTIES.iter().map(|p| p.api)) {props.insert(key.into(),api_value(key,val(&resolved,&control_key(key))));}
        if tag=="ParagraphStyle" {
            props.insert("lastLineIndent".into(),api_value("lastLineIndent",val(&resolved,"LastLineIndent")));
            for p in folio_idml::paragraph_options::PROPERTIES { props.insert(p.api.into(),p.decode(val(&resolved,p.xml))); }
            for p in folio_idml::paragraph_columns::PROPERTIES { props.insert(p.api.into(),p.decode(val(&resolved,p.xml))); }
            for p in folio_idml::paragraph_decorations::PROPERTIES {
                props.insert(p.api.into(), p.decode(val(&resolved, p.xml)));
            }
            for p in folio_idml::listprops::PROPERTIES {props.insert(p.api.into(),p.decode(resolved.get(p.xml).map(|v|v.value.as_str()).unwrap_or(p.default)));}
            for key in ["bulletsAndNumberingListType","numberingContinue"] {props.insert(key.into(),api_value(key,val(&resolved,&control_key(key))));}
            for p in folio_idml::paragraph_decorations::PROPERTIES {props.insert(p.api.into(), p.decode(val(&resolved, p.xml)));}
        }
        if tag=="ParagraphStyle" {props.insert("composer".into(),api_value("composer",val(&resolved,"Composer")));}
        if tag=="ParagraphStyle" {for p in folio_idml::parcomp::FLAGS {props.insert(p.api.into(),api_value(p.api,val(&resolved,p.xml)));}}
        let grep = if tag == "ParagraphStyle" {
            folio_idml::grep::rules(self.part(PART).unwrap_or_default(), &id).map_err(|e|EditError::new("xml",e))?
        } else { Vec::new() };
        let nested = if tag == "ParagraphStyle" {
            folio_idml::nested::rules(self.part(PART).unwrap_or_default(), &id).map_err(|e|EditError::new("xml",e))?
        } else { Vec::new() };
        let lines = if tag == "ParagraphStyle" {
            folio_idml::nested::line_rules(self.part(PART).unwrap_or_default(), &id).map_err(|e|EditError::new("xml",e))?
        } else { Vec::new() };
        let shortcut=folio_idml::style_shortcut::Shortcut::from_idml(val(a,"KeyboardShortcut"),val(a,"ExtendedKeyboardShortcut"));
        Ok(serde_json::json!({"id":id,"name":shown_name(val(a,"Name")),"basedOn":based(a,tag),"nextStyle":val(a,"NextStyle"),"props":props,"protected":root(&id),"builtIn":protected(&id),"grepStyles":grep,"nestedStyles":nested,"lineStyles":lines,
            "shortcut":shortcut.map(|s|s.spec()),"shortcutLabel":shortcut.map(|s|s.label())}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const STYLES:&str=r#"<Styles><RootParagraphStyleGroup><ParagraphStyle Self="ParagraphStyle/A" Name="A" PointSize="12"><Properties><Private type="string">keep</Private></Properties><Unknown value="opaque"/></ParagraphStyle><ParagraphStyle Self="ParagraphStyle/B" Name="B"><Properties><BasedOn type="object">ParagraphStyle/A</BasedOn></Properties></ParagraphStyle></RootParagraphStyleGroup><RootCharacterStyleGroup><CharacterStyle Self="CharacterStyle/N" Name="N"/></RootCharacterStyleGroup></Styles>"#;
    const STORY:&str=r#"<Story Self="s"><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/A"><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/N" PointSize="14"><Content>Text</Content></CharacterStyleRange></ParagraphStyleRange></Story>"#;
    fn editor()->Editor {Editor::new(BTreeMap::from([(PART.into(),STYLES.into()),("Stories/s.xml".into(),STORY.into())]))}
    fn edit(props:Value)->Command {serde_json::from_value(serde_json::json!({"op":"styleEdit","kind":"paragraph","style":"A","props":props})).unwrap()}
    #[test]
    fn parjust_style_inheritance_and_invalid_range_rollback() {
        let mut e=editor();
        e.apply("spacing", &[edit(serde_json::json!({"minimumWordSpacing":70,"desiredWordSpacing":110,"maximumWordSpacing":150}))]).unwrap();
        assert_eq!(e.style_info("paragraph","B").unwrap()["props"]["desiredWordSpacing"],110.0);
        let before=e.parts().clone();
        assert!(e.apply("bad", &[edit(serde_json::json!({"maximumWordSpacing":105}))]).is_err());
        assert_eq!(e.parts(),&before);
        e.undo().unwrap();
        assert_eq!(e.style_info("paragraph","B").unwrap()["props"]["desiredWordSpacing"],100.0);
    }
    #[test]
    fn paragraph_rules_preserve_opaque_xml_inherit_and_undo() {
        let mut e=editor();let before=e.parts().clone();
        let out=e.apply("rules",&[edit(serde_json::json!({"ruleAbove":true,"ruleAboveLineWeight":2,"ruleAboveWidth":"TEXT_WIDTH","ruleBelowOffset":-3}))]).unwrap();
        assert!(out.recompose.contains(&"s".into()));
        let child=e.style_info("paragraph","B").unwrap();
        assert_eq!(child["props"]["ruleAboveLineWeight"].as_f64(),Some(2.));
        assert_eq!(child["props"]["ruleAboveWidth"],"TEXT_WIDTH");
        assert!(e.part(PART).unwrap().contains(r#"<Unknown value="opaque"/>"#));
        assert!(e.part(PART).unwrap().contains("keep</Private>"));
        e.undo().unwrap();assert_eq!(&before,e.parts());
    }
    #[test]
    fn split_columns_default_inherit_and_reject_all_atomically() {
        let mut e=editor();let before=e.parts().clone();
        e.apply("split",&[edit(serde_json::json!({"spanColumnType":"SPLIT_COLUMNS"}))]).unwrap();
        let child=e.style_info("paragraph","B").unwrap();
        assert_eq!(child["props"]["spanColumnType"],"SPLIT_COLUMNS");
        assert_eq!(child["props"]["spanSplitColumnCount"],2.0);
        let split=e.parts().clone();
        assert!(e.apply("invalid",&[edit(serde_json::json!({"pointSize":18,"spanSplitColumnCount":"ALL"}))]).is_err());
        assert_eq!(e.parts(),&split);
        assert!(e.part(PART).unwrap().contains(r#"<Unknown value="opaque"/>"#));
        e.undo().unwrap();assert_eq!(e.parts(),&before);
    }
    #[test]
    fn invalid_paragraph_rule_rolls_back_all_style_changes() {
        let mut e=editor();let before=e.parts().clone();
        assert!(e.apply("bad",&[edit(serde_json::json!({"ruleAbove":true})),edit(serde_json::json!({"ruleAboveWidth":"PAGE_WIDTH"}))]).is_err());
        assert_eq!(&before,e.parts());assert!(e.undo().is_none());
    }
    #[test]
    fn cyclic_inheritance_rolls_back_the_entire_style_transaction() {
        let mut e=editor();let before=e.parts().clone();
        assert!(e.apply("bad",&[edit(serde_json::json!({"pointSize":16})),edit(serde_json::json!({"basedOn":"B"}))]).is_err());
        assert_eq!(&before,e.parts());
        assert!(e.undo().is_none());
    }
    #[test]
    fn style_edit_keeps_opaque_definition_and_undo_restores_redundant_override() {
        let mut e=editor();let before=e.parts().clone();
        let out=e.apply("size",&[edit(serde_json::json!({"pointSize":14}))]).unwrap();
        assert!(out.recompose.contains(&"s".into()));
        assert!(e.part(PART).unwrap().contains(r#"<Unknown value="opaque"/>"#));
        assert!(e.part(PART).unwrap().contains("keep</Private>"));
        assert!(!e.part("Stories/s.xml").unwrap().contains("PointSize"));
        e.undo().unwrap();assert_eq!(&before,e.parts());
        e.redo().unwrap();assert!(!e.part("Stories/s.xml").unwrap().contains("PointSize"));
    }
    #[test]
    fn opaque_story_still_recomposes_without_rewriting_its_containers() {
        let mut e=editor();
        let opaque=STORY.replace("<Content>Text</Content>","<UnknownInline>opaque</UnknownInline>");
        e.parts.insert("Stories/s.xml".into(),opaque.clone());
        let out=e.apply("size",&[edit(serde_json::json!({"pointSize":16}))]).unwrap();
        assert!(out.recompose.contains(&"s".into()));
        assert_eq!(e.part("Stories/s.xml"),Some(opaque.as_str()));
    }
    #[test]
    fn delete_changes_references_but_preserves_literal_style_names() {
        let mut e=editor();
        e.parts.insert("Stories/s.xml".into(),STORY.replace("Text","ParagraphStyle/A"));
        let label=r#"<Extra><Label><KeyValuePair Key="memo" Value="ParagraphStyle/A"/></Label><Content>ParagraphStyle/A</Content></Extra>"#;
        e.parts.insert("Extra.xml".into(),label.into());
        e.apply("delete",&[Command::StyleDelete{kind:"paragraph".into(),style:"A".into(),replacement:"B".into()}]).unwrap();
        assert!(e.part("Stories/s.xml").unwrap().contains("<Content>ParagraphStyle/A</Content>"));
        assert_eq!(e.part("Extra.xml"),Some(label));
        assert!(e.part("Stories/s.xml").unwrap().contains(r#"AppliedParagraphStyle="ParagraphStyle/B""#));
    }
    #[test]
    fn rename_keeps_literals_and_unknown_xml_and_roundtrips_exactly() {
        let mut e=editor();
        e.parts.insert("Extra.xml".into(),r#"<Extra><Label><KeyValuePair Key="memo" Value="ParagraphStyle/A"/></Label><Content>ParagraphStyle/A</Content><NestedStyle AppliedParagraphStyle="ParagraphStyle/A"/><Properties><Reference type="object">ParagraphStyle/A</Reference><Literal type="string">ParagraphStyle/A</Literal></Properties></Extra>"#.into());
        let before=e.parts().clone();
        e.apply("rename",&[edit(serde_json::json!({"name":"본문 & 제목"}))]).unwrap();
        assert!(e.part("Stories/s.xml").unwrap().contains("ParagraphStyle/본문 &amp; 제목"));
        let extra=e.part("Extra.xml").unwrap();
        assert!(extra.contains("<Content>ParagraphStyle/A</Content>"));
        assert!(extra.contains("<Literal type=\"string\">ParagraphStyle/A</Literal>"));
        assert!(extra.contains("<Reference type=\"object\">ParagraphStyle/본문 &amp; 제목</Reference>"));
        assert!(e.part(PART).unwrap().contains("<Unknown value=\"opaque\"/>"));
        let changed=e.parts().clone();
        e.undo().unwrap();assert_eq!(e.parts(),&before);
        e.redo().unwrap();assert_eq!(e.parts(),&changed);
        let back=serde_json::from_value(serde_json::json!({"op":"styleEdit","kind":"paragraph","style":"본문 & 제목","props":{"name":"A"}})).unwrap();
        e.apply("back",&[back]).unwrap();assert_eq!(e.parts(),&before);
    }
    #[test]
    fn rename_collision_rolls_back_other_formatting() {
        let mut e=editor();let before=e.parts().clone();
        assert!(e.apply("collision",&[edit(serde_json::json!({"name":"B","pointSize":17}))]).is_err());
        assert_eq!(e.parts(),&before);assert!(e.undo().is_none());
    }
    #[test]
    fn charpanel_values_are_style_properties() {
        let mut e=editor();let before=e.parts().clone();
        e.apply("panel",&[edit(serde_json::json!({"horizontalScale":150,"verticalScale":80,"skew":12,"lastLineIndent":30,"appliedLanguage":"English: UK","hyphenation":false}))]).unwrap();
        let styles=e.part(PART).unwrap();
        for needle in ["HorizontalScale=\"150\"","VerticalScale=\"80\"","Skew=\"12\"","LastLineIndent=\"30\"","AppliedLanguage=\"$ID/English: UK\"","Hyphenation=\"false\""] {assert!(styles.contains(needle),"{needle}");}
        let info=e.style_info("paragraph","B").unwrap();
        assert_eq!(info["props"]["lastLineIndent"],30.0);
        assert_eq!(info["props"]["horizontalScale"],150.0);
        for bad in [serde_json::json!({"horizontalScale":24}),serde_json::json!({"verticalScale":401}),serde_json::json!({"skew":86}),serde_json::json!({"appliedLanguage":"Klingon"})] {
            assert!(e.apply("bad",&[edit(bad)]).is_err());
        }
        e.undo().unwrap();assert_eq!(e.parts(),&before);
    }
    #[test]
    fn manual_kerning_in_style_rolls_back_other_properties() {
        let mut e=editor();let before=e.parts().clone();
        assert!(e.apply("invalid kerning",&[edit(serde_json::json!({"pointSize":16})),edit(serde_json::json!({"kerningValue":30}))]).is_err());
        assert_eq!(e.parts(),&before);assert!(e.undo().is_none());
    }
}
