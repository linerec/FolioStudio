use serde_json::{Value, json};

// Explicit capability review: a new engine command is never exposed automatically.
pub const ALLOWED: &[&str] = &[
    "open", "documentInfo", "saveCopy", "exportIdml", "undo", "redo", "editState",
    "edit.createByDrag",
];

/// Tools that exist only in the MCP server (not engine commands). They never touch the project.
pub const NATIVE: &[&str] = &["coverage"];

pub use super::attached::registry;

pub fn registry_names() -> Vec<String> {
    registry()["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect()
}

// One list of secret-looking keys for the engine and every MCP mode (attached.rs).
#[allow(unused_imports)]
pub use super::attached::{find_secret, is_secret_name};

fn native_tool(name: &str) -> Value {
    match name {
        "coverage" => json!({
            "name": "coverage",
            "description": "List the registry decisions for a project server and the attached app. For the app see attached.registryOpen, attached.withheld and attached.unreviewed. Pass name for the specific reason and any equivalent value tool.",
            "inputSchema": {"type":"object","properties":{"name":{"type":"string"}},"required":[],"additionalProperties":false},
            "outputSchema": {"type":"object","properties":{"ok":{"type":"boolean"},"type":{"type":"string"},"total":{"type":"integer"},
                "exposed":{"type":"array","items":{"type":"string"}},"withheld":{"type":"array"},"unreviewed":{"type":"array"}}}
        }),
        "appConnection" => json!({
            "name": "appConnection",
            "description": "Whether this MCP server is attached to the running Folio app (folio-mcp --attach). Attached, the tools act on the document open on screen, its selection, current page, common dialogs, panels and recent list, and the screen shows every change at once. Not attached: why, and how to turn the connection on in Folio.",
            "inputSchema": {"type":"object","properties":{},"required":[],"additionalProperties":false},
            "outputSchema": {"type":"object","properties":{"ok":{"type":"boolean"},"attached":{"type":"boolean"},"code":{"type":"string"},
                "reason":{"type":"string"},"howTo":{"type":"string"}}}
        }),
        // the app's Place: the engine registry has `placeGraphic` with its prepared `source` only, so the tool is declared here
        "edit.placeGraphic" => json!({
            "name": "edit.placeGraphic",
            "description": "Place a picture file of the project (File ▸ Place) in the running Folio app's open document: into the frame id, or on page at point, or into the rectangle rect [x0, y0, x1, y1] (spread pt) on layer. MCP: file is relative to the project open in the app (attached.rs PATHS rules); needs Preferences ▸ General ▸ LLM may write files. The screen shows the change at once.",
            "inputSchema": {"type":"object","properties":{
                "file":{"type":"string","description":"Project-relative path of a picture (JPEG, PNG, TIFF, PSD, GIF, BMP, PDF)."},
                "id":{"type":"string","description":"A frame to place into (an id from pageItems)."},
                "page":{"type":"string","description":"Page id (documentInfo) when there is no frame."},
                "point":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2},
                "rect":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4},
                "layer":{"type":"string","description":"Layer id (documentInfo)."}},
                "required":["file"],"additionalProperties":false},
            "outputSchema": {"type":"object","properties":{"ok":{"type":"boolean"},"created":{"type":"array"},"label":{"type":"string"}}}
        }),
        // Load Styles…: the registry's edit.loadStyles takes the parts the dialog read; an attached client names the file instead
        "edit.loadStyles" => json!({
            "name": "edit.loadStyles",
            "description": "Load styles from another document of the project (Paragraph / Character / Object Styles ▸ Load Styles…) into the running Folio app's open document. MCP: path is a .folio or .idml relative to the project open in the app; the engine reads its styles as the dialog does (stylesFile shows them and which clash). kinds: paragraph, character, object or text; clash: what to do with a style of the same name (as the dialog); only: the style ids to load (empty: all of kinds). The screen shows the change at once; Edit ▸ Undo names it \"LLM: loadStyles\".",
            "inputSchema": {"type":"object","properties":{
                "path":{"type":"string","description":"Project-relative path of a .folio or .idml document."},
                "kinds":{"type":"array","items":{"type":"string"}},
                "clash":{"type":"string"},
                "only":{"type":"array","items":{"type":"string"}}},
                "required":["path","kinds","clash","only"],"additionalProperties":false},
            "outputSchema": {"type":"object","properties":{"ok":{"type":"boolean"},"label":{"type":"string"}}}
        }),
        _ => unreachable!("native tool must be declared"),
    }
}

