//! Character line property names and value conversion.
use serde_json::Value;
#[derive(Clone, Copy)]
enum Kind { Weight, Offset, Tint, Color, Toggle, Stroke }
pub struct Property { pub api: &'static str, pub xml: &'static str, kind: Kind }
pub const PROPERTIES: &[Property] = &[
    Property { api: "underlineWeight", xml: "UnderlineWeight", kind: Kind::Weight },
    Property { api: "underlineOffset", xml: "UnderlineOffset", kind: Kind::Offset },
    Property { api: "underlineTint", xml: "UnderlineTint", kind: Kind::Tint },
    Property { api: "underlineColor", xml: "UnderlineColor", kind: Kind::Color },
    Property { api: "underlineType", xml: "UnderlineType", kind: Kind::Stroke },
    Property { api: "underlineGapColor", xml: "UnderlineGapColor", kind: Kind::Color },
    Property { api: "underlineGapTint", xml: "UnderlineGapTint", kind: Kind::Tint },
    Property { api: "underlineOverprint", xml: "UnderlineOverprint", kind: Kind::Toggle },
    Property { api: "underlineGapOverprint", xml: "UnderlineGapOverprint", kind: Kind::Toggle },
    Property { api: "strikeThroughWeight", xml: "StrikeThroughWeight", kind: Kind::Weight },
    Property { api: "strikeThroughOffset", xml: "StrikeThroughOffset", kind: Kind::Offset },
    Property { api: "strikeThroughTint", xml: "StrikeThroughTint", kind: Kind::Tint },
    Property { api: "strikeThroughColor", xml: "StrikeThroughColor", kind: Kind::Color },
    Property { api: "strikeThroughType", xml: "StrikeThroughType", kind: Kind::Stroke },
    Property { api: "strikeThroughGapColor", xml: "StrikeThroughGapColor", kind: Kind::Color },
    Property { api: "strikeThroughGapTint", xml: "StrikeThroughGapTint", kind: Kind::Tint },
    Property { api: "strikeThroughOverprint", xml: "StrikeThroughOverprint", kind: Kind::Toggle },
    Property { api: "strikeThroughGapOverprint", xml: "StrikeThroughGapOverprint", kind: Kind::Toggle },
 ];
pub fn property(api: &str) -> Option<&'static Property> { PROPERTIES.iter().find(|p| p.api == api) }
pub fn xml_order(xml: &str) -> Option<u8> {
    PROPERTIES.iter().find(|p| p.xml == xml).map(|p| if p.api.starts_with("underline") { 2 } else { 3 })
}
impl Property {
    pub fn default(&self) -> &'static str {
        match self.kind {
            Kind::Weight | Kind::Offset => "-9999",
            Kind::Tint => "-1",
            Kind::Color if self.api.ends_with("GapColor") => "Swatch/None",
            Kind::Color => "Text Color",
            Kind::Toggle => "false",
            Kind::Stroke => "StrokeStyle/$ID/Solid",
        }
    }
    pub fn property_type(&self, value: &str) -> Option<&'static str> {
        match self.kind { Kind::Color if value == "Text Color" => Some("string"), Kind::Color | Kind::Stroke => Some("object"), _ => None }
    }
    pub fn encode(&self, value: &Value) -> Option<String> {
        Some(match self.kind {
            Kind::Toggle => value.as_bool()?.to_string(),
            Kind::Weight | Kind::Offset | Kind::Tint => {
                let n = value.as_f64()?;
                if !n.is_finite() { return None; }
                match self.kind {
                    Kind::Tint if n != -1. && !(0. ..=100.).contains(&n) => return None,
                    _ => ()
                }
                n.to_string()
            }
            Kind::Color => {
                let s=value.as_str()?;
                if s != "Text Color" && !["Color/", "Swatch/", "Tint/", "Gradient/"].iter().any(|p| s.starts_with(p)) { return None; }
                s.into()
            }
            Kind::Stroke => {
                let s=value.as_str()?;
                // a built-in type, or one of the document's own dashed / dotted / striped styles
                if !["StrokeStyle/", "DashedStrokeStyle/", "DottedStrokeStyle/", "StripedStrokeStyle/"].iter().any(|p| s.starts_with(p)) { return None; }
                s.into()
            }
        })
    }
    pub fn decode(&self, value: &str) -> Value {
        match self.kind {
            Kind::Toggle => match value { "true" => true.into(), "false" => false.into(), _ => Value::Null },
            Kind::Weight | Kind::Offset | Kind::Tint => value.parse::<f64>().ok().map(Value::from).unwrap_or(Value::Null),
            _ => value.into(),
        }
    }
}
