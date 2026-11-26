//! Choice lists from the document, application and dialog schema.

use super::{
    Refusal, document_info, flattener, graphic_of, mixed_ink, newdoc, pdfdlg, printdlg, profdlg,
    shortcuts, swatches, tocdlg, userdict,
};
use serde_json::{Map, Value, json};

/// The lists a dialog's `choice` fields pick from (every source named once), each `[{value, label}]`; labels are the document's own names.
pub(super) fn choice_lists(def: &Value, s: Option<&crate::Session>, ids: &Value, requested: &Value, profiles: Option<&crate::Profiles>) -> Result<Value, Refusal> {
    let mut sources = vec![];
    for f in def["fields"].as_array().into_iter().flatten() {
        for f in std::iter::once(f).chain(f["fields"].as_array().into_iter().flatten()) {
            if let Some(source) = f["source"].as_str().filter(|_| f["type"] == "choice") { if !sources.contains(&source) { sources.push(source); } }
            if let Some(source) = f["unique"].as_str() { if !sources.contains(&source) { sources.push(source); } }
        }
    }
    let mut lists = Map::new();
    // the lists that are not the document's (the user's document presets, the installed fonts)
    sources.retain(|source| newdoc::choice_list(source, requested).or_else(|| project_folders(source, requested)).or_else(|| printer_list(source)).or_else(|| userdict::choice_list(source))
        .or_else(|| profile_space(source).filter(|_| !source.starts_with("document")).map(|space| profile_choices(def, source, space, profiles)))
        .or_else(|| (*source == "flattenerPresets").then(flattener::saved_choices))
        // the PDF presets: the built-in ones and the project's (`project`)
        .or_else(|| (*source == "pdfPresets").then(|| pdfdlg::choices(requested["project"].as_str().unwrap_or(""))))
        // the print presets: [Default], then the user's (the manager's own list once it has one: `saved`)
        .or_else(|| (*source == "printPresets").then(|| printdlg::choices(requested)))
        .or_else(|| (*source == "shortcutCommands").then(|| shortcuts::opening_rows(requested,
            def["fields"].as_array().into_iter().flatten().find(|f| f["source"] == "shortcutCommands").and_then(|f| f["default"].as_str()).unwrap_or(""))))
        .map(|list| lists.insert(source.to_string(), Value::Array(list))).is_none());
    if sources.is_empty() { return Ok(Value::Object(lists)); }
    let s = s.ok_or("no document open")?;
    let info = document_info(s)?;
    for source in sources {
        let list: Vec<Value> = match source {
            "paragraphStyles" | "characterStyles" | "objectStyles" | "tableStyles" | "cellStyles" | "characterStylesPlain" => info[if source == "characterStylesPlain" { "characterStyles" } else { source }]
                .as_array().into_iter().flatten().filter(|style| source != "characterStylesPlain" || !style["id"].as_str().unwrap_or("").contains("$ID/")).map(|style| {
                let mut path: Vec<String> = style["group"].as_array().into_iter().flatten().filter_map(|g| g.as_str().map(str::to_owned)).collect();
                path.push(style["name"].as_str().unwrap_or("").to_owned());
                json!({"value": style["id"], "label": path.join(" / ")})
            }).collect(),
            "layers" | "swatches" => info[source].as_array().into_iter().flatten().map(|x| json!({"value": x["id"], "label": x["name"]})).collect(),
            "pages" => info["pages"].as_array().into_iter().flatten().map(|p| json!({"value": p["id"], "label": p["name"]})).collect(),
            // the targets a cross-reference can point at: the named text anchors and, by their text, the paragraphs of the other stories
            "strokeStyles" => s.doc.stroke_styles.iter().filter(|x| x.kind != "StrokeStyle").map(|x| json!({"value": x.id, "label": x.name})).collect(),
            "preflightProfiles" => s.doc.preflight_profiles.profiles.iter().map(|p| json!({"value": p.name, "label": p.name})).collect(),
            "crossReferenceDestinations" => s.doc.hyperlinks.destinations.iter().filter(|d| !d.story.is_empty()).map(|d| json!({"value": d.id, "label": d.name})).collect(),
            "crossReferenceFormats" => s.doc.hyperlinks.formats.iter().map(|f| json!({"value": f.id, "label": f.name})).collect(),
            "crossReferenceParagraphs" => {
                let story = requested["story"].as_str().unwrap_or("");
                let targets = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
                    .editor.cross_reference_targets().map_err(|e| e.to_string())?;
                targets.as_array().into_iter().flatten().filter(|p| p["story"].as_str() != Some(story))
                    .map(|p| json!({"value": format!("{}:{}", p["story"].as_str().unwrap_or(""), p["at"]), "label": p["text"]})).collect()
            }
            // the source profiles a placed picture may be given: the document's default, None when it has none, the installed
            // profiles of its colour space (and the one it has); the first two are screen texts (`translate`)
            "imageProfiles" => {
                let (space, current) = graphic_of(s, ids[0].as_str().unwrap_or("")).map(|g| (g.image_space.clone().unwrap_or_default(),
                    g.image_color.as_ref().map(|c| c.profile.clone()).unwrap_or_default())).unwrap_or_default();
                let mut names: Vec<String> = profiles.map(|p| p.get().entries().iter().filter(|e| e.space.trim() == space)
                    .filter_map(|e| e.names.first().cloned()).collect()).unwrap_or_default();
                names.push(current.clone());
                names.retain(|n| !n.is_empty() && n != folio_model::image_color::DOCUMENT_PROFILE && n != "None");
                names.sort(); names.dedup();
                let mut list = vec![json!({"value": folio_model::image_color::DOCUMENT_PROFILE, "label": "문서 기본값 사용", "translate": true})];
                if current == "None" { list.push(json!({"value": "None", "label": "없음", "translate": true})); }
                list.extend(names.into_iter().map(|n| json!({"value": n, "label": n})));
                list
            }
            // a placed PSD's layer comps as the Layer Comp popup lists them: [Custom] (only while it is the value), Last Document State, then the
            // comps by their position (an unnamed one is the Default Comp); the first two and that one are screen texts
            "layerComps" => {
                let comps = graphic_of(s, ids[0].as_str().unwrap_or("")).and_then(|g| g.layer_options.as_ref()).map(|o| o.comps.clone()).unwrap_or_default();
                let mut list = vec![json!({"value": "-2", "label": "[사용자 지정]", "translate": true, "only": true}), json!({"value": "-1", "label": "마지막 문서 상태", "translate": true})];
                list.extend(comps.iter().enumerate().map(|(i, c)| if c.name.is_empty() { json!({"value": i.to_string(), "label": "기본 컴프", "translate": true}) } else { json!({"value": i.to_string(), "label": c.name}) }));
                list
            }
            // Assign Profiles: the installed profiles of the space, and "not assigned" while the document names none (offered only then)
            "documentRgbProfiles" | "documentCmykProfiles" => {
                let space = profile_space(source).unwrap_or_default();
                let key = if space == "RGB" { "RGBProfile" } else { "CMYKProfile" };
                let named = s.edit.as_ref().ok_or("no document open for editing")?.lock().expect("shared state must remain unpoisoned after each operation")
                    .editor.color_preferences().map_err(|e| e.to_string())?.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
                let mut list = profile_choices(def, source, space, profiles);
                if named.is_empty() { list.insert(0, json!({"value": "", "label": "지정 안 함", "translate": true, "only": true})); }
                list
            }
            // the document's mixed ink groups by their names
            "mixedInkGroups" => s.doc.print.mixed_ink_groups.keys().map(|id| json!({"value": id, "label": mixed_ink::mixed_group_name(id)})).collect(),
            // what takes the place of a mixed ink group's swatches where they were used: not the group, its swatches, their tints, a gradient
            "replacementSwatches" => {
                let group = ids[0].as_str().unwrap_or("");
                let members = s.doc.print.mixed_ink_groups.get(group).cloned().unwrap_or_default();
                info["swatches"].as_array().into_iter().flatten().filter(|x| {
                    let id = x["id"].as_str().unwrap_or("");
                    id != group && !members.iter().any(|m| m == id) && x["model"] != "gradient" && !x["base"].as_str().is_some_and(|b| members.iter().any(|m| m == b))
                }).map(|x| json!({"value": x["id"], "label": x["name"]})).collect()
            }
            // the document's colour groups (not the root one that holds the loose swatches)
            "colorGroups" => swatches::color_groups(s)?.into_iter().filter(|g| g["root"] != true).map(|g| json!({"value": g["id"], "label": g["name"]})).collect(),
            // the colours a gradient stop takes (as the stop editor lists them), with the screen colour its ramp is drawn in
            // the alpha channels and Photoshop paths of the picture's linked file (Object ▸ Clipping Path ▸ Options)
            "clippingAlphaChannels" => crate::clippaths::sources(s, ids[0].as_str().unwrap_or("")).alpha_channels.into_iter().map(|n| json!({"value": n, "label": n})).collect(),
            "clippingPhotoshopPaths" => crate::clippaths::sources(s, ids[0].as_str().unwrap_or("")).paths.into_iter().map(|n| json!({"value": n, "label": n})).collect(),
            "tocStyles" => tocdlg::choices(s),
            // the other documents of the project (Index panel ▸ Import Topics)
            "projectDocuments" => crate::bookindex::project_documents(s).into_iter().map(|p| json!({"value": p, "label": p})).collect(),
            "preflightProfileList" => profdlg::choices(s, requested),
            "stopColors" => info["swatches"].as_array().into_iter().flatten().filter(|x| ["process", "spot", "tint", "paper"].contains(&x["model"].as_str().unwrap_or("")))
                .map(|x| json!({"value": x["id"], "label": x["name"], "model": x["model"], "rgb": x["rgb"]})).collect(),
            other => return Err(format!("unknown dialog choice source {other}").into()),
        };
        lists.insert(source.into(), Value::Array(list));
    }
    Ok(Value::Object(lists))
}

