use super::Runtime;
use folio_ai::{
    connection::{Connection, templates},
    providers::network,
    transport::Cancel,
};
use serde_json::{Map, Value, json};

pub(super) type Credential = (folio_ai::connection::Kind, String, String);
fn credential_scope(c:&Connection) -> String {
    if c.kind == folio_ai::connection::Kind::Acp {super::agent_profile(c)} else {c.base_url.clone()}
}
pub(super) fn key_for<'a>(
    credentials: &'a std::collections::HashMap<String, Credential>,
    c: &Connection,
) -> &'a str {
    credentials
        .get(&c.id)
        .filter(|(kind, url, _)| *kind == c.kind && url == &credential_scope(c))
        .map(|(_, _, key)| key.as_str())
        .unwrap_or("")
}
pub(super) const LIST: &str = "folio.ai.connections";
pub(super) const DEFAULT: &str = "folio.ai.defaultConnection";

pub fn saved() -> Result<Value, String> {
    Ok(crate::user_settings::run(&Value::Null)?["settings"]["preferences"].clone())
}
fn list(p: &Value) -> Result<Vec<Connection>, String> {
    p[LIST]
        .as_array()
        .into_iter()
        .flatten()
        .map(Connection::parse)
        .collect()
}
pub fn find(p: &Value, id: &Value) -> Result<Connection, String> {
    list(p)?
        .into_iter()
        .find(|c| id == &c.id)
        .ok_or("aiNotConfigured".into())
}
fn prefs(settings: &mut Map<String, Value>) -> Result<&mut Value, String> {
    let p = settings.entry("preferences").or_insert_with(|| json!({}));
    if !p.is_object() {
        return Err("invalid user preferences".into());
    }
    Ok(p)
}
impl Runtime {
    pub(super) fn refresh_secrets(&mut self) {
        if self.active.is_none() && self.model_requests == 0 {
            self.secrets = Default::default();
            for (_, _, key) in self.credentials.values() {
                self.secrets.remember(key);
            }
        }
    }
    pub(super) fn start_model_list(
        &mut self,
        req: &Value,
        tx: &std::sync::mpsc::Sender<crate::attach::Incoming>,
    ) -> Result<(), String> {
        self.queue_model_list(&saved()?, req, tx)
    }
    fn queue_model_list(
        &mut self,
        current: &Value,
        req: &Value,
        tx: &std::sync::mpsc::Sender<crate::attach::Incoming>,
    ) -> Result<(), String> {
        if self.model_requests >= 4 {
            return Err("connection tests already running".into());
        }
        let c = find(current, &req["connectionId"])?;
        let key = key_for(&self.credentials, &c).to_owned();
        c.ready(&key, current["folio.ai.localOnly"] == true)?;
        let t = self.transport.clone();
        let tx = tx.clone();
        let request = json!({"cmd":req["cmd"],"id":req["id"]});
        std::thread::Builder::new()
            .name("folio-ai-model-list".into())
            .spawn(move || {
                #[cfg(any(windows,target_os="linux",target_os="macos"))]
                if c.kind == folio_ai::connection::Kind::Acp {
                    let result=folio_ai::acp::executable(&c.command).map(|_|json!({"available":true,"models":[]}));
                    let _=tx.send(crate::attach::Incoming::AiModelList{request,result});return;
                }
                let result = network::models(t.as_ref(), &c, &key, &Cancel::default())
                    .map(|ids| json!({"available":true,"models":ids}));
                let _ = tx.send(crate::attach::Incoming::AiModelList { request, result });
            })
            .map_err(|_| "could not start connection test")?;
        self.model_requests += 1;
        Ok(())
    }
    pub(super) fn connections(&mut self, cmd: &str, req: &Value) -> Result<Value, String> {
        let current = saved()?;
        if cmd == "aiConnections" {
            let entries = list(&current)?
                .into_iter()
                .map(|c| {
                    let has = !key_for(&self.credentials, &c).is_empty();
                    let mut value = json!(c);
                    if c.kind == folio_ai::connection::Kind::Acp {value["agentProfile"]=json!(super::agent_profile(&c));}
                    value["hasKey"] = json!(has);
                    value
                })
                .collect::<Vec<_>>();
            return Ok(
                json!({"connections":entries,"defaultConnection":current[DEFAULT],"templates":templates()}),
            );
        }
        if cmd == "aiSaveConnection" {
            if self.secrets.clean(&req["connection"]) != req["connection"] {
                return Err("invalid connection fields".into());
            }
            let c = Connection::parse(&req["connection"])?;
            crate::user_settings::update(|s, _| {
                let p = prefs(s)?;
                let mut entries = list(p)?;
                if let Some(old) = entries.iter_mut().find(|v| v.id == c.id) {
                    *old = c.clone();
                } else {
                    entries.push(c.clone())
                }
                if p[DEFAULT].is_null() {
                    p[DEFAULT] = json!(c.id)
                }
                p[LIST] = json!(entries);
                Ok(())
            })?;
            if key_for(&self.credentials, &c).is_empty() {
                self.credentials.remove(&c.id);
            }
            return Ok(json!({"connectionId":c.id}));
        }
        let id = &req["connectionId"];
        let c = find(&current, id)?;
        match cmd {
            "aiSetCredential" => {
                let key = req["key"]
                    .as_str()
                    .filter(|v| v.len() <= 8192 && v.bytes().all(|b| (33..127).contains(&b)))
                    .ok_or("invalid credential")?;
                if req.get("agentProfile").is_some_and(|p| p != &super::agent_profile(&c))
                    || req.get("baseUrl").is_some_and(|u| u != &c.base_url)
                    || req.get("kind").is_some_and(|k| k != &json!(c.kind))
                {
                    return Err("aiNotConfigured".into());
                }
                self.secrets.remember(key);
                if key.is_empty() {
                    self.credentials.remove(&c.id);
                } else {
                    self.credentials
                        .insert(c.id.clone(), (c.kind, credential_scope(&c), key.into()));
                }
                Ok(json!({"hasKey":!key.is_empty()}))
            }
            "aiDeleteConnection" | "aiSetDefaultConnection" => {
                crate::user_settings::update(|s, _| {
                    let p = prefs(s)?;
                    find(p, id)?;
                    if cmd == "aiDeleteConnection" {
                        let mut entries = list(p)?;
                        entries.retain(|v| v.id != c.id);
                        if &p[DEFAULT] == id {
                            p[DEFAULT] = Value::Null
                        }
                        p[LIST] = json!(entries)
                    } else {
                        p[DEFAULT] = id.clone()
                    }
                    Ok(())
                })?;
                if cmd == "aiDeleteConnection" {
                    self.credentials.remove(&c.id);
                }
                Ok(json!({}))
            }
            _ => Err("unknown connection command".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_ai::transport::{Request, Response, Transport};
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };

    struct Models;
    impl Transport for Models {
        fn send(&self, req: &Request, _: &Cancel) -> Result<Response, String> {
            assert_eq!(req.url, "https://api.anthropic.com/v1/models");
            assert!(req.body.is_none());
            assert!(
                req.headers
                    .iter()
                    .any(|(h, v)| *h == "x-api-key" && v == "test-key-not-real")
            );
            Ok(Response {
                status: 200,
                retry_after: None,
                body: Box::new(std::io::Cursor::new(
                    br#"{"data":[{"id":"claude-opus-5-5"}]}"#,
                )),
            })
        }
    }
    #[test]
    fn model_commands_use_the_selected_connection_without_sending_a_message() {
        let mut runtime = Runtime {
            transport: Arc::new(Models),
            ..Default::default()
        };
        let c = Connection::parse(&json!({"id":"selected","kind":"anthropic","name":"fixture"}))
            .unwrap();
        let prefs = json!({(LIST): [c]});
        runtime.credentials.insert(
            c.id.clone(),
            (c.kind, c.base_url, "test-key-not-real".into()),
        );
        let (tx, rx) = mpsc::channel();
        for cmd in ["aiModels", "aiTestConnection"] {
            let request = json!({"cmd":cmd,"id":42,"connectionId":"selected"});
            runtime.queue_model_list(&prefs, &request, &tx).unwrap();
            let crate::attach::Incoming::AiModelList { request, result } =
                rx.recv_timeout(Duration::from_secs(5)).unwrap()
            else {
                panic!("expected model list")
            };
            let reply = runtime.model_list_result(&request, result);
            assert_eq!(
                reply,
                json!({"available":true,"models":["claude-opus-5-5"],"ok":true,"type":cmd,"id":42})
            );
            assert_eq!(runtime.model_requests, 0);
        }
    }
}
