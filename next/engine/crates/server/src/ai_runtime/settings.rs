use serde_json::{Map, Value, json};

pub fn settings(req: &Value) -> Result<Value, String> {
    let mut query = json!({});
    if req["cmd"] == "aiSetSettings" {
        let values = req["values"]
            .as_object()
            .ok_or("expected settings values")?;
        for (key, v) in values {
            let valid = valid_preference(key, v);
            if !valid {
                return Err("invalid AI setting".into());
            }
        }
        query["values"] = json!(values);
    }
    let all = crate::user_settings::run(&query)?;
    let mut settings = Map::from_iter([
        ("folio.ai.mode".to_owned(), json!("direct")),
        ("folio.ai.localOnly".into(), json!(false)),
        (super::connections::LIST.into(), json!([])),
        (super::connections::DEFAULT.into(), Value::Null),
        ("folio.ai.sendImages".into(), json!(true)),
        ("folio.ai.highlightChanges".into(), json!(true)),
    ]);
    for (key, v) in all["settings"]["preferences"]
        .as_object()
        .into_iter()
        .flatten()
    {
        if settings.contains_key(key) {
            settings.insert(key.clone(), v.clone());
        }
    }
    Ok(json!({"settings":settings}))
}

pub(crate) fn valid_preference(key: &str, v: &Value) -> bool {
    match key {
        "folio.ai.mode" => matches!(v.as_str(), Some("direct" | "ask" | "readOnly")),
        "folio.ai.sendImages" | "folio.ai.highlightChanges" | "folio.ai.localOnly" => {
            v.is_boolean()
        }
        _ => false,
    }
}
