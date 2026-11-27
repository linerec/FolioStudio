use crate::tools::{self, Action, Gate, Risk};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    ReadOnly,
    Ask,
    #[default]
    Direct,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Step {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    Tool {
        id: String,
        name: String,
        args: Value,
    },
    #[serde(skip)]
    ExternalPermission { card:Value },
    #[serde(skip)]
    Pending,
    #[serde(skip)]
    Preview {
        record: Value,
    },
    #[serde(skip)]
    Response {
        record: Value,
    },
    #[serde(skip)]
    Skipped {
        id: String,
    },
    End,
}
pub struct Input<'a> {
    pub system: &'a str,
    pub tools: &'a [Value],
    pub messages: &'a [Value],
}
pub trait Provider {
    fn pending_request(&self) -> Option<&crate::connection::Connection> {
        None
    }
    fn configure(&mut self, _: crate::connection::Connection, _: String) {}
    fn external_response(&mut self,_id:&str,_allow:bool)->Result<bool,String>{Ok(false)}
    fn cancel(&mut self) {}
    fn next(&mut self, input: Input<'_>) -> Result<Step, String>;
}
pub struct AttachedCall {
    pub name:String,
    pub args:Value,
    pub reply:std::sync::mpsc::Sender<Value>,
}
pub struct Call {
    pub id: String,
    pub name: String,
    pub args: Value,
    pub risk: Risk,
}
pub enum Effect {
    Pending,
    Preview(Value),
    Event(Value),
    Execute(Call),
    Permission(Value),
    End,
}

pub struct Agent {
    pub messages: Vec<Value>,
    mode: Mode,
    approved_edits: bool,
    grants: std::collections::HashSet<String>,
    pending: Option<(String, Call)>,
    external_offer: Option<(String, String)>,
}
impl Agent {
    pub fn new(mode: Mode, messages: Vec<Value>) -> Self {
        Self {
            messages,
            mode,
            approved_edits: false,
            grants: Default::default(),
            pending: None,
            external_offer: None,
        }
    }
    pub fn restore_grants(&mut self, records: &[Value]) {
        self.grants.extend(records.iter().filter(|r| r["kind"] == "permissionGrant" && r["risk"] == "edit")
            .filter_map(|r| r["tool"].as_str().map(str::to_owned)));
    }
    pub fn awaiting_permission(&self) -> bool { self.pending.is_some() }