/// A path tool of an attached connection (`attached::PATHS`): the registry tool with only the arguments the engine admits.
fn path_tool(tool: &super::attached::PathTool) -> Value {
    if tool.name == "edit.relinkGraphic" {
        return json!({"name":tool.name,
            "description":"Replace the graphic in a frame or update it from the same project-relative file. Use owner from links as id. Requires LLM may write files. Returns dirty and the LLM undo label. Example: {\"id\":\"frame1\",\"file\":\"materials/photo.png\"}.",
            "inputSchema":{"type":"object","properties":{"id":{"type":"string"},"file":{"type":"string"}},"required":["id","file"],"additionalProperties":false},
            "outputSchema":{"type":"object","properties":{"ok":{"type":"boolean"},"dirty":{"type":"boolean"},"label":{"type":"string"}}}});
    }
    if tool.name == "edit.placeText" {
        let mut schema = native_tool("edit.placeGraphic");
        schema["name"] = json!(tool.name);
        let example = super::attached::text_tools::example(tool.name).expect("attached text placement example");
        schema["description"] = json!(format!("Place a project-relative TXT, RTF or DOCX into the current document. Give a frame id or a page and rectangle. Imported styles and swatches can affect this document. Requires LLM may write files. Returns created IDs and the undo label. Example: {example}."));
        schema["inputSchema"]["properties"]["file"]["description"] = json!("Project-relative text file to import.");
        schema["inputSchema"]["properties"]["options"] = json!({"type":"object","description":"Text import options, for example characterSet for TXT."});
        return schema;
    }
    if matches!(tool.name, "edit.placeGraphic" | "edit.loadStyles") { return native_tool(tool.name); }
    let mut schema = registry()["tools"].as_array().unwrap().iter()
        .find(|t| t["name"] == tool.name).expect("attached path tool must be registered").clone();
    let properties = schema["inputSchema"]["properties"].as_object().cloned().unwrap_or_default();
    schema["inputSchema"]["properties"] = Value::Object(properties.into_iter().filter(|(k, _)| tool.fields.contains(&k.as_str())).collect());
    // the arguments the engine fills itself (`project` of an import, `root` of a book) are not required of the client
    if let Some(required) = schema["inputSchema"]["required"].as_array().cloned() {
        schema["inputSchema"]["required"] = json!(required.into_iter().filter(|k| k.as_str().is_some_and(|k| tool.fields.contains(&k))).collect::<Vec<_>>());
    }
    schema["inputSchema"]["additionalProperties"] = json!(false);
    let paths = if tool.fields.iter().any(|f| matches!(*f, "path" | "source" | "dir" | "folder")) { " Paths (path, source, dir, folder) are relative to the project open in the app ('/' separators; no '..', drive, absolute or @named location)." } else { "" };
    let extra = match tool.name {
        "importPlan" | "importRun" => " The source is an INDD / IDML of the project; the document is made in the same project (a new .folio). Links outside the project are not copied in: they stay missing with their address as a hint.",
        "package" | "packagePlan" => " dir is a new folder of the project (its parent must exist); nothing is replaced.",
        "book" => " action: read (open the book at path), create (a new book at path), new (fileName in folder), add (the document, a .folio or .idml of the project), synchronize (the style source's styles into the other documents: they are written), exportPdf (the book's documents as one new PDF at output, with export's PDF options). Other actions stay with the person in the app; every action but read needs the setting LLM may write files.",
        "library" => " action: read (open the library at path), create (a new library at path), new (fileName in folder), store (the items ids of the open document as one asset, named name; their pictures are copied to assets/library/), place (the asset on page, or in text: story, start, end; the edit runs in the open document like the panel's Place, Edit ▸ Undo names it LLM: placeLibraryAsset). Other actions stay with the person in the app; every action but read needs the setting LLM may write files.",
        _ => "",
    };
    let writes = if tool.writes { " Needs Preferences ▸ General ▸ LLM may write files in Folio; new files only (a file a person exported to from the screen may be exported to again)." } else { "" };
    schema["description"] = json!(format!("{} MCP: acts on the running Folio app.{paths}{extra}{writes}", schema["description"].as_str().unwrap_or("")));
    schema
}

