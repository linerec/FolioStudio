//! `folio-mcp --attach`: the running app's engine (folio-engine, as Electron starts it) opens a local endpoint only while
//! the user allows it, refuses a missing or wrong token, and runs an attached client's requests in the same session.
#[path = "../src/bin/mcp/mod.rs"]
#[allow(dead_code)]
mod mcp;

#[path = "../../../test_support.rs"]
mod test_support;

use mcp::attached;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).unwrap().to_path_buf()
}

fn scratch() -> PathBuf {
    let dir = workspace().join("artifacts/mcp-c3-20261023/tests").join(folio_project::util::uuid_v4());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn has_pdf_profiles() -> bool {
    let installed = folio_color::ProfileIndex::scan_installed();
    let missing: Vec<_> = ["U.S. Web Coated (SWOP) v2", "sRGB IEC61966-2.1"].into_iter()
        .filter(|name| installed.find(name).is_none()).collect();
    if missing.is_empty() { return true; }
    test_support::skip(&format!("attached PDF export requires the fixture profiles: {}", missing.join(", ")));
    false
}

/// Only Folio's own state is isolated: LOCALAPPDATA / APPDATA stay (user fonts and colour profiles live there).
fn isolate(command: &mut Command, data: &Path) {
    command.env("FOLIO_USER_DATA", data.join("app")).env("FOLIO_CACHE_DIR", data.join("cache"))
        .env("FOLIO_COLOR_CACHE", data.join("cache/color-cache"));
}

fn allow(data: &Path, on: bool) {
    std::fs::create_dir_all(data.join("app")).unwrap();
    std::fs::write(data.join("app/settings.json"), json!({"preferences":{attached::ALLOW_CONNECTION: on}}).to_string()).unwrap();
}

/// folio-engine spoken to as the app does: request lines on stdin, framed replies and events on stdout.
struct App {
    child: Child,
    input: ChildStdin,
    frames: mpsc::Receiver<Value>,
    events: Vec<Value>,
    next: u64,
}

impl App {
    fn start(data: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_folio-engine"));
        isolate(&mut command, data);
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let (tx, frames) = mpsc::channel();
        std::thread::spawn(move || loop {
            let mut size = [0u8; 4];
            if output.read_exact(&mut size).is_err() { break; }
            let mut header = vec![0u8; u32::from_le_bytes(size) as usize];
            if output.read_exact(&mut header).is_err() { break; }
            let header: Value = serde_json::from_slice(&header).unwrap();
            let mut body = vec![0u8; header["bodyBytes"].as_u64().unwrap_or(0) as usize];
            if output.read_exact(&mut body).is_err() || tx.send(header).is_err() { break; }
        });
        let mut app = Self { child, input, frames, events: Vec::new(), next: 0 };
        assert!(app.event("ready", |_| true).is_some(), "the engine starts");
        app
    }

    fn request(&mut self, mut request: Value) -> Value {
        self.next += 1;
        request["id"] = json!(self.next);
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        let until = Instant::now() + Duration::from_secs(300);
        loop {
            let frame = self.frames.recv_timeout(until - Instant::now()).expect("the engine answers");
            if frame["id"] == json!(self.next) && frame["event"].is_null() { return frame; }
            if frame["event"].is_string() { self.events.push(frame); }
        }
    }

    /// The first event named `name` (seen or still coming) that `test` accepts.
    fn event(&mut self, name: &str, test: impl Fn(&Value) -> bool) -> Option<Value> {
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(i) = self.events.iter().position(|e| e["event"] == name && test(e)) { return Some(self.events.remove(i)); }
            match self.frames.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(frame) => if frame["event"].is_string() { self.events.push(frame) },
                Err(_) => return None,
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "{}", json!({"cmd":"quit"}));
        let _ = self.input.flush();
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until {
            if matches!(self.child.try_wait(), Ok(Some(_))) { return; }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `folio-mcp --attach` as an MCP client starts it.
struct Mcp {
    child: Child,
    input: ChildStdin,
    output: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl Mcp {
    fn start(data: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_folio-mcp"));
        command.arg("--attach");
        isolate(&mut command, data);
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut mcp = Self { child, input, output, next: 0 };
        mcp.rpc("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"tests","version":"1"}}));
        writeln!(mcp.input, "{}", json!({"jsonrpc":"2.0","method":"notifications/initialized"})).unwrap();
        mcp
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        writeln!(self.input, "{}", json!({"jsonrpc":"2.0","id":self.next,"method":method,"params":params})).unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let reply = self.rpc("tools/call", json!({"name":name,"arguments":arguments}));
        assert!(reply.get("error").is_none(), "{name}: {reply}");
        reply["result"].clone()
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.tool(name, arguments);
        assert_eq!(result["isError"], false, "{name}: {result}");
        result["structuredContent"].clone()
    }

    fn names(&mut self) -> Vec<String> {
        self.rpc("tools/list", json!({}))["result"]["tools"].as_array().unwrap().iter()
            .map(|t| t["name"].as_str().unwrap().to_owned()).collect()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connection_file(data: &Path) -> PathBuf { data.join("app").join(attached::CONNECTION_FILE) }

fn wait_for(test: impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if test() { return true; }
        std::thread::sleep(Duration::from_millis(50));
    }
    test()
}

#[test]
fn attached_policy_admits_only_the_reviewed_app_commands() {
    let edit = |op: &str| json!({"cmd":"edit","label":"x","commands":[{"op":op}]});
    for allowed in [json!({"cmd":"dialogState"}), json!({"cmd":"openDialog","dialog":"stepRepeat"}), json!({"cmd":"panelState"}),
        json!({"cmd":"recentFiles"}), json!({"cmd":"setSelection","ids":[]}), json!({"cmd":"dataMergeSourceStatus"}),
        edit("createByDrag"), edit("stepRepeat")] {
        assert_eq!(attached::admit(&allowed, false, false), Ok(()), "{allowed}");
    }
    // (open, saveCopy, export, dataMergeCreate and the path edits are admitted by name since mcp-c4; their paths are checked
    // against the open project: attached_path_policy below and attached_paths_stay_in_the_open_project)
    for refused in [json!({"cmd":"quit"}), json!({"cmd":"printToDevice"}), json!({"cmd":"dbConnections"}),
        json!({"cmd":"importRun","source":"a.idml","project":"C:/p"}), json!({"cmd":"package","dir":"p","roots":[]}), json!({"cmd":"projectCreate","dir":"p"}),
        json!({"cmd":"edit.createByDrag"}), json!({"nothing":1}), edit("embedLink"),
        json!({"cmd":"edit","commands":[]}), json!({"cmd":"edit","conveyor":{"item":1},"commands":[{"op":"createByDrag"}]})] {
        assert!(attached::admit(&refused, true, true).is_err(), "{refused}");
    }
    // a database is read through the app's unlocked connection only while the user lets the LLM refresh it
    for database in [edit("dataMergeRefresh"), edit("dataMergeSelectDatabase"), json!({"cmd":"dataMergeSourceStatus","check":true})] {
        assert!(attached::admit(&database, false, true).unwrap_err().contains("LLM may refresh database sources"), "{database}");
        assert_eq!(attached::admit(&database, true, false), Ok(()), "{database}");
    }
    // passwords never travel over MCP, whatever the command; the refusal names the key, never the value
    let secret = json!({"cmd":"openDialog","dialog":"stepRepeat","values":{"dbPassword":"hunter2-value"}});
    let refusal = attached::admit(&secret, true, true).unwrap_err();
    assert!(refusal.contains("dbPassword") && !refusal.contains("hunter2-value"), "{refusal}");
    assert_eq!(attached::llm_label("Move"), "LLM: Move");
    assert_eq!(attached::llm_label("LLM: Move"), "LLM: Move");
    for request in [json!({"cmd":"textFind","query":{"text":"a+","grep":true}}),
        json!({"cmd":"edit","commands":[{"op":"findReplace","query":{"text":"a+","grep":true},"text":"b"}]})] {
        assert!(attached::admit(&request, false, false).unwrap_err().contains("query.grep"));
    }
}

#[test]
fn attached_tools_come_from_the_registry_and_coverage_names_them() {
    let names = mcp::catalog::registry_names();
    for name in attached::document_tools().chain(attached::DATABASE) {
        assert!(names.iter().any(|n| n == name) || attached::legacy_tool(name).is_some(), "{name} has no tool declaration");
        // never a secret, a printer, a path or the engine process itself
        if let mcp::coverage::Status::Withheld(reason) = mcp::coverage::status(name) {
            assert!(["needs-session", "external-data"].contains(&reason), "{name}: {reason}");
        }
    }
    let tools = mcp::catalog::attached_tools();
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        if let Some(args) = attached::edit_example(name) {
            mcp::catalog::validate(args, &tool["inputSchema"]).unwrap_or_else(|e| panic!("{name}: {e}"));
            let mut request = mcp::catalog::request_for(name, args).unwrap();
            attached::admit(&request, false, true).unwrap();
            if attached::object_tool(name).is_some() {
                let mut extra = request.clone();
                extra["commands"][0]["unlisted"] = json!(true);
                assert!(attached::admit(&extra, true, true).is_err(), "{name}");
            }
            if name == "edit.placeText" {
                assert!(attached::admit(&request, false, false).is_err());
                continue;
            }
            attached::text_tools::prepare(&mut request);
            serde_json::from_value::<folio_edit::Command>(request["commands"][0].clone())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
    for command in [json!({"op":"generateIndex","include_book_documents":true}), json!({"op":"createTOC","sources":[]}),
                    json!({"op":"placeText","source":{}}), json!({"op":"saveTOCStyle","style":{"includeBookDocuments":true}})] {
        assert!(attached::admit(&json!({"cmd":"edit","commands":[command]}), true, true).is_err());
    }
    assert_eq!(tools.len(), attached::document_tools().count() + attached::DATABASE.len() + attached::PATHS.len() + attached::reads::READS.len() + 2);
    for (name, fields) in attached::value_tools::TOOLS {
        let tool = tools.iter().find(|t| t["name"] == *name).unwrap();
        let schema = &tool["inputSchema"]["properties"];
        let mut args = json!({});
        for field in *fields {
            assert!(schema.get(field).is_some(), "{name}.{field}");
        }
        assert_eq!(schema.as_object().unwrap().len(), fields.len());
        if *name == "setViewState" { args["patch"] = json!({"zoom":1.0}); }
        let request = mcp::catalog::request_for(name, &args).unwrap();
        attached::admit(&request, false, false).unwrap_or_else(|e| panic!("{name}: {e}"));
        args["unlisted"] = json!(true);
        let extra = mcp::catalog::request_for(name, &args).unwrap();
        assert!(attached::admit(&extra, true, true).is_err(), "{name}");
    }
    assert!(attached::admit(&json!({"cmd":"setViewState","patch":{"showHiddenSpreads":true}}), true, true).is_err());
    for (name, args) in [
        ("edit.createOutlinesRange", json!({"story":"s","start":0,"end":1})),
        ("edit.createOutlinesKeepText", json!({"ids":["a"]})),
        ("edit.fillPlaceholderText", json!({"story":"s","at":0})),
        ("edit.updateCrossReference", json!({"id":"x"})),
    ] {
        let mut request = mcp::catalog::request_for(name, &args).unwrap();
        attached::text_tools::prepare(&mut request);
        serde_json::from_value::<folio_edit::Command>(request["commands"][0].clone()).unwrap();
    }
    for read in attached::reads::READS {
        let tool = tools.iter().find(|t| t["name"] == read.name).unwrap();
        let args = match read.name {
            "render" => json!({}),
            "textFind" => json!({"query":{"text":"a"},"resultLimit":1}),
            _ => json!({"resultLimit":1}),
        };
        let req = mcp::catalog::request_for(read.name, &args).unwrap();
        assert_eq!(attached::admit(&req, false, false), Ok(()), "{}", read.name);
        assert_eq!(mcp::coverage::attached_status(read.name), "exposed");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(attached::admit(&json!({"cmd":read.name,"unlisted":true}), true, true).is_err());
    }
    for invalid in [json!({"cmd":"links","roots":[]}), json!({"cmd":"preflightProfileFile","action":"export"}),
                    json!({"cmd":"proofSetup","patch":{}}), json!({"cmd":"textStory","resultLimit":32769})] {
        assert!(attached::admit(&invalid, true, true).is_err());
    }
    for dialog in ["dialogState", "openDialog", "setDialogField", "applyDialog", "cancelDialog", "panelState", "setPanelState",
        "recentFiles", "setRecentFiles", "rememberRecent", "dataMergeSource", "dataMergeEvaluate", "dataMergeSourceStatus",
        "edit.dataMergeRefresh", "coverage", "appConnection"] {
        assert!(tools.iter().any(|t| t["name"] == dialog), "{dialog}");
    }
    let report = mcp::coverage::report(&json!({})).unwrap();
    assert_eq!(report["attached"]["exposed"].as_array().unwrap().len(), attached::document_tools().count() + attached::reads::READS.len());
    assert_eq!(mcp::coverage::report(&json!({"name":"openDialog"})).unwrap()["attached"], "exposed");
    assert_eq!(mcp::coverage::report(&json!({"name":"edit.dataMergeRefresh"})).unwrap()["attached"], "needsDatabaseSetting");
    assert_eq!(mcp::coverage::report(&json!({"name":"dbUnlockConnection"})).unwrap()["attached"], "withheld");
    let drag = mcp::catalog::request_for("edit.createByDrag", &json!({"page":"p","kind":"rectangle"})).unwrap();
    assert_eq!(drag, json!({"cmd":"edit","label":"createByDrag","commands":[{"op":"createByDrag","page":"p","kind":"rectangle"}]}));
    assert_eq!(mcp::catalog::request_for("openDialog", &json!({"dialog":"move"})).unwrap(), json!({"cmd":"openDialog","dialog":"move"}));
}

#[test]
fn attached_renderer_preferences_use_the_app_profile() {
    let data = std::env::var_os("FOLIO_ORACLE_PROFILE_ROOT").map_or_else(scratch, |root| {
        PathBuf::from(root).join(folio_project::util::uuid_v4())
    });
    allow(&data, true);
    let mut app = App::start(&data);
    let programs = json!(["editor-a", "editor-b"]).to_string();
    let seed = app.request(json!({
        "cmd": "userSettings",
        "migrate": {"folio.editWith.programs": programs, "unrelated.setting": "private"}
    }));
    assert_eq!(seed["ok"], true, "{seed}");
    assert!(wait_for(|| connection_file(&data).exists()));
    let mut mcp = Mcp::start(&data);
    let names = mcp.names();
    assert!(names.iter().any(|name| name == "rendererPreferences"));
    assert!(!names.iter().any(|name| name == "userSettings"));
    assert_eq!(
        mcp.ok("rendererPreferences", json!({}))["values"],
        json!({"folio.editWith.programs": programs})
    );

    let expected = json!({
        "folio.editWith.programs": json!(["editor-b"]).to_string(),
        "folio.layersSmallRows": "1",
        "folio.linksPanel.autoSize": "1",
        "folio.linksPanel.columns": json!({"status": false, "page": true}).to_string(),
        "folio.linksPanel.info": "0",
        "folio.objectStyles.smallRows": "1",
        "folio.pagesPanel.options": json!({"pageSize": "l", "parentSize": "s", "top": "pages"}).to_string(),
        "folio.pagesPanel.view": "horizontal",
        "folio.panelOptions": json!({"paragraph": false}).to_string(),
        "folio.preflight.rowLimit": "25",
    });
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(mcp.ok("rendererPreferences", json!({"values": {key: value}}))["values"][key], *value, "{key}");
        assert_eq!(mcp.ok("rendererPreferences", json!({}))["values"][key], *value, "{key}");
        assert!(app.event("user-settings", |event| event["detail"]["settings"]["preferences"][key] == *value).is_some(), "{key}");
    }
    for refused in [
        json!({"unrelated.setting": "changed"}),
        json!({"folio.layersSmallRows": false}),
        json!({"folio.editWith.programs": "[\"new-editor\"]", "folio.linksPanel.info": "1"}),
    ] {
        assert_eq!(mcp.tool("rendererPreferences", json!({"values": refused}))["isError"], true);
    }
    assert_eq!(mcp.ok("rendererPreferences", json!({}))["values"], expected);
    let stored: Value = serde_json::from_slice(&std::fs::read(data.join("app/settings.json")).unwrap()).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(stored["preferences"][key], *value, "{key}");
    }
    assert_eq!(stored["preferences"]["unrelated.setting"], "private");
}

#[test]
fn the_endpoint_follows_the_setting_and_refuses_a_wrong_token() {
    let data = scratch();
    allow(&data, false);
    let mut app = App::start(&data);
    assert_eq!(app.request(json!({"cmd":"userSettings"}))["ok"], true);
    assert!(!connection_file(&data).exists(), "no endpoint while the setting is off");
    {
        // not attached: only the MCP-only tools, and appConnection says why and how to turn it on
        let mut client = Mcp::start(&data);
        let names = client.names();
        assert_eq!(names, ["coverage", "appConnection"]);
        let status = client.ok("appConnection", json!({}));
        assert_eq!(status["attached"], false);
        assert_eq!(status["code"], "notRunning");
        assert!(status["howTo"].as_str().unwrap().contains("Allow LLM connection (MCP)"));
    }
    // turned on in Preferences: the endpoint opens at once with a fresh token
    let on = app.request(json!({"cmd":"userSettings","values":{attached::ALLOW_CONNECTION: true}}));
    assert_eq!(on["ok"], true);
    assert!(wait_for(|| connection_file(&data).exists()));
    let info: Value = serde_json::from_str(&std::fs::read_to_string(connection_file(&data)).unwrap()).unwrap();
    let token = info["token"].as_str().unwrap().to_owned();
    assert!(token.len() >= 32 && info["endpoint"].is_string());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(connection_file(&data)).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // a missing or wrong token is refused before any request is read
    for bad in [json!({"version":1,"endpoint":info["endpoint"],"token":"0".repeat(token.len())}),
        json!({"version":1,"endpoint":info["endpoint"],"token":""})] {
        let file = data.join("forged.json");
        std::fs::write(&file, bad.to_string()).unwrap();
        let failure = mcp::client::connect(&file).err().expect("a wrong token is refused");
        assert_eq!(failure.code, "refused", "{}", failure.message);
    }
    let mut good = mcp::client::connect(&connection_file(&data)).ok().expect("the right token attaches");
    assert_eq!(good.request(json!({"cmd":"recentFiles"})).unwrap()["ok"], true);
    // refused commands do not reach the engine (no project is open: a path has nothing to be relative to)
    let refused = good.request(json!({"cmd":"open","path":"x.folio"})).unwrap();
    assert_eq!(refused["code"], "refused");
    assert_eq!(good.request(json!({"cmd":"quit"})).unwrap()["code"], "refused");
    // turned off: the file goes, the open connection is closed and the old endpoint no longer attaches
    assert_eq!(app.request(json!({"cmd":"userSettings","values":{attached::ALLOW_CONNECTION: false}}))["ok"], true);
    assert!(wait_for(|| !connection_file(&data).exists()));
    let closed = good.request(json!({"cmd":"recentFiles"}));
    assert!(closed.is_err() || closed.unwrap()["ok"] == false);
    let file = data.join("old.json");
    std::fs::write(&file, info.to_string()).unwrap();
    assert!(mcp::client::connect(&file).is_err());
}

/// A `.folio` in a new project, from the basic oracle fixture (through the project MCP server, as in tests/mcp.rs).
fn document(dir: &Path) -> (PathBuf, String, String) {
    let project = dir.join("project");
    folio_project::manifest::create(&project, "MCP 붙기 시험").unwrap();
    std::fs::copy(workspace().join("next/oracle/fixtures/basic.idml"), project.join("source.idml")).unwrap();
    let mut engine = mcp::document::Engine::new(&project).unwrap();
    let opened = engine.call("open", &json!({"path":"source.idml","edit":true})).unwrap();
    let page = opened["summary"]["spreads"][0]["pages"][0]["id"].as_str().unwrap().to_owned();
    let layer = opened["summary"]["layers"][0]["id"].as_str().unwrap().to_owned();
    engine.call("saveCopy", &json!({"path":"doc.folio"})).unwrap();
    (project.join("doc.folio"), page, layer)
}

#[test]
fn an_attached_client_works_in_the_app_session_and_the_screen_hears_of_it() {
    let data = scratch();
    allow(&data, true);
    let (path, page, layer) = document(&data);
    let mut app = App::start(&data);
    let opened = app.request(json!({"cmd":"open","path":path,"edit":true}));
    assert_eq!(opened["ok"], true, "{opened}");
    assert!(wait_for(|| connection_file(&data).exists()));
    let mut client = Mcp::start(&data);
    assert_eq!(client.ok("appConnection", json!({}))["attached"], true);
    let names = client.names();
    assert!(names.contains(&"openDialog".to_owned()) && names.contains(&"open".to_owned()) && !names.contains(&"projectCreate".to_owned()));
    assert_eq!(client.ok("dbCapabilities", json!({}))["platform"], std::env::consts::OS);
    assert_eq!(client.ok("setViewState", json!({"patch":{"zoom":1.25}}))["viewState"]["zoom"].as_f64(), Some(1.25));
    let metadata = client.ok("edit.setFileInfo", json!({"props":{"documentTitle":"MCP"}}));
    assert_eq!(metadata["label"], "LLM: setFileInfo");
    let directory = data.to_string_lossy().replace('\\', "/");
    for reply in [metadata, client.ok("documentInfo", json!({}))] {
        assert!(!reply.to_string().replace(r"\\", "/").contains(&directory), "{reply}");
    }

    // an object the LLM makes is in the app's own session: its history names the LLM, and the screen is told to draw it
    let before = app.request(json!({"cmd":"editState"}))["state"]["rev"].clone();
    let made = client.ok("edit.createByDrag", json!({"page":page,"kind":"rectangle","rect":[20,20,80,60],"layer":layer,"sides":4,"inset":0}));
    let id = made["created"][0].as_str().unwrap().to_owned();
    assert_eq!(made["label"], "LLM: createByDrag");
    let state = app.request(json!({"cmd":"editState"}));
    assert_ne!(state["state"]["rev"], before);
    assert_eq!(state["state"]["undo"], "LLM: createByDrag");
    let drawn = app.event("external-edit", |e| e["detail"]["edit"]["created"][0] == id.as_str()).expect("the screen hears of the edit");
    assert_eq!(drawn["detail"]["source"], "mcp");
    assert!(!drawn["detail"]["edit"]["pages"].as_array().unwrap().is_empty());

    // the selection the screen sets is the one the LLM reads, and the other way round
    assert_eq!(app.request(json!({"cmd":"setSelection","ids":[id],"page":0}))["ok"], true);
    assert_eq!(client.ok("viewState", json!({}))["viewState"]["selection"][0]["id"], id.as_str());
    app.events.clear();
    client.ok("setSelection", json!({"ids":[]}));
    let shown = app.event("view-state", |e| e["detail"]["patch"].get("selection").is_some()).expect("the screen follows the LLM's selection");
    assert_eq!(shown["detail"]["viewState"]["selection"], json!([]));

    // a common dialog opened, filled and applied by the LLM is the screen's dialog (dialog-state events)
    client.ok("openDialog", json!({"dialog":"stepRepeat","ids":[id]}));
    assert!(app.event("dialog-state", |e| e["detail"]["dialogState"]["dialog"] == "stepRepeat").is_some());
    client.ok("setDialogField", json!({"field":"count","value":2}));
    assert!(app.event("dialog-state", |e| e["detail"]["dialogState"]["values"]["count"] == 2).is_some());
    let applied = client.ok("applyDialog", json!({"action":"ok"}));
    assert_eq!(applied["created"].as_array().map(Vec::len), Some(2), "{applied}");
    assert!(app.event("dialog-state", |e| e["detail"]["dialogState"].is_null() && !e["detail"]["edit"].is_null()).is_some());
    assert_eq!(app.request(json!({"cmd":"editState"}))["state"]["undo"], "LLM: 단계 및 반복");

    // Undo from the LLM undoes in the app session too
    let undone = client.ok("undo", json!({}));
    assert_eq!(undone["undo"], "LLM: createByDrag");
    assert_eq!(app.request(json!({"cmd":"editState"}))["state"]["undo"], "LLM: createByDrag");
    assert!(app.event("external-edit", |e| e["detail"]["edit"]["undo"] == "LLM: createByDrag").is_some());

    // still refused when attached: a command outside the reviewed lists, a secret, a database without the setting
    let opened = client.rpc("tools/call", json!({"name":"projectCreate","arguments":{"dir":"x"}}));
    assert_eq!(opened["error"]["code"], -32602);
    let secret = client.rpc("tools/call", json!({"name":"openDialog","arguments":{"dialog":"move","values":{"password":"never-shown"}}}));
    assert_eq!(secret["error"]["code"], -32602);
    assert!(!secret.to_string().contains("never-shown"));
    let database = client.tool("dataMergeSourceStatus", json!({"check":true}));
    assert_eq!(database["isError"], true);
    assert!(database["structuredContent"]["error"].as_str().unwrap().contains("LLM may refresh database sources"));
}

#[test]
fn attached_path_policy() {
    // the path tools are admitted by name, with their reviewed arguments only (MCP.ko.md "경로를 받는 도구와 경로 규칙")
    let edit = |command: Value| json!({"cmd":"edit","label":"x","commands":[command]});
    for reads in [json!({"cmd":"open","path":"a.folio"}), edit(json!({"op":"selectDataSource","path":"d.csv"})),
        edit(json!({"op":"importPdfComments","path":"c.pdf","again":false}))] {
        assert_eq!(attached::admit(&reads, false, false), Ok(()), "{reads}");
    }
    // writing a file needs Preferences ▸ General ▸ LLM may write files
    for writes in [json!({"cmd":"save"}), json!({"cmd":"saveCopy","path":"b.folio"}), json!({"cmd":"exportIdml","path":"b.idml"}),
        json!({"cmd":"export","path":"b.pdf","preset":"x4"}), json!({"cmd":"dataMergeCreate","path":"d.csv"}),
        edit(json!({"op":"placeGraphic","file":"p.png","page":"u1"})), edit(json!({"op":"relinkGraphic","id":"u1","file":"p.png"}))] {
        assert_eq!(attached::admit(&writes, false, false).unwrap_err(), attached::FILES_REFUSED, "{writes}");
        assert_eq!(attached::admit(&writes, false, true), Ok(()), "{writes}");
    }
    // arguments that would skip the path check, or reach beyond the project, are refused even with the setting on
    for skips in [json!({"cmd":"open","path":"a.folio","edit":true}), json!({"cmd":"export","path":"b.pdf","printingJob":{}}),
        json!({"cmd":"export","path":"b.pdf","settings":{}}), json!({"cmd":"exportIdml","path":"b.idml","roots":["C:/"]}),
        edit(json!({"op":"selectDataSource","path":"d.csv","source":"file:///C:/x.csv"})),
        edit(json!({"op":"importPdfComments","path":"c.pdf","again":false,"comments":[]})),
        edit(json!({"op":"placeGraphic","file":"p.png","source":{"uri":"file:///C:/x.png"}})),
        edit(json!({"op":"relinkGraphic","id":"u1","source":{}})),
        json!({"cmd":"dataMergeCreate","path":"d.csv","_projectImages":false})] {
        assert!(attached::admit(&skips, true, true).unwrap_err().contains("not available"), "{skips}");
    }
    // folio-mcp lists them (placeGraphic is its own tool) and coverage says which need the setting
    let tools = mcp::catalog::attached_tools();
    let open = tools.iter().find(|t| t["name"] == "open").unwrap();
    assert_eq!(open["inputSchema"]["properties"].as_object().unwrap().keys().collect::<Vec<_>>(), ["path"]);
    assert!(tools.iter().any(|t| t["name"] == "edit.placeGraphic" && t["inputSchema"]["required"] == json!(["file"])));
    assert_eq!(mcp::coverage::report(&json!({"name":"exportIdml"})).unwrap()["attached"], "needsFileSetting");
    assert_eq!(mcp::coverage::report(&json!({"name":"open"})).unwrap()["attached"], "exposed");
    assert_eq!(mcp::coverage::report(&json!({"name":"edit.placeGraphic"})).unwrap()["attached"], "needsFileSetting");
    assert_eq!(mcp::coverage::report(&json!({"name":"projectCreate"})).unwrap()["attached"], "withheld");

    // mcp-c5: import, package, styles, book and library, with the same rules
    for reads in [json!({"cmd":"importPlan","source":"in/a.indd","document":"a.folio"}), json!({"cmd":"packagePlan","dir":"pkg","fonts":true}),
        json!({"cmd":"stylesFile","path":"s.idml","kinds":["paragraph"]}),
        edit(json!({"op":"loadStyles","path":"s.idml","kinds":["paragraph"],"clash":"rename","only":[]})),
        json!({"cmd":"book","action":"read","path":"b.foliobook"}), json!({"cmd":"library","action":"read","path":"l.foliolibrary","search":"x"}),
        json!({"cmd":"importCancel"}), json!({"cmd":"packageCancel"})] {
        assert_eq!(attached::admit(&reads, false, false), Ok(()), "{reads}");
    }
    for writes in [json!({"cmd":"importRun","source":"in/a.idml"}), json!({"cmd":"package","dir":"pkg","pdf":false}),
        json!({"cmd":"book","action":"new","folder":"","fileName":"b"}), json!({"cmd":"book","action":"create","path":"b.foliobook","name":"B"}),
        json!({"cmd":"library","action":"new","fileName":"l"}), json!({"cmd":"library","action":"create","path":"l.foliolibrary","name":"L"})] {
        assert_eq!(attached::admit(&writes, false, false).unwrap_err(), attached::FILES_REFUSED, "{writes}");
        assert_eq!(attached::admit(&writes, false, true), Ok(()), "{writes}");
    }
    // the engine fills the project, root and parts; search folders and protected roots of this machine are not taken
    for skips in [json!({"cmd":"importRun","source":"a.idml","project":"C:/other"}), json!({"cmd":"importRun","source":"a.idml","roots":["C:/"]}),
        json!({"cmd":"importPlan","source":"a.idml","newDocument":true}), json!({"cmd":"importRun","source":"a.idml","_projectOnly":false}),
        json!({"cmd":"package","dir":"pkg","roots":["C:/"]}), json!({"cmd":"package","dir":"pkg","protectedRoots":[]}),
        json!({"cmd":"package","dir":"pkg","export":{"settings":{}}}), json!({"cmd":"stylesFile","path":"s.idml","parts":{}}),
        edit(json!({"op":"loadStyles","path":"s.idml","kinds":[],"clash":"skip","only":[],"parts":{}})),
        json!({"cmd":"book","action":"read","path":"b.foliobook","root":"C:/"}), json!({"cmd":"library","action":"new","fileName":"l","replace":true})] {
        assert!(attached::admit(&skips, true, true).unwrap_err().contains("not available"), "{skips}");
    }
    // book and library (mcp-c6): a book's documents added, synchronized and exported to PDF, a library item stored and placed, each
    // needing the file setting; moving, removing, relinking, packaging and the library's other actions stay with the person
    for (cmd, action) in [("book", "add"), ("book", "synchronize"), ("book", "exportPdf"), ("library", "store"), ("library", "place")] {
        let request = json!({"cmd":cmd,"action":action,"path":"x"});
        assert_eq!(attached::admit(&request, false, false).unwrap_err(), attached::FILES_REFUSED, "{cmd} {action}");
        assert_eq!(attached::admit(&request, false, true), Ok(()), "{cmd} {action}");
    }
    for (cmd, action) in [("book", "remove"), ("book", "package"), ("book", "relink"), ("book", "move"), ("book", "preflight"), ("book", "store"),
        ("library", "remove"), ("library", "relink"), ("library", "update"), ("library", "add"), ("library", "exportPdf")] {
        assert!(attached::admit(&json!({"cmd":cmd,"action":action,"path":"x"}), true, true).unwrap_err().contains("not available"), "{cmd} {action}");
    }
    // the PDF of a book takes export's options, not a preset settings object, a print job or a booklet
    for skips in [json!({"cmd":"book","action":"exportPdf","path":"b","output":"b.pdf","settings":{}}),
        json!({"cmd":"book","action":"exportPdf","path":"b","output":"b.pdf","printingJob":{}}), json!({"cmd":"book","action":"package","dir":"p","roots":[]})] {
        assert!(attached::admit(&skips, true, true).unwrap_err().contains("not available"), "{skips}");
    }
    // the settings that say what an LLM may do are a person's: no dialog field of folio.mcp.* over MCP (mcp-c6)
    for asks in [json!({"cmd":"setDialogField","field":attached::ALLOW_FILE_WRITES,"value":true}),
        json!({"cmd":"openDialog","dialog":"preferences","values":{attached::NEW_PROJECT_FOLDER:"C:/"}})] {
        assert!(attached::admit(&asks, true, true).unwrap_err().contains("set by a person"), "{asks}");
    }
    assert_eq!(mcp::coverage::report(&json!({"name":"importRun"})).unwrap()["attached"], "needsFileSetting");
    assert_eq!(mcp::coverage::report(&json!({"name":"package"})).unwrap()["attached"], "needsFileSetting");
    assert_eq!(mcp::coverage::report(&json!({"name":"stylesFile"})).unwrap()["attached"], "exposed");
    assert_eq!(mcp::coverage::report(&json!({"name":"edit.loadStyles"})).unwrap()["attached"], "exposed");
    assert_eq!(mcp::coverage::report(&json!({"name":"importCancel"})).unwrap()["attached"], "exposed");
    let load = tools.iter().find(|t| t["name"] == "edit.loadStyles").unwrap();
    assert!(load["inputSchema"]["properties"].get("parts").is_none() && load["inputSchema"]["required"][0] == "path");
    let import = tools.iter().find(|t| t["name"] == "importRun").unwrap();
    assert_eq!(import["inputSchema"]["required"], json!(["source"]), "the engine fills project");
    let book = tools.iter().find(|t| t["name"] == "book").unwrap();
    assert_eq!(book["inputSchema"]["required"], json!(["action"]), "the engine fills root");
    let loaded = mcp::catalog::request_for("edit.loadStyles", &json!({"path":"s.idml","kinds":["paragraph"],"clash":"rename","only":[]})).unwrap();
    assert_eq!(loaded["commands"][0]["op"], "loadStyles");
    let place = mcp::catalog::request_for("edit.placeGraphic", &json!({"file":"p.png","page":"u1","point":[1,2]})).unwrap();
    assert_eq!(place, json!({"cmd":"edit","label":"placeGraphic","commands":[{"op":"placeGraphic","file":"p.png","page":"u1","point":[1,2]}]}));
}

fn settings(data: &Path, files: bool) {
    std::fs::create_dir_all(data.join("app")).unwrap();
    std::fs::write(data.join("app/settings.json"),
        json!({"preferences":{attached::ALLOW_CONNECTION: true, attached::ALLOW_FILE_WRITES: files}}).to_string()).unwrap();
}

fn sha(path: &Path) -> String { folio_project::hash::sha256_file(path).unwrap().0 }

/// The XML parts of an IDML package, by name (what InDesign reads; the zip's own times aside).
fn parts(path: &Path) -> std::collections::BTreeMap<String, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len()).map(|i| {
        let mut entry = zip.by_index(i).unwrap();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        (entry.name().unwrap().into_owned(), text)
    }).collect()
}

#[test]
fn attached_paths_stay_in_the_open_project() {
    let data = scratch();
    settings(&data, true);
    let (path, page, layer) = document(&data);
    let project = path.parent().unwrap().to_path_buf();
    let original = project.join("source.idml");
    let original_hash = sha(&original);
    std::fs::copy(workspace().join("next/oracle/fixtures/Links/checker.png"), project.join("checker.png")).unwrap();
    std::fs::create_dir_all(project.join("out")).unwrap();
    // a document and a picture outside the project, which no path may reach
    let outside = data.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::copy(&path, outside.join("doc.folio")).unwrap();
    std::fs::copy(workspace().join("next/oracle/fixtures/Links/checker.png"), outside.join("secret.png")).unwrap();
    let mut app = App::start(&data);
    assert_eq!(app.request(json!({"cmd":"projectInfo","path":project}))["ok"], true);
    assert_eq!(app.request(json!({"cmd":"open","path":path}))["ok"], true);
    assert!(wait_for(|| connection_file(&data).exists()));
    let mut client = Mcp::start(&data);
    let refused = |client: &mut Mcp, tool: &str, arguments: Value| {
        let result = client.tool(tool, arguments.clone());
        assert_eq!(result["isError"], true, "{tool} {arguments}: {result}");
        result["structuredContent"]["error"].as_str().unwrap_or("").to_owned()
    };

    // escapes: every one is refused before the engine touches a file
    let outside_slash = outside.to_string_lossy().replace('\\', "/");
    for escape in ["../outside/doc.folio", "out/../../outside/doc.folio", "./doc.folio", &format!("{outside_slash}/doc.folio"),
        "/etc/doc.folio", "\\\\localhost\\c$\\doc.folio", "sub\\doc.folio", "file:///C:/outside/doc.folio", "C:doc.folio",
        "@사진창고/doc.folio", ".folio/locks/doc.folio", "doc.folio:secret", "CON.folio", "doc.folio.", "", "out//doc.folio"] {
        refused(&mut client, "open", json!({"path":escape}));
    }
    for escape in ["../outside/x.idml", ".folio/cache/x.idml", "x.idml:ads", "NUL.idml", "out/x.idml ", "@사진창고/x.idml",
        "folio.project", "missing-folder/x.idml"] {
        refused(&mut client, "exportIdml", json!({"path":escape}));
    }
    refused(&mut client, "edit.placeGraphic", json!({"file":"../outside/secret.png","page":page,"point":[10,10],"layer":layer}));
    refused(&mut client, "edit.placeGraphic", json!({"file":format!("{outside_slash}/secret.png"),"page":page,"point":[10,10]}));
    let name = refused(&mut client, "dataMergeCreate", json!({"path":"data.csv","name":"x"}));
    assert!(name.contains("data.csv"), "{name}");
    std::fs::write(project.join("data.csv"), "Name\nA\n").unwrap();
    let name = refused(&mut client, "dataMergeCreate", json!({"path":"data.csv","name":"../../evil"}));
    assert!(name.contains("one file name"), "{name}");

    // originals and existing files are never overwritten
    for (tool, target) in [("exportIdml", "source.idml"), ("saveCopy", "doc.folio"), ("export", "source.idml")] {
        refused(&mut client, tool, json!({"path":target}));
    }
    assert_eq!(sha(&original), original_hash);

    // an allowed path gives what the app's own command gives: the screen exports with this machine's path, the LLM with the project's
    let screen = app.request(json!({"cmd":"exportIdml","path":project.join("out/app.idml")}));
    assert_eq!(screen["ok"], true, "{screen}");
    let llm = client.ok("exportIdml", json!({"path":"out/mcp.idml"}));
    assert_eq!(llm["path"], "out/mcp.idml", "the reply names the project path, not this machine's: {llm}");
    let (a, b) = (parts(&project.join("out/app.idml")), parts(&project.join("out/mcp.idml")));
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (part, text) in &a {
        if part.starts_with("Spreads/") || part.starts_with("Stories/") || part == "Resources/Styles.xml" { assert_eq!(text, &b[part], "{part}"); }
    }
    assert_eq!(screen["report"]["differences"], llm["report"]["differences"]);
    // the file a person exported to may be exported to again; the LLM's own file may not
    client.ok("exportIdml", json!({"path":"out/app.idml"}));
    refused(&mut client, "exportIdml", json!({"path":"out/mcp.idml"}));
    if has_pdf_profiles() {
        let pdf = client.ok("export", json!({"path":"out/mcp.pdf"}));
        assert_eq!(pdf["result"]["path"], "out/mcp.pdf");
        assert!(std::fs::read(project.join("out/mcp.pdf")).unwrap().starts_with(b"%PDF"));
    }

    // a picture of the project, placed as the app places it: the document refers to it by its project path
    let placed = client.ok("edit.placeGraphic", json!({"file":"checker.png","page":page,"point":[30,30],"layer":layer}));
    assert_eq!(placed["label"], "LLM: placeGraphic");
    assert!(app.event("external-edit", |e| e["detail"]["edit"]["label"] == "LLM: placeGraphic").is_some());

    // save a copy, open it (the screen is told), edit, save, open again: the edit is there and the original is untouched
    let copy = client.ok("saveCopy", json!({"path":"copy.folio"}));
    assert_eq!(copy["path"], "copy.folio");
    assert!(app.event("external-file", |e| e["detail"]["kind"] == "saveCopy").is_some());
    let dirty = refused(&mut client, "open", json!({"path":"copy.folio"}));
    assert!(dirty.contains("unsaved changes"), "{dirty}");
    client.ok("save", json!({}));
    assert!(app.event("external-file", |e| e["detail"]["kind"] == "save").is_some());
    let opened = client.ok("open", json!({"path":"copy.folio"}));
    assert_eq!(opened["document"]["folio"]["document"]["path"], "copy.folio");
    let shown = app.event("external-open", |e| e["path"].as_str().is_some_and(|p| p.ends_with("copy.folio"))).expect("the screen hears of the open");
    assert_eq!(shown["detail"]["source"], "mcp");
    let made = client.ok("edit.createByDrag", json!({"page":page,"kind":"rectangle","rect":[20,20,80,60],"layer":layer,"sides":4,"inset":0}));
    let id = made["created"][0].as_str().unwrap().to_owned();
    client.ok("save", json!({}));
    assert_eq!(app.request(json!({"cmd":"editState"}))["state"]["dirty"], false);
    client.ok("open", json!({"path":"copy.folio"}));
    let items = client.ok("pageItems", json!({"page":0}));
    assert!(items.to_string().contains(&id), "the saved edit is in the reopened document");
    assert_eq!(sha(&original), original_hash, "the original IDML is never written");

    // the setting off: nothing is written over MCP any more (reading still works)
    assert_eq!(app.request(json!({"cmd":"userSettings","values":{attached::ALLOW_FILE_WRITES: false}}))["ok"], true);
    for (tool, arguments) in [("save", json!({})), ("saveCopy", json!({"path":"copy2.folio"})), ("exportIdml", json!({"path":"out/two.idml"})),
        ("export", json!({"path":"out/two.pdf"})), ("edit.placeGraphic", json!({"file":"checker.png","page":page,"point":[5,5]}))] {
        assert!(refused(&mut client, tool, arguments).contains("LLM may write files"), "{tool}");
    }
    assert!(!project.join("copy2.folio").exists() && !project.join("out/two.idml").exists());
    client.ok("open", json!({"path":"doc.folio"}));

    // a link or junction in the project: a path through it is refused; since mcp-c5 only the requested path's names are checked,
    // so a link elsewhere in the tree no longer blocks the other paths (the project's own walks never follow links)
    let linked = project.join("linked");
    #[cfg(windows)]
    let made_link = Command::new("cmd").args(["/C", "mklink", "/J"]).arg(linked.to_string_lossy().replace('/', "\\"))
        .arg(outside.to_string_lossy().replace('/', "\\")).stdout(Stdio::null()).status().is_ok_and(|s| s.success());
    #[cfg(unix)]
    let made_link = std::os::unix::fs::symlink(&outside, &linked).is_ok();
    assert!(made_link, "the junction is made");
    let link = refused(&mut client, "open", json!({"path":"linked/doc.folio"}));
    assert!(link.contains("link"), "{link}");
    assert_eq!(app.request(json!({"cmd":"userSettings","values":{attached::ALLOW_FILE_WRITES: true}}))["ok"], true);
    refused(&mut client, "saveCopy", json!({"path":"linked/stolen.folio"}));
    refused(&mut client, "edit.placeGraphic", json!({"file":"linked/secret.png","page":page,"point":[5,5]}));
    assert!(!outside.join("stolen.folio").exists());
    client.ok("open", json!({"path":"doc.folio"}));
    #[cfg(windows)]
    std::fs::remove_dir(&linked).unwrap();
    #[cfg(unix)]
    std::fs::remove_file(&linked).unwrap();
    client.ok("open", json!({"path":"doc.folio"}));
    assert!(outside.join("secret.png").exists() && outside.join("doc.folio").exists());
    assert_eq!(sha(&original), original_hash);
}

/// Every file below `dir` (no links are followed).
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let kind = entry.file_type().unwrap();
            if kind.is_dir() { pending.push(entry.path()) } else if kind.is_file() { out.push(entry.path()) }
        }
    }
    out
}

