#[path = "../src/bin/mcp/mod.rs"]
mod mcp;

use mcp::{boundary::Boundary, document::Engine, Server};
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Read, Write}, path::{Path, PathBuf}, process::{Command, Stdio}};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).unwrap().to_path_buf()
}

fn project() -> PathBuf {
    let dir = workspace().join("artifacts/mcp-c2-20261023/tests").join(folio_project::util::uuid_v4());
    folio_project::manifest::create(&dir, "MCP 시험").unwrap();
    dir
}

fn fixture(dir: &Path) {
    std::fs::copy(workspace().join("next/oracle/fixtures/basic.idml"), dir.join("source.idml")).unwrap();
}

fn replace_part(source: &Path, target: &Path, part: &str, text: &str) {
    let input = std::fs::File::open(source).unwrap();
    let mut archive = zip::ZipArchive::new(input).unwrap();
    let output = std::fs::File::create(target).unwrap();
    let extra: Vec<_> = if part.starts_with("folio/") { vec![(part, text.as_bytes())] } else { vec![] };
    folio_project::format::write_package_with_parts(output, &mut archive,
        &mut |name, _| Ok(if name == part { folio_project::format::PartAction::Replace(text.into()) }
            else { folio_project::format::PartAction::Keep }), &extra, &[(part, text)]).unwrap();
}

fn request(server: &mut Server, method: &str, params: Value) -> Value {
    server.receive(json!({"jsonrpc":"2.0","id":"시험","method":method,"params":params}).to_string().as_bytes()).unwrap()
}