fn read_tool(read: &super::attached::reads::Read) -> Value {
    let mut tool = registry()["tools"].as_array().unwrap().iter()
        .find(|t| t["name"] == read.name).expect("registered read").clone();
    let schema = &mut tool["inputSchema"];
    schema["additionalProperties"] = json!(false);
    let props = schema["properties"].as_object_mut().expect("registered arguments");
    props.retain(|key, _| read.fields.contains(&key.as_str()));
    if read.name == "textFind" {
        props.insert("query".into(), super::attached::text_tools::find_query_schema().clone());
    }
    let note = if read.name == "render" {
        props.insert("longEdge".into(), json!({"type":"integer","minimum":1,"maximum":2000,"description":"Longest edge in pixels, default 1200. An explicit scale is pixels per point, capped by longEdge."}));
        for key in ["guides", "selection"] { props.insert(key.into(), json!({"type":"boolean","default":false})); }
        "Return one PNG image and its width and height. page is zero-based. region is [left, top, right, bottom] in points from the page's top-left corner. guides and selection draw overlays."
    } else {
        props.insert("resultOffset".into(), json!({"type":"integer","minimum":0}));
        props.insert("resultLimit".into(), json!({"type":"integer","minimum":1,"maximum":32768}));
        "Returns the query result. Large results or an explicit resultOffset/resultLimit return {chunk,offset,nextOffset,total} in Unicode characters of serialized JSON. Repeat the same query with nextOffset until null."
    };
    if let Some(required) = schema["required"].as_array_mut() {
        required.retain(|v| v.as_str().is_some_and(|key| read.fields.contains(&key)));
    }
    let fields = if read.fields.is_empty() { "none".into() } else { read.fields.join(", ") };
    let output = tool["outputSchema"]["properties"].as_object().into_iter().flatten()
        .map(|(k,_)| k.as_str()).collect::<Vec<_>>().join(", ");
    let path = if super::attached::reads::input_file(read.name).is_some() { " The input file must already exist inside the open project." } else { "" };
    let detail = if read.name == "fileMetadata" { " info contains size, modified and parsed XMP fields." } else { "" };
    tool["description"] = json!(format!("{} Attached read. Arguments: {}. Result fields: {}. {note}{path}{detail}",
        tool["description"].as_str().unwrap_or("").lines().next().unwrap_or(""), fields, output));
    if read.name == "render" {
        tool["description"] = json!(format!("Render a page of the open document. Arguments: {fields}. {note} Image metadata: type, page, width, height, region, selection, guides."));
    }
    // A chunk and an image's metadata are also valid structured results.
    tool["outputSchema"] = json!({"type":"object"});
    tool
}