/// An IDML whose one picture link is `uri` (the fixture's `file:/oracle/Links/checker.png` replaced).
fn idml_linking(source: &Path, target: &Path, uri: &str) {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(source).unwrap()).unwrap();
    let mut changed = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new();
        if entry.read_to_string(&mut text).is_ok() && text.contains("file:/oracle/Links/checker.png") {
            changed.push((name, text.replace("file:/oracle/Links/checker.png", uri)));
        }
    }
    assert!(!changed.is_empty());
    let mut archive = zip::ZipArchive::new(std::fs::File::open(source).unwrap()).unwrap();
    let parts: Vec<(&str, &str)> = changed.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    folio_project::format::write_package_with_parts(std::fs::File::create(target).unwrap(), &mut archive,
        &mut |name, _| Ok(match changed.iter().find(|(n, _)| n == name) {
            Some((_, text)) => folio_project::format::PartAction::Replace(text.clone()),
            None => folio_project::format::PartAction::Keep,
        }), &[], &parts).unwrap();
}

/// mcp-c5: import (INDD / IDML of the project into it), Load Styles from a project file, Package into a new project folder, and
/// open / make a book or library of the project, over an attached connection: each allowed, each escape refused, originals kept.
#[test]
fn attached_import_styles_package_book_and_library_stay_in_the_project() {
    let data = scratch();
    settings(&data, true);
    let (path, _page, _layer) = document(&data);
    let project = path.parent().unwrap().to_path_buf();
    let fixtures = workspace().join("next/oracle/fixtures");
    // an IDML of the project with its picture beside it, one whose picture is outside the project, and a styles source
    std::fs::create_dir_all(project.join("incoming/Links")).unwrap();
    std::fs::copy(fixtures.join("basic.idml"), project.join("incoming/story.idml")).unwrap();
    std::fs::copy(fixtures.join("Links/checker.png"), project.join("incoming/Links/checker.png")).unwrap();
    std::fs::copy(fixtures.join("grep-styles.idml"), project.join("styles.idml")).unwrap();
    let outside = data.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::copy(fixtures.join("Links/cmyk-patches.tif"), outside.join("secret.tif")).unwrap();
    std::fs::copy(fixtures.join("basic.idml"), outside.join("other.idml")).unwrap();
    let secret = sha(&outside.join("secret.tif"));
    let slashed = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let secret_uri = format!("file:///{}", slashed(&outside.join("secret.tif")).trim_start_matches('/'));
    idml_linking(&fixtures.join("basic.idml"), &project.join("incoming/outside-link.idml"), &secret_uri);
    let originals: Vec<(PathBuf, String)> = ["incoming/story.idml", "incoming/outside-link.idml", "styles.idml", "source.idml", "doc.folio"].iter()
        .map(|p| (project.join(p), sha(&project.join(p)))).collect();
    let mut app = App::start(&data);
    assert_eq!(app.request(json!({"cmd":"projectInfo","path":project}))["ok"], true);
    assert_eq!(app.request(json!({"cmd":"open","path":path,"edit":true}))["ok"], true);
    assert!(wait_for(|| connection_file(&data).exists()));
    let mut client = Mcp::start(&data);
    let refused = |client: &mut Mcp, tool: &str, arguments: Value| {
        let result = client.tool(tool, arguments.clone());
        assert_eq!(result["isError"], true, "{tool} {arguments}: {result}");
        result["structuredContent"]["error"].as_str().unwrap_or("").to_owned()
    };
    let native = project.to_string_lossy().into_owned();
    let names_machine = |reply: &Value| { let text = reply.to_string(); text.contains(&native.replace('\\', "\\\\")) || text.contains(&slashed(&project)) };

    // import: the source and the new document in the project only
    for arguments in [json!({"source":"../outside/other.idml"}), json!({"source":format!("{}/other.idml", slashed(&outside))}),
        json!({"source":"@사진창고/other.idml"}), json!({"source":"doc.folio"}), json!({"source":"incoming/missing.idml"}),
        json!({"source":"incoming/story.idml","document":"../evil.folio"}), json!({"source":"incoming/story.idml","document":"doc.folio"}),
        json!({"source":"incoming/story.idml","document":".folio/x.folio"}), json!({"source":"incoming/story.idml","document":"x.idml"})] {
        refused(&mut client, "importRun", arguments);
    }
    assert_eq!(client.rpc("tools/call", json!({"name":"importRun","arguments":{"source":"incoming/story.idml","project":native}}))["error"]["code"], -32602);
    let plan = client.ok("importPlan", json!({"source":"incoming/story.idml"}));
    assert_eq!(plan["plan"]["document"], "story.folio", "{plan}");
    let imported = client.ok("importRun", json!({"source":"incoming/story.idml"}));
    assert!(project.join("story.folio").is_file(), "{imported}");
    assert!(!names_machine(&imported), "the reply names project paths only: {imported}");
    refused(&mut client, "importRun", json!({"source":"incoming/story.idml"}));
    // a picture outside the project is not copied in: the link stays missing (its address is a hint)
    let away = client.ok("importRun", json!({"source":"incoming/outside-link.idml","document":"away.folio"}));
    assert!(project.join("away.folio").is_file(), "{away}");
    assert!(files_below(&project).iter().all(|f| sha(f) != secret), "no copy of the outside picture in the project: {away}");

    // Load Styles from a project file: the styles the dialog shows, then loaded into the open document
    let styles = client.ok("stylesFile", json!({"path":"styles.idml","kinds":["paragraph"]}));
    let ids: Vec<Value> = styles["styles"].as_array().unwrap().iter().map(|s| s["id"].clone()).collect();
    assert!(ids.contains(&json!("ParagraphStyle/Grep 0")), "{styles}");
    for escape in ["../outside/other.idml", "incoming/Links/checker.png", ".folio/cache/x.idml", "missing.idml"] {
        refused(&mut client, "edit.loadStyles", json!({"path":escape,"kinds":["paragraph"],"clash":"rename","only":ids}));
        refused(&mut client, "stylesFile", json!({"path":escape}));
    }
    assert_eq!(client.rpc("tools/call", json!({"name":"edit.loadStyles","arguments":{"path":"styles.idml","kinds":[],"clash":"skip","only":[],"parts":{}}}))["error"]["code"], -32602);
    let loaded = client.ok("edit.loadStyles", json!({"path":"styles.idml","kinds":["paragraph"],"clash":"rename","only":ids}));
    assert_eq!(loaded["label"], "LLM: loadStyles", "{loaded}");
    assert_eq!(app.request(json!({"cmd":"editState"}))["state"]["undo"], "LLM: loadStyles");
    assert!(app.event("external-edit", |e| e["detail"]["edit"]["label"] == "LLM: loadStyles").is_some());
    // the styles are in the open document now: the same file shows them as clashing with it
    let grep0 = |styles: &Value| styles["styles"].as_array().unwrap().iter().find(|s| s["id"] == "ParagraphStyle/Grep 0").unwrap()["conflict"].clone();
    assert_eq!(grep0(&styles), false, "{styles}");
    let again = client.ok("stylesFile", json!({"path":"styles.idml","kinds":["paragraph"]}));
    assert_eq!(grep0(&again), true, "the loaded style is in the open document: {again}");

    // Package into a new folder of the project
    std::fs::create_dir_all(project.join("out")).unwrap();
    for dir in ["../pkg", "out", "missing/pkg", ".folio/pkg", "@사진창고/pkg", "C:/pkg"] {
        refused(&mut client, "package", json!({"dir":dir,"pdf":false}));
    }
    client.ok("packagePlan", json!({"dir":"out/pkg"}));
    let packaged = client.ok("package", json!({"dir":"out/pkg","pdf":false,"fonts":false}));
    assert_eq!(packaged["result"]["dir"], "out/pkg", "{packaged}");
    assert!(!names_machine(&packaged), "{packaged}");
    assert!(!files_below(&project.join("out/pkg")).is_empty());
    refused(&mut client, "package", json!({"dir":"out/pkg","pdf":false}));

    // a book and a library of the project: make, open; nothing else
    let book = client.ok("book", json!({"action":"new","folder":"","fileName":"issue"}));
    assert!(project.join("issue.foliobook").is_file(), "{book}");
    assert_eq!(client.ok("book", json!({"action":"read","path":"issue.foliobook"}))["path"], "issue.foliobook");
    client.ok("library", json!({"action":"create","path":"out/art.foliolibrary","name":"Art"}));
    assert_eq!(client.ok("library", json!({"action":"read","path":"out/art.foliolibrary"}))["library"]["name"], "Art");
    for (tool, arguments) in [("book", json!({"action":"new","fileName":"issue"})), ("book", json!({"action":"new","folder":"../outside","fileName":"b"})),
        ("book", json!({"action":"new","fileName":"../b"})), ("book", json!({"action":"read","path":"../outside/b.foliobook"})),
        ("book", json!({"action":"add","path":"issue.foliobook"})), ("library", json!({"action":"create","path":"out/art.foliolibrary","name":"x"})),
        ("library", json!({"action":"read","path":"doc.folio"})), ("library", json!({"action":"store","path":"out/art.foliolibrary"}))] {
        refused(&mut client, tool, arguments);
    }

    // the setting off: import, package and making a book or library are refused; plans, reads and styles still work
    assert_eq!(app.request(json!({"cmd":"userSettings","values":{attached::ALLOW_FILE_WRITES: false}}))["ok"], true);
    for (tool, arguments) in [("importRun", json!({"source":"incoming/story.idml","document":"two.folio"})), ("package", json!({"dir":"out/two","pdf":false})),
        ("book", json!({"action":"new","fileName":"two"})), ("library", json!({"action":"create","path":"two.foliolibrary","name":"T"}))] {
        assert!(refused(&mut client, tool, arguments).contains("LLM may write files"), "{tool}");
    }
    assert!(!project.join("two.folio").exists() && !project.join("out/two").exists() && !project.join("two.foliobook").exists());
    client.ok("stylesFile", json!({"path":"styles.idml"}));
    client.ok("book", json!({"action":"read","path":"issue.foliobook"}));
    for (file, hash) in &originals { assert_eq!(&sha(file), hash, "{} is never written", file.display()); }
    assert!(outside.join("secret.tif").exists() && outside.join("other.idml").exists());
}

