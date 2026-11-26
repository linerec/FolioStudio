//! Fields and starting values for anchored object options.

use super::Defaults;
use crate::Session;
use serde_json::{Map, Value, json};

pub(super) fn object_defaults(s: &Session, ids: &Value) -> Defaults {
    let mut values = Map::new();
    let id = ids[0].as_str().ok_or("dialog needs an anchored object id")?;
    let object = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.anchored_object(id).map_err(|e| e.to_string())?;
    for (key, value) in object["props"].as_object().into_iter().flatten() { values.insert(key.clone(), value.clone()); }
    Ok((values, json!({"story":object["story"], "at":object["at"]})))
}

pub(super) fn new_defaults(s: &Session) -> Defaults {
    let mut values = Map::new();
    // the size and position a new anchored object starts with (the document's Preferences)
    let defaults = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
        .editor.anchored_defaults().map_err(|e| e.to_string())?;
    for (key, value) in defaults.as_object().into_iter().flatten() { values.insert(key.clone(), value.clone()); }
    Ok((values, json!({})))
}

pub(super) fn anchored_definition() -> Value {
    use folio_idml::anchored::FIELDS;
    fn label(key: &str) -> &str { match key {
        "anchoredPosition" => "고정 위치", "anchorPoint" => "개체 기준점", "horizontalReferencePoint" => "가로 기준", "horizontalAlignment" => "가로 정렬",
        "verticalReferencePoint" => "세로 기준", "verticalAlignment" => "세로 정렬", "anchorXoffset" => "X 오프셋 (pt)", "anchorYoffset" => "Y 오프셋 (pt)",
        "anchorSpaceAbove" => "위쪽 간격 (pt)", "spineRelative" => "등쪽 기준", "pinPosition" => "열 안에 유지", "lockPosition" => "수동 위치 조정 방지", other => other }
    }
    fn choice(value: &str) -> &str { match value {
        "InlinePosition" => "인라인", "AboveLine" => "줄 위", "Anchored" => "사용자 지정", "TopLeftAnchor" => "왼쪽 위", "TopCenterAnchor" => "가운데 위",
        "TopRightAnchor" => "오른쪽 위", "LeftCenterAnchor" => "왼쪽 가운데", "CenterAnchor" => "가운데", "RightCenterAnchor" => "오른쪽 가운데",
        "BottomLeftAnchor" => "왼쪽 아래", "BottomCenterAnchor" => "가운데 아래", "BottomRightAnchor" => "오른쪽 아래", "AnchorLocation" => "고정 표식",
        "ColumnEdge" => "단 가장자리", "TextFrame" => "텍스트 프레임", "PageMargins" => "페이지 여백", "PageEdge" => "페이지 가장자리",
        "LineBaseline" => "줄 기준선", "Capheight" => "대문자 높이", "LineXheight" => "소문자 높이", "TopOfLeading" => "행간 위쪽",
        "EmboxTop" => "전각 상자 위쪽", "EmboxMiddle" => "전각 상자 가운데", "EmboxBottom" => "전각 상자 아래쪽", "LeftAlign" => "왼쪽",
        "CenterAlign" => "가운데", "RightAlign" => "오른쪽", "TextAlign" => "텍스트 정렬", "TopAlign" => "위", "BottomAlign" => "아래", other => other }
    }
    let fields: Vec<Value> = FIELDS.iter().map(|f| {
        let mut def = json!({"name":f.key, "label":label(f.key), "group":""});
        if f.boolean() {
            def["type"] = json!("boolean"); def["default"] = json!(f.default == "true");
        } else if !f.choices.is_empty() {
            def["type"] = json!("enum"); def["default"] = json!(f.default);
            def["options"] = f.choices.iter().map(|c| json!({"value":c, "label":choice(c)})).collect();
        } else {
            def["type"] = json!("number"); def["default"] = json!(f.default.parse::<f64>().unwrap_or(0.)); def["min"] = json!(-100000); def["max"] = json!(100000);
        }
        def
    }).collect();
    json!({
        "title": "고정 개체 옵션", "target": "explicit", "maxIds": 1, "defaults": "anchoredObject", "skipEmpty": true, "style": "options", "fields": fields,
        "actions": {"ok": [{"op":"setAnchoredObject", "ids":"$ids", "props":{"$changed":true}}]},
        "description": "ids is the id of the anchored object; the values start from its current settings, and only those that differ are sent."
    })
}
