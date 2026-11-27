use super::checkpoints::{self, Completed, OpenGroup};
use crate::{
    attach::{self, Incoming},
    requests::Context,
};
use folio_ai::{
    agent::{Agent, Effect, Provider},
    session::{Secrets, Store},
    tools::Risk,
};
use folio_engine::mcp::gate::{self, Attached};
use serde_json::{Value, json};
use std::sync::mpsc::Sender;

pub(super) struct Input {
    pub text: String,
    pub attachments: Vec<Value>,
    pub send_images: bool,
}
pub(super) struct Active {
    pub session: String,
    pub turn: String,
    pub label: String,
    pub input: Option<Input>,
    pub store: Store,
    pub agent: Agent,
    pub provider: Box<dyn Provider + Send>,
    #[cfg(any(windows,target_os="linux",target_os="macos"))]
    pub _endpoint:Option<crate::attach::TurnEndpoint>,
    pub approved_send: bool,
    pub outbound_permission: Option<String>,
    pub system: String,
    pub tools: Vec<Value>,
    pub groups: Vec<OpenGroup>,
    pub seq: u64,
    pub stored: usize,
    pub turn_start: usize,
    pub waiting: Option<String>,
    pub stop: bool,
    pub effect: Option<Effect>,
}
impl Active {
    pub fn record(
        &mut self,
        ctx: &Context<'_>,
        value: Value,
        secrets: &Secrets,
    ) -> Result<(), String> {
        let mut value = secrets.clean(&value);
        value["sessionId"] = json!(self.session);
        value["turnId"] = json!(self.turn);
        value["seq"] = json!(self.seq + 1);
        self.store.append(&self.session, &value, secrets)?;
        self.seq += 1;
        value["event"] = json!("aiEvent");
        crate::send(ctx.out, value, &[]);
        Ok(())
    }
    fn flush(&mut self, ctx: &Context<'_>, secrets: &Secrets) -> Result<(), String> {
        while self.stored < self.agent.messages.len() {
            let clean = secrets.clean(&self.agent.messages[self.stored]);
            let original = &self.agent.messages[self.stored];
            if original["kind"] == "assistant"
                && original["blocks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .any(|(i, b)| {
                        matches!(b["type"].as_str(), Some("thinking" | "redacted_thinking"))
                            && clean["blocks"][i] != *b
                    })
            {
                self.agent.messages.truncate(self.stored);
                return Err("signed block contained a credential".into());
            }
            self.agent.messages[self.stored] = clean.clone();
            let mut record = clean;
            self.store.persist_images(&self.session, &mut record)?;
            self.record(ctx, record, secrets)?;
            self.stored += 1;
        }
        Ok(())
    }
    pub fn tick(
        &mut self,
        ctx: &Context<'_>,
        tx: &Sender<Incoming>,
        secrets: &Secrets,
        credentials: &std::collections::HashMap<String, super::connections::Credential>,
    ) -> Result<bool, String> {
        if self.stop {
            return Ok(true);
        }
        if self.waiting.is_some() || self.outbound_permission.is_some() || self.agent.awaiting_permission() {
            return Ok(false);
        }
        let session = ctx
            .session
            .lock()
            .map_err(|_| "document unavailable")?
            .clone();
        checkpoints::include(&mut self.groups, session.as_deref(), &self.label)?;
        if let Some(input) = self.input.take() {
            for message in &mut self.agent.messages {
                if let Some(o) = message.as_object_mut() {
                    for key in ["sessionId", "turnId", "seq"] {
                        o.remove(key);
                    }
                }
                self.store.hydrate(&self.session, message)?;
            }
            self.agent.cancel_call();
            let mut attachments = Vec::new();
            for item in input.attachments {
                let value = crate::ai_context::build_with_images(
                    ctx,
                    session.as_deref().ok_or("no document open")?,
                    &item,
                    input.send_images,
                )?;
                attachments.push(value);
            }
            self.agent.messages.push(
                secrets.clean(&json!({"kind":"user","text":input.text,"attachments":attachments})),
            );
        }
        self.flush(ctx, secrets)?;
        if self.stop {
            return Ok(true);
        }
        if self.effect.is_none()
            && let Some(previous) = self.provider.pending_request()
        {
            let prefs = super::connections::saved()?;
            let mut connection = super::connections::find(&prefs, &json!(previous.id))?;
            if previous.kind != connection.kind || previous.base_url != connection.base_url || previous.kind==folio_ai::connection::Kind::Acp && super::agent_profile(previous)!=super::agent_profile(&connection) {
                return Err("aiNewSessionRequired".into());
            }
            connection.model = previous.model.clone();
            connection.send_images &= prefs["folio.ai.sendImages"] != false;
            let key = super::connections::key_for(credentials, &connection).to_owned();
            connection.ready(&key, prefs["folio.ai.localOnly"] == true)?;
            if !folio_ai::connection::is_loopback(&connection.base_url) && !self.approved_send {
                let id = folio_project::util::uuid_v4();
                self.record(ctx,json!({"kind":"permission","permissionId":id,"risk":"external","destination":if connection.kind==folio_ai::connection::Kind::Acp{&connection.command}else{&connection.base_url}}),secrets)?;
                self.outbound_permission = Some(id);
                return Ok(false);
            }
            self.approved_send = false;
            if let Some(policy) =
                folio_ai::providers::image_policy(&self.agent.messages, connection.send_images)?
            {
                self.agent.messages.push(policy);
                self.flush(ctx, secrets)?;
            }
            self.provider.configure(connection, key);
        }
        let effect = match self.effect.take() {
            Some(e) => e,
            None => {
                self.agent
                    .advance(&Attached, self.provider.as_mut(), &self.system, &self.tools)?
            }
        };
        self.flush(ctx, secrets)?;
        match effect {
            Effect::Pending => {}
            Effect::Preview(record) => {
                let mut value = secrets.clean(&record);
                value["event"] = json!("aiEvent");
                value["sessionId"] = json!(self.session);
                value["turnId"] = json!(self.turn);
                value["afterSeq"] = json!(self.seq);
                crate::send(ctx.out, value, &[]);
                let _ = tx.send(Incoming::AiPump);
            }
            Effect::End => return Ok(true),
            Effect::Permission(card) => self.record(ctx, card, secrets)?,
            Effect::Event(_) => {
                let _ = tx.send(Incoming::AiPump);
            }
            Effect::Execute(call) => {
                if secrets.clean(&call.args) != call.args {
                    self.agent.result(
                        &call.id,
                        json!({"error":"credential in tool arguments"}),
                        true,
                    );
                    let _ = tx.send(Incoming::AiPump);
                    return Ok(false);
                }
                let files = call.risk == Risk::File;
                match gate::prepare(&call.name, &call.args, call.risk == Risk::Database, files) {
                    Ok(request) => {
                        self.waiting = Some(call.id.clone());
                        attach::ai_route(request, call.id, files, tx);
                    }
                    Err(e) => {
                        self.agent
                            .result(&call.id, json!({"error":secrets.text(&e)}), true);
                        let _ = tx.send(Incoming::AiPump);
                    }
                }
            }
        }
        Ok(false)
    }
    pub fn end(
        mut self,
        ctx: &Context<'_>,
        secrets: &Secrets,
        error: Option<String>,
    ) -> Result<Vec<Completed>, String> {
        if self.stop || error.is_some(){self.provider.cancel();}
        self.agent.cancel_call();
        let groups = checkpoints::finish(std::mem::take(&mut self.groups))?;
        self.flush(ctx, secrets)?;
        let mut end = json!({"kind":"end","status":if error.is_some(){"error"}else if self.stop{"stopped"}else{"complete"},
            "error":error,"documents":checkpoints::cards(&groups)});
        folio_ai::providers::usage::summarize(&self.agent.messages, self.turn_start, &mut end);
        self.record(ctx, end, secrets)?;
        Ok(groups)
    }
}