fn initialize(server: &mut Server) {
    let response = request(server, "initialize", json!({"protocolVersion":"2099-01-01","capabilities":{},"clientInfo":{"name":"tests","version":"1"}}));
    assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(response["id"], "시험");
    assert!(server.receive(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
}

#[test]
fn protocol_lifecycle_errors_and_notifications() {
    let mut server = Server::new(Engine::new(&project()).unwrap());
    assert_eq!(server.receive(b"{").unwrap()["error"]["code"], -32700);
    for invalid in ["[]", "null", r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#, r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#] {
        assert_eq!(server.receive(invalid.as_bytes()).unwrap()["error"]["code"], -32600);
    }
    assert_eq!(request(&mut server, "tools/list", json!({}))["error"]["code"], -32002);
    assert_eq!(request(&mut server, "initialize", json!({}))["error"]["code"], -32602);
    initialize(&mut server);
    assert_eq!(request(&mut server, "initialize", json!({}))["error"]["code"], -32600);
    assert_eq!(request(&mut server, "unknown", json!({}))["error"]["code"], -32601);
    assert_eq!(request(&mut server, "ping", json!([]))["error"]["code"], -32602);
    assert_eq!(request(&mut server, "ping", json!({}))["result"], json!({}));
    assert!(server.receive(br#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":4}}"#).is_none());
    assert_eq!(request(&mut server, "tools/list", json!({"cursor":"x"}))["error"]["code"], -32602);
}

#[test]
fn registry_tools_are_reviewed_and_arguments_checked() {
    let mut server = Server::new(Engine::new(&project()).unwrap());
    initialize(&mut server);
    let listing = request(&mut server, "tools/list", json!({}));
    let tools = listing["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 9);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(mcp::catalog::ALLOWED.contains(&name) || mcp::catalog::NATIVE.contains(&name));
        assert!(tool["description"].is_string());
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    }
    for (name, args) in [("delete",json!({})), ("print",json!({})), ("open",json!({})),
        ("open",json!({"path":1})), ("open",json!({"path":"a.idml","extra":true})),
        ("edit.createByDrag",json!({"page":"p","kind":"textFrame","rect":[1,2,3,4],"layer":"l","sides":3,"inset":0}))] {
        assert_eq!(request(&mut server, "tools/call", json!({"name":name,"arguments":args}))["error"]["code"], -32602);
    }
    let result = request(&mut server, "tools/call", json!({"name":"open","arguments":{"path":"missing.idml"}}));
    assert_eq!(result["result"]["isError"], true);
    let structured = &result["result"]["structuredContent"];
    assert_eq!(structured["ok"], false);
    assert_eq!(serde_json::from_str::<Value>(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap(), *structured);
}

#[test]
fn paths_and_original_overwrite_are_refused() {
    let root = project();
    fixture(&root);
    let boundary = Boundary::new(&root).unwrap();
    for path in ["../secret.idml", "a/../../secret.idml", "/secret.idml", "C:/secret.idml", "C:secret.idml",
        "//server/share/file.idml", r"\\server\share\file.idml", r"a\..\secret.idml", "a/./b", "source.idml:stream"] {
        assert!(boundary.path(path).is_err(), "accepted {path}");
    }
    assert!(boundary.output("source.idml", "idml").is_err());
    assert!(boundary.output("original.indd", "folio").is_err());
    assert!(boundary.output("source.idml", "folio").is_err());
    let mut engine = Engine::new(&root).unwrap();
    engine.call("open", &json!({"path":"source.idml"})).unwrap();
    assert!(engine.call("saveCopy", &json!({"path":"../outside.folio"})).is_err());
    assert!(engine.call("saveCopy", &json!({"path":"source.idml"})).is_err());
    assert!(engine.call("undo", &json!({})).is_err());
    std::fs::write(root.join("source.idml"), b"changed").unwrap();
    assert!(engine.call("documentInfo", &json!({})).unwrap_err().contains("source changed"));
}

#[test]
fn project_settings_cannot_escape() {
    let root = project();
    let path = root.join("folio.project");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest["settings"]["assetsFolder"] = json!("../outside");
    std::fs::write(path, manifest.to_string()).unwrap();
    assert!(Engine::new(&root).is_err());
}

#[test]
fn links_cannot_escape_or_hide_in_implicit_cache_paths() {
    let root = project();
    let outside = project();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    #[cfg(windows)]
    {
        let status = Command::new("cmd").args(["/c", "mklink", "/J"])
            .arg(root.join("escape").to_string_lossy().replace('/', "\\"))
            .arg(outside.to_string_lossy().replace('/', "\\")).output().unwrap();
        assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    }
    // (mcp-c6) no walk over the whole tree: a link elsewhere does not refuse the project, a path through it is refused
    let boundary = Boundary::new(&root).unwrap();
    assert!(boundary.path("escape/folio.project").unwrap_err().contains("link"));
    let mut engine = Engine::new(&root).unwrap();
    assert!(engine.call("open", &json!({"path":"escape/source.idml"})).is_err());
    let clean = project();
    let boundary = Boundary::new(&clean).unwrap();
    let mut engine = Engine::new(&clean).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, clean.join(".folio")).unwrap();
    #[cfg(windows)]
    {
        let status = Command::new("cmd").args(["/c", "mklink", "/J"])
            .arg(clean.join(".folio").to_string_lossy().replace('/', "\\"))
            .arg(outside.to_string_lossy().replace('/', "\\")).output().unwrap();
        assert!(status.status.success());
    }
    // Folio's own folders are checked before every call: the hash index and caches cannot be written through a link
    assert!(boundary.own_folders().is_err());
    assert!(boundary.path(".folio/cache/hash.json").is_err());
    assert!(Engine::new(&clean).is_err());
    assert!(engine.call("open", &json!({"path":"folio.project"})).unwrap_err().contains(".folio"));
    // the project's assets folder too (a .folio saved from an IDML writes its previews there)
    let assets = project();
    fixture(&assets);
    let mut engine = Engine::new(&assets).unwrap();
    engine.call("open", &json!({"path":"source.idml"})).unwrap();
    std::fs::create_dir_all(assets.join("assets")).unwrap();
    assert!(link_to(&assets.join("assets/copy"), &outside));
    assert!(engine.call("saveCopy", &json!({"path":"copy.folio"})).unwrap_err().contains("link"));
    assert!(!assets.join("copy.folio").exists());
    unlink(&assets.join("assets/copy"));
    engine.call("saveCopy", &json!({"path":"copy.folio"})).unwrap();
    for link in [root.join("escape"), clean.join(".folio")] { unlink(&link); }
}

#[test]
fn bounded_stdio_and_notification_framing() {
    let engine = Engine::new(&project()).unwrap();
    let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\"}\n{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    mcp::serve(&input[..], &mut output, engine).unwrap();
    let lines = String::from_utf8(output).unwrap();
    assert_eq!(lines.lines().count(), 1);
    assert_eq!(serde_json::from_str::<Value>(&lines).unwrap()["id"], 42);
    let mut output = Vec::new();
    mcp::serve(&vec![b' '; 1024 * 1024 + 1][..], &mut output, Engine::new(&project()).unwrap()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap()["error"]["code"], -32600);
}

#[test]
fn symbolic_links_and_removed_project_marker_are_refused() {
    let root = project();
    let outside = project();
    let mut engine = Engine::new(&root).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, root.join("escape")).unwrap();
    assert!(engine.call("open", &json!({"path":"escape/source.idml"})).is_err());
    let root = project();
    fixture(&root);
    let mut engine = Engine::new(&root).unwrap();
    std::fs::rename(root.join("folio.project"), root.join("removed.project")).unwrap();
    assert!(engine.call("open", &json!({"path":"source.idml"})).is_err());
}

#[test]
fn package_references_cannot_widen_the_boundary() {
    let root = project();
    fixture(&root);
    let mut engine = Engine::new(&root).unwrap();
    engine.call("open", &json!({"path":"source.idml"})).unwrap();
    engine.call("saveCopy", &json!({"path":"good.folio"})).unwrap();
    let source = root.join("good.folio");
    let (manifest, _) = folio_project::format::read_manifest(&source).unwrap();
    let mut manifest = serde_json::to_value(manifest).unwrap();
    for (i, path) in ["../outside.bin", "/outside.bin", "//host/share/file.bin"].iter().enumerate() {
        manifest["references"] = json!([{"id":"bad","kind":"link","path":path}]);
        let name = format!("bad-{i}.folio");
        replace_part(&source, &root.join(&name), folio_project::format::MANIFEST_PART, &manifest.to_string());
        assert!(engine.call("open", &json!({"path":name})).is_err());
    }
    // An original URI is inert provenance, never an instruction to resolve it.
    manifest["references"] = json!([{"id":"hint","kind":"link","hint":"file:///outside/secret.bin"}]);
    replace_part(&source, &root.join("hint.folio"), folio_project::format::MANIFEST_PART, &manifest.to_string());
    engine.call("open", &json!({"path":"hint.folio"})).unwrap();
    engine.call("exportIdml", &json!({"path":"hint.idml"})).unwrap();
}

struct Client {
    process: std::process::Child,
    input: std::process::ChildStdin,
    output: BufReader<std::process::ChildStdout>,
    errors: std::process::ChildStderr,
    userdata: PathBuf,
    seen: String,
}
impl Client {
    fn start(root: &Path) -> Self {
        // Only Folio's own state is isolated: LOCALAPPDATA / APPDATA stay (user fonts and colour profiles live there).
        let userdata = workspace().join("artifacts/mcp-c2-20261023/userdata").join(folio_project::util::uuid_v4());
        let mut process = Command::new(env!("CARGO_BIN_EXE_folio-mcp")).arg(root)
            .env("FOLIO_USER_DATA", userdata.join("app")).env("FOLIO_CACHE_DIR", userdata.join("cache"))
            .env("FOLIO_COLOR_CACHE", userdata.join("cache/color-cache"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let input = process.stdin.take().unwrap();
        let output = BufReader::new(process.stdout.take().unwrap());
        let errors = process.stderr.take().unwrap();
        Self { process, input, output, errors, userdata, seen: String::new() }
    }
    /// Closes the connection; returns everything the process wrote to stdout and stderr, and whether it exited cleanly.
    fn finish(mut self) -> (String, bool) {
        drop(self.input);
        let mut text = self.seen;
        self.output.read_to_string(&mut text).unwrap();
        self.errors.read_to_string(&mut text).unwrap();
        (text, self.process.wait().unwrap().success())
    }
    fn send(&mut self, value: Value) { writeln!(self.input, "{value}").unwrap(); self.input.flush().unwrap(); }
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        self.seen.push_str(&line);
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }
    /// A request whose answer is a JSON-RPC error: returns the error object.
    fn rpc_error(&mut self, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":2,"method":method,"params":params}));
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        self.seen.push_str(&line);
        let reply: Value = serde_json::from_str(&line).unwrap();
        reply["error"].clone()
    }
    fn tool(&mut self, name: &str, args: Value) -> Value {
        let result = self.rpc("tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(result["isError"], false, "{result}");
        result["structuredContent"].clone()
    }
}

#[test]
fn real_stdio_open_create_save_reopen_export() {
    let root = project();
    fixture(&root);
    let unknown = "<Unknown vendor=\"example\"><Value>preserve me</Value></Unknown>";
    replace_part(&root.join("source.idml"), &root.join("with-unknown.idml"), "Unknown/vendor.xml", unknown);
    std::fs::rename(root.join("with-unknown.idml"), root.join("source.idml")).unwrap();
    let original = std::fs::read(root.join("source.idml")).unwrap();
    let mut client = Client::start(&root);
    client.rpc("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"1"}}));
    client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert_eq!(client.rpc("tools/list", json!({}))["tools"].as_array().unwrap().len(), 9);
    // The default answer is the compact summary: it is all a client needs to name the page and the layer.
    let opened = client.tool("open", json!({"path":"source.idml","edit":true}));
    assert!(opened.get("document").is_none());
    let doc = &opened["summary"];
    let created = client.tool("edit.createByDrag", json!({"page":doc["spreads"][0]["pages"][0]["id"],
        "kind":"rectangle","rect":[10,10,50,70],"layer":doc["layers"][0]["id"],"sides":5,"inset":0}));
    let id = created["created"][0].as_str().unwrap();
    assert!(client.tool("undo", json!({}))["undo"].is_null());
    let undone = client.tool("documentInfo", json!({"detail":"full"}));
    assert!(!undone["info"]["spreads"].as_array().unwrap().iter()
        .flat_map(|s| s["items"].as_array().unwrap()).any(|item| item["id"] == id));
    assert_eq!(client.tool("redo", json!({}))["dirty"], true);
    client.tool("saveCopy", json!({"path":"편집.folio"}));
    let reopened = client.tool("open", json!({"path":"편집.folio","edit":true,"detail":"full"}));
    assert!(reopened["document"]["spreads"].as_array().unwrap().iter()
        .flat_map(|s| s["items"].as_array().unwrap()).any(|item| item["id"] == id));
    client.tool("saveCopy", json!({"path":"copy.folio"}));
    client.tool("open", json!({"path":"copy.folio"}));
    client.tool("exportIdml", json!({"path":"export.idml","contents":"none"}));
    let exported = client.tool("open", json!({"path":"export.idml","detail":"full"}));
    assert!(exported["document"]["spreads"].as_array().unwrap().iter()
        .flat_map(|s| s["items"].as_array().unwrap()).any(|item| item["id"] == id));
    assert_eq!(std::fs::read(root.join("source.idml")).unwrap(), original);
    assert_eq!(folio_project::format::read_part(&root.join("export.idml"), "Unknown/vendor.xml").unwrap().unwrap(), unknown.as_bytes());
    let (_, clean) = client.finish();
    assert!(clean);
}

const SENTINEL: &str = "S3cr3t-Pw-9f1c7a2e";

fn registry_names() -> Vec<String> { mcp::catalog::registry_names() }

#[test]
fn every_registry_name_is_reviewed() {
    // A new engine command or query must be classified in bin/mcp/coverage.rs (exposed or withheld with a reason).
    let names = registry_names();
    let unreviewed: Vec<_> = names.iter().filter(|n| mcp::coverage::status(n) == mcp::coverage::Status::Unreviewed).collect();
    assert!(unreviewed.is_empty(), "add a rule in bin/mcp/coverage.rs for: {unreviewed:?}");
    for name in mcp::catalog::ALLOWED {
        assert!(names.iter().any(|n| n == name), "{name} is exposed but not registered");
        assert_eq!(mcp::coverage::status(name), mcp::coverage::Status::Exposed);
    }
    let exposed: Vec<_> = names.iter().filter(|n| mcp::coverage::status(n) == mcp::coverage::Status::Exposed).collect();
    assert_eq!(exposed.len(), mcp::catalog::ALLOWED.len());
    for reason in mcp::coverage::REASONS { assert!(!reason.text.is_empty(), "{} needs an explanation", reason.id); }
}

#[test]
fn new_commands_and_queries_appear_in_the_coverage_tool() {
    let mut server = Server::new(Engine::new(&project()).unwrap());
    initialize(&mut server);
    let result = request(&mut server, "tools/call", json!({"name":"coverage","arguments":{}}));
    assert_eq!(result["result"]["isError"], false, "{result}");
    let report = &result["result"]["structuredContent"];
    let names = registry_names();
    assert_eq!(report["total"], names.len());
    assert_eq!(report["unreviewed"], json!([]));
    let mut listed: Vec<String> = report["exposed"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned()).collect();
    for group in report["withheld"].as_array().unwrap() {
        assert!(!group["why"].as_str().unwrap().is_empty());
        listed.extend(group["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned()));
    }
    let mut expected = names.clone();
    expected.sort();
    listed.sort();
    assert_eq!(listed, expected, "every registry name appears exactly once");
    // The commands added for database sources, data merge, the shared dialogs, panels and the recent list.
    let mut seen = 0;
    for name in expected.iter().filter(|n| n.starts_with("db") || n.starts_with("dataMerge") || n.starts_with("edit.dataMerge")
        || matches!(n.as_str(), "dialogState" | "openDialog" | "setDialogField" | "applyDialog" | "cancelDialog"
            | "panelState" | "setPanelState" | "recentFiles" | "setRecentFiles" | "rememberRecent")) {
        let one = request(&mut server, "tools/call", json!({"name":"coverage","arguments":{"name":name}}));
        let one = &one["result"]["structuredContent"];
        assert_eq!(one["name"], name.as_str());
        assert_eq!(one["status"], "withheld", "{name} is not exposed in the first pass");
        assert!(!one["reason"].as_str().unwrap().is_empty() && !one["why"].as_str().unwrap().is_empty());
        seen += 1;
    }
    assert!(seen >= 25, "only {seen} of the new names were found");
    let missing = request(&mut server, "tools/call", json!({"name":"coverage","arguments":{"name":"noSuchCommand"}}));
    assert_eq!(missing["result"]["isError"], true);
}

#[test]
fn password_carrying_commands_are_never_exposed() {
    let registry = mcp::catalog::registry();
    let mut carrying = Vec::new();
    for tool in registry["tools"].as_array().unwrap() {
        if mcp::catalog::find_secret(&tool["inputSchema"]["properties"]).is_some() {
            let name = tool["name"].as_str().unwrap();
            assert_ne!(mcp::coverage::status(name), mcp::coverage::Status::Exposed, "{name} takes a secret");
            carrying.push(name);
        }
    }
    for name in ["dbSaveConnection", "dbUnlockConnection", "dbTestConnection"] {
        assert!(carrying.contains(&name), "{name} should be found by the secret scan");
        assert_eq!(mcp::coverage::status(name), mcp::coverage::Status::Withheld("secret"));
    }
    for tool in mcp::catalog::tools() {
        assert!(mcp::catalog::find_secret(&tool["inputSchema"]).is_none(), "{}", tool["name"]);
        assert!(mcp::catalog::find_secret(&tool["outputSchema"]).is_none(), "{}", tool["name"]);
    }
}

#[test]
fn secrets_are_refused_for_every_tool_and_never_echoed() {
    let mut server = Server::new(Engine::new(&project()).unwrap());
    initialize(&mut server);
    let cases = [
        ("dbUnlockConnection", json!({"name":"main","password":SENTINEL})),
        ("dbSaveConnection", json!({"name":"main","driver":"postgres","database":"d","password":SENTINEL,"savePassword":true})),
        ("open", json!({"path":"a.idml","password":SENTINEL})),
        ("open", json!({"path":"a.idml","tls":{"nested":[{"Passphrase":SENTINEL}]}})),
        ("coverage", json!({"apiKey":SENTINEL})),
        ("noSuchTool", json!({"client_secret":SENTINEL})),
    ];
    for (name, arguments) in cases {
        let reply = request(&mut server, "tools/call", json!({"name":name,"arguments":arguments}));
        assert_eq!(reply["error"]["code"], -32602, "{name}: {reply}");
        assert!(reply["error"]["message"].as_str().unwrap().contains("Secrets are not accepted"));
        assert!(!reply.to_string().contains(SENTINEL), "{name} echoed the secret");
    }
    // The same call without a secret is an ordinary refusal that does not look at the password rule.
    let reply = request(&mut server, "tools/call", json!({"name":"dbUnlockConnection","arguments":{"name":"main"}}));
    assert_eq!(reply["error"]["message"], "Unknown or disabled tool");
}

#[test]
fn secrets_do_not_reach_output_logs_or_files() {
    let root = project();
    fixture(&root);
    let mut client = Client::start(&root);
    client.rpc("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"1"}}));
    client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    for (name, arguments) in [("dbUnlockConnection", json!({"name":"main","password":SENTINEL,"save":true})),
        ("open", json!({"path":"source.idml","edit":true,"password":SENTINEL}))] {
        let error = client.rpc_error("tools/call", json!({"name":name,"arguments":arguments}));
        assert_eq!(error["code"], -32602);
    }
    // The refused calls opened nothing, and a normal open still works afterwards.
    client.tool("open", json!({"path":"source.idml"}));
    let userdata = client.userdata.clone();
    let (text, clean) = client.finish();
    assert!(clean);
    assert!(!text.contains(SENTINEL), "the password appeared in the replies or the log");
    let mut pending = vec![root, userdata];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() { pending.push(path); continue; }
            let bytes = std::fs::read(&path).unwrap();
            assert!(!bytes.windows(SENTINEL.len()).any(|w| w == SENTINEL.as_bytes()), "{} holds the password", path.display());
        }
    }
}

#[test]
fn open_and_document_info_answer_with_a_small_summary_unless_full_is_asked() {
    let root = project();
    fixture(&root);
    let mut engine = Engine::new(&root).unwrap();
    let full = engine.call("open", &json!({"path":"source.idml","edit":true,"detail":"full"})).unwrap();
    let full_size = full.to_string().len();
    assert!(full_size > 100_000, "the full model is large ({full_size} bytes)");
    let opened = engine.call("open", &json!({"path":"source.idml","edit":true})).unwrap();
    assert_eq!(opened["type"], "opened");
    assert!(opened.get("document").is_none());
    assert!(opened.to_string().len() < 25_000, "summary is {} bytes", opened.to_string().len());
    let summary = &opened["summary"];
    let layer = summary["layers"][0]["id"].as_str().unwrap();
    let page = summary["spreads"][0]["pages"][0]["id"].as_str().unwrap();
    assert_eq!(page, full["document"]["spreads"][0]["pages"][0]["id"].as_str().unwrap());
    assert!(summary["spreads"][0]["pages"][0]["bounds"]["right"].is_number());
    assert_eq!(summary["spreads"][0]["itemCount"], full["document"]["spreads"][0]["items"].as_array().unwrap().len());
    let created = engine.call("edit.createByDrag", &json!({"page":page,"kind":"ellipse","rect":[5,5,40,40],"layer":layer,"sides":4,"inset":0})).unwrap();
    let id = created["created"][0].as_str().unwrap();
    let info = engine.call("documentInfo", &json!({})).unwrap();
    assert_eq!(info["type"], "documentInfo");
    let listed = info["summary"]["spreads"].as_array().unwrap().iter().flat_map(|s| s["items"].as_array().unwrap().clone()).collect::<Vec<_>>();
    let new_item = listed.iter().find(|item| item["id"] == id).expect("the created object is listed in the summary");
    assert_eq!(new_item["kind"], "Oval");
    assert!(engine.call("documentInfo", &json!({"detail":"full"})).unwrap()["info"]["spreads"].is_array());
    assert!(engine.call("documentInfo", &json!({"detail":"everything"})).is_err());
    // The schema the client sees lists the choice.
    let tools = mcp::catalog::tools();
    let open = tools.iter().find(|t| t["name"] == "open").unwrap();
    assert_eq!(open["inputSchema"]["properties"]["detail"]["enum"], json!(["summary","full"]));
}

/// A junction (Windows) or symbolic link (elsewhere) at `link` to `target`.
fn link_to(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    return Command::new("cmd").args(["/c", "mklink", "/J"]).arg(link.to_string_lossy().replace('/', "\\"))
        .arg(target.to_string_lossy().replace('/', "\\")).stdout(Stdio::null()).status().is_ok_and(|s| s.success());
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link).is_ok();
}

fn unlink(link: &Path) {
    #[cfg(windows)]
    let _ = std::fs::remove_dir(link);
    #[cfg(unix)]
    let _ = std::fs::remove_file(link);
}

/// The boundary of a connection attached to the running app (mcp-c5): only the names of the requested path, root to target.
#[test]
fn attached_boundary_checks_the_requested_names_only() {
    let root = project();
    fixture(&root);
    let outside = project();
    std::fs::write(outside.join("secret.idml"), b"secret").unwrap();
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::copy(root.join("source.idml"), root.join("sub/a.idml")).unwrap();
    assert!(link_to(&root.join("escape"), &outside));
    // only paths through the link are refused (the project server too since mcp-c6: `new` is `components` + Folio's own folders)
    assert!(Boundary::new(&root).is_ok());
    let boundary = Boundary::components(&root).unwrap();
    assert!(boundary.existing("source.idml").is_ok());
    assert!(boundary.existing("sub/a.idml").is_ok());
    assert!(boundary.path("escape/secret.idml").unwrap_err().contains("link"));
    assert!(boundary.path("escape").is_err());
    assert!(boundary.output("escape/new.idml", "idml").is_err());
    for path in ["../secret.idml", "a/../../secret.idml", "/secret.idml", "C:/secret.idml", "C:secret.idml",
        "//server/share/file.idml", r"\server\share\file.idml", r"a\..\secret.idml", "a/./b", "source.idml:stream"] {
        assert!(boundary.path(path).is_err(), "accepted {path}");
    }
    assert!(boundary.output("source.idml", "idml").is_err());
    // a folder swapped for a link between the check and the use: the same check after the use (attach_paths `recheck`) refuses it
    assert!(boundary.path("sub/a.idml").is_ok());
    std::fs::remove_dir_all(root.join("sub")).unwrap();
    std::fs::create_dir_all(outside.join("swapped")).unwrap();
    std::fs::write(outside.join("swapped/a.idml"), b"outside").unwrap();
    assert!(link_to(&root.join("sub"), &outside.join("swapped")));
    assert!(boundary.path("sub/a.idml").is_err());
    // the project folder itself a link: refused
    let linked_root = outside.join("project-link");
    assert!(link_to(&linked_root, &root));
    assert!(Boundary::components(&linked_root).is_err());
    for link in [root.join("escape"), root.join("sub"), linked_root] { unlink(&link); }
    assert!(outside.join("secret.idml").exists() && outside.join("swapped/a.idml").exists());
}

/// The walk over the whole tree both MCP modes made before mcp-c5 / mcp-c6 (`Boundary::audit`, removed): kept here so the
/// measurements below can be run again.
fn whole_tree_walk(root: &Path) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(path) = pending.pop() {
        count += 1;
        if count > 100_000 { return Err("project exceeds MCP file limit".into()); }
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() { return Err("project links and reparse points are not allowed".into()); }
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path).map_err(|e| e.to_string())? { pending.push(entry.map_err(|e| e.to_string())?.path()); }
        }
    }
    Ok(())
}

/// A measurement, not a check (mcp-c5): the time of one attached path check on a project of 50,000 files, the whole-tree walk
/// (before) and the requested names only (after). Run with
/// `cargo test --release --target-dir target -p folio-engine --test mcp -- --ignored path_check_speed --nocapture`.
#[test]
#[ignore]
fn path_check_speed_on_a_large_project() {
    let root = workspace().join("artifacts/mcp-c5-20261023/bench").join(folio_project::util::uuid_v4());
    folio_project::manifest::create(&root, "MCP 속도 시험").unwrap();
    fixture(&root);
    let made = std::time::Instant::now();
    for folder in 0..50 {
        let dir = root.join(format!("호/{folder:02}/사진"));
        std::fs::create_dir_all(&dir).unwrap();
        for file in 0..1000 { std::fs::write(dir.join(format!("{file:04}.txt")), b"x").unwrap(); }
    }
    let made = made.elapsed().as_secs_f64();
    let target = "호/49/사진/0999.txt";
    let time = |f: &dyn Fn() -> Result<PathBuf, String>| {
        let mut runs: Vec<f64> = (0..7).map(|_| { let t = std::time::Instant::now(); f().unwrap(); t.elapsed().as_secs_f64() * 1000.0 }).collect();
        runs.sort_by(f64::total_cmp);
        (runs[runs.len() / 2], runs)
    };
    let (before, before_runs) = time(&|| { whole_tree_walk(&root)?; Boundary::components(&root)?.existing(target) });
    let (after, after_runs) = time(&|| Boundary::components(&root)?.existing(target));
    let result = json!({"files": 50_000 + 2, "folders": 101, "makeSeconds": made, "target": target,
        "beforeMs": {"median": before, "runs": before_runs, "what": "walk over the whole tree + existing"},
        "afterMs": {"median": after, "runs": after_runs, "what": "Boundary::components (the requested names only) + existing"},
        "os": std::env::consts::OS});
    println!("{result:#}");
    let out = workspace().join("artifacts/mcp-c5-20261023/results");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("path-speed.json"), serde_json::to_string_pretty(&result).unwrap()).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(after < before);
}

/// A measurement, not a check (mcp-c6): the project server (`folio-mcp PROJECT`) on a project of 50,000 files, starting
/// (`Engine::new`) and one tool call after `open` (`editState`, which checks the project first). `FOLIO_SPEED_LABEL` names the
/// result file (`before`: the whole-tree walk, `after`: the requested names and Folio's own folders only). Run with
/// `cargo test --release --target-dir target -p folio-engine --test mcp -- --ignored project_server_speed --nocapture`.
#[test]
#[ignore]
fn project_server_speed_on_a_large_project() {
    let label = std::env::var("FOLIO_SPEED_LABEL").unwrap_or_else(|_| "now".into());
    let root = workspace().join("artifacts/mcp-c6-20261023/bench").join(folio_project::util::uuid_v4());
    folio_project::manifest::create(&root, "MCP 속도 시험").unwrap();
    fixture(&root);
    for folder in 0..50 {
        let dir = root.join(format!("호/{folder:02}/사진"));
        std::fs::create_dir_all(&dir).unwrap();
        for file in 0..1000 { std::fs::write(dir.join(format!("{file:04}.txt")), b"x").unwrap(); }
    }
    let median = |mut runs: Vec<f64>| { runs.sort_by(f64::total_cmp); (runs[runs.len() / 2], runs) };
    let (start, start_runs) = median((0..7).map(|_| { let t = std::time::Instant::now(); Engine::new(&root).unwrap(); t.elapsed().as_secs_f64() * 1000.0 }).collect());
    let mut engine = Engine::new(&root).unwrap();
    engine.call("open", &json!({"path":"source.idml"})).unwrap();
    let (call, call_runs) = median((0..7).map(|_| { let t = std::time::Instant::now(); engine.call("editState", &json!({})).unwrap(); t.elapsed().as_secs_f64() * 1000.0 }).collect());
    let result = json!({"label": label, "files": 50_000 + 2, "folders": 101, "os": std::env::consts::OS,
        "startMs": {"median": start, "runs": start_runs, "what": "Engine::new"},
        "callMs": {"median": call, "runs": call_runs, "what": "editState after open (the checks every tool call makes first)"}});
    println!("{result:#}");
    let out = workspace().join("artifacts/mcp-c6-20261023/results");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join(format!("project-server-speed-{label}.json")), serde_json::to_string_pretty(&result).unwrap()).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}
