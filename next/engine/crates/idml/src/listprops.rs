//! List option value conversion shared by direct formatting and paragraph styles.
use serde_json::Value;

enum Kind { Text, StringProperty, Integer(u64, u64), Boolean, Alignment, CharacterStyle, NumberingList, Bullet }
pub struct Property { pub api: &'static str, pub xml: &'static str, pub default: &'static str, kind: Kind }
macro_rules! prop { ($api:literal,$xml:literal,$default:literal,$kind:expr) => { Property { api:$api,xml:$xml,default:$default,kind:$kind } }; }
pub const PROPERTIES: &[Property] = &[
    prop!("appliedNumberingList","AppliedNumberingList","NumberingList/$ID/[Default]",Kind::NumberingList),
    prop!("bulletsTextAfter","BulletsTextAfter","^t",Kind::Text),
    prop!("numberingFormat","NumberingFormat","1, 2, 3, 4...",Kind::StringProperty),
    prop!("numberingExpression","NumberingExpression","^#.^t",Kind::Text),
    prop!("numberingStartAt","NumberingStartAt","1",Kind::Integer(0,90000)),
    prop!("numberingLevel","NumberingLevel","1",Kind::Integer(1,9)),
    prop!("numberingApplyRestartPolicy","NumberingApplyRestartPolicy","true",Kind::Boolean),
    prop!("bulletsAlignment","BulletsAlignment","LeftAlign",Kind::Alignment),
    prop!("numberingAlignment","NumberingAlignment","LeftAlign",Kind::Alignment),
    prop!("bulletsCharacterStyle","BulletsCharacterStyle","CharacterStyle/$ID/[No character style]",Kind::CharacterStyle),
    prop!("numberingCharacterStyle","NumberingCharacterStyle","CharacterStyle/$ID/[No character style]",Kind::CharacterStyle),
    prop!("bulletChar","BulletChar","",Kind::Bullet),
];
const ALIGNMENTS: &[(&str,&str)] = &[("LEFT_ALIGN","LeftAlign"),("CENTER_ALIGN","CenterAlign"),("RIGHT_ALIGN","RightAlign")];
pub fn property(api: &str) -> Option<&'static Property> { PROPERTIES.iter().find(|p| p.api == api) }
impl Property {
    /// The second value is an optional complete Properties child; otherwise an attribute.
    pub fn encode(&self, value: &Value) -> Option<(String,Option<String>)> {
        let value = match self.kind {
            Kind::Integer(min,max) => value.as_u64().filter(|n| (min..=max).contains(n))?.to_string(),
            Kind::Boolean => value.as_bool()?.to_string(),
            Kind::Alignment => ALIGNMENTS.iter().find(|v| Some(v.0)==value.as_str())?.1.into(),
            Kind::Text => value.as_str()?.to_owned(),
            Kind::StringProperty => {
                let s=value.as_str()?.to_owned();
                if s.chars().any(char::is_control) { return None; }
                let xml=format!("<{} type=\"string\">{}</{}>",self.xml,quick_xml::escape::escape(&s),self.xml);
                return Some((s,Some(xml)));
            }
            Kind::CharacterStyle => {
                let s=value.as_str()?.to_owned();
                if !s.starts_with("CharacterStyle/") { return None; }
                let xml=format!("<{} type=\"object\">{}</{}>",self.xml,quick_xml::escape::escape(&s),self.xml);
                return Some((s,Some(xml)));
            }
            Kind::NumberingList => {
                let s=value.as_str()?.to_owned();if !s.starts_with("NumberingList/") {return None;}s
            }
            Kind::Bullet => {
                let mut chars=value.as_str()?.chars();let c=chars.next()?;
                if chars.next().is_some() || c.is_control() {return None;}
                let xml=format!("<BulletChar BulletCharacterType=\"UnicodeOnly\" BulletCharacterValue=\"{}\"/>",c as u32);
                return Some((xml.clone(),Some(xml)));
            }
        };
        if value.chars().any(char::is_control) { return None; }
        Some((value,None))
    }
    pub fn decode(&self, value: &str) -> Value {
        match self.kind {
            Kind::Integer(_,_) => value.parse::<u64>().ok().map(Value::from).unwrap_or(Value::Null),
            Kind::Boolean => match value {"true"=>true.into(),"false"=>false.into(),_=>Value::Null},
            Kind::Alignment => ALIGNMENTS.iter().find(|v| v.1==value).map(|v|Value::from(v.0)).unwrap_or(Value::Null),
            Kind::Bullet => {
                if value.is_empty() {return "•".into();}
                let Ok(e)=crate::xml::parse(value) else {return Value::Null;};
                if e.attr("BulletCharacterType")!=Some("UnicodeOnly") {return Value::Null;}
                e.attr("BulletCharacterValue").and_then(|s|s.parse::<u32>().ok()).and_then(char::from_u32).map(|c|Value::from(c.to_string())).unwrap_or(Value::Null)
            }
            _ => value.into(),
        }
    }
}
