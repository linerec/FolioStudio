//! Paragraph tab stops: IDML records and the text-format value API.
use folio_model::{TabAlignment, TabStop};
use serde_json::{Value, json};
use crate::xml::Element;

pub(crate) fn read(list: &Element<'_>) -> Vec<TabStop> {
    list.children_named("ListItem").map(|item| {
        let text = |key| item.child(key).map(|e| e.text()).unwrap_or_default();
        TabStop {
            position: crate::props::num(&text("Position")).unwrap_or(0.0),
            alignment: match text("Alignment").as_str() {
                "CenterAlign" => TabAlignment::Center,
                "RightAlign" => TabAlignment::Right,
                "CharacterAlign" => TabAlignment::Character,
                _ => TabAlignment::Left,
            },
            leader: text("Leader"), align_char: text("AlignmentCharacter"),
        }
    }).collect()
}

const ALIGNMENTS: &[(&str, &str)] = &[("LEFT_ALIGN", "LeftAlign"), ("CENTER_ALIGN", "CenterAlign"),
    ("RIGHT_ALIGN", "RightAlign"), ("CHARACTER_ALIGN", "CharacterAlign")];

/// Encode a complete replacement list. Positions are points from the column's left edge.
pub fn encode(value: &Value) -> Option<String> {
    let list = value.as_array()?;
    if list.len() > 512 { return None; }
    let mut stops = Vec::new();
    for item in list {
        let pos = item.get("position")?.as_f64().filter(|n| n.is_finite() && *n >= 0.0 && *n <= 10000.0)?;
        let alignment = item.get("alignment").and_then(Value::as_str).unwrap_or("LEFT_ALIGN");
        let xml_alignment = ALIGNMENTS.iter().find(|(api, _)| *api == alignment)?.1;
        let string = |key, default: &str| -> Option<String> {
            let s = match item.get(key) { Some(v) => v.as_str()?, None => default };
            if s.chars().any(char::is_control) || s.chars().count() > 8 { return None; }
            Some(quick_xml::escape::escape(s).into_owned())
        };
        let leader = string("leader", "")?;
        let character = string("alignmentCharacter", ".")?;
        stops.push((pos, format!("<ListItem type=\"record\"><Alignment type=\"enumeration\">{xml_alignment}</Alignment><AlignmentCharacter type=\"string\">{character}</AlignmentCharacter><Leader type=\"string\">{leader}</Leader><Position type=\"unit\">{pos}</Position></ListItem>")));
    }
    stops.sort_by(|a,b| a.0.total_cmp(&b.0));
    if stops.windows(2).any(|w| w[0].0 == w[1].0) { return None; }
    Some(stops.into_iter().map(|(_,xml)|xml).collect())
}

pub fn decode(inner: &str) -> Value {
    let xml = format!("<TabList>{inner}</TabList>");
    let Ok(list) = crate::xml::parse(&xml) else { return Value::Null; };
    Value::Array(read(&list).iter().map(|s| json!({
        "position": s.position, "alignment": ALIGNMENTS[match s.alignment {
            TabAlignment::Left=>0,TabAlignment::Center=>1,TabAlignment::Right=>2,TabAlignment::Character=>3
        }].0, "leader":s.leader,"alignmentCharacter":s.align_char
    })).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_sort_and_preserve_xml_characters() {
        let input=json!([{"position":72.0,"alignment":"CHARACTER_ALIGN","leader":"<&","alignmentCharacter":","},
            {"position":36.0,"alignment":"LEFT_ALIGN","leader":".","alignmentCharacter":"."}]);
        let output=decode(&encode(&input).unwrap());
        assert_eq!(output[0],input[1]);
        assert_eq!(output[1],input[0]);
        assert!(encode(&json!([{"position":36},{"position":36}])).is_none());
        assert!(encode(&json!([{"position":-1}])).is_none());
        assert!(encode(&json!([{"position":36,"alignment":"invalid"}])).is_none());
        assert_eq!(decode(&encode(&json!([])).unwrap()),json!([]));
    }
}