/// The tools of a connection attached to the running app: `attached::ATTACHED` and `attached::DATABASE` from the registry,
/// then the MCP-only tools. The engine checks the same list again (attached.rs `admit`).
pub fn attached_tools() -> Vec<Value> {
    let registry = registry();
    let mut tools: Vec<Value> = super::attached::document_tools().chain(super::attached::DATABASE).map(|name| {
        let mut tool = registry["tools"].as_array().unwrap().iter().find(|t| t["name"] == *name)
            .or_else(|| super::attached::legacy_tool(name)).expect("reviewed attached tool").clone();
        tool["name"] = json!(name);
        if let Some(definition) = super::attached::object_tool(name) {
            tool["description"] = definition["description"].clone();
            tool.as_object_mut().unwrap().remove("example");
            if tool.get("outputSchema").is_none() {
                tool["outputSchema"] = json!({"type":"object","properties":{
                    "created":{"type":"array","items":{"type":"string"}},
                    "dirty":{"type":"boolean"},"label":{"type":"string"}
                }});
            }
        }
        if let Some(fields) = super::attached::value_tools::fields(name) {
            if let Some(args) = tool["_meta"]["arguments"].as_array_mut() {
                args.retain(|arg| arg["name"].as_str().is_some_and(|n| fields.contains(&n)));
            }
            super::attached::value_tools::restrict_schema(name, &mut tool["inputSchema"], fields);
        }
        if let Some(example) = super::attached::edit_example(name) {
            let host = super::attached::text_tools::host_fields(name);
            tool["inputSchema"]["properties"].as_object_mut().unwrap().retain(|key, _| !host.contains(&key.as_str()));
            if let Some(required) = tool["inputSchema"]["required"].as_array_mut() {
                required.retain(|key| !host.contains(&key.as_str().unwrap_or("")));
            }
            tool["description"] = json!(format!("{} Open document only. Returns dirty, created IDs and the LLM undo label. Example: {example}", tool["description"].as_str().unwrap_or("")));
        }
        tool["inputSchema"]["additionalProperties"] = json!(false);
        let note = if super::attached::DATABASE.contains(name) {
            " MCP: needs Preferences ▸ General ▸ LLM may refresh database sources in Folio; uses the connection a person unlocked in the app (passwords never travel over MCP)."
        } else if *name == "dataMergeSourceStatus" {
            " MCP: check true reaches the database and needs Preferences ▸ General ▸ LLM may refresh database sources."
        } else if *name == "openDialog" {
            " MCP: acts on the running Folio app; the screen shows the change at once. App dialogs: newDocument, newBook and newLibrary are made in the project open in the app, newProject in the folder a person chose in Preferences ▸ General (refused with how to choose it when there is none); Folio fills their project / parent fields; applying them needs the setting LLM may write files and replaces nothing. Fields named folio.mcp.* (what an LLM may do) are set by a person only."
        } else { " MCP: acts on the running Folio app; the screen shows the change at once." };
        tool["description"] = json!(format!("{}{note}", tool["description"].as_str().unwrap_or("")));
        tool
    }).collect();
    // the project paths (MCP.ko.md "경로를 받는 도구와 경로 규칙"), before the MCP-only tools
    tools.extend(super::attached::reads::READS.iter().map(read_tool));
    tools.extend(super::attached::PATHS.iter().map(path_tool));
    tools.extend(["coverage", "appConnection"].iter().map(|name| native_tool(name)));
    for tool in &tools {
        assert!(find_secret(&tool["inputSchema"]["properties"]).is_none(), "an exposed MCP tool must not accept secrets");
    }
    tools
}

