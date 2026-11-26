//! The measured, editable subset of object style options.
use super::*;

const ROOT: &[&str] = &["enableFill", "enableStroke", "enableStrokeAndCornerOptions", "enableTextFrameGeneralOptions", "enableTextWrapAndOthers",
    "enableAnchoredObjectOptions", "enableStoryOptions", "enableParagraphStyle", "appliedParagraphStyle", "applyNextParagraphStyle",
    "fillColor", "fillTint", "strokeColor", "strokeWeight", "strokeTint", "topLeftCornerOption", "topRightCornerOption", "bottomLeftCornerOption", "bottomRightCornerOption",
    "topLeftCornerRadius", "topRightCornerRadius", "bottomLeftCornerRadius", "bottomRightCornerRadius"];
const FRAME: &[&str] = &["textColumnCount", "textColumnGutter", "insetSpacing"];
const WRAP: &[&str] = &["textWrapMode", "textWrapOffset"];
const EFFECTS: &[&str] = &["enableTransparency", "enableDropShadow", "enableFeather", "enableInnerShadow", "enableOuterGlow", "enableInnerGlow",
    "enableBevelEmboss", "enableSatin", "enableDirectionalFeather", "enableGradientFeather"];

fn scalar(key: &str, value: &Value) -> Result<String, EditError> {
    if key == "appliedParagraphStyle" {
        let name = value.as_str().ok_or_else(|| EditError::new("invalid", "expected a paragraph style"))?;
        return Ok(if name.starts_with("ParagraphStyle/") { name.into() } else { format!("ParagraphStyle/{name}") });
    }
    if key.starts_with("enable") || key == "applyNextParagraphStyle" {
        return value.as_bool().map(|v| v.to_string()).ok_or_else(|| EditError::new("invalid", "expected a switch"));
    }
    if key.ends_with("Color") {
        let name = value.as_str().ok_or_else(|| EditError::new("invalid", "expected a swatch"))?;
        return Ok(if name.contains('/') { name.to_owned() } else if name == "None" { "Swatch/None".to_owned() } else { format!("Color/{name}") });
    }
    if key.ends_with("Option") || key == "textWrapMode" {
        let s = value.as_str().unwrap_or("");
        let pairs = if key=="textWrapMode" {[("TextWrapModes.NONE", "None"), ("TextWrapModes.BOUNDING_BOX_TEXT_WRAP", "BoundingBoxTextWrap")]}
            else {[("CornerOptions.NONE", "None"), ("CornerOptions.ROUNDED_CORNER", "RoundedCorner")]};
        return pairs.iter().find(|(k,v)| s == *k || s == *v).map(|(_,v)| v.to_string()).ok_or_else(|| EditError::new("invalid", "unsupported style option"));
    }
    let v = value.as_f64().filter(|v| v.is_finite()).ok_or_else(|| EditError::new("invalid", "expected a finite number"))?;
    let valid = if key.ends_with("Tint") { (-1.0..=100.0).contains(&v) }
        else if key == "opacity" { (0.0..=100.0).contains(&v) }
        else if key == "textColumnCount" { (1.0..=40.0).contains(&v) && v.fract() == 0.0 }
        else { (0.0..=10000.0).contains(&v) };
    if !valid { return Err(EditError::new("invalid", format!("invalid {key}"))); }
    Ok(geom::num(v))
}

fn xml_key(key: &str) -> String { format!("{}{}", key[..1].to_uppercase(), &key[1..]) }

fn list(value: &Value) -> Result<Vec<String>, EditError> {
    let array = value.as_array().filter(|v| v.len() == 4).ok_or_else(|| EditError::new("invalid", "expected four inset values"))?;
    array.iter().map(|v| scalar("inset", v)).collect()
}

fn insets(text: &mut String, value: &Value) -> Result<(), EditError> {
    let values = list(value)?;
    let ix = Index::scan(text).map_err(|e| EditError::new("xml",e))?;
    let i = ix.path(text, 0, &["TextFramePreference", "Properties", "InsetSpacing"]).ok_or_else(|| EditError::new("unsupported", "missing style insets"))?;
    let n = &ix.nodes[i];
    text.replace_range(n.start..n.end, &format!("<InsetSpacing type=\"list\">{}</InsetSpacing>", values.iter().map(|v|format!("<ListItem type=\"unit\">{v}</ListItem>")).collect::<String>()));
    Ok(())
}

fn group(text: &mut String, path: &[&str], keys: &[&str], value: &Value) -> Result<(), EditError> {
    let values = value.as_object().ok_or_else(|| EditError::new("invalid", "expected style options"))?;
    for (key, value) in values {
        if !keys.contains(&key.as_str()) { return Err(EditError::new("unsupported", format!("object style option {key}"))); }
        if key == "insetSpacing" { insets(text, value)?; }
        else if key == "textWrapOffset" {
            for (key,v) in ["Top", "Left", "Bottom", "Right"].iter().zip(list(value)?) {
                set_attr(text, &["TextWrapPreference", "Properties", "TextWrapOffset"], key, &v)?;
            }
        } else { set_attr(text, path, &xml_key(key), &scalar(key, value)?)?; }
    }
    Ok(())
}