/// mcp-c6: over an attached connection, a new document (the New Document dialog) in the open project, a book's documents
/// added, synchronized and exported to one PDF, a library item stored and placed, a new project only in the folder a person chose
/// (none chosen: refused with how to choose it), and no `folio.mcp.*` setting changed through a dialog. Each escape refused,
/// originals kept.
#[test]
fn attached_new_files_books_and_libraries_stay_in_the_project() {
    let data = scratch();
    settings(&data, true);
    let (path, page, layer) = document(&data);
    let project = path.parent().unwrap().to_path_buf();
    let outside = data.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::copy(&path, outside.join("other.folio")).unwrap();
    let originals: Vec<(PathBuf, String)> = [project.join("source.idml"), outside.join("other.folio")].into_iter().map(|p| { let h = sha(&p); (p, h) }).collect();
    let mut app = App::start(&data);
    assert_eq!(app.request(json!({"cmd":"projectInfo","path":project}))["ok"], true);
    assert_eq!(app.request(json!({"cmd":"open","path":path,"edit":true}))["ok"], true);
    assert!(wait_for(|| connection_file(&data).exists()));
    let mut client = Mcp::start(&data);
    let refused = |client: &mut Mcp, tool: &str, arguments: Value| {
        let result = client.tool(tool, arguments.clone());
        assert_eq!(result["isError"], true, "{tool} {arguments}: {result}");
        result["structuredContent"].clone()
    };
    let error = |reply: &Value| reply["error"].as_str().unwrap_or("").to_owned();
    let native = project.to_string_lossy().into_owned();
    let names_machine = |reply: &Value| reply.to_string().contains(&native.replace('\\', "\\\\"));

    // New Document while the open one has unsaved changes: refused (the screen opens what the dialog makes)
    client.ok("edit.createByDrag", json!({"page":page,"kind":"rectangle","rect":[20,20,80,60],"layer":layer,"sides":4,"inset":0}));
    client.ok("openDialog", json!({"dialog":"newDocument"}));
    assert!(error(&refused(&mut client, "applyDialog", json!({"action":"ok"}))).contains("unsaved changes"));
    client.ok("cancelDialog", json!({}));
    client.ok("save", json!({}));
    // the project is the open one, filled by Folio: the client may not name it
    assert!(error(&refused(&mut client, "openDialog", json!({"dialog":"newDocument","values":{"project":outside}}))).contains("filled by Folio"));
    let opened = client.ok("openDialog", json!({"dialog":"newDocument"}));
    assert_eq!(opened["dialogState"]["values"]["project"], ".", "{opened}");
    assert!(error(&refused(&mut client, "setDialogField", json!({"field":"project","value":outside}))).contains("filled by Folio"));
    client.ok("setDialogField", json!({"field":"pages","value":2}));
    let new_doc = client.ok("applyDialog", json!({"action":"ok"}));
    let made_doc = new_doc["result"]["result"]["path"].as_str().unwrap_or_else(|| panic!("the new document's path: {new_doc}")).to_owned();
    assert!(made_doc.ends_with(".folio") && !made_doc.contains('/') && project.join(&made_doc).is_file(), "{new_doc}");
    assert!(!names_machine(&new_doc), "{new_doc}");

    // a library of the project: an item stored from the open document (the new one, which links no picture), then placed in it
    // (the screen's Place, one undo step)
    client.ok("open", json!({"path":made_doc}));
    let info = client.ok("documentInfo", json!({}))["info"].clone();
    let page = info["pages"][0]["id"].as_str().unwrap().to_owned();
    let layer = info["layers"].as_array().unwrap().iter().find(|l| l["locked"] != true).unwrap()["id"].as_str().unwrap().to_owned();
    client.ok("library", json!({"action":"create","path":"art.foliolibrary","name":"Art"}));
    let made = client.ok("edit.createByDrag", json!({"page":page,"kind":"rectangle","rect":[20,20,80,60],"layer":layer,"sides":4,"inset":0}));
    let id = made["created"][0].as_str().unwrap().to_owned();
    let stored = client.ok("library", json!({"action":"store","path":"art.foliolibrary","ids":[id],"name":"Box"}));
    let asset = stored["created"].as_str().expect("the stored item's id").to_owned();
    assert_eq!(stored["library"]["assets"][0]["name"], "Box", "{stored}");
    assert!(app.event("external-file", |e| e["detail"]["kind"] == "library").is_some());
    for (arguments, why) in [(json!({"action":"store","path":"../outside/a.foliolibrary","ids":[id]}), "outside"),
        (json!({"action":"place","path":"art.foliolibrary","asset":asset,"page":page,"libraryRoot":"C:/"}), "libraryRoot"),
        (json!({"action":"remove","path":"art.foliolibrary","asset":asset}), "remove")] {
        let reply = client.rpc("tools/call", json!({"name":"library","arguments":arguments}));
        assert!(reply.get("error").is_some() || reply["result"]["isError"] == true, "{why}: {reply}");
    }
    let placed = client.ok("library", json!({"action":"place","path":"art.foliolibrary","asset":asset,"page":page}));
    assert_eq!(placed["label"], "LLM: placeLibraryAsset", "{placed}");
    let copy = placed["created"][0].as_str().expect("the placed copy's id").to_owned();
    assert_ne!(copy, id);
    assert_eq!(app.request(json!({"cmd":"editState"}))["state"]["undo"], "LLM: placeLibraryAsset");
    assert!(app.event("external-edit", |e| e["detail"]["edit"]["label"] == "LLM: placeLibraryAsset").is_some());
    assert!(!names_machine(&placed), "{placed}");

    client.ok("save", json!({}));

    // a book of the project: its documents added, synchronized, exported to one PDF. The new document is the target of the
    // Synchronize, so it is closed first: the screen shows doc.folio again (a document someone has open is not written under them)
    client.ok("open", json!({"path":"doc.folio"}));
    client.ok("book", json!({"action":"new","folder":"","fileName":"issue"}));
    for escape in ["../outside/other.folio", "missing.folio", "art.foliolibrary", ".folio/x.folio"] {
        refused(&mut client, "book", json!({"action":"add","path":"issue.foliobook","document":escape}));
    }
    // (doc.folio first: it is the style source and has a spot colour, which Synchronize brings, with its Ink, into the new document
    // of File ▸ New. That stopped with "Graphic.xml has no Ink" before bookink-c1)
    client.ok("book", json!({"action":"add","path":"issue.foliobook","document":"doc.folio"}));
    let added = client.ok("book", json!({"action":"add","path":"issue.foliobook","document":made_doc}));
    assert_eq!(added["book"]["documents"].as_array().map(Vec::len), Some(2), "{added}");
    let synced = client.ok("book", json!({"action":"synchronize","path":"issue.foliobook"}));
    assert_eq!(synced["sync"]["documents"].as_array().map(Vec::len), Some(2), "{synced}");
    std::fs::create_dir_all(project.join("out")).unwrap();
    for output in ["../outside/issue.pdf", "out/issue.idml", "doc.folio", "missing/issue.pdf", "@사진창고/issue.pdf"] {
        refused(&mut client, "book", json!({"action":"exportPdf","path":"issue.foliobook","output":output}));
    }
    assert_eq!(client.rpc("tools/call", json!({"name":"book","arguments":{"action":"exportPdf","path":"issue.foliobook","output":"out/x.pdf","settings":{}}}))["error"]["code"], -32602);
    if has_pdf_profiles() {
        let exported = client.ok("book", json!({"action":"exportPdf","path":"issue.foliobook","output":"out/issue.pdf"}));
        assert!(std::fs::read(project.join("out/issue.pdf")).unwrap().starts_with(b"%PDF"), "{exported}");
        assert!(exported["exported"]["pages"].as_u64().is_some_and(|n| n >= 3), "doc.folio's pages and the new document's two: {exported}");
        assert!(app.event("external-file", |e| e["detail"]["kind"] == "book" && e["detail"]["file"].as_str().is_some_and(|f| f.ends_with("issue.pdf"))).is_some());
        refused(&mut client, "book", json!({"action":"exportPdf","path":"issue.foliobook","output":"out/issue.pdf"}));
    }
    for action in ["remove", "package", "move"] {
        assert!(client.rpc("tools/call", json!({"name":"book","arguments":{"action":action,"path":"issue.foliobook"}})).to_string().contains("not available"));
    }

    // New Project: none until a person chooses the folder for it (the refusal says where), then only there
    let none = refused(&mut client, "openDialog", json!({"dialog":"newProject","values":{"name":"Mag"}}));
    assert_eq!((none["reason"].clone(), none["setting"].clone()), (json!("noNewProjectFolder"), json!(attached::NEW_PROJECT_FOLDER)), "{none}");
    assert!(none["howTo"].as_str().unwrap().contains("Preferences"));
    let projects = data.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    // the person chooses it (the screen's Preferences send this); the client may not
    assert!(error(&refused(&mut client, "openDialog", json!({"dialog":"preferences","values":{attached::NEW_PROJECT_FOLDER: projects}}))).contains("set by a person"));
    assert_eq!(app.request(json!({"cmd":"userSettings","values":{attached::NEW_PROJECT_FOLDER: projects.to_string_lossy()}}))["ok"], true);
    assert!(error(&refused(&mut client, "openDialog", json!({"dialog":"newProject","values":{"name":"Mag","parent":outside}}))).contains("filled by Folio"));
    let dialog = client.ok("openDialog", json!({"dialog":"newProject","values":{"name":"Mag"}}));
    assert_eq!(dialog["dialogState"]["values"]["parent"], ".", "{dialog}");
    assert!(error(&refused(&mut client, "setDialogField", json!({"field":"parent","value":outside}))).contains("filled by Folio"));
    let made_project = client.ok("applyDialog", json!({"action":"ok"}));
    assert!(projects.join("Mag/folio.project").is_file(), "{made_project}");
    assert_eq!(made_project["result"]["project"]["root"], "Mag", "{made_project}");
    // a New Project dialog the person filled with another place: the client may not apply it
    assert_eq!(app.request(json!({"cmd":"openDialog","dialog":"newProject","values":{"name":"Elsewhere","parent":outside}}))["ok"], true);
    assert!(error(&refused(&mut client, "applyDialog", json!({"action":"ok"}))).contains("folder a person chose"));
    assert_eq!(app.request(json!({"cmd":"cancelDialog","scope":"app"}))["ok"], true);
    assert!(!outside.join("Elsewhere").exists());

    // Preferences: what an LLM may do is never changed over MCP, also when the person changed it in the open dialog
    assert!(error(&refused(&mut client, "setDialogField", json!({"field":attached::ALLOW_FILE_WRITES,"value":true}))).contains("set by a person"));
    assert_eq!(app.request(json!({"cmd":"openDialog","dialog":"preferences"}))["ok"], true);
    assert_eq!(app.request(json!({"cmd":"setDialogField","field":attached::ALLOW_CONNECTION,"value":false}))["ok"], true);
    assert!(error(&refused(&mut client, "applyDialog", json!({"action":"ok"}))).contains("set by a person"));
    assert_eq!(app.request(json!({"cmd":"cancelDialog"}))["ok"], true);

    // the file setting off: none of these is made over MCP any more
    assert_eq!(app.request(json!({"cmd":"userSettings","values":{attached::ALLOW_FILE_WRITES: false}}))["ok"], true);
    for (tool, arguments) in [("book", json!({"action":"add","path":"issue.foliobook","document":"doc.folio"})),
        ("book", json!({"action":"synchronize","path":"issue.foliobook"})), ("book", json!({"action":"exportPdf","path":"issue.foliobook","output":"out/two.pdf"})),
        ("library", json!({"action":"store","path":"art.foliolibrary","ids":[copy]})), ("library", json!({"action":"place","path":"art.foliolibrary","asset":asset,"page":page}))] {
        assert!(error(&refused(&mut client, tool, arguments)).contains("LLM may write files"), "{tool}");
    }
    client.ok("openDialog", json!({"dialog":"newDocument"}));
    assert!(error(&refused(&mut client, "applyDialog", json!({"action":"ok"}))).contains("LLM may write files"));
    client.ok("cancelDialog", json!({}));
    assert!(!project.join("out/two.pdf").exists());
    for (file, hash) in &originals { assert_eq!(&sha(file), hash, "{} is never written", file.display()); }
}
