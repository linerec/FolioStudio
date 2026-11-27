mod active;
mod checkpoints;
mod connections;
pub(crate) mod settings;
use crate::{Session, attach::Incoming, requests::Context};
use active::{Active, Input};
use folio_ai::connection::Kind;
use folio_ai::{
    agent::{Agent, Mode, Provider},
    providers::scripted::Scripted,
    session::{Secrets, Store},
    tools,
};
use folio_ai::{
    providers::network::Network,
    transport::{Desktop, Transport},
};
use folio_engine::mcp::gate::Attached;
use folio_project::{hash::sha256_bytes, util::uuid_v4};
use serde_json::{Value, json};
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf, sync::mpsc::Sender};

pub struct Runtime {
    transport: Arc<dyn Transport>,
    model_requests: usize,
    active: Option<Active>,
    completed: HashMap<(String, String), Vec<checkpoints::Completed>>,
    secrets: Secrets,
    credentials: HashMap<String, connections::Credential>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            model_requests: 0,
            transport: Arc::new(Desktop::default()),
            active: None,
            completed: HashMap::new(),
            secrets: Secrets::default(),
            credentials: HashMap::new(),
        }
    }
}
fn scripted_test() -> bool {
    std::env::var_os("FOLIO_AI_SCRIPTED_TEST").is_some_and(|v| v == "1")
}
pub(crate) fn document_id(s: &Session) -> String {
    s.folio
        .as_ref()
        .map(|f| f.opened.manifest.id.clone())
        .unwrap_or_else(|| sha256_bytes(s.source_path.as_os_str().as_encoded_bytes()))
}
fn project(s: Option<&Session>) -> String {
    if let Some(f) = s.and_then(|s| s.folio.as_ref()) {
        return f.opened.project.id().to_owned();
    }
    if let Ok(root) = crate::attach_paths::project_root(s)
        && let Ok(p) = folio_project::manifest::load(&root)
    {
        return p.id().to_owned();
    }
    s.and_then(|s| folio_project::manifest::open(&s.source_path).ok())
        .map(|p| p.id().to_owned())
        .unwrap_or_else(|| s.map(document_id).unwrap_or_else(|| "workspace".into()))
}
fn agent_profile(c:&folio_ai::connection::Connection)->String{
    sha256_bytes(json!([c.command,c.args,c.env]).to_string().as_bytes())
}
fn string<'a>(req: &'a Value, key: &str) -> Result<&'a str, String> {
    req[key].as_str().ok_or_else(|| format!("expected {key}"))
}

