//! Anchored-object option names, values and defaults shared by reading and editing.
use crate::styles::{Cat, ObjectProps};
use serde_json::Value;
use std::collections::BTreeMap;

pub struct Field { pub key: &'static str, pub attr: &'static str, pub default: &'static str, pub choices: &'static [&'static str] }
pub const FIELDS: &[Field] = &[
    Field {key:"anchoredPosition",attr:"AnchoredPosition",default:"InlinePosition",choices:&["InlinePosition","AboveLine","Anchored"]},
    Field {key:"anchorPoint",attr:"AnchorPoint",default:"BottomRightAnchor",choices:&["TopLeftAnchor","TopCenterAnchor","TopRightAnchor","LeftCenterAnchor","CenterAnchor","RightCenterAnchor","BottomLeftAnchor","BottomCenterAnchor","BottomRightAnchor"]},
    Field {key:"horizontalReferencePoint",attr:"HorizontalReferencePoint",default:"TextFrame",choices:&["AnchorLocation","ColumnEdge","TextFrame","PageMargins","PageEdge"]},
    Field {key:"horizontalAlignment",attr:"HorizontalAlignment",default:"LeftAlign",choices:&["LeftAlign","CenterAlign","RightAlign","TextAlign"]},
    Field {key:"verticalReferencePoint",attr:"VerticalReferencePoint",default:"LineBaseline",choices:&["LineBaseline","Capheight","LineXheight","TopOfLeading","EmboxTop","EmboxMiddle","EmboxBottom","ColumnEdge","TextFrame","PageMargins","PageEdge"]},
    Field {key:"verticalAlignment",attr:"VerticalAlignment",default:"TopAlign",choices:&["TopAlign","CenterAlign","BottomAlign"]},
    Field {key:"anchorXoffset",attr:"AnchorXoffset",default:"0",choices:&[]},
    Field {key:"anchorYoffset",attr:"AnchorYoffset",default:"0",choices:&[]},
    Field {key:"anchorSpaceAbove",attr:"AnchorSpaceAbove",default:"0",choices:&[]},
    Field {key:"spineRelative",attr:"SpineRelative",default:"false",choices:&[]},
    Field {key:"pinPosition",attr:"PinPosition",default:"true",choices:&[]},
    Field {key:"lockPosition",attr:"LockPosition",default:"false",choices:&[]},
];
impl Field {
    pub fn boolean(&self)->bool { matches!(self.default,"true"|"false") }
    pub fn value(&self, v:&Value)->Result<String,String> {
        if self.boolean() {return v.as_bool().map(|v|v.to_string()).ok_or_else(||format!("{}: boolean required",self.key));}
        if !self.choices.is_empty() {return v.as_str().filter(|v|self.choices.contains(v)).map(str::to_owned).ok_or_else(||format!("{}: invalid value",self.key));}
        v.as_f64().filter(|v|v.is_finite()&&v.abs()<=100000.0).map(|v|v.to_string()).ok_or_else(||format!("{}: finite points required",self.key))
    }
    fn json(&self,v:&str)->Value {
        if self.boolean() {Value::Bool(v=="true")} else if self.choices.is_empty() {serde_json::json!(v.parse::<f64>().unwrap_or(0.0))} else {Value::String(v.into())}
    }
}
pub fn values(item:&str, styles:&str)->Result<BTreeMap<String,Value>,String> {
    let item=crate::xml::parse(item).map_err(|e|e.0)?; let styles=crate::xml::parse(styles).map_err(|e|e.0)?;
    let mut warnings=crate::warn::Warnings::default();
    let sheet=crate::styles::StyleSheet::new(&[&styles],&mut warnings);
    let o=ObjectProps{item:&item,sheet:&sheet,chain:sheet.object_chain(item.attr("AppliedObjectStyle"),&mut warnings)};
    Ok(FIELDS.iter().map(|f|(f.key.into(),f.json(o.str(&["AnchoredObjectSetting"],f.attr,Cat::Anchor).unwrap_or(f.default)))).collect())
}

/// `AnchoredObjectSetting` of an item (resolved with its object style); `None` without one
/// (an item on a spread, or an anchored item with InDesign's inline defaults).
pub(crate) fn read(o: &ObjectProps<'_, '_>) -> Option<folio_model::AnchorSettings> {
    use folio_model::{AnchorAlign, AnchorPosition, AnchorReference, AnchorSettings};
    let p = ["AnchoredObjectSetting"];
    let s = |k: &str| o.str(&p, k, Cat::Anchor);
    let present = FIELDS.iter().any(|f| s(f.attr).is_some());
    if !present {
        return None;
    }
    let d = AnchorSettings::default();
    let reference = |v: Option<&str>, d: AnchorReference| match v {
        Some("AnchorLocation") => AnchorReference::AnchorLocation,
        Some("ColumnEdge") => AnchorReference::ColumnEdge,
        Some("TextFrame") => AnchorReference::TextFrame,
        Some("PageMargins") => AnchorReference::PageMargins,
        Some("PageEdge") => AnchorReference::PageEdge,
        Some("LineBaseline") => AnchorReference::LineBaseline,
        Some("Capheight") => AnchorReference::Capheight,
        Some("LineXheight") => AnchorReference::LineXheight,
        Some("TopOfLeading") => AnchorReference::TopOfLeading,
        Some("EmboxTop") => AnchorReference::EmboxTop,
        Some("EmboxMiddle") => AnchorReference::EmboxMiddle,
        Some("EmboxBottom") => AnchorReference::EmboxBottom,
        _ => d,
    };
    let align = |v: Option<&str>| match v {
        Some("CenterAlign") => AnchorAlign::Center,
        Some("RightAlign" | "BottomAlign" | "AwayFromBindingSide") => AnchorAlign::End,
        Some("TextAlign") => AnchorAlign::TextAlign,
        _ => AnchorAlign::Start,
    };
    let point = match s("AnchorPoint") {
        Some("TopLeftAnchor") => (0.0, 0.0),
        Some("TopCenterAnchor") => (0.5, 0.0),
        Some("TopRightAnchor") => (1.0, 0.0),
        Some("LeftCenterAnchor") => (0.0, 0.5),
        Some("CenterAnchor") => (0.5, 0.5),
        Some("RightCenterAnchor") => (1.0, 0.5),
        Some("BottomLeftAnchor") => (0.0, 1.0),
        Some("BottomCenterAnchor") => (0.5, 1.0),
        _ => d.point,
    };
    let f = |k: &str| o.f64(&p, k, Cat::Anchor).unwrap_or(0.0);
    let b = |k: &str, d: bool| o.bool(&p, k, Cat::Anchor).unwrap_or(d);
    Some(AnchorSettings {
        position: match s("AnchoredPosition") {
            Some("AboveLine") => AnchorPosition::AboveLine,
            Some("Anchored") => AnchorPosition::Custom,
            _ => AnchorPosition::Inline,
        },
        point,
        horizontal_reference: reference(s("HorizontalReferencePoint"), d.horizontal_reference),
        horizontal_alignment: align(s("HorizontalAlignment")),
        vertical_reference: reference(s("VerticalReferencePoint"), d.vertical_reference),
        vertical_alignment: align(s("VerticalAlignment")),
        x_offset: f("AnchorXoffset"),
        y_offset: f("AnchorYoffset"),
        space_above: f("AnchorSpaceAbove"),
        pin: b("PinPosition", d.pin),
        spine_relative: b("SpineRelative", d.spine_relative),
    })
}

