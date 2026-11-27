pub use folio_engine::mcp::attached;
pub mod boundary;
pub use folio_engine::mcp::catalog;
pub mod client;
pub mod coverage;
pub mod document;
mod import;
pub mod summary;

use serde_json::{Value, json};
use std::io::{BufRead, Write};

const VERSION: &str = "2025-06-18";
const MAX_MESSAGE: usize = 1024 * 1024;

/// Where tool calls go: a project folder of this process, the running app (`--attach`), or nowhere (not attached, no project).
pub enum Backend {
    Project(document::Engine),
    App(client::Client),
    None,
}

pub struct Server {
    backend: Backend,
    /// `--attach` was asked for: the result (`appConnection`); `None` for a plain project server.
    attach: Option<Result<(), client::Failure>>,
    initialized: bool,
    ready: bool,
    tools: Vec<Value>,
}

impl Server {
    pub fn new(engine: document::Engine) -> Self {
        Self { backend: Backend::Project(engine), attach: None, initialized: false, ready: false, tools: catalog::tools() }
    }

    /// `folio-mcp --attach`: the running app when it answers, else the project folder (if given) or only the MCP-only tools.
    pub fn attached(connected: Result<client::Client, client::Failure>, fallback: Option<document::Engine>) -> Self {
        let (backend, attach, tools) = match (connected, fallback) {
            (Ok(app), _) => (Backend::App(app), Ok(()), catalog::attached_tools()),
            (Err(failure), Some(engine)) => {
                let mut tools = catalog::tools();
                tools.push(catalog::attached_tools().pop().expect("appConnection"));
                (Backend::Project(engine), Err(failure), tools)
            }
            (Err(failure), None) => {
                let mut tools = catalog::attached_tools();
                let native = tools.split_off(tools.len() - 2);
                (Backend::None, Err(failure), native)
            }
        };
        Self { backend, attach: Some(attach), initialized: false, ready: false, tools }
    }

    fn connection(&self) -> Value {
        match &self.attach {
            Some(Ok(())) => json!({"ok":true,"type":"appConnection","attached":true}),
            Some(Err(failure)) => json!({"ok":true,"type":"appConnection","attached":false,"code":failure.code,
                "reason":failure.message,"howTo":client::HOW_TO,
                "fallback": if matches!(self.backend, Backend::Project(_)) { "project folder (this process, not the screen)" } else { "none" }}),
            None => json!({"ok":true,"type":"appConnection","attached":false,"code":"notRequested","reason":"started without --attach","howTo":client::HOW_TO}),
        }
    }

    fn instructions(&self) -> String {
        match (&self.backend, &self.attach) {
            (Backend::App(_), _) => "Attached to the running Folio app: tools act on the document open on screen, its selection, current page, common dialogs, panels and recent list; the screen shows each change at once and Edit ▸ Undo names it \"LLM: …\". Ids come from documentInfo and pageItems. Paths (open, saveCopy, exportIdml, export, dataMergeCreate, edit.placeGraphic, importRun, package, stylesFile, edit.loadStyles, book, library …) are relative to the project open in the app, with '/' separators; nothing outside it, no existing file is overwritten, and saving, exporting, importing, packaging or placing needs the app setting LLM may write files. Passwords and other secrets are never accepted. Call coverage to see what is not exposed and why.".into(),
            (_, Some(Err(failure))) => format!("Not attached to the Folio app ({}: {}). {} {}", failure.code, failure.message, client::HOW_TO,
                if matches!(self.backend, Backend::Project(_)) { "Meanwhile the tools work on the project folder given, in this process." } else { "Until then only coverage and appConnection work." }),
            _ => "One project only. Paths are relative. Existing files are never overwritten. Save edits with saveCopy. Passwords and other secrets are never accepted: a person enters them in the Folio app. Call coverage to see which engine commands are not exposed and why.".into(),
        }
    }

    /// A tool's result, or its refusal (`{ok: false, code, error}` and, from the app, what the client can do about it: the
    /// `reason`, `setting` and `howTo` of attach_paths.rs, a dialog's `field`).
    fn call(&mut self, name: &str, arguments: &Value) -> Result<Value, Value> {
        let refusal = |message: String| json!({"ok":false,"code":"refused","error":message});
        match name {
            "coverage" => coverage::report(arguments).map_err(refusal),
            "appConnection" => Ok(self.connection()),
            _ => match &mut self.backend {
                Backend::Project(engine) => engine.call(name, arguments).map_err(refusal),
                Backend::App(app) => {
                    let request = if app.turn_scoped {json!({"tool":name,"arguments":arguments})}
                        else {catalog::request_for(name, arguments).map_err(refusal)?};
                    let reply = app.request(request).map_err(refusal)?;
                    if reply["ok"] != false { return Ok(reply); }
                    let mut failed = reply;
                    if let Some(object) = failed.as_object_mut() { object.remove("id"); object.remove("bodyBytes"); }
                    if !failed["code"].is_string() { failed["code"] = json!("refused"); }
                    if !failed["error"].is_string() { failed["error"] = json!("refused by the Folio app"); }
                    Err(failed)
                }
                Backend::None => Err(refusal(format!("not attached to the Folio app. {}", client::HOW_TO))),
            },
        }
    }