impl Runtime {
    pub fn running(&self) -> bool {
        self.active.is_some()
    }
    pub fn blocks(&self, cmd: &str, req: &Value) -> bool {
        use folio_ai::tools::{Gate, Risk};
        self.running()
            && !(Attached.risk(cmd, req) == Risk::Read
                || matches!(
                    cmd,
                    "quit"
                        | "tile"
                        | "renderPage"
                        | "render"
                        | "page"
                        | "setViewState"
                        | "setSelection"
                        | "setTextSelection"
                        | "goToPage"
                        | "setZoom"
                        | "setTool"
                        | "setScrollCenter"
                        | "dockActionResult"
                        | "debugDigest"
                        | "editState"
                        | "textHit"
                        | "textCaret"
                        | "textSelection"
                        | "textNavigate"
                        | "textLayout"
                        | "tableHit"
                        | "selectionSize"
                ))
    }
    fn end(&mut self, ctx: &Context<'_>, error: Option<String>) {
        let Some(active) = self.active.take() else {
            return;
        };
        let key = (active.session.clone(), active.turn.clone());
        let seq = active.seq + 1;
        match active.end(ctx, &self.secrets, error) {
            Ok(groups) => {
                self.completed.insert(key, groups);
            }
            Err(e) => crate::send(
                ctx.out,
                json!({"event":"aiEvent","kind":"end","status":"error","error":self.secrets.text(&e),
                "sessionId":key.0,"turnId":key.1,"seq":seq}),
                &[],
            ),
        }
        self.refresh_secrets();
    }
    pub fn shutdown(&mut self, ctx: &Context<'_>) {
        if let Some(a) = &mut self.active {
            a.stop = true;
        }
        self.end(ctx, None);
    }
    pub fn tick(&mut self, ctx: &Context<'_>, tx: &Sender<Incoming>) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        match active.tick(ctx, tx, &self.secrets, &self.credentials) {
            Ok(false) => {}
            Ok(true) => self.end(ctx, None),
            Err(e) => self.end(ctx, Some(e)),
        }
    }
    pub fn result(&mut self, call: &str, reply: Value, tx: &Sender<Incoming>) {
        if let Some(a) = &mut self.active
            && a.waiting.as_deref() == Some(call)
        {
            a.waiting = None;
            let error = reply["ok"] != true;
            a.agent.result(call, self.secrets.clean(&reply), error);
            let _ = tx.send(Incoming::AiPump);
        }
    }
    pub fn handle(
        &mut self,
        req: &Value,
        ctx: &Context<'_>,
        tx: &Sender<Incoming>,
    ) -> Option<Value> {
        if matches!(req["cmd"].as_str(), Some("aiModels" | "aiTestConnection")) {
            return match self.start_model_list(req, tx) {
                Ok(()) => None,
                Err(e) => Some(self.answer(req, Err(e))),
            };
        }
        let result = self.command(req, ctx, tx);
        let answer = self.answer(req, result);
        self.refresh_secrets();
        Some(answer)
    }
    pub fn model_list_result(&mut self, req: &Value, result: Result<Value, String>) -> Value {
        self.model_requests -= 1;
        let answer = self.answer(req, result);
        self.refresh_secrets();
        answer
    }
    fn answer(&self, req: &Value, result: Result<Value, String>) -> Value {
        let mut answer = match result {
            Ok(v) => {
                let mut v = self.secrets.clean(&v);
                v["ok"] = json!(true);
                v["type"] = req["cmd"].clone();
                v
            }
            Err(e) => {
                json!({"ok":false,"code":match e.as_str(){"aiTurnRunning"|"aiNotConfigured"|"aiLocalOnly"|"aiNewSessionRequired"=>e.as_str(),_=>"ai"},"error":self.secrets.text(&e)})
            }
        };
        answer["id"] = self.secrets.clean(&req["id"]);
        answer
    }
    fn command(
        &mut self,
        req: &Value,
        ctx: &Context<'_>,
        tx: &Sender<Incoming>,
    ) -> Result<Value, String> {
        let cmd = string(req, "cmd")?;
        if self.running()
            && matches!(
                cmd,
                "aiSend" | "aiDeleteSession" | "aiRevertTurn" | "aiRestoreCheckpoint"
            )
        {
            return Err("aiTurnRunning".into());
        }
        match cmd {
            "aiSettings" | "aiSetSettings" => {
                if self.secrets.clean(&req["values"]) != req["values"] {
                    return Err("invalid AI setting".into());
                }
                return settings::settings(req);
            }
            "aiSetCredential"
            | "aiConnections"
            | "aiSaveConnection"
            | "aiDeleteConnection"
            | "aiSetDefaultConnection" => return self.connections(cmd, req),
            "aiStop" | "aiRespond" => {
                let a = self.active.as_mut().ok_or("no active turn")?;
                if req.get("sessionId").is_some_and(|id| id != &a.session)
                    || req.get("turnId").is_some_and(|id| id != &a.turn)
                {
                    return Err("unknown turn".into());
                }
                if cmd == "aiStop" {
                    a.stop = true;
                } else {
                    let allow = match string(req, "decision")? {
                        "session" | "allow" => true,
                        "deny" => false,
                        _ => return Err("unknown decision".into()),
                    };
                    let id = string(req, "permissionId")?;
                    if a.outbound_permission.as_deref() == Some(id) {
                        if req["decision"] == "session" {return Err("session permission unavailable".into());}
                        a.outbound_permission = None;
                        a.approved_send = allow;
                        if !allow {
                            a.stop = true;
                        }
                    } else if !a.agent.external_response(a.provider.as_mut(), id, allow, req["decision"] == "session")? {
                        a.effect = Some(if req["decision"] == "session" { a.agent.grant_session(id)? } else { a.agent.respond(id, allow)? });
                    }
                }
                let _ = tx.send(Incoming::AiPump);
                return Ok(json!({"turnId":a.turn}));
            }
            _ => {}
        }
        let session = ctx
            .session
            .lock()
            .map_err(|_| "document unavailable")?
            .clone();
        let s = session.as_deref();
        if cmd == "aiContext" {
            return crate::ai_context::build(ctx, s.ok_or("no document open")?, req);
        }
        let project = project(s);
        let root = std::env::var_os("FOLIO_USER_DATA")
            .map(PathBuf::from)
            .ok_or("user data unavailable")?;
        let store = Store::open(&root, &project)?;
        let sid = req["sessionId"].as_str().unwrap_or("");
        match cmd {
            "aiSessions" => Ok(json!({"sessions":store.list()?})),
            "aiSession" => {
                let mut records = store.records(sid)?;
                if req["includeImages"] == true {
                    for r in &mut records {
                        store.hydrate(sid, r)?;
                    }
                }
                Ok(json!({"records":records}))
            }
            "aiNewSession" => {
                let defaults = settings::settings(&Value::Null)?;
                let mode: Mode = serde_json::from_value(
                    req.get("mode")
                        .unwrap_or(&defaults["settings"]["folio.ai.mode"])
                        .clone(),
                )
                .map_err(|_| "unknown mode")?;
                let language = req["language"].as_str().unwrap_or("ko");
                let preferences = connections::saved()?;
                let scripted = req["provider"] == "scripted" && scripted_test();
                let connection = if scripted {
                    None
                } else {
                    Some(connections::find(
                        &preferences,
                        req.get("connectionId")
                            .unwrap_or(&preferences[connections::DEFAULT]),
                    )?)
                };
                let (provider, model, cid) = match &connection {
                    Some(c) if c.model.is_empty() => return Err("aiNotConfigured".into()),
                    Some(c) => (json!(c.kind), c.model.as_str(), json!(c.id)),
                    None => (json!("scripted"), "scripted", Value::Null),
                };
                if req.get("model").is_some_and(|v| v != model)
                    || req.get("provider").is_some_and(|v| v != &provider)
                {
                    return Err("aiNewSessionRequired".into());
                }
                let id=store.create(&json!({"kind":"session","name":req["name"].as_str().unwrap_or("Folio"),
                    "mode":mode,"language":language,"system":folio_ai::system_prompt(language),"tools":tools::fixed(&Attached),
                    "provider":provider,"model":model,"connectionId":cid,"agentProfile":connection.as_ref().filter(|c|c.kind==Kind::Acp).map(agent_profile),"baseUrl":connection.as_ref().map(|c|&c.base_url),"summary":req["summary"].as_str()}),&self.secrets)?;
                Ok(json!({"sessionId":id}))
            }
            "aiRenameSession" => {
                store.rename(sid, string(req, "name")?, &self.secrets)?;
                Ok(json!({}))
            }
            "aiDeleteSession" => {
                store.delete(sid)?;
                self.completed.retain(|(id, _), _| id != sid);
                Ok(json!({}))
            }
            "aiExportSession" => Ok(json!({"markdown":store.markdown(sid)?})),
            "aiSend" => {
                if sid.is_empty() {
                    return Err("aiNotConfigured".into());
                }
                let records = store.records(sid)?;
                let header = records.first().ok_or("empty session")?;
                if ["model", "connectionId", "provider"]
                    .iter()
                    .any(|k| req.get(*k).is_some_and(|v| v != &header[*k]))
                {
                    return Err("aiNewSessionRequired".into());
                }
                let preferences = connections::saved()?;
                let mut send_images = preferences["folio.ai.sendImages"] != false;
                #[cfg(any(windows,target_os="linux",target_os="macos"))]
                let mut endpoint=None;
                let provider: Box<dyn Provider + Send> =
                    if header["provider"] == "scripted" && scripted_test() {
                        Box::new(Scripted::from_json(&self.secrets.clean(&req["script"]))?)
                    } else {
                        let mut c = connections::find(&preferences, &header["connectionId"])?;
                        if header.get("baseUrl").is_some_and(|v| v != &c.base_url)
                            || json!(c.kind) != header["provider"]
                        {
                            return Err("aiNewSessionRequired".into());
                        }
                        c.model = string(header, "model")?.into();
                        if c.model.is_empty() {
                            return Err("aiNotConfigured".into());
                        }
                        let key = connections::key_for(&self.credentials, &c).to_owned();
                        c.ready(&key, preferences["folio.ai.localOnly"] == true)?;
                        c.send_images &= send_images;
                        folio_ai::providers::image_policy(&records, c.send_images)?;
                        send_images = c.send_images;
                        if c.kind==Kind::Acp {
                            #[cfg(any(windows,target_os="linux",target_os="macos"))]{
                                if header["agentProfile"]!=agent_profile(&c){return Err("aiNewSessionRequired".into())}
                                let cwd=crate::attach_paths::project_root(s)?;
                                let mcp=std::env::current_exe().map_err(|_|"missing Folio executable")?
                                    .with_file_name(format!("folio-mcp{}",std::env::consts::EXE_SUFFIX));
                                if !mcp.is_file(){return Err("missing folio-mcp executable".into())}
                                let (lease,calls)=crate::attach::TurnEndpoint::open(&root,tx)?;
                                let previous=records.iter().rev().find(|r|r["kind"]=="acpSession").and_then(|r|r["remoteId"].as_str()).map(str::to_owned);
                                let tx=tx.clone();
                                let provider=folio_ai::acp::Acp::new(c,cwd,mcp,lease.file.clone(),previous,calls,Arc::new(move||{let _=tx.send(Incoming::AiPump);}));
                                endpoint=Some(lease);
                                Box::new(provider)
                            }
                            #[cfg(not(any(windows,target_os="linux",target_os="macos")))]
                            return Err("ACP is desktop only".into());
                        } else {
                        let tx = tx.clone();
                        let questions_only = records
                            .iter()
                            .any(|r| r["kind"] == "assistant" && r["questionsOnly"] == true);
                        Box::new(Network::new(
                            c,
                            key,
                            self.transport.clone(),
                            Arc::new(move || {
                                let _ = tx.send(Incoming::AiPump);
                            }),
                            questions_only,
                        ))}
                    };
                let mode =
                    serde_json::from_value(header["mode"].clone()).map_err(|_| "invalid mode")?;
                let tools = header["tools"].as_array().ok_or("invalid tools")?.clone();
                let system = string(header, "system")?.to_owned();
                let attachments = req
                    .get("attachments")
                    .map(|v| v.as_array().cloned().ok_or("invalid attachments"))
                    .transpose()?
                    .unwrap_or_default();
                let summary = header["summary"]
                    .as_str()
                    .filter(|_| !records.iter().any(|r| r["kind"] == "user"))
                    .map(str::to_owned);
                let mut agent = Agent::new(mode, Vec::new());
                agent.restore_grants(&records);
                let messages: Vec<Value> = records
                    .into_iter()
                    .filter(|r| {
                        matches!(
                            r["kind"].as_str(),
                            Some(
                                "imagePolicy"
                                    | "assistant"
                                    | "user"
                                    | "text"
                                    | "thinking"
                                    | "tool_use"
                                    | "tool_result"
                            )
                        )
                    })
                    .collect();
                let stored = messages.len();
                let input = Input {
                    text: {
                        let text = string(req, "text")?;
                        self.secrets.text(
                            &summary
                                .map(|s| format!("{s}\n\n{text}"))
                                .unwrap_or_else(|| text.into()),
                        )
                    },
                    attachments,
                    send_images,
                };
                let turn = uuid_v4();
                let label = folio_ai::turn::history_label(&input.text);
                let mut groups = Vec::new();
                checkpoints::include(&mut groups, s, &label)?;
                agent.messages = messages;
                self.active = Some(Active {
                    session: sid.into(),
                    turn: turn.clone(),
                    label,
                    input: Some(input),
                    store,
                    agent,
                    provider,
                    #[cfg(any(windows,target_os="linux",target_os="macos"))]
                    _endpoint:endpoint,
                    approved_send: false,
                    outbound_permission: None,
                    system,
                    tools,
                    groups,
                    seq: 0,
                    turn_start: stored,
                    stored,
                    waiting: None,
                    stop: false,
                    effect: None,
                });
                let _ = tx.send(Incoming::AiPump);
                Ok(json!({"sessionId":sid,"turnId":turn}))
            }
            "aiRevertTurn" | "aiRestoreCheckpoint" => {
                store.records(sid)?;
                let tid = string(req, "turnId")?;
                let groups = self
                    .completed
                    .get(&(sid.into(), tid.into()))
                    .ok_or("checkpoint unavailable in this engine session")?;
                let reply = checkpoints::restore(ctx, groups, req)?;
                if reply["applied"] == true {
                    store.append(
                        sid,
                        &json!({"kind":"restore","turnId":tid,"result":reply}),
                        &self.secrets,
                    )?;
                }
                Ok(reply)
            }
            _ => Err("unknown AI request".into()),
        }
    }
}
