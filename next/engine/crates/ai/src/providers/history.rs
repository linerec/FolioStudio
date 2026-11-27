use serde_json::{Value, json};

pub fn text(value: &str) -> Value {
    json!({"type":"text","text":value})
}
fn image(png: &Value) -> Value {
    json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":png}})
}
fn content(value: &Value, send_images: bool) -> Vec<Value> {
    let mut fields = value.clone();
    let mut out = Vec::new();
    let img = if fields["type"] == "pageImage" {
        Some(&mut fields)
    } else {
        fields.get_mut("image")
    };
    if let Some(obj) = img.and_then(Value::as_object_mut) {
        // The session store adds the file hash when saving and restores the bytes on read.
        obj.remove("pngRef");
        obj.remove("sha256");
        if let Some(png) = obj.remove("png")
            && send_images
        {
            out.push(image(&png))
        }
    }
    out.insert(0, text(&fields.to_string()));
    out
}
/// Pin the image choice to the first authorized request that includes this part of history.
pub fn image_policy(records: &[Value], send_images: bool) -> Result<Option<Value>, String> {
    if !send_images
        && records
            .iter()
            .any(|r| r["kind"] == "imagePolicy" && r["sendImages"] == true)
    {
        return Err("aiNewSessionRequired".into());
    }
    let image = |v: &Value| v.get("png").is_some() || v.get("pngRef").is_some();
    let pending = records
        .iter()
        .rev()
        .take_while(|r| r["kind"] != "imagePolicy");
    let has_images = pending.into_iter().any(|r| match r["kind"].as_str() {
        Some("user") => r["attachments"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|a| image(&a["image"])),
        Some("tool_result") => image(&r["content"]) || image(&r["content"]["image"]),
        _ => false,
    });
    Ok(has_images.then(|| json!({"kind":"imagePolicy","sendImages":send_images})))
}
pub fn messages(records: &[Value], send_images: bool) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut policies = records
        .iter()
        .filter(|r| r["kind"] == "imagePolicy")
        .map(|r| r["sendImages"] == true);
    let mut include = policies.next().unwrap_or(send_images);
    for r in records {
        match r["kind"].as_str() {
            Some("imagePolicy") => include = policies.next().unwrap_or(send_images),
            Some("user") => {
                let mut blocks = vec![text(r["text"].as_str().unwrap_or(""))];
                for a in r["attachments"].as_array().into_iter().flatten() {
                    blocks.extend(content(a, include && send_images))
                }
                out.push(json!({"role":"user","content":blocks}));
            }
            Some("assistant") => out.push(json!({"role":"assistant","content":r["blocks"]})),
            Some("tool_result") => {
                let block = json!({"type":"tool_result","tool_use_id":r["toolUseId"],"is_error":r["isError"],
                    "content":content(&r["content"],include && send_images)});
                if let Some(last) = out.last_mut()
                    && last["role"] == "user"
                    && last["content"][0]["type"] == "tool_result"
                {
                    last["content"].as_array_mut().unwrap().push(block)
                } else {
                    out.push(json!({"role":"user","content":[block]}))
                }
            }
            _ => {}
        }
    }
    out
}