    pub fn receive(&mut self, bytes: &[u8]) -> Option<Value> {
        let request: Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => return Some(error(Value::Null, -32700, "Parse error")),
        };
        let id = request.get("id").cloned();
        let valid_id = id.as_ref().is_none_or(|id| id.is_string() || id.is_i64() || id.is_u64());
        if !request.is_object() || request["jsonrpc"] != "2.0" || !request["method"].is_string() || !valid_id {
            return Some(error(Value::Null, -32600, "Invalid Request"));
        }
        let method = request["method"].as_str().unwrap();
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        if id.is_none() {
            if method == "notifications/initialized" && params.is_object() && self.initialized { self.ready = true; }
            return None;
        }
        let id = id.unwrap();
        if !params.is_object() { return Some(error(id, -32602, "Object params required")); }
        let response = match method {
            "ping" => Ok(json!({})),
            "initialize" => {
                if self.initialized { Err((-32600, "Already initialized")) }
                else if !params["protocolVersion"].is_string() || !params["capabilities"].is_object()
                    || !params["clientInfo"]["name"].is_string() || !params["clientInfo"]["version"].is_string() {
                    Err((-32602, "Invalid initialize params"))
                } else {
                    self.initialized = true;
                    Ok(json!({"protocolVersion":VERSION,"capabilities":{"tools":{"listChanged":false}},
                        "serverInfo":{"name":"folio-mcp","version":env!("CARGO_PKG_VERSION")},
                        "instructions":self.instructions()}))
                }
            }
            "tools/list" | "tools/call" if !self.ready => Err((-32002, "Initialize and send notifications/initialized first")),
            "tools/list" => {
                if params.get("cursor").is_some() { Err((-32602, "Pagination is not supported")) }
                else { Ok(json!({"tools":self.tools})) }
            }
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                // Secrets never travel over MCP, for any tool (known or not). The reply names the key, never the value.
                if catalog::find_secret(&arguments).is_some() {
                    return Some(error(id, -32602, "Secrets are not accepted over MCP. Enter passwords in the Folio app."));
                }
                match self.tools.iter().find(|tool| tool["name"] == name).map(|tool| tool["inputSchema"].clone()) {
                    None => Err((-32602, "Unknown or disabled tool")),
                    Some(schema) => {
                        if let Err(message) = catalog::validate(&arguments, &schema) {
                            return Some(error(id, -32602, &message));
                        }
                        let called = self.call(name, &arguments);
                        let (mut result, failed) = match called {
                            Ok(value) => (value, false),
                            Err(refused) => (refused, true),
                        };
                        let content = if name == "render" && !failed && result["type"] == "pageImage" {
                            json!({"type":"image","data":result["png"].take(),"mimeType":"image/png"})
                        } else {
                            json!({"type":"text","text":result.to_string()})
                        };
                        if name == "render" && let Some(obj) = result.as_object_mut() { obj.remove("png"); }
                        Ok(json!({"content":[content],"structuredContent":result,"isError":failed}))
                    }
                }
            }
            _ => Err((-32601, "Method not found")),
        };
        Some(match response {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err((code, message)) => error(id, code, message),
        })
    }
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub fn serve(input: impl BufRead, output: impl Write, engine: document::Engine) -> std::io::Result<()> {
    serve_with(input, output, Server::new(engine))
}

pub fn serve_with(mut input: impl BufRead, mut output: impl Write, mut server: Server) -> std::io::Result<()> {
    loop {
        // Bounded read; oversized messages terminate the transport instead of allocating indefinitely.
        let mut bytes = Vec::new();
        let count = std::io::Read::take(&mut input, (MAX_MESSAGE + 1) as u64).read_until(b'\n', &mut bytes)?;
        if count == 0 { break; }
        let reply = if bytes.len() > MAX_MESSAGE { Some(error(Value::Null, -32600, "Message exceeds 1 MiB")) }
            else { server.receive(&bytes) };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
        if bytes.len() > MAX_MESSAGE { break; }
    }
    Ok(())
}
