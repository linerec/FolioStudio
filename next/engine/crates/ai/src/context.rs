use folio_project::{hash::sha256_bytes, idmlparts::decode_base64};
use serde_json::{Value, json};

pub trait Source {
    fn read(&self, name: &str, args: Value) -> Result<Value, String>;
    fn image(&self, args: Value, ids: &[String]) -> Result<Value, String>;
    fn text(&self, story: &str, start: usize, end: Option<usize>) -> Result<Value, String>;
}

fn rect(value: &Value) -> Result<[f64; 4], String> {
    let r: [f64; 4] =
        serde_json::from_value(value.clone()).map_err(|_| "expected four coordinates in pt")?;
    if r[0] >= r[2] || r[1] >= r[3] {
        return Err("empty rectangle".into());
    }
    Ok(r)
}
fn items<'a>(list: &'a Value, out: &mut Vec<&'a Value>) {
    for item in list.as_array().into_iter().flatten() {
        out.push(item);
        items(&item["children"], out);
    }
}
pub fn build(source: &impl Source, request: &Value, send_image: bool) -> Result<Value, String> {
    let kind = request["kind"].as_str().ok_or("missing context kind")?;
    let page = request
        .get("page")
        .map_or(Ok(0), |p| p.as_u64().ok_or("invalid page"))?;
    let mut context = json!({"kind":kind, "page":page, "objects":[]});
    match kind {
        "style" => {
            context["style"] = source.read(
                "styleInfo",
                json!({"kind":request["styleKind"],"style":request["style"]}),
            )?;
            return Ok(context);
        }
        "file" => {
            context["file"] = source.read("fileMetadata", json!({"file":request["file"]}))?;
            return Ok(context);
        }
        "preflight" => {
            let report = source.read("preflight", json!({}))?;
            if let Some(selector) = request.get("finding") {
                let finding = report["report"]["findings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|f| ["rule", "page", "item", "messageId", "messageArgs"]
                        .iter().all(|key| f[key] == selector[key]))
                    .ok_or("finding is missing")?;
                context["finding"] = finding.clone();
                context["region"] = finding["bounds"].clone();
                context["page"] = finding["page"].clone();
            } else {
                context["preflight"] = report;
            }
            return Ok(context);
        }
        "text" => {
            let story = request["story"].as_str().ok_or("missing story")?;
            let offset = |k: &str| {
                request
                    .get(k)
                    .map(|v| {
                        v.as_u64()
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or("invalid text offset")
                    })
                    .transpose()
            };
            context["text"] = source.text(story, offset("start")?.unwrap_or(0), offset("end")?)?;
        }
        "objects" | "region" | "page" => {}
        _ => return Err("unknown context kind".into()),
    }
    let inspected = source.read("pageItems", json!({"page":page}))?;
    let data = &inspected["items"];
    let mut all = Vec::new();
    items(&data["items"], &mut all);
    items(&data["masterItems"], &mut all);
    let region = if kind == "region" {
        Some(rect(&request["region"])?)
    } else {
        None
    };
    let ids: Vec<String> = if kind == "objects" {
        serde_json::from_value(request["ids"].clone()).map_err(|_| "objects needs an ids array")?
    } else {
        Vec::new()
    };
    if kind == "objects"
        && (ids.is_empty()
            || ids
                .iter()
                .any(|id| !all.iter().any(|i| i["id"] == id.as_str())))
    {
        return Err("selection is missing from this page".into());
    }
    let mut crop: Option<[f64; 4]> = None;
    let mut objects = Vec::new();
    let mut inspected_objects = Vec::new();
    for i in all {
        let b: [f64; 4] =
            serde_json::from_value(i["bounds"].clone()).map_err(|_| "invalid item bounds")?;
        if kind == "objects" && !ids.iter().any(|id| i["id"] == id.as_str())
            || kind == "text" && i["text"]["story"] != request["story"]
            || region.is_some_and(|r| b[2] < r[0] || r[2] < b[0] || b[3] < r[1] || r[3] < b[1])
        {
            continue;
        }
        let story = i["text"]["story"].as_str();
        let text = if kind == "text" {
            context["text"].clone()
        } else {
            story
                .map(|s| source.text(s, 0, None))
                .transpose()?
                .unwrap_or(Value::Null)
        };
        inspected_objects.push(i);
        objects.push(json!({"kind":i["kind"],"id":i["id"],"name":i["name"],"page":page,
            "bounds":b,"style":i["style"],"fill":i["fill"],"stroke":i["stroke"],"link":i["content"]["link"],"text":text}));
        crop = Some(crop.map_or(b, |r| {
            [
                r[0].min(b[0]),
                r[1].min(b[1]),
                r[2].max(b[2]),
                r[3].max(b[3]),
            ]
        }));
    }
    context["objects"] = json!(objects);
    context["structureHash"] = json!(sha256_bytes(
        &serde_json::to_vec(&(&inspected_objects, &context)).map_err(|e| e.to_string())?,
    ));
    let mut render = json!({"page":page,"longEdge":1568,"selection":kind=="objects"});
    if let Some(r) = region.or(crop.filter(|_| matches!(kind, "objects" | "text"))) {
        render["region"] = json!([r[0], r[1], r[2].max(r[0] + 0.01), r[3].max(r[1] + 0.01)]);
        context["region"] = render["region"].clone();
    }
    if !send_image {
        return Ok(context);
    }
    let mut image = source.image(render, &ids)?;
    let bytes = decode_base64(image["png"].as_str().ok_or("render did not return PNG")?)
        .ok_or("invalid PNG data")?;
    for k in ["width", "height"] {
        if !image[k].as_u64().is_some_and(|v| (1..=1568).contains(&v)) {
            return Err("invalid image dimensions".into());
        }
    }
    image["sha256"] = json!(sha256_bytes(&bytes));
    context["image"] = image;
    Ok(context)
}