/// The engine request for a registry tool (`_meta.wire`): `{"cmd":…, …arguments}`, or an edit with one command.
pub fn request_for(name: &str, arguments: &Value) -> Result<Value, String> {
    if matches!(name, "edit.placeGraphic" | "edit.relinkGraphic" | "edit.placeText") || super::attached::legacy_tool(name).is_some() {
        let op = name.trim_start_matches("edit.");
        let mut fields = arguments.as_object().cloned().unwrap_or_default();
        fields.insert("op".into(), json!(op));
        return Ok(json!({"cmd": "edit", "label": op, "commands": [Value::Object(fields)]}));
    }
    let tool = registry()["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).ok_or("not a registered tool")?;
    let wire = &tool["_meta"]["wire"];
    let cmd = wire["cmd"].as_str().ok_or("tool has no wire command")?;
    let mut fields = arguments.as_object().cloned().unwrap_or_default();
    if let Some(op) = wire["op"].as_str() {
        fields.insert("op".into(), json!(op));
        return Ok(json!({"cmd": cmd, "label": op, wire["container"].as_str().unwrap_or("commands"): [Value::Object(fields)]}));
    }
    fields.insert("cmd".into(), json!(cmd));
    Ok(Value::Object(fields))
}

pub fn tools() -> Vec<Value> {
    let registry = registry();
    let mut tools: Vec<Value> = ALLOWED.iter().map(|name| {
        let mut tool = registry["tools"].as_array().unwrap().iter()
            .find(|t| t["name"] == *name).expect("reviewed tool must be registered").clone();
        tool["inputSchema"]["additionalProperties"] = json!(false);
        if *name == "edit.createByDrag" {
            tool["inputSchema"]["properties"]["kind"]["enum"] = json!(["rectangle", "ellipse", "polygon", "line", "rectangleFrame", "ellipseFrame", "polygonFrame"]);
            tool["inputSchema"]["properties"]["sides"]["maximum"] = json!(1000);
        }
        if matches!(*name, "open" | "documentInfo") {
            // The full model is over 100 KB for a small page: clients cannot take it, so the default is a summary.
            tool["inputSchema"]["properties"]["detail"] = json!({"type":"string","enum":["summary","full"],"default":"summary",
                "description":"summary: ids of layers, spreads, pages and items, page boxes and counts (small). full: the complete document model (over 100 KB)."});
        }
        let note = match *name {
            "open" => " MCP: project-relative .idml/.folio only; external assets and fonts are not imported. Returns a compact summary (ids of layers, pages and items) unless detail is full.",
            "saveCopy" => " MCP: new .folio path only; never overwrites an existing file.",
            "exportIdml" => " MCP: open a saved .folio first; new .idml path only.",
            "documentInfo" => " MCP: returns the parsed model (summary by default; detail full for everything), without text composition or rendering.",
            _ => "",
        };
        tool["description"] = json!(format!("{}{note}", tool["description"].as_str().unwrap_or("")));
        tool
    }).collect();
    tools.extend(NATIVE.iter().map(|name| native_tool(name)));
    for tool in &tools {
        // Fail fast: an exposed tool must never declare a place for a secret.
        assert!(find_secret(&tool["inputSchema"]["properties"]).is_none(), "an exposed MCP tool must not accept secrets");
    }
    tools
}

// Validate the type, required fields and bounds used by the reviewed registry tools.
pub fn validate(value: &Value, schema: &Value) -> Result<(), String> {
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) { return Err("value is not an allowed choice".into()); }
    }
    // a registry argument without a type (a dialog field's value) takes any JSON value
    if schema.is_object() && schema.get("type").is_none() { return Ok(()); }
    let valid = match schema["type"].as_str() {
        Some("object") => value.is_object(), Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(), Some("number") => value.is_number(),
        Some("integer") => value.is_u64() || value.is_i64(), Some("array") => value.is_array(),
        _ => return Err("unsupported tool schema".into()),
    };
    if !valid { return Err("argument type does not match inputSchema".into()); }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) { return Err(format!("missing argument: {key}")); }
            }
        }
        // an object without declared properties (dialog values, a patch) is free-form; its secrets are still refused
        if schema.get("properties").is_some() || schema["additionalProperties"] == false {
            for (key, value) in object {
                let property = schema["properties"].get(key).ok_or_else(|| format!("unknown argument: {key}"))?;
                validate(value, property)?;
            }
        }
    }
    if let Some(values) = value.as_array() {
        if schema["minItems"].as_u64().is_some_and(|n| values.len() < n as usize)
            || schema["maxItems"].as_u64().is_some_and(|n| values.len() > n as usize) {
            return Err("invalid array length".into());
        }
        if schema.get("items").is_some() { for item in values { validate(item, &schema["items"])?; } }
    }
    if let Some(n) = value.as_f64() {
        if schema["minimum"].as_f64().is_some_and(|min| n < min)
            || schema["maximum"].as_f64().is_some_and(|max| n > max) {
            return Err("number outside allowed range".into());
        }
    }
    Ok(())
}