fn transparency(text: &mut String, value: &Value) -> Result<(), EditError> {
    let outer = value.as_object().ok_or_else(|| EditError::new("invalid", "expected transparency options"))?;
    if outer.len() != 1 || !outer.contains_key("blendingSettings") { return Err(EditError::new("unsupported", "only opacity is measured")); }
    let values = outer["blendingSettings"].as_object().ok_or_else(|| EditError::new("invalid", "expected blending options"))?;
    if values.len() != 1 || !values.contains_key("opacity") { return Err(EditError::new("unsupported", "only opacity is measured")); }
    let opacity = scalar("opacity", &values["opacity"])?;
    let ix = Index::scan(text).map_err(|e| EditError::new("xml",e))?;
    if let Some(n)=ix.path(text, 0, &["TransparencySetting", "BlendingSetting"]) {
        if opacity=="100" {
            if ix.attrs(text,n).iter().all(|a|&text[a.name.0..a.name.1]=="Opacity") {
                let parent=ix.nodes[n].parent.unwrap();
                let remove=if ix.children(parent).len()==1 {parent}else{n};
                let node=&ix.nodes[remove];text.replace_range(node.start..node.end,"");
            } else {let edit=attr_removal(&ix,text,n,"Opacity");local_edit(text,edit.into_iter().collect());}
            return Ok(());
        }
        return set_attr(text, &["TransparencySetting", "BlendingSetting"], "Opacity", &opacity);
    }
    if ix.path(text, 0, &["TransparencySetting"]).is_some() { return Err(EditError::new("unsupported", "complex object style transparency")); }
    if opacity == "100" { return Ok(()); }
    let i = ix.path(text, 0, &["ObjectStyleObjectEffectsCategorySettings"]).ok_or_else(|| EditError::new("unsupported", "missing effect categories"))?;
    text.insert_str(ix.nodes[i].start, &format!("<TransparencySetting><BlendingSetting Opacity=\"{opacity}\" /></TransparencySetting>\n\t\t\t"));
    Ok(())
}

pub(super) fn apply(text: &mut String, props: &Map<String, Value>) -> Result<(), EditError> {
    for (key, value) in props {
        match key.as_str() {
            "textFramePreferences" => group(text, &["TextFramePreference"], FRAME, value)?,
            "textWrapPreferences" => group(text, &["TextWrapPreference"], WRAP, value)?,
            "objectEffectsEnablingSettings" => group(text, &["ObjectStyleObjectEffectsCategorySettings"], EFFECTS, value)?,
            "transparencySettings" => transparency(text, value)?,
            key if ROOT.contains(&key) => {
                let v = scalar(key, value)?;
                set_attr(text, &[], &xml_key(key), &v)?;
                if key == "fillColor" && v != "Swatch/None" { set_attr(text, &[], "OverprintFill", "false")?; }
                if (key=="fillColor" || key=="strokeColor") && v=="Swatch/None" {
                    let ix=Index::scan(text).map_err(|e|EditError::new("xml",e))?;
                    let key=if key=="fillColor" {"OverprintFill"}else{"OverprintStroke"};
                    let edit=attr_removal(&ix,text,0,key);local_edit(text,edit.into_iter().collect());
                }
                if key == "topLeftCornerOption" { set_attr(text, &[], "CornerOption", &v)?; }
                if key == "topLeftCornerRadius" { set_attr(text, &[], "CornerRadius", &v)?; }
            }
            _ => return Err(EditError::new("unsupported", format!("object style option {key}"))),
        }
    }
    Ok(())
}

pub(super) fn validate_resources(w: &Work<'_>, props: &Map<String,Value>) -> Result<(),EditError> {
    if let Some(value) = props.get("appliedParagraphStyle") {
        let id = scalar("appliedParagraphStyle", value)?;
        let text = w.text(PART)?;
        let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
        if ix.by_self(text, &id).filter(|&i| ix.local(text, i) == "ParagraphStyle").is_none() {
            return Err(EditError::new("notFound", format!("paragraph style {id}")));
        }
    }
    let graphic=w.text("Resources/Graphic.xml")?;
    let ix=Index::scan(graphic).map_err(|e|EditError::new("xml",e))?;
    for key in ["fillColor","strokeColor"] {
        if let Some(value)=props.get(key) {
            let id=scalar(key,value)?;
            if ix.by_self(graphic,&id).filter(|&i|["Color","Swatch","Tint","Gradient","MixedInk"].contains(&ix.local(graphic,i))).is_none() {
                return Err(EditError::new("notFound",format!("swatch {id}")));
            }
        }
    }
    Ok(())
}

pub(super) fn api_value(value: &str) -> Value {
    if let Ok(b)=value.parse::<bool>() {Value::Bool(b)}
    else if let Ok(n)=value.parse::<f64>() {serde_json::json!(n)}
    else {Value::String(value.into())}
}

pub(super) fn read(t: &str, ix: &Index, i: usize) -> Map<String,Value> {
    let mut result=Map::new();
    for key in ROOT {
        if let Some(v)=ix.value(t,i,&xml_key(key)) {result.insert((*key).into(),api_value(&v));}
    }
    for (key,path,keys) in [("textFramePreferences",vec!["TextFramePreference"],FRAME),("textWrapPreferences",vec!["TextWrapPreference"],WRAP),
        ("objectEffectsEnablingSettings",vec!["ObjectStyleObjectEffectsCategorySettings"],EFFECTS)] {
        if let Some(n)=ix.path(t,i,&path) {
            let mut props=Map::new();
            for name in keys {if let Some(v)=ix.value(t,n,&xml_key(name)) {props.insert((*name).into(),api_value(&v));}}
            result.insert(key.into(),Value::Object(props));
        }
    }
    let opacity=ix.path(t,i,&["TransparencySetting","BlendingSetting"]).and_then(|n|ix.value(t,n,"Opacity")).unwrap_or_else(||"100".into());
    result.insert("transparencySettings".into(),serde_json::json!({"blendingSettings":{"opacity":api_value(&opacity)}}));
    result
}
