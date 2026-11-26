use super::*;

pub(in crate::ops) fn clear_not_in_style(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let (part,ix,i)=w.find(id)?;let t=w.text(&part)?.to_owned();
        check_item(&ix,&t,i,id)?;
        if !["Rectangle","Oval","Polygon","GraphicLine"].contains(&ix.local(&t,i)) {
            return Err(EditError::new("unsupported","clearing undefined attributes is measured for shapes only"));
        }
        let style=ix.value(&t,i,"AppliedObjectStyle").unwrap_or_else(||NONE.into());
        let styles=Styles::load(w).ok_or_else(||EditError::new("xml","no object styles"))?;
        let mut edits=Vec::new();
        for (enable,names) in [("EnableFill",&["FillColor","FillTint"][..]),("EnableStroke",&["StrokeWeight","StrokeType","StrokeColor","StrokeTint","GapColor","GapTint"][..]),
            ("EnableStrokeAndCornerOptions", &["CornerOption","CornerRadius","TopLeftCornerOption","TopRightCornerOption","BottomLeftCornerOption","BottomRightCornerOption","TopLeftCornerRadius","TopRightCornerRadius","BottomLeftCornerRadius","BottomRightCornerRadius","EndCap","EndJoin","MiterLimit","StrokeAlignment","LeftLineEnd","RightLineEnd","ArrowHeadAlignment","LeftArrowHeadScale","RightArrowHeadScale"][..])] {
            if styles.enabled(&style,enable) {continue;}
            for name in names {
                let v=styles.value(NONE,&[],name,enable).ok_or_else(||EditError::new("unsupported","missing root style attribute"))?;
                edits.push(attr_edit(&ix,&t,i,name,&v));
            }
            if enable!="EnableStrokeAndCornerOptions" {
                let overprint=if enable=="EnableFill" {"OverprintFill"}else{"OverprintStroke"};
                edits.extend(attr_removal(&ix,&t,i,overprint));
            }
        }
        if !styles.enabled(&style,"EnableTextWrapAndOthers") {
            if let Some(n)=ix.path(&t,i,&["TextWrapPreference"]) {
                for key in ["TextWrapMode","Inverse","ApplyToMasterPageOnly","TextWrapSide"] {
                    if let Some(value)=styles.value(NONE,&["TextWrapPreference"],key,"") {
                        edits.push(attr_edit(&ix,&t,n,key,&value));
                    }
                }
            }
            if let Some(n)=ix.path(&t,i,&["TextWrapPreference","Properties","TextWrapOffset"]) {
                for key in ["Top","Left","Bottom","Right"] { edits.push(attr_edit(&ix,&t,n,key,"0")); }
            }
        }
        let enabled=styles.chain(&style).first().and_then(|&n|styles.ix.path(styles.t,n,&["ObjectStyleObjectEffectsCategorySettings"]))
            .and_then(|n|styles.ix.attr(styles.t,n,"EnableTransparency"))!=Some("false");
        if !enabled {
            if let Some(n)=ix.path(&t,i,&["TransparencySetting","BlendingSetting"]) {
                edits.push(attr_edit(&ix,&t,n,"Opacity","100"));
            } else {
                return Err(EditError::new("unsupported","clearing inherited transparency without local settings is not measured"));
            }
        }
        w.splice(&part,edits)?;
    }
    Ok(())
}

