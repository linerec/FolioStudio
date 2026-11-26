use super::{attr_edit, check_item, content_of, fitting_anchor, frame_box, geom, Mat};
use crate::{EditError, Work};
use std::collections::BTreeMap;
use folio_idml::fitting::{ALIGNMENTS, FIELDS, MODES};

fn invalid(message: impl Into<String>) -> EditError { EditError::new("invalid", message) }

pub(super) fn automatic(w: &Work<'_>, ix: &super::Index, t: &str, i: usize) -> bool {
    ix.named(t, i, "FrameFittingOption").first().and_then(|n| ix.attr(t, *n, "AutoFit")).map(str::to_owned)
        .or_else(|| super::objectstyle::from_style(w, ix, t, i, &["FrameFittingOption"], "AutoFit", "EnableFrameFittingOptions"))
        .as_deref() == Some("true")
}

pub(crate) fn set(w: &mut Work<'_>, ids: &[String], props: &BTreeMap<String, serde_json::Value>) -> Result<(), EditError> {
    let mut values = BTreeMap::new();
    for (key, value) in props {
        let attr = FIELDS.iter().find(|(k, _)| *k == key).ok_or_else(|| invalid(format!("unknown fitting option: {key}")))?.1;
        let text = match key.as_str() {
            "autoFit" => value.as_bool().ok_or_else(|| invalid("autoFit: boolean required"))?.to_string(),
            "fittingAlignment" => value.as_str().filter(|v| ALIGNMENTS.contains(v)).ok_or_else(|| invalid("invalid fitting alignment"))?.to_owned(),
            "fittingOnEmptyFrame" => value.as_str().filter(|v| MODES.contains(v)).ok_or_else(|| invalid("invalid fitting mode"))?.to_owned(),
            _ => geom::num(value.as_f64().filter(|v| v.is_finite() && v.abs() <= 100000.0).ok_or_else(|| invalid("crop: finite points required"))?),
        };
        values.insert(attr, text);
    }
    for id in ids { apply(w, id, &values, true)?; }
    Ok(())
}

pub(crate) fn clear(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    let defaults = serde_json::to_value(folio_model::fitting::FrameFitting::default()).unwrap();
    let values = FIELDS.iter().map(|(key, attr)| (*attr, defaults[*key].as_str().map(str::to_owned).unwrap_or_else(|| defaults[*key].to_string()))).collect();
    for id in ids { apply(w, id, &values, false)?; }
    Ok(())
}

fn apply(w: &mut Work<'_>, id: &str, changed: &BTreeMap<&str, String>, refit: bool) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, id)?;
    if ix.local(&t, i) == "Group" { return Err(invalid("select individual frames")); }
    let option = ix.named(&t, i, "FrameFittingOption").first().copied();
    let defaults = serde_json::to_value(folio_model::fitting::FrameFitting::default()).unwrap();
    let mut values: BTreeMap<&str, String> = FIELDS.iter().map(|(key, attr)| {
        let value = changed.get(attr).cloned().or_else(|| option.and_then(|n| ix.attr(&t, n, attr)).map(str::to_owned))
            .or_else(|| super::objectstyle::from_style(w, &ix, &t, i, &["FrameFittingOption"], attr, "EnableFrameFittingOptions"))
            .unwrap_or_else(|| defaults[*key].as_str().map(str::to_owned).unwrap_or_else(|| defaults[*key].to_string()));
        (*attr, value)
    }).collect();
    if refit && values["AutoFit"] == "true" && values["FittingOnEmptyFrame"] == "None" {
        values.insert("FittingOnEmptyFrame", "FillProportionally".into());
    }
    let mut edits = Vec::new();
    if refit && let Some(c) = content_of(&ix, &t, i) {
        let [a,b,cx,d,_,_] = c.m.0;
        if b.abs() > 1e-9 || cx.abs() > 1e-9 || a <= 0.0 || d <= 0.0 { return Err(EditError::new("unsupported", "rotated or flipped graphic fitting")); }
        let [fx,fy,fr,fb] = frame_box(&ix, &t, i).ok_or_else(|| invalid("frame has no bounds"))?;
        let [gx,gy,gr,gb] = c.bounds;
        let crop = |key| values[key].parse::<f64>().unwrap_or(0.0);
        let (left,top,right,bottom) = (crop("LeftCrop"),crop("TopCrop"),crop("RightCrop"),crop("BottomCrop"));
        let (gw,gh) = (gr-gx-left-right,gb-gy-top-bottom);
        if gw <= 0.0 || gh <= 0.0 { return Err(invalid("crops leave no image area")); }
        let (fw,fh) = (fr-fx,fb-fy);
        let (sx,sy) = match values["FittingOnEmptyFrame"].as_str() {
            "Proportionally" => { let s=(fw/gw).min(fh/gh);(s,s) },
            "FillProportionally" => { let s=(fw/gw).max(fh/gh);(s,s) },
            "ContentToFrame" => (fw/gw,fh/gh),
            _ => (a,d),
        };
        let anchor = fitting_anchor(&values["FittingAlignment"]);
        let m=Mat([sx,0.0,0.0,sy,fx+(fw-gw*sx)*anchor.0-(gx+left)*sx,fy+(fh-gh*sy)*anchor.1-(gy+top)*sy]);
        edits.push(attr_edit(&ix,&t,c.graphic,"ItemTransform",&m.text()));
        if let Some(ppi)=ix.attr(&t,c.graphic,"ActualPpi").and_then(geom::parse_pair) {
            edits.push(attr_edit(&ix,&t,c.graphic,"EffectivePpi",&format!("{} {}",(ppi.0/sx).round(),(ppi.1/sy).round())));
        }
    }
    if let Some(n)=option {
        for (key,value) in &values { edits.push(attr_edit(&ix,&t,n,key,value)); }
    } else {
        let attrs=values.iter().map(|(k,v)|format!(" {k}=\"{v}\"")).collect::<String>();
        let at=ix.nodes[i].close_start;
        edits.push((at..at,format!("<FrameFittingOption{attrs}/>")));
    }
    w.splice(&part,edits)
}
