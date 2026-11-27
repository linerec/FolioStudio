mod content;
mod process;
mod program;
use crate::{
    agent::{AttachedCall, Input, Provider, Step},
    connection::Connection,
};
use process::Process;
pub use program::executable;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

struct Permission {
    rpc: Value,
    card: String,
    allow: Option<String>,
    deny: Option<String>,
}
impl Permission {
    fn answer(&self, allow: Option<bool>) -> Value {
        let selected = allow.and_then(|allow| {
            if allow {
                self.allow.as_ref()
            } else {
                self.deny.as_ref()
            }
        });
        let outcome = selected.map_or_else(
            || json!({"outcome":"cancelled"}),
            |id| json!({"outcome":"selected","optionId":id}),
        );
        json!({"jsonrpc":"2.0","id":self.rpc,"result":{"outcome":outcome}})
    }
}

pub struct Acp {
    config: Connection,
    authorized: Option<String>,
    stream: content::StreamText,
    deferred: Option<Step>,
    process: Option<Process>,
    cwd: PathBuf,
    mcp: PathBuf,
    file: PathBuf,
    wake: Arc<dyn Fn() + Send + Sync>,
    calls: mpsc::Receiver<AttachedCall>,
    reply: Option<(String, mpsc::Sender<Value>)>,
    permission: Option<Permission>,
    previous: Option<String>,
    session: Option<String>,
    caps: Value,
    phase: u32,
    loading: bool,
}
impl Acp {
    pub fn new(
        config: Connection,
        cwd: PathBuf,
        mcp: PathBuf,
        file: PathBuf,
        previous: Option<String>,
        calls: mpsc::Receiver<AttachedCall>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            config,
            authorized: None,
            stream: content::StreamText::default(),
            deferred: None,
            process: None,
            cwd,
            mcp,
            file,
            previous,
            calls,
            wake,
            reply: None,
            permission: None,
            session: None,
            caps: Value::Null,
            phase: 0,
            loading: false,
        }
    }
    fn rpc(&self) -> &Process {
        self.process.as_ref().unwrap()
    }
    fn incoming(&mut self, msg: Value, input: &Input<'_>) -> Result<Option<Step>, String> {
        if msg["jsonrpc"] != "2.0" {
            return Err("invalid ACP envelope".into());
        }
        if let Some(method) = msg["method"].as_str() {
            let p = &msg["params"];
            let active = self.phase == 3 && self.session.as_deref() == p["sessionId"].as_str();
            if msg.get("id").is_none() {
                if active && method == "session/update" {
                    return Ok(content::update(&p["update"]));
                }
                return Ok(None);
            }
            if active && method == "session/request_permission" {
                let option = |kind: &str| {
                    p["options"]
                        .as_array()?
                        .iter()
                        .find(|v| v["kind"] == kind)?["optionId"]
                        .as_str()
                        .map(str::to_owned)
                };
                let perm = Permission {
                    rpc: msg["id"].clone(),
                    card: folio_project::util::uuid_v4(),
                    allow: option("allow_once"),
                    deny: option("reject_once"),
                };
                if perm.allow.is_none() {
                    self.rpc().send(perm.answer(Some(false)))?;
                    return Ok(None);
                }
                let mut card = json!({"kind":"permission","permissionId":perm.card,"risk":"external","outside":true,
                    "tool":p["toolCall"]["title"].as_str().unwrap_or("ACP"),"args":p["toolCall"]["rawInput"],"allowedDecisions":["deny","allow"]});
                if option("allow_always").is_some() {
                    card["requestedSessionTool"] = p["toolCall"]["name"].clone();
                }
                self.permission = Some(perm);
                return Ok(Some(Step::ExternalPermission { card }));
            }
            self.rpc().send(json!({"jsonrpc":"2.0","id":msg["id"],"error":{"code":-32601,"message":"Client method unavailable"}}))?;
            return Ok(None);
        }
        if msg["id"] != self.phase {
            return Err("unexpected ACP reply".into());
        }
        if msg.get("error").is_some() {
            return Err("ACP request failed".into());
        }
        let result = &msg["result"];
        match self.phase {
            1 => {
                if result["protocolVersion"] != 1 {
                    return Err("unsupported ACP protocol version".into());
                }
                self.caps = result["agentCapabilities"].clone();
                let environment = ["FOLIO_USER_DATA", "FOLIO_CACHE_DIR", "FOLIO_COLOR_CACHE"]
                    .iter()
                    .filter_map(|name| {
                        std::env::var(name)
                            .ok()
                            .map(|value| json!({"name":name,"value":value}))
                    })
                    .collect::<Vec<_>>();
                let mut params = json!({"cwd":self.cwd,"mcpServers":[{"name":"folio","command":self.mcp,"args":["--attach","--connection",self.file],"env":environment}]});
                if self.caps["sessionCapabilities"]["systemPrompt"].is_object() {
                    params["systemPrompt"] =
                        json!({"mode":"append","content":[{"type":"text","text":input.system}]});
                }
                self.loading = self.previous.is_some() && self.caps["loadSession"] == true;
                if self.loading {
                    params["sessionId"] = json!(self.previous);
                }
                self.phase = 2;
                self.rpc().request(
                    2,
                    if self.loading {
                        "session/load"
                    } else {
                        "session/new"
                    },
                    params,
                )?;
            }
            2 => {
                let sid = if self.loading {
                    self.previous.as_deref()
                } else {
                    result["sessionId"].as_str()
                }
                .filter(|s| !s.is_empty() && s.len() <= 4096)
                .ok_or("missing ACP session")?
                .to_owned();
                let system = if self.caps["sessionCapabilities"]["systemPrompt"].is_object() {
                    ""
                } else {
                    input.system
                };
                let blocks = content::prompt(
                    input.messages,
                    system,
                    self.config.send_images && self.caps["promptCapabilities"]["image"] == true,
                    !self.loading,
                );
                self.rpc().request(
                    3,
                    "session/prompt",
                    json!({"sessionId":sid,"prompt":blocks}),
                )?;
                self.phase = 3;
                self.session = Some(sid.clone());
                return Ok(Some(Step::Response {
                    record: json!({"kind":"acpSession","remoteId":sid}),
                }));
            }
            3 => {
                if result["stopReason"].as_str().is_none() {
                    return Err("missing ACP stop reason".into());
                }
                self.phase = 4;
                return Ok(Some(Step::End));
            }
            _ => return Err("unexpected ACP response".into()),
        }
        Ok(None)
    }
}
impl Provider for Acp {
    fn pending_request(&self) -> Option<&Connection> {
        (self.phase == 0 && self.authorized.is_none()).then_some(&self.config)
    }
    fn configure(&mut self, c: Connection, key: String) {
        self.config = c;
        self.stream.key = key.clone();
        self.authorized = Some(key);
    }
    fn external_response(&mut self, id: &str, allow: bool) -> Result<bool, String> {
        let Some(p) = self.permission.as_ref().filter(|p| p.card == id) else {
            return Ok(false);
        };
        self.rpc().send(p.answer(Some(allow)))?;
        self.permission = None;
        Ok(true)
    }
    fn cancel(&mut self) {
        if let Some(rpc) = &self.process {
            if self.phase == 3 {
                let _=rpc.send(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":self.session}}));
            }
            if let Some(p) = self.permission.take() {
                let _ = rpc.send(p.answer(None));
            }
        }
    }
    fn next(&mut self, input: Input<'_>) -> Result<Step, String> {
        if let Some(step) = self.deferred.take() {
            return Ok(step);
        }
        if self.phase == 0 {
            let key = self.authorized.take().ok_or("ACP permission required")?;
            let proc = Process::start(&self.config, &key, &self.cwd, self.wake.clone())?;
            proc.request(1,"initialize",json!({"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},"clientInfo":{"name":"folio-studio","version":env!("CARGO_PKG_VERSION")}}))?;
            self.process = Some(proc);
            self.phase = 1;
        }
        if self.permission.is_some() {
            return Ok(Step::Pending);
        }
        if let Some((id, reply)) = self.reply.take() {
            let r = input
                .messages
                .iter()
                .rev()
                .find(|v| v["kind"] == "tool_result" && v["toolUseId"] == id)
                .ok_or("missing Folio tool result")?;
            let mut content = r["content"].clone();
            if r["isError"] == true {
                content["ok"] = json!(false);
            }
            let _ = reply.send(content);
        }
        if self.phase == 3
            && let Ok(call) = self.calls.try_recv()
        {
            let id = folio_project::util::uuid_v4();
            self.reply = Some((id.clone(), call.reply));
            return Ok(Step::Tool {
                id,
                name: "folio_call".into(),
                args: json!({"name":call.name,"args":call.args}),
            });
        }
        match self.rpc().inbox.try_recv() {
            Ok(msg) => match self.incoming(msg?, &input)? {
                Some(step) => {
                    let (step, deferred) = self.stream.filter(step);
                    self.deferred = deferred;
                    Ok(step)
                }
                None => {
                    (self.wake)();
                    Ok(Step::Pending)
                }
            },
            Err(mpsc::TryRecvError::Empty) => Ok(Step::Pending),
            Err(_) => Err("ACP agent disconnected".into()),
        }
    }
}