/// The colour space of a profile list (`rgbProfiles`, `documentCmykProfiles`, …).
pub(super) fn profile_space(source: &str) -> Option<&'static str> {
    match source { "rgbProfiles" | "documentRgbProfiles" => Some("RGB"), "cmykProfiles" | "documentCmykProfiles" => Some("CMYK"), _ => None }
}

/// The colour profiles installed on this computer of a space (Folio bundles none), by their names, with the defaults of the fields that pick
/// from the list (a working space this computer lacks is still offered, as the screen offered the value it had); sorted, each once.
pub(super) fn profile_choices(def: &Value, source: &str, space: &str, profiles: Option<&crate::Profiles>) -> Vec<Value> {
    let mut names: Vec<String> = profiles.map(|p| p.get().entries().iter().filter(|e| e.space.trim() == space)
        .filter_map(|e| e.names.first().cloned()).collect()).unwrap_or_default();
    names.extend(def["fields"].as_array().into_iter().flatten().filter(|f| f["type"] == "choice" && f["source"] == source).filter_map(|f| f["default"].as_str().map(str::to_owned)));
    names.retain(|n| !n.is_empty());
    names.sort(); names.dedup();
    names.into_iter().map(|n| json!({"value": n, "label": n})).collect()
}

/// `projectFolders`: the folders of the project a file can be saved in (`project` of the values; its project-relative paths, as the
/// project explorer lists them; the project folder itself is the empty choice).
fn project_folders(source: &str, requested: &Value) -> Option<Vec<Value>> {
    if source != "projectFolders" { return None; }
    fn walk(entries: &Value, out: &mut Vec<Value>) {
        for e in entries.as_array().into_iter().flatten().filter(|e| e["kind"] == "dir") {
            out.push(json!({"value": e["path"], "label": e["path"]}));
            walk(&e["children"], out);
        }
    }
    let mut out = vec![];
    if let Ok(project) = folio_project::manifest::open(std::path::Path::new(requested["project"].as_str().unwrap_or(""))) {
        walk(&folio_project::tree::list(&project)["entries"], &mut out);
    }
    Some(out)
}

/// `printers`: the printers of this computer (`listPrinters`; none when they cannot be listed), the default one named so (a screen text with its name).
fn printer_list(source: &str) -> Option<Vec<Value>> {
    if source != "printers" { return None; }
    let listed = crate::printing::list_printers();
    Some(listed["printers"].as_array().into_iter().flatten().map(|p| if p["isDefault"] == true {
        json!({"value": p["name"], "label": "{0} (기본)", "translate": true, "args": {"0": p["name"]}})
    } else { json!({"value": p["name"], "label": p["name"]}) }).collect())
}