    pub fn grant_session(&mut self, id: &str) -> Result<Effect,String> {
        let (_,call) = self.pending.as_ref().filter(|(key,call)| key == id && call.risk == Risk::Edit)
            .ok_or("session permission unavailable")?;
        self.record_grant(call.name.clone());
        Ok(Effect::Execute(self.pending.take().unwrap().1))
    }
    fn record_grant(&mut self, tool: String) {
        if self.grants.insert(tool.clone()) {
            self.messages.push(json!({"kind":"permissionGrant","risk":"edit","tool":tool}));
        }
    }
    pub fn external_response(&mut self, provider: &mut dyn Provider, id: &str, allow: bool, session: bool) -> Result<bool, String> {
        let offer = self.external_offer.as_ref().filter(|(key, _)| key == id);
        if session && offer.is_none() || !provider.external_response(id, allow)? {
            return Ok(false);
        }
        if session && let Some((_, tool)) = self.external_offer.take() {
            self.record_grant(tool);
        }
        self.external_offer = None;
        Ok(true)
    }
    fn external_card(&mut self, gate: &impl Gate, fixed: &[Value], mut card: Value) -> Effect {
        self.external_offer = None;
        let requested = card.as_object_mut().and_then(|map| map.remove("requestedSessionTool"));
        if self.mode == Mode::Ask && let Some(name) = requested.and_then(|v| v.as_str().map(str::to_owned))
            && let Ok(Action::Call { name, risk: Risk::Edit, .. }) =
                tools::resolve(gate, fixed, "folio_call", &json!({"name":name, "args":card["args"]}))
            && let Some(id) = card["permissionId"].as_str()
        {
            self.external_offer = Some((id.to_owned(), name.clone()));
            card["sessionTool"] = json!(name);
            card["allowedDecisions"] = json!(["deny","allow","session"]);
        }
        Effect::Permission(card)
    }
    pub fn result(&mut self, id: &str, value: Value, error: bool) -> Effect {
        self.event(json!({"kind":"tool_result","toolUseId":id,"isError":error,"content":value}))
    }
    fn event(&mut self, event: Value) -> Effect {
        self.messages.push(event.clone());
        Effect::Event(event)
    }
    pub fn advance(
        &mut self,
        gate: &impl Gate,
        provider: &mut (impl Provider + ?Sized),
        system: &str,
        fixed: &[Value],
    ) -> Result<Effect, String> {
        if self.pending.is_some() {
            return Err("awaiting permission".into());
        }
        match provider.next(Input {
            system,
            tools: fixed,
            messages: &self.messages,
        })? {
            Step::ExternalPermission{card} => Ok(self.external_card(gate, fixed, card)),
            Step::Preview { record } => Ok(Effect::Preview(record)),
            Step::Pending => Ok(Effect::Pending),
            Step::Response { record } => Ok(self.event(record)),
            Step::Skipped { id } => {
                Ok(self.result(&id, json!({"error":"tool execution suppressed"}), true))
            }
            Step::End => Ok(Effect::End),
            Step::Text { text } => Ok(self.event(json!({"kind":"text","text":text}))),
            Step::Thinking { text } => Ok(self.event(json!({"kind":"thinking","text":text}))),
            Step::Tool { id, name, args } => {
                self.messages
                    .push(json!({"kind":"tool_use","id":id,"name":name,"args":args}));
                let action = match tools::resolve(gate, fixed, &name, &args) {
                    Ok(a) => a,
                    Err(e) => return Ok(self.result(&id, json!({"error":e}), true)),
                };
                let (name, args, risk) = match action {
                    Action::Found(found) => return Ok(self.result(&id, found, false)),
                    Action::Call { name, args, risk } => (name, args, risk),
                };
                if self.mode == Mode::ReadOnly && risk != Risk::Read {
                    return Ok(self.result(&id, json!({"error":"readOnly"}), true));
                }
                let call = Call {
                    id,
                    name,
                    args,
                    risk,
                };
                if matches!(risk, Risk::File | Risk::Database)
                    || risk == Risk::Edit && self.mode == Mode::Ask && !self.approved_edits && !self.grants.contains(&call.name)
                {
                    let permission = folio_project::util::uuid_v4();
                    let mut card = json!({"kind":"permission","permissionId":permission,"tool":call.name,"args":call.args,"risk":risk});
                    if risk == Risk::Edit { card["allowedDecisions"] = json!(["deny","allow","session"]); }
                    self.pending = Some((permission, call));
                    Ok(Effect::Permission(card))
                } else {
                    Ok(Effect::Execute(call))
                }
            }
        }
    }
    pub fn cancel_call(&mut self) {
        self.pending = None;
        let mut missing = Vec::<String>::new();
        for record in self
            .messages
            .iter()
            .rev()
            .take_while(|m| m["kind"] != "user")
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            match record["kind"].as_str() {
                Some("assistant") => {
                    for block in record["blocks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|b| b["type"] == "tool_use")
                    {
                        if let Some(id) = block["id"].as_str() {
                            missing.push(id.into())
                        }
                    }
                }
                Some("tool_use") => {
                    if let Some(id) = record["id"].as_str()
                        && !missing.iter().any(|v| v == id)
                    {
                        missing.push(id.into())
                    }
                }
                Some("tool_result") => missing.retain(|id| record["toolUseId"] != *id),
                _ => {}
            }
        }
        for id in missing {
            self.result(&id, json!({"error":"stopped"}), true);
        }
    }
    pub fn respond(&mut self, permission: &str, allow: bool) -> Result<Effect, String> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|(id, _)| id == permission)
        {
            return Err("unknown permission".into());
        }
        let (_, call) = self.pending.take().unwrap();
        if !allow {
            return Ok(self.result(&call.id, json!({"error":"denied"}), true));
        }
        if call.risk == Risk::Edit {
            self.approved_edits = true;
        }
        Ok(Effect::Execute(call))
    }
}
