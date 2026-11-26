//! The measured visual attributes captured from a shape.
use super::*;

pub(super) fn item(w: &mut Work<'_>, id: &str) -> Result<Map<String,Value>,EditError> {
    let (part,ix,i)=w.find(id)?;let t=w.text(&part)?;
    check_item(&ix,t,i,id)?;
    if !["Rectangle","Oval","Polygon","GraphicLine"].contains(&ix.local(t,i)) {
        return Err(EditError::new("unsupported","creating or redefining from this item kind is not measured"));
    }
    let style=ix.value(t,i,"AppliedObjectStyle").unwrap_or_else(||NONE.into());
    let styles=objectstyle::Styles::load(w).ok_or_else(||EditError::new("xml","no styles"))?;
    let mut props=Map::new();
    for (enable,keys) in [
        ("EnableFill",vec!["fillColor","fillTint"]),
        ("EnableStroke",vec!["strokeColor","strokeWeight","strokeTint"]),
        ("EnableStrokeAndCornerOptions",vec!["topLeftCornerOption","topRightCornerOption","bottomLeftCornerOption","bottomRightCornerOption","topLeftCornerRadius","topRightCornerRadius","bottomLeftCornerRadius","bottomRightCornerRadius"]),
    ] {
        let key=format!("{}{}",enable[..1].to_lowercase(),&enable[1..]);props.insert(key,Value::Bool(true));
        for key in keys {
            let attr=format!("{}{}",key[..1].to_uppercase(),&key[1..]);
            let value=ix.value(t,i,&attr).or_else(||styles.value(&style,&[],&attr,enable)).or_else(||styles.value(NONE,&[],&attr,enable))
                .ok_or_else(||EditError::new("unsupported",format!("missing object style attribute {attr}")))?;
            props.insert(key.into(),properties::api_value(&value));
        }
    }
    let path=["TransparencySetting","BlendingSetting"];
    let opacity=ix.path(t,i,&path).and_then(|n|ix.value(t,n,"Opacity"))
        .or_else(||styles.value(&style,&path,"Opacity","EnableTransparency")).unwrap_or_else(||"100".into());
    props.insert("objectEffectsEnablingSettings".into(),serde_json::json!({"enableTransparency":true}));
    props.insert("transparencySettings".into(),serde_json::json!({"blendingSettings":{"opacity":properties::api_value(&opacity)}}));
    Ok(props)
}
